//! Command approval policy for the non-human execution paths — the assistant's
//! "run" action today, the `ferriterm-cli` agent and MCP tools next.
//!
//! The engine answers one question: *may this command line run against this
//! host right now, or does a person have to say so?* It never executes
//! anything; it returns a [`Decision`] the caller is expected to honour.
//!
//! ```
//! use ft_policy::{evaluate, Context, Mode, Policy, Verdict};
//!
//! let policy = Policy::default();
//! let decision = evaluate(&policy, "ls -la", &Context::host("web-1"));
//! assert_eq!(decision.verdict, Verdict::Allow);
//!
//! let decision = evaluate(&policy, "rm -rf /", &Context::host("web-1"));
//! assert_eq!(decision.verdict, Verdict::Deny);
//! ```
//!
//! Typed commands are never routed through here: a person at the keyboard is
//! the authorisation. The engine exists because anything else — a model, an
//! agent, a script over the control socket — is not.

pub mod audit;
pub mod parse;
pub mod risk;
pub mod rules;

use serde::{Deserialize, Serialize};

pub use audit::{AuditEntry, AuditLog, Choice, Origin, VerifyReport};
pub use parse::{Command, Segment};
pub use risk::{Assessment, Reason, Risk};
pub use rules::{Effect, Rule};

/// How much rope the automatic paths get.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// Anything short of a fatal delete runs unattended.
    #[default]
    Auto,
    /// Every write asks first.
    Ask,
    /// Nothing runs; commands are only shown.
    Plan,
}

impl Mode {
    pub fn id(self) -> &'static str {
        match self {
            Mode::Auto => "auto",
            Mode::Ask => "ask",
            Mode::Plan => "plan",
        }
    }

    /// The risk level at which this mode stops and asks.
    fn threshold(self) -> Option<Risk> {
        match self {
            Mode::Auto => Some(Risk::R3),
            Mode::Ask => Some(Risk::R2),
            Mode::Plan => None,
        }
    }
}

/// What the caller must do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Verdict {
    /// Run it.
    Allow,
    /// Ask the user first; the decision card shows [`Decision::risk`] and
    /// [`Decision::reasons`].
    Ask,
    /// Refuse. Only a person can override this, by editing the command.
    Deny,
}

/// Where the command would run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Context {
    /// Host name or label, matched against a rule's `host` field.
    pub host: String,
    /// The session already has root, so nothing needs `sudo` to be dangerous.
    pub elevated: bool,
}

impl Context {
    pub fn host(host: &str) -> Self {
        Self {
            host: host.to_string(),
            elevated: false,
        }
    }

    pub fn elevated(host: &str) -> Self {
        Self {
            host: host.to_string(),
            elevated: true,
        }
    }
}

/// The user's policy configuration.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Policy {
    pub mode: Mode,
    pub rules: Vec<Rule>,
}

/// The answer, with enough detail for the approval card.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Decision {
    pub verdict: Verdict,
    pub risk: Risk,
    pub reasons: Vec<Reason>,
    /// Id of the rule that settled it, if any.
    pub rule: Option<String>,
    /// The mode in force, so the card can say why it asked.
    pub mode: Mode,
    /// Fatal: the command is refused no matter which rule matches.
    pub hard: bool,
}

impl Decision {
    pub fn requires_approval(&self) -> bool {
        self.verdict == Verdict::Ask
    }
}

/// Grade `command` and apply the policy rules to it.
pub fn evaluate(policy: &Policy, command: &str, context: &Context) -> Decision {
    let mut assessment = risk::assess_line(command);
    if context.elevated {
        assessment.risk = assessment.risk.at_least(Risk::R2);
        if !assessment.reasons.contains(&Reason::Elevated) {
            assessment.reasons.push(Reason::Elevated);
        }
    }

    let normalized = parse::collapse(command.trim());
    let hit = rules::matching(&policy.rules, &normalized, &context.host);

    // A fatal delete is never negotiable: no user rule may whitelist it. The
    // only way past it is to rewrite the command.
    if assessment.hard {
        return Decision {
            verdict: Verdict::Deny,
            risk: assessment.risk,
            reasons: assessment.reasons,
            rule: None,
            mode: policy.mode,
            hard: true,
        };
    }

    let verdict = match hit.map(|rule| rule.effect) {
        Some(Effect::Deny) => Verdict::Deny,
        Some(Effect::Allow) => Verdict::Allow,
        Some(Effect::Ask) => Verdict::Ask,
        None => match policy.mode.threshold() {
            None => Verdict::Deny,
            Some(threshold) if assessment.risk >= threshold => Verdict::Ask,
            Some(_) => Verdict::Allow,
        },
    };

    Decision {
        verdict,
        risk: assessment.risk,
        reasons: assessment.reasons,
        rule: hit.map(|rule| rule.id.clone()).filter(|id| !id.is_empty()),
        mode: policy.mode,
        hard: false,
    }
}

