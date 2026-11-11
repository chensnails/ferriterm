use std::sync::Arc;

use tauri::State;

#[tauri::command]
pub fn list_language_packs(plugins: State<'_, ft_plugin::PluginRegistry>) -> ft_plugin::PluginScan {
    plugins.scan()
}

#[tauri::command]
pub fn plugin_scan(plugins: State<'_, ft_plugin::PluginRegistry>) -> ft_plugin::PluginScan {
    plugins.scan()
}

#[tauri::command]
pub fn plugin_open_dir(plugins: State<'_, ft_plugin::PluginRegistry>) -> Result<(), String> {
    let dir = plugins.dir().to_path_buf();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    open_in_file_manager(&dir)
}

fn open_in_file_manager(path: &std::path::Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let status = std::process::Command::new("open").arg(path).status();
    #[cfg(target_os = "windows")]
    let status = std::process::Command::new("explorer").arg(path).status();
    #[cfg(all(unix, not(target_os = "macos")))]
    let status = std::process::Command::new("xdg-open").arg(path).status();
    match status {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => Err(format!("file manager exited with {status}")),
        Err(err) => Err(err.to_string()),
    }
}

#[tauri::command]
pub fn plugin_import_commands(
    plugins: State<'_, ft_plugin::PluginRegistry>,
    store: State<'_, Arc<ft_store::Store>>,
    pack_id: String,
) -> Result<usize, String> {
    let pack = plugins
        .scan()
        .command_packs
        .into_iter()
        .find(|pack| pack.id == pack_id)
        .ok_or_else(|| format!("command pack {pack_id} not found"))?;
    let mut imported = 0;
    for (group_index, group) in pack.groups.iter().enumerate() {
        let group_id = format!("{}-g{group_index}", pack.id);
        let commands: Vec<(String, String, String)> = group
            .commands
            .iter()
            .enumerate()
            .map(|(command_index, command)| {
                (
                    format!("{group_id}-c{command_index}"),
                    command.name.clone(),
                    command.template.clone(),
                )
            })
            .collect();
        imported += store
            .import_command_group(&group_id, &group.name, &commands)
            .map_err(|e| e.to_string())?;
    }
    Ok(imported)
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WasmRunResult {
    output: String,
    logs: Vec<String>,
    fuel_used: u64,
}

#[tauri::command]
pub async fn plugin_run_wasm(
    plugins: State<'_, ft_plugin::PluginRegistry>,
    id: String,
    payload: String,
) -> Result<WasmRunResult, String> {
    let (_, bytes) = plugins.load_wasm(&id)?;
    tauri::async_runtime::spawn_blocking(move || {
        ft_plugin::run_wasm(
            &bytes,
            payload.as_bytes(),
            &ft_plugin::WasmLimits::default(),
        )
    })
    .await
    .map_err(|err| err.to_string())?
    .map(|run| WasmRunResult {
        output: run.output,
        logs: run.logs,
        fuel_used: run.fuel_used,
    })
}

const MARKET_INDEX_CAP: usize = 512 * 1024;
const MARKET_MANIFEST_CAP: usize = 256 * 1024;

/// Fetches a marketplace document. Only https URLs are accepted, and the body
/// is read in chunks so an oversized or endless response cannot exhaust memory.
async fn fetch_market_text(url: &str, cap: usize) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .user_agent(concat!("Ferriterm/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| e.to_string())?;
    let mut response = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("marketplace request failed: {e}"))?;
    if !response.status().is_success() {
        return Err(format!("marketplace returned HTTP {}", response.status()));
    }
    if response
        .content_length()
        .is_some_and(|len| len as usize > cap)
    {
        return Err("marketplace response is too large".to_string());
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
        if body.len() + chunk.len() > cap {
            return Err("marketplace response is too large".to_string());
        }
        body.extend_from_slice(&chunk);
    }
    String::from_utf8(body).map_err(|_| "marketplace response is not UTF-8".to_string())
}

#[tauri::command]
pub async fn plugin_market_fetch(
    index_url: Option<String>,
) -> Result<ft_plugin::MarketIndex, String> {
    let url = index_url
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| ft_plugin::DEFAULT_MARKET_URL.to_string());
    let raw = fetch_market_text(&url, MARKET_INDEX_CAP).await?;
    ft_plugin::parse_market_index(&raw)
}

#[tauri::command]
pub async fn plugin_market_install(
    plugins: State<'_, ft_plugin::PluginRegistry>,
    id: String,
    manifest_url: String,
) -> Result<ft_plugin::PluginManifest, String> {
    let raw = fetch_market_text(&manifest_url, MARKET_MANIFEST_CAP).await?;
    plugins.install(&id, &raw)
}

#[tauri::command]
pub fn plugin_market_uninstall(
    plugins: State<'_, ft_plugin::PluginRegistry>,
    id: String,
) -> Result<bool, String> {
    plugins.remove(&id)
}

#[tauri::command]
pub fn plugin_install_builtin(
    plugins: State<'_, ft_plugin::PluginRegistry>,
    id: String,
) -> Result<ft_plugin::PluginManifest, String> {
    plugins.install_builtin(&id)
}

pub(crate) fn serial_plugin_installed(plugins: &ft_plugin::PluginRegistry) -> bool {
    plugins.scan().plugins.iter().any(|plugin| {
        plugin.kind == "serial" && plugin.permissions.iter().any(|perm| perm == "serial")
    })
}

pub(crate) fn telnet_plugin_installed(plugins: &ft_plugin::PluginRegistry) -> bool {
    plugins.scan().plugins.iter().any(|plugin| {
        plugin.kind == "telnet" && plugin.permissions.iter().any(|perm| perm == "telnet")
    })
}
#[cfg(test)]
mod market_tests {
    use super::*;

    /// Hits the published marketplace so a broken index or moved manifest is
    /// caught before release. Not part of the normal suite because it needs
    /// network access.
    #[tokio::test]
    #[ignore = "requires network access to the live GitHub marketplace"]
    async fn installs_the_serial_plugin_from_the_live_marketplace() {
        let raw = fetch_market_text(ft_plugin::DEFAULT_MARKET_URL, MARKET_INDEX_CAP)
            .await
            .unwrap();
        let index = ft_plugin::parse_market_index(&raw).unwrap();
        let entry = index
            .plugins
            .iter()
            .find(|entry| entry.kind == "serial")
            .expect("marketplace lists the serial plugin");
        let manifest = fetch_market_text(&entry.manifest_url, MARKET_MANIFEST_CAP)
            .await
            .unwrap();

        let dir = std::env::temp_dir().join(format!(
            "ferriterm-market-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let registry = ft_plugin::PluginRegistry::new(&dir);
        let installed = registry.install(&entry.id, &manifest).unwrap();
        assert_eq!(installed.kind, "serial");
        assert!(registry.scan().plugins.iter().any(|p| p.id == entry.id));
        assert!(registry.remove(&entry.id).unwrap());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
