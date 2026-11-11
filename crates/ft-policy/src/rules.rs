//! User rules on top of the built-in grading.
//!
//! A rule matches a command with shell-style globbing and may optionally be
//! scoped to a host. When several rules match, the **last** one wins, so a
//! narrow exception can be written after a broad rule without reordering
//! everything — the same convention sshd and gitignore use.

use serde::{Deserialize, Serialize};

/// What a matching rule decides.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Effect {
    Allow,
    Ask,
    Deny,
}

impl Effect {
    pub fn id(self) -> &'static str {
        match self {
            Effect::Allow => "allow",
            Effect::Ask => "ask",
            Effect::Deny => "deny",
        }
    }
}

/// One entry of the user's rule list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rule {
    pub id: String,
    /// Glob over the whole command line, e.g. `systemctl restart *`.
    pub pattern: String,
    pub effect: Effect,
    /// Glob over the host name or tags. Empty matches every host.
    #[serde(default)]
    pub host: String,
    #[serde(default)]
    pub note: String,
    /// Written by the approval card ("always allow"), not typed by hand.
    #[serde(default)]
    pub builtin: bool,
}

impl Default for Rule {
    fn default() -> Self {
        Self {
            id: String::new(),
            pattern: String::new(),
            effect: Effect::Ask,
            host: String::new(),
            note: String::new(),
            builtin: false,
        }
    }
}

impl Rule {
    pub fn new(pattern: &str, effect: Effect) -> Self {
        Self {
            pattern: pattern.to_string(),
            effect,
            id: String::new(),
            host: String::new(),
            note: String::new(),
            builtin: false,
        }
    }

    fn applies_to_host(&self, host: &str) -> bool {
        if self.host.trim().is_empty() {
            return true;
        }
        self.host
            .split(',')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .any(|part| glob_match(part, host))
    }

    pub fn matches(&self, command: &str, host: &str) -> bool {
        !self.pattern.trim().is_empty()
            && glob_match(self.pattern.trim(), command)
            && self.applies_to_host(host)
    }
}

/// Case-insensitive glob with `*` (any run, including `/`) and `?` (one
/// character). Backtracking is linear thanks to the single-star memory slot.
pub fn glob_match(pattern: &str, text: &str) -> bool {
    let pattern: Vec<char> = pattern.to_lowercase().chars().collect();
    let text: Vec<char> = text.to_lowercase().chars().collect();
    let (mut pi, mut ti) = (0usize, 0usize);
    let mut star: Option<usize> = None;
    let mut mark = 0usize;
    while ti < text.len() {
        if pi < pattern.len() && (pattern[pi] == '?' || pattern[pi] == text[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < pattern.len() && pattern[pi] == '*' {
            star = Some(pi);
            mark = ti;
            pi += 1;
        } else if let Some(start) = star {
            pi = start + 1;
            mark += 1;
            ti = mark;
        } else {
            return false;
        }
    }
    while pi < pattern.len() && pattern[pi] == '*' {
        pi += 1;
    }
    pi == pattern.len()
}

/// Last match wins, matching the order the rules are stored in.
pub fn matching<'a>(rules: &'a [Rule], command: &str, host: &str) -> Option<&'a Rule> {
    rules.iter().rev().find(|rule| rule.matches(command, host))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn globs_anywhere() {
        assert!(glob_match("rm *", "rm -rf build"));
        assert!(glob_match("*rm*", "/bin/rm -rf build"));
        assert!(glob_match("git status", "git status"));
        assert!(!glob_match("git status", "git push"));
        assert!(glob_match("kubectl get ?od", "kubectl get pod"));
        assert!(glob_match("*", ""));
    }

    #[test]
    fn globbing_ignores_case() {
        assert!(glob_match("GIT *", "git push"));
    }

    #[test]
    fn last_rule_wins() {
        let rules = vec![
            Rule::new("rm *", Effect::Deny),
            Rule::new("rm ./build/*", Effect::Allow),
        ];
        assert_eq!(
            matching(&rules, "rm ./build/main.o", "web-1").map(|r| r.effect),
            Some(Effect::Allow)
        );
        assert_eq!(
            matching(&rules, "rm ./src/main.rs", "web-1").map(|r| r.effect),
            Some(Effect::Deny)
        );
    }

    #[test]
    fn host_scope_limits_a_rule() {
        let mut rule = Rule::new("systemctl restart nginx", Effect::Allow);
        rule.host = "web-*".into();
        assert!(rule.matches("systemctl restart nginx", "web-01"));
        assert!(!rule.matches("systemctl restart nginx", "db-01"));
    }

    #[test]
    fn host_scope_accepts_a_list() {
        let mut rule = Rule::new("df -h", Effect::Allow);
        rule.host = "web-1, db-1".into();
        assert!(rule.matches("df -h", "db-1"));
        assert!(!rule.matches("df -h", "cache-1"));
    }

    #[test]
    fn empty_pattern_never_matches() {
        assert!(!Rule::new("  ", Effect::Allow).matches("anything", ""));
    }
}
