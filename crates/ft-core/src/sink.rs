/// Transport-agnostic output sink. The Tauri shell implements this over
/// `AppHandle::emit`; transports never depend on Tauri directly.
pub trait TerminalSink: Send + Sync {
    fn output(&self, id: &str, data: Vec<u8>);
    fn closed(&self, id: &str);
}
