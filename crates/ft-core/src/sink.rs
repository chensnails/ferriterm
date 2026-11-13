/// Transport-agnostic output sink. The Tauri shell implements this over
/// `AppHandle::emit`; transports never depend on Tauri directly.
use crate::lifecycle::ClosedPayload;

pub trait TerminalSink: Send + Sync {
    fn output(&self, id: &str, data: Vec<u8>);
    fn closed(&self, id: &str);

    /// Structured close notification. The default keeps older sinks (and
    /// lightweight test doubles) working with no changes: they still receive
    /// the plain `closed` call they always did.
    fn closed_with(&self, id: &str, payload: ClosedPayload) {
        let _ = payload;
        self.closed(id);
    }
}