impl Policy {
    pub fn with_rules(rules: Vec<Rule>) -> Self {
        Self {
            mode: Mode::default(),
            rules,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decide(mode: Mode, line: &str) -> Decision {
        let policy = Policy {
            mode,
            rules: Vec::new(),
        };
        evaluate(&policy, line, &Context::host("web-1"))
    }

    #[test]
    fn auto_runs_reads_and_asks_for_danger() {
        assert_eq!(decide(Mode::Auto, "ls -la").verdict, Verdict::Allow);
        assert_eq!(decide(Mode::Auto, "mkdir build").verdict, Verdict::Allow);
        assert_eq!(decide(Mode::Auto, "rm -rf build").verdict, Verdict::Ask);
    }

    #[test]
    fn ask_mode_stops_at_every_write() {
        assert_eq!(decide(Mode::Ask, "ls -la").verdict, Verdict::Allow);
        assert_eq!(decide(Mode::Ask, "mkdir build").verdict, Verdict::Allow);
        assert_eq!(decide(Mode::Ask, "cp a b").verdict, Verdict::Ask);
        assert_eq!(decide(Mode::Ask, "rm build.log").verdict, Verdict::Ask);
    }

    #[test]
    fn plan_mode_runs_nothing() {
        assert_eq!(decide(Mode::Plan, "ls").verdict, Verdict::Deny);
    }

    #[test]
    fn fatal_delete_survives_an_allow_rule() {
        let policy = Policy {
            mode: Mode::Auto,
            rules: vec![Rule {
                id: "r1".into(),
                pattern: "rm *".into(),
                effect: Effect::Allow,
                ..Rule::default()
            }],
        };
        let decision = evaluate(&policy, "rm -rf /", &Context::host("web-1"));
        assert_eq!(decision.verdict, Verdict::Deny);
        assert!(decision.hard);
    }

    #[test]
    fn an_allow_rule_overrides_the_mode_threshold() {
        let policy = Policy {
            mode: Mode::Ask,
            rules: vec![Rule {
                id: "r1".into(),
                pattern: "systemctl restart nginx".into(),
                effect: Effect::Allow,
                ..Rule::default()
            }],
        };
        let decision = evaluate(&policy, "systemctl restart nginx", &Context::host("web-1"));
        assert_eq!(decision.verdict, Verdict::Allow);
        assert_eq!(decision.rule.as_deref(), Some("r1"));
    }

    #[test]
    fn a_deny_rule_stops_an_harmless_command() {
        let policy = Policy {
            mode: Mode::Auto,
            rules: vec![Rule {
                id: "r1".into(),
                pattern: "cat *".into(),
                effect: Effect::Deny,
                ..Rule::default()
            }],
        };
        let decision = evaluate(&policy, "cat notes.txt", &Context::host("web-1"));
        assert_eq!(decision.verdict, Verdict::Deny);
        assert!(!decision.hard);
    }

    #[test]
    fn rules_are_host_scoped() {
        let mut rule = Rule::new("rm *", Effect::Allow);
        rule.id = "r1".into();
        rule.host = "build-*".into();
        let policy = Policy {
            mode: Mode::Ask,
            rules: vec![rule],
        };
        assert_eq!(
            evaluate(&policy, "rm cache.bin", &Context::host("build-09")).verdict,
            Verdict::Allow
        );
        assert_eq!(
            evaluate(&policy, "rm cache.bin", &Context::host("prod-1")).verdict,
            Verdict::Ask
        );
    }

    #[test]
    fn a_root_session_raises_the_floor() {
        let policy = Policy::default();
        let decision = evaluate(&policy, "cat /etc/motd", &Context::elevated("web-1"));
        assert!(decision.risk >= Risk::R2);
        assert!(decision.reasons.contains(&Reason::Elevated));
    }

    #[test]
    fn fake_spacing_does_not_escape_a_rule() {
        let policy = Policy {
            mode: Mode::Ask,
            rules: vec![Rule {
                id: "r1".into(),
                pattern: "systemctl restart nginx".into(),
                effect: Effect::Allow,
                ..Rule::default()
            }],
        };
        let decision = evaluate(
            &policy,
            "systemctl    restart   nginx",
            &Context::host("web-1"),
        );
        assert_eq!(decision.verdict, Verdict::Allow);
    }

    #[test]
    fn decision_round_trips_through_json() {
        let decision = decide(Mode::Auto, "rm -rf build");
        let text = serde_json::to_string(&decision).expect("serialize");
        let back: Decision = serde_json::from_str(&text).expect("deserialize");
        assert_eq!(back, decision);
    }
}
