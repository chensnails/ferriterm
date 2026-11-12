//! Built-in AI commands (BYOK).
//!
//! Provider definitions live in the `aiProviders` setting as JSON; API keys go
//! to the credential vault under `ai:<providerId>` and are never returned to the
//! frontend. Context is built and redacted here, so a UI bug cannot leak raw
//! terminal output to a third-party endpoint.

use std::sync::Arc;

use ft_ai::{ContextRequest, HeaderPair, Kind, Provider, Reply};
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
    /// The model declared the goal met and deliberately returned no command.
    #[serde(default)]
    pub done: bool,
    pub latency_ms: u64,
    pub model: String,
    /// i18n keys for parameters that had to be dropped (see `ft_ai::Reply`).
    #[serde(default)]
    pub notes: Vec<String>,
}

/// Error tokens the frontend maps to localized text. A raw English sentence
/// here would end up untranslated in the step log.
const ERR_EMPTY_REPLY: &str = "ai.err.emptyReply";
const ERR_EMPTY_COMMAND: &str = "ai.err.emptyCommand";

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
    // Clamp numbers and trim text once, on the way in, so every later request
    // uses the same sanitised shape the UI shows.
    let provider = provider.sanitize();
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
        // One-shot task that must return an object.
        true,
    )
    .await
    .map_err(|err| err.to_string())?;
    if reply.text.trim().is_empty() {
        return Err(ERR_EMPTY_REPLY.into());
    }
    let mut notes = reply.notes.clone();
    let mut model = reply.model.clone();
    let mut latency_ms = reply.latency_ms;
    let mut extract = ft_ai::extract_command_reply(&reply.text);
    // The reply was prose rather than JSON: ask once more, spelling out the
    // contract and showing the model what it actually sent. A blank reply is
    // not retried here because repeating the same request would not help.
    if !extract.done && extract.command.is_empty() {
        let repair_user = format!(
            "{user}\n\nYour previous reply was:\n{}",
            clip(&reply.text, 600)
        );
        if let Ok(second) = ft_ai::chat(
            &provider,
            &key,
            ft_ai::COMMAND_REPAIR_PROMPT,
            &repair_user,
            Some(1024),
            true,
        )
        .await
        {
            let retried = ft_ai::extract_command_reply(&second.text);
            if retried.done || !retried.command.is_empty() {
                extract = retried;
                latency_ms += second.latency_ms;
                model = second.model;
                for note in second.notes {
                    if !notes.contains(&note) {
                        notes.push(note);
                    }
                }
            }
        }
    }
    if extract.done {
        return Ok(CommandReply {
            command: String::new(),
            explanation: extract.explanation,
            done: true,
            latency_ms,
            model,
            notes,
        });
    }
    if extract.command.is_empty() {
        return Err(ERR_EMPTY_COMMAND.into());
    }
    Ok(CommandReply {
        command: extract.command,
        explanation: extract.explanation,
        done: false,
        latency_ms,
        model,
        notes,
    })
}

/// First `max` characters of `text`, on a char boundary.
fn clip(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}

#[tauri::command]
#[allow(clippy::too_many_arguments)] // IPC signature mirrors the frontend payload
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
    let user =
        ft_ai::build_explain_prompt(&command, code, host.as_deref().unwrap_or("local"), &tail);
    let user = match context.as_ref().map(|context| context.build().0) {
        Some(extra) if !extra.is_empty() => {
            format!("{user}\n\nTerminal context:\n```\n{extra}\n```")
        }
        _ => user,
    };
    ft_ai::chat(
        &provider,
        &key,
        ft_ai::EXPLAIN_SYSTEM_PROMPT,
        &user,
        Some(600),
        false,
    )
    .await
    .map_err(|err| err.to_string())
}

/// Preset catalogue returned to the settings UI. Every entry carries a fully
/// formed [`Provider`], so picking a vendor also adopts the protocol, model,
/// cap field and header defaults that vendor needs. Nothing here is special
/// cased later: the request path only ever reads the stored provider.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderPreset {
    pub id: &'static str,
    pub name: &'static str,
    pub provider: Provider,
}

fn preset(
    id: &'static str,
    name: &'static str,
    kind: Kind,
    base_url: &str,
    model: &str,
) -> ProviderPreset {
    ProviderPreset {
        id,
        name,
        provider: Provider {
            name: name.to_string(),
            kind,
            base_url: base_url.to_string(),
            model: model.to_string(),
            preset: id.to_string(),
            ..Provider::default()
        },
    }
}

