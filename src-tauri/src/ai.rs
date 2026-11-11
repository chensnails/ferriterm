//! Built-in AI commands (BYOK).
//!
//! Provider definitions live in the `aiProviders` setting as JSON; API keys go
//! to the credential vault under `ai:<providerId>` and are never returned to the
//! frontend. Context is built and redacted here, so a UI bug cannot leak raw
//! terminal output to a third-party endpoint.

use std::sync::Arc;

use ft_ai::{ContextRequest, Kind, Provider, Reply};
use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;
use tauri::State;

type Credentials = Arc<ft_security::CredentialStore>;
type Store = Arc<ft_store::Store>;

const PROVIDERS_KEY: &str = "aiProviders";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderView {
    #[serde(flatten)]
    pub provider: Provider,
    pub has_key: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamDelta {
    pub delta: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandReply {
    pub command: String,
    pub explanation: String,
    pub latency_ms: u64,
    pub model: String,
}

fn load_providers(store: &Store) -> Result<Vec<Provider>, String> {
    let raw = store
        .get_setting(PROVIDERS_KEY)
        .map_err(|err| err.to_string())?;
    Ok(raw
        .and_then(|value| serde_json::from_str::<Vec<Provider>>(&value).ok())
        .unwrap_or_default())
}

fn save_providers(store: &Store, providers: &[Provider]) -> Result<(), String> {
    let encoded = serde_json::to_string(providers).map_err(|err| err.to_string())?;
    store
        .set_setting(PROVIDERS_KEY, &encoded)
        .map_err(|err| err.to_string())
}

fn find_provider(store: &Store, id: &str) -> Result<Provider, String> {
    load_providers(store)?
        .into_iter()
        .find(|provider| provider.id == id)
        .ok_or_else(|| format!("unknown AI provider: {id}"))
}

fn key_for(credentials: &Credentials, provider: &Provider) -> Result<String, String> {
    let key = credentials
        .get(&provider.secret_ref())
        .map_err(|err| err.to_string())?
        .unwrap_or_default();
    if key.trim().is_empty() {
        return Err("no API key saved for this provider".into());
    }
    Ok(key)
}

fn attach_context(messages: &mut [ChatMessage], context: Option<&ContextRequest>) -> usize {
    let Some(context) = context else {
        return 0;
    };
    let (text, redacted) = context.build();
    if text.is_empty() {
        return redacted;
    }
    if let Some(last) = messages
        .iter_mut()
        .rev()
        .find(|message| message.role == "user")
    {
        last.content
            .push_str(&format!("\n\nTerminal context:\n```\n{text}\n```"));
    }
    redacted
}

fn to_wire(messages: &[ChatMessage]) -> Vec<(String, String)> {
    messages
        .iter()
        .map(|message| (message.role.clone(), message.content.clone()))
        .collect()
}

#[tauri::command]
pub fn ai_list_providers(
    store: State<'_, Store>,
    credentials: State<'_, Credentials>,
) -> Result<Vec<ProviderView>, String> {
    Ok(load_providers(&store)?
        .into_iter()
        .map(|provider| ProviderView {
            has_key: credentials.exists(&provider.secret_ref()),
            provider,
        })
        .collect())
}

#[tauri::command]
pub fn ai_save_provider(
    store: State<'_, Store>,
    mut provider: Provider,
) -> Result<ProviderView, String> {
    if provider.name.trim().is_empty() {
        return Err("provider name is required".into());
    }
    if provider.base_url.trim().is_empty() {
        return Err("base URL is required".into());
    }
    if provider.model.trim().is_empty() {
        return Err("model is required".into());
    }
    if provider.id.trim().is_empty() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|value| value.as_millis())
            .unwrap_or_default();
        provider.id = format!("p{stamp}");
    }
    let mut providers = load_providers(&store)?;
    match providers.iter_mut().find(|item| item.id == provider.id) {
        Some(existing) => *existing = provider.clone(),
        None => providers.push(provider.clone()),
    }
    save_providers(&store, &providers)?;
    Ok(ProviderView {
        has_key: false,
        provider,
    })
}

#[tauri::command]
pub fn ai_delete_provider(
    store: State<'_, Store>,
    credentials: State<'_, Credentials>,
    id: String,
) -> Result<(), String> {
    let mut providers = load_providers(&store)?;
    providers.retain(|provider| provider.id != id);
    save_providers(&store, &providers)?;
    let _ = credentials.delete(&format!("ai:{id}"));
    Ok(())
}

