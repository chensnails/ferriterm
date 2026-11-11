//! Command approval policy.
//!
//! The grading engine lives in `ft-policy`; this module owns its persistence
//! and the commands that expose it. Nothing here executes anything — the
//! caller runs a command only after a [`ft_policy::Verdict::Allow`], and only
//! a person can turn an [`ft_policy::Verdict::Ask`] into one.
//!
//! The policy is stored as one JSON document under the `policy` setting so a
//! single read gives the evaluator both the mode and the rule list.

use std::sync::Arc;

use ft_policy::{
    AuditEntry, AuditLog, Choice, Context, Decision, Effect, Mode, Origin, Policy, Reason, Risk,
    Rule, Verdict, VerifyReport,
};
use serde::{Deserialize, Serialize};
use tauri::State;

type Store = Arc<ft_store::Store>;

const POLICY_KEY: &str = "policy";
/// Months of audit records kept on disk before the oldest file is dropped.
const AUDIT_MONTHS_KEPT: usize = 12;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PolicyConfig {
    pub mode: Mode,
    pub rules: Vec<Rule>,
}

impl Default for PolicyConfig {
    fn default() -> Self {
        Self {
            mode: Mode::Auto,
            rules: Vec::new(),
        }
    }
}

impl PolicyConfig {
    fn to_policy(&self) -> Policy {
        Policy {
            mode: self.mode,
            rules: self.rules.clone(),
        }
    }
}

/// The policy in force. Shared with the agent service, which must grade a
/// command with exactly the rules the interface shows.
pub(crate) fn load(store: &Store) -> Result<PolicyConfig, String> {
    let raw = store
        .get_setting(POLICY_KEY)
        .map_err(|err| err.to_string())?;
    Ok(raw
        .and_then(|value| serde_json::from_str::<PolicyConfig>(&value).ok())
        .unwrap_or_default())
}

fn persist(store: &Store, config: &PolicyConfig) -> Result<(), String> {
    let encoded = serde_json::to_string(config).map_err(|err| err.to_string())?;
    store
        .set_setting(POLICY_KEY, &encoded)
        .map_err(|err| err.to_string())
}

/// Current mode and rule list.
#[tauri::command]
pub fn policy_get(store: State<'_, Store>) -> Result<PolicyConfig, String> {
    load(&store)
}

/// Replace the whole configuration (mode switches and rule edits in settings).
#[tauri::command]
pub fn policy_save(store: State<'_, Store>, config: PolicyConfig) -> Result<PolicyConfig, String> {
    let mut config = config;
    // A rule with no pattern would match nothing yet sit in the list looking
    // active, so it is dropped on write rather than stored and ignored.
    config.rules.retain(|rule| !rule.pattern.trim().is_empty());
    persist(&store, &config)?;
    Ok(config)
}

/// Grade one command line against the stored policy. Shared with the agent
/// service so both entry points answer with the same engine and the same rules.
pub(crate) fn decide(
    store: &Store,
    command: &str,
    host: &str,
    elevated: bool,
) -> Result<Decision, String> {
    let config = load(store)?;
    Ok(ft_policy::evaluate(
        &config.to_policy(),
        command,
        &Context {
            host: host.to_string(),
            elevated,
        },
    ))
}

/// Grade one command line. The answer is advisory to the caller: honour it.
#[tauri::command]
pub fn policy_evaluate(
    store: State<'_, Store>,
    command: String,
    host: String,
    elevated: bool,
) -> Result<Decision, String> {
    let config = load(&store)?;
    Ok(ft_policy::evaluate(
        &config.to_policy(),
        &command,
        &Context { host, elevated },
    ))
}

/// Append a rule, used by the approval card's "always allow" and by settings.
#[tauri::command]
pub fn policy_add_rule(
    store: State<'_, Store>,
    pattern: String,
    effect: Effect,
    host: Option<String>,
    note: Option<String>,
) -> Result<PolicyConfig, String> {
    let pattern = pattern.trim().to_string();
    if pattern.is_empty() {
        return Err("a rule needs a pattern".into());
    }
    let mut config = load(&store)?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_millis())
        .unwrap_or_default();
    config.rules.push(Rule {
        id: format!("r{stamp}"),
        pattern,
        effect,
        host: host.unwrap_or_default().trim().to_string(),
        note: note.unwrap_or_default(),
        builtin: false,
    });
    persist(&store, &config)?;
    Ok(config)
}

