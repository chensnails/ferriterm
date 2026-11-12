//! Assistant conversation history IPC.
//!
//! The panel owns the transcript in memory while a reply streams; rows are
//! appended once a message is final, so a cancelled or failed stream never
//! leaves a half-written answer in the database. Nothing here touches the
//! network — history stays local.

use std::sync::Arc;

use ft_store::{AiMessageRow, AiSession, Store};
use tauri::State;

type StoreState = Arc<Store>;

#[tauri::command]
pub fn ai_sessions(store: State<'_, StoreState>) -> Result<Vec<AiSession>, String> {
    store.ai_sessions().map_err(|err| err.to_string())
}

#[tauri::command]
pub fn ai_session_create(
    store: State<'_, StoreState>,
    title: String,
    provider_id: String,
    model: String,
) -> Result<AiSession, String> {
    store
        .ai_session_create(&title, &provider_id, &model)
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn ai_session_messages(
    store: State<'_, StoreState>,
    id: String,
) -> Result<Vec<AiMessageRow>, String> {
    store
        .ai_session_messages(&id)
        .map_err(|err| err.to_string())
}

/// Append one final message; `None` means the session was deleted meanwhile.
#[tauri::command]
pub fn ai_session_append(
    store: State<'_, StoreState>,
    id: String,
    role: String,
    content: String,
    thinking: String,
) -> Result<Option<AiSession>, String> {
    store
        .ai_session_append(&id, &role, &content, &thinking)
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn ai_session_rename(
    store: State<'_, StoreState>,
    id: String,
    title: String,
) -> Result<(), String> {
    store
        .ai_session_set_title(&id, &title)
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn ai_session_delete(store: State<'_, StoreState>, id: String) -> Result<bool, String> {
    store.ai_session_delete(&id).map_err(|err| err.to_string())
}
