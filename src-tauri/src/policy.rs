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

use ft_policy::{Context, Decision, Effect, Mode, Policy, Rule};
use serde::{Deserialize, Serialize};
use tauri::State;

type Store = Arc<ft_store::Store>;

const POLICY_KEY: &str = "policy";

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

fn load(store: &Store) -> Result<PolicyConfig, String> {
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
pub fn policy_save(
    store: State<'_, Store>,
    config: PolicyConfig,
) -> Result<PolicyConfig, String> {
    let mut config = config;
    // A rule with no pattern would match nothing yet sit in the list looking
    // active, so it is dropped on write rather than stored and ignored.
    config.rules.retain(|rule| !rule.pattern.trim().is_empty());
    persist(&store, &config)?;
    Ok(config)
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