#[tauri::command]
pub async fn ai_test_provider(
    store: State<'_, Store>,
    credentials: State<'_, Credentials>,
    id: String,
) -> Result<Reply, String> {
    let provider = find_provider(&store, &id)?;
    let key = key_for(&credentials, &provider)?;
    ft_ai::test_connection(&provider, &key)
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn ai_chat(
    store: State<'_, Store>,
    credentials: State<'_, Credentials>,
    provider_id: String,
    messages: Vec<ChatMessage>,
    context: Option<ContextRequest>,
    system: Option<String>,
) -> Result<Reply, String> {
    let provider = find_provider(&store, &provider_id)?;
    let key = key_for(&credentials, &provider)?;
    let mut messages = messages;
    attach_context(&mut messages, context.as_ref());
    let system = system.unwrap_or_else(|| ft_ai::CHAT_SYSTEM_PROMPT.to_string());
    let wire = to_wire(&messages);
    ft_ai::chat_stream(&provider, &key, &system, &wire, |_| true)
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn ai_chat_stream(
    store: State<'_, Store>,
    credentials: State<'_, Credentials>,
    provider_id: String,
    messages: Vec<ChatMessage>,
    context: Option<ContextRequest>,
    system: Option<String>,
    channel: Channel<StreamDelta>,
) -> Result<Reply, String> {
    let provider = find_provider(&store, &provider_id)?;
    let key = key_for(&credentials, &provider)?;
    let mut messages = messages;
    attach_context(&mut messages, context.as_ref());
    let system = system.unwrap_or_else(|| ft_ai::CHAT_SYSTEM_PROMPT.to_string());
    let wire = to_wire(&messages);
    let mut send_error: Option<String> = None;
    let reply = ft_ai::chat_stream(&provider, &key, &system, &wire, |delta| {
        if send_error.is_some() {
            return false;
        }
        if let Err(err) = channel.send(StreamDelta {
            delta: delta.to_string(),
        }) {
            send_error = Some(err.to_string());
            return false;
        }
        true
    })
    .await
    .map_err(|err| err.to_string())?;
    if let Some(error) = send_error {
        return Err(error);
    }
    Ok(reply)
}

#[tauri::command]
pub async fn ai_generate_command(
    store: State<'_, Store>,
    credentials: State<'_, Credentials>,
    provider_id: String,
    request: String,
    host: Option<String>,
    shell: Option<String>,
    context: Option<ContextRequest>,
) -> Result<CommandReply, String> {
    let mut provider = find_provider(&store, &provider_id)?;
    let key = key_for(&credentials, &provider)?;
    let context_text = context
        .as_ref()
        .map(|context| context.build().0)
        .unwrap_or_default();
    let user = ft_ai::build_user_prompt(
        &request,
        shell.as_deref().unwrap_or("bash"),
        host.as_deref().unwrap_or("local"),
        &context_text,
    );
    // Command generation is a one-shot task; prefer the cheaper model when set.
    let fast = provider.command_model().to_string();
    if fast != provider.model {
        provider.model = fast;
    }
    let reply = ft_ai::chat(
        &provider,
        &key,
        ft_ai::COMMAND_SYSTEM_PROMPT,
        &user,
        // Reasoning models spend part of the budget on `reasoning_content`
        // before emitting the JSON, so keep this well above the reply size.
        Some(2048),
    )
    .await
    .map_err(|err| err.to_string())?;
    let (command, explanation) = ft_ai::extract_command(&reply.text);
    if command.is_empty() {
        return Err("the model did not return a command".into());
    }
    Ok(CommandReply {
        command,
        explanation,
        latency_ms: reply.latency_ms,
        model: reply.model,
    })
}

#[tauri::command]
pub async fn ai_explain(
    store: State<'_, Store>,
    credentials: State<'_, Credentials>,
    provider_id: String,
    command: String,
    code: i32,
    host: Option<String>,
    output_tail: String,
    context: Option<ContextRequest>,
) -> Result<Reply, String> {
    let provider = find_provider(&store, &provider_id)?;
    let key = key_for(&credentials, &provider)?;
    // The tail is redacted like any other context before it leaves.
    let (tail, _) = ft_ai::redact(&output_tail);
    let user = ft_ai::build_explain_prompt(
        &command,
        code,
        host.as_deref().unwrap_or("local"),
        &tail,
    );
    let user = match context.as_ref().map(|context| context.build().0) {
        Some(extra) if !extra.is_empty() => format!("{user}\n\nTerminal context:\n```\n{extra}\n```"),
        _ => user,
    };
    ft_ai::chat(&provider, &key, ft_ai::EXPLAIN_SYSTEM_PROMPT, &user, Some(600))
        .await
        .map_err(|err| err.to_string())
}

/// Preset catalogue returned to the settings UI. Base URLs and model names are
/// public defaults; the user can override any of them.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderPreset {
    pub id: &'static str,
    pub name: &'static str,
    pub kind: Kind,
    pub base_url: &'static str,
    pub model: &'static str,
}

#[tauri::command]
pub fn ai_presets() -> Vec<ProviderPreset> {
    vec![
        ProviderPreset {
            id: "openai",
            name: "OpenAI",
            kind: Kind::OpenAi,
            base_url: "https://api.openai.com/v1",
            model: "gpt-4.1-mini",
        },
        ProviderPreset {
            id: "anthropic",
            name: "Anthropic",
            kind: Kind::Anthropic,
            base_url: "https://api.anthropic.com/v1",
            model: "claude-sonnet-4-5",
        },
        ProviderPreset {
            id: "dashscope",
            name: "阿里云百炼（通义）",
            kind: Kind::OpenAi,
            base_url: "https://dashscope.aliyuncs.com/compatible-mode/v1",
            model: "qwen-plus",
        },
        ProviderPreset {
            id: "deepseek",
            name: "DeepSeek",
            kind: Kind::OpenAi,
            base_url: "https://api.deepseek.com/v1",
            model: "deepseek-chat",
        },
        ProviderPreset {
            id: "zhipu",
            name: "智谱 GLM",
            kind: Kind::OpenAi,
            base_url: "https://open.bigmodel.cn/api/paas/v4",
            model: "glm-4-flash",
        },
        ProviderPreset {
            id: "moonshot",
            name: "Kimi（月之暗面）",
            kind: Kind::OpenAi,
            base_url: "https://api.moonshot.cn/v1",
            model: "moonshot-v1-8k",
        },
        ProviderPreset {
            id: "ollama",
            name: "Ollama（本地）",
            kind: Kind::OpenAi,
            base_url: "http://127.0.0.1:11434/v1",
            model: "qwen2.5:7b",
        },
        ProviderPreset {
            id: "custom",
            name: "自定义",
            kind: Kind::OpenAi,
            base_url: "https://",
            model: "",
        },
    ]
}