#[tauri::command]
pub fn ai_presets() -> Vec<ProviderPreset> {
    let mut list = vec![
        preset(
            "openai",
            "OpenAI",
            Kind::OpenAi,
            "https://api.openai.com/v1",
            "gpt-4.1-mini",
        ),
        preset(
            "anthropic",
            "Anthropic",
            Kind::Anthropic,
            "https://api.anthropic.com/v1",
            "claude-sonnet-4-5",
        ),
        // Gemini exposes an OpenAI-compatible face; the native generateContent
        // shape is intentionally not implemented.
        preset(
            "gemini",
            "Google Gemini",
            Kind::OpenAi,
            "https://generativelanguage.googleapis.com/v1beta/openai",
            "gemini-2.5-flash",
        ),
        preset(
            "azure",
            "Azure OpenAI",
            Kind::Azure,
            "https://your-resource.openai.azure.com/openai/v1",
            "gpt-4.1-mini",
        ),
        preset(
            "deepseek",
            "DeepSeek",
            Kind::OpenAi,
            "https://api.deepseek.com/v1",
            "deepseek-chat",
        ),
        preset(
            "dashscope",
            "阿里云百炼（通义）",
            Kind::OpenAi,
            "https://dashscope.aliyuncs.com/compatible-mode/v1",
            "qwen-plus",
        ),
        preset(
            "zhipu",
            "智谱 GLM",
            Kind::OpenAi,
            "https://open.bigmodel.cn/api/paas/v4",
            "glm-4-flash",
        ),
        preset(
            "moonshot",
            "Kimi（月之暗面）",
            Kind::OpenAi,
            "https://api.moonshot.cn/v1",
            "moonshot-v1-8k",
        ),
        preset(
            "minimax",
            "MiniMax",
            Kind::OpenAi,
            "https://api.minimaxi.com/v1",
            "MiniMax-Text-01",
        ),
        preset(
            "siliconflow",
            "硅基流动",
            Kind::OpenAi,
            "https://api.siliconflow.cn/v1",
            "deepseek-ai/DeepSeek-V3",
        ),
        preset(
            "ark",
            "火山方舟（豆包）",
            Kind::OpenAi,
            "https://ark.cn-beijing.volces.com/api/v3",
            "doubao-pro-32k",
        ),
        preset(
            "qianfan",
            "百度千帆（文心）",
            Kind::OpenAi,
            "https://qianfan.baidubce.com/v2",
            "ernie-4.5-turbo-128k",
        ),
        preset(
            "hunyuan",
            "腾讯混元",
            Kind::OpenAi,
            "https://tokenhub.tencentmaas.com/v1",
            "hunyuan-turbos-latest",
        ),
        preset(
            "yi",
            "零一万物",
            Kind::OpenAi,
            "https://api.lingyiwanwu.com/v1",
            "yi-lightning",
        ),
        preset(
            "stepfun",
            "阶跃星辰",
            Kind::OpenAi,
            "https://api.stepfun.com/v1",
            "step-2-16k",
        ),
        preset(
            "groq",
            "Groq",
            Kind::OpenAi,
            "https://api.groq.com/openai/v1",
            "llama-3.3-70b-versatile",
        ),
        preset(
            "openrouter",
            "OpenRouter",
            Kind::OpenAi,
            "https://openrouter.ai/api/v1",
            "openai/gpt-4.1-mini",
        ),
        preset(
            "ollama",
            "Ollama（本地）",
            Kind::OpenAi,
            "http://127.0.0.1:11434/v1",
            "qwen2.5:7b",
        ),
    ];
    // Azure needs a version for the classic deployment path.
    if let Some(azure) = list.iter_mut().find(|item| item.id == "azure") {
        azure.provider.api_version = "2024-10-21".into();
    }
    // Cloud OpenAI-compatible vendors accept a JSON response format, which is
    // what keeps command generation parseable on small models. Ollama's local
    // shim is left alone because older builds reject the field outright.
    for item in list.iter_mut() {
        if item.id != "ollama" && !item.provider.kind.is_anthropic() {
            item.provider.json_mode = true;
        }
    }
    // OpenRouter asks callers to identify themselves; other vendors ignore it.
    if let Some(router) = list.iter_mut().find(|item| item.id == "openrouter") {
        router.provider.extra_headers = vec![HeaderPair {
            name: "HTTP-Referer".into(),
            value: "https://github.com/chensnails/ferriterm".into(),
        }];
    }
    list.push(ProviderPreset {
        id: "custom",
        name: "自定义",
        provider: Provider {
            name: "自定义".into(),
            base_url: "https://".into(),
            model: String::new(),
            preset: "custom".into(),
            ..Provider::default()
        },
    });
    list
}

/// Read the model catalogue straight from the vendor. Works on an unsaved
/// draft: the form key wins, otherwise the stored one is used.
#[tauri::command]
pub async fn ai_list_models(
    credentials: State<'_, Credentials>,
    provider: Provider,
    key: Option<String>,
) -> Result<Vec<String>, String> {
    let typed = key.unwrap_or_default().trim().to_string();
    let key = if typed.is_empty() {
        if provider.id.trim().is_empty() {
            return Err("enter an API key first".into());
        }
        credentials
            .get(&provider.secret_ref())
            .map_err(|err| err.to_string())?
            .unwrap_or_default()
    } else {
        typed
    };
    if key.trim().is_empty() {
        return Err("no API key saved for this provider".into());
    }
    ft_ai::models(&provider, &key)
        .await
        .map_err(|err| err.to_string())
}