#[tauri::command]
pub fn policy_remove_rule(store: State<'_, Store>, id: String) -> Result<PolicyConfig, String> {
    let mut config = load(&store)?;
    config.rules.retain(|rule| rule.id != id);
    persist(&store, &config)?;
    Ok(config)
}

// ------------------------------------------------------------------- audit

/// What the caller did with a decision, sent back after the fact.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditInput {
    pub command: String,
    pub host: String,
    pub decision: Decision,
    pub choice: Choice,
    /// Who asked. Absent means the window: a record written before this field
    /// existed has none, and neither does a call that forgot to say.
    #[serde(default)]
    pub origin: Option<Origin>,
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_secs())
        .unwrap_or_default()
}

/// Append one decision to the chain. Written for every verdict, including the
/// ones that never ran, because "what almost ran" is worth keeping.
pub(crate) fn record_decision(audit: &AuditLog, entry: AuditInput) -> Result<AuditEntry, String> {
    let mut record = AuditEntry {
        id: String::new(),
        ts: String::new(),
        host: entry.host,
        command: entry.command,
        risk: entry.decision.risk,
        reasons: entry.decision.reasons,
        rule: entry.decision.rule,
        verdict: entry.decision.verdict,
        choice: entry.choice,
        mode: entry.decision.mode,
        // Always written for new records, so a missing origin genuinely means
        // "recorded before the field existed" rather than "not said".
        origin: Some(entry.origin.unwrap_or(Origin::Ui)),
        prev_hash: String::new(),
        hash: String::new(),
    };
    audit
        .append(&mut record, now_secs())
        .map_err(|err| err.to_string())?;
    // Retention is applied on write: a log that grows forever is a log nobody
    // keeps checking.
    let _ = audit.prune(AUDIT_MONTHS_KEPT);
    Ok(record)
}

/// The command wrapper: the interface records what the person chose, the agent
/// service records what it was told to do, and both land in the same file.
#[tauri::command]
pub fn policy_record(audit: State<'_, AuditLog>, entry: AuditInput) -> Result<AuditEntry, String> {
    record_decision(&audit, entry)
}

#[tauri::command]
pub fn policy_audit_months(audit: State<'_, AuditLog>) -> Result<Vec<String>, String> {
    audit.months().map_err(|err| err.to_string())
}

#[tauri::command]
pub fn policy_audit_list(
    audit: State<'_, AuditLog>,
    month: String,
) -> Result<Vec<AuditEntry>, String> {
    audit.read_month(&month).map_err(|err| err.to_string())
}

/// Verify one month, or every month when `month` is empty.
#[tauri::command]
pub fn policy_audit_verify(
    audit: State<'_, AuditLog>,
    month: Option<String>,
) -> Result<Vec<VerifyReport>, String> {
    let months = match month.filter(|value| !value.trim().is_empty()) {
        Some(month) => vec![month],
        None => audit.months().map_err(|err| err.to_string())?,
    };
    let mut reports = Vec::new();
    for month in months {
        reports.push(audit.verify(&month).map_err(|err| err.to_string())?);
    }
    Ok(reports)
}

/// Absolute path of a month file, so the interface can reveal it.
#[tauri::command]
pub fn policy_audit_path(audit: State<'_, AuditLog>, month: String) -> String {
    audit
        .dir()
        .join(format!("audit-{month}.jsonl"))
        .display()
        .to_string()
}

/// Grading summary used by the settings page to explain a rule before saving.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RulePreview {
    pub risk: Risk,
    pub reasons: Vec<Reason>,
    pub verdict: Verdict,
}

#[tauri::command]
pub fn policy_preview(
    store: State<'_, Store>,
    pattern: String,
    host: String,
) -> Result<RulePreview, String> {
    let config = load(&store)?;
    let decision = ft_policy::evaluate(
        &config.to_policy(),
        &pattern,
        &Context {
            host,
            elevated: false,
        },
    );
    Ok(RulePreview {
        risk: decision.risk,
        reasons: decision.reasons,
        verdict: decision.verdict,
    })
}
