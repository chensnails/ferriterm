//! Connection lifecycle primitives shared by every transport.
//!
//! A connection used to announce its end with a bare `closed(id)` call. That
//! lost *why* it ended, let a reader thread and an explicit close both announce
//! it, and left stale registry entries behind when a device or socket died on
//! its own. These types give all four transports one contract:
//!
//! * `CloseReason` / `ClosedPayload` carry the cause to the webview;
//! * `SessionGuard` guarantees at most one announcement per connection and
//!   gives reader loops a cheap "should I stop?" flag;
//! * `GuardMap` reserves an id for the whole connect-in-flight window, so two
//!   concurrent opens cannot both win, and a close arriving mid-connect is
//!   visible to the opener as soon as it finishes.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::TerminalSink;

/// Why a session ended. Serialised as snake_case so the webview can map each
/// value onto a localised explanation without parsing prose.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CloseReason {
    /// The person closed the tab, disconnected, or cancelled a connect.
    User,
    /// The peer closed the stream cleanly (EOF / SSH channel close).
    RemoteEof,
    /// A transport-level read or write error.
    TransportError,
    /// A connect, handshake, authentication or read deadline elapsed.
    Timeout,
    /// A removable serial device stopped answering.
    DeviceRemoved,
    /// A locally spawned shell exited.
    ShellExit,
    /// The initial connection never completed.
    ConnectFailed,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClosedPayload {
    pub reason: CloseReason,
    /// Raw transport detail, shown as secondary text; never localised here.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl ClosedPayload {
    pub fn new(reason: CloseReason) -> Self {
        Self {
            reason,
            message: None,
        }
    }

    pub fn with_message(reason: CloseReason, message: impl Into<String>) -> Self {
        Self {
            reason,
            message: Some(message.into()),
        }
    }
}

#[derive(Default)]
struct GuardInner {
    stopped: AtomicBool,
    announced: AtomicBool,
}

/// One generation of one connection. Clones share the same flags.
#[derive(Clone, Default)]
pub struct SessionGuard {
    inner: Arc<GuardInner>,
}

impl SessionGuard {
    pub fn is_stopped(&self) -> bool {
        self.inner.stopped.load(Ordering::Acquire)
    }

    /// Ask the reader loop to stop. Returns true only for the first caller.
    pub fn stop(&self) -> bool {
        !self.inner.stopped.swap(true, Ordering::AcqRel)
    }

    /// True when both guards describe the very same connection generation.
    pub fn same(&self, other: &SessionGuard) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }

    /// Announce the close exactly once. A reader thread that wakes up after an
    /// explicit close, or a second close command, cannot emit a duplicate.
    pub fn finish(
        &self,
        sink: &dyn TerminalSink,
        id: &str,
        reason: CloseReason,
        message: Option<String>,
    ) -> bool {
        self.stop();
        if self.inner.announced.swap(true, Ordering::AcqRel) {
            return false;
        }
        sink.closed_with(id, ClosedPayload { reason, message });
        true
    }
}

struct GuardEntry {
    guard: SessionGuard,
    sink: Arc<dyn TerminalSink>,
}

/// Id reservations, independent of the per-transport session maps. A `std`
/// mutex is deliberate: reader threads (which are not async) must be able to
/// release their own reservation without a Tokio runtime handle.
#[derive(Default, Clone)]
pub struct GuardMap {
    inner: Arc<Mutex<HashMap<String, GuardEntry>>>,
}

impl GuardMap {
    /// Reserve `id`. Returns `None` when a connection is already live or in
    /// flight, which also closes the check/insert race between two opens.
    pub fn begin(&self, id: &str, sink: Arc<dyn TerminalSink>) -> Option<SessionGuard> {
        let mut map = self.inner.lock().ok()?;
        if map.contains_key(id) {
            return None;
        }
        let guard = SessionGuard::default();
        map.insert(
            id.to_string(),
            GuardEntry {
                guard: guard.clone(),
                sink,
            },
        );
        Some(guard)
    }

    pub fn contains(&self, id: &str) -> bool {
        self.inner
            .lock()
            .is_ok_and(|map| map.contains_key(id))
    }

    /// Non-destructive generation check, used by `open` just before it
    /// publishes a session: a close may have arrived while the socket/device
    /// was being opened.
    pub fn is_current(&self, id: &str, guard: &SessionGuard) -> bool {
        self.inner
            .lock()
            .is_ok_and(|map| map.get(id).is_some_and(|e| e.guard.same(guard)))
    }

    /// Release the reservation only when `guard` is still the current
    /// generation. A stale reader from a replaced connection gets `None` and
    /// must not touch the registry or emit any event.
    pub fn take_if(
        &self,
        id: &str,
        guard: &SessionGuard,
    ) -> Option<(Arc<dyn TerminalSink>, SessionGuard)> {
        let mut map = self.inner.lock().ok()?;
        if !map.get(id)?.guard.same(guard) {
            return None;
        }
        let entry = map.remove(id)?;
        Some((entry.sink, entry.guard))
    }

    /// Release whatever generation currently owns `id` (used by explicit
    /// close, which does not know the generation).
    pub fn take_any(&self, id: &str) -> Option<(Arc<dyn TerminalSink>, SessionGuard)> {
        let entry = self.inner.lock().ok()?.remove(id)?;
        Some((entry.sink, entry.guard))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Recorder {
        events: Mutex<Vec<ClosedPayload>>,
    }

    impl TerminalSink for Recorder {
        fn output(&self, _id: &str, _data: Vec<u8>) {}
        fn closed(&self, _id: &str) {}
        fn closed_with(&self, _id: &str, payload: ClosedPayload) {
            self.events.lock().unwrap().push(payload);
        }
    }

    #[test]
    fn a_connection_is_announced_exactly_once() {
        let sink = Arc::new(Recorder::default());
        let guard = SessionGuard::default();
        assert!(guard.finish(sink.as_ref(), "s", CloseReason::RemoteEof, None));
        assert!(!guard.finish(sink.as_ref(), "s", CloseReason::User, None));
        assert!(guard.is_stopped());
        let events = sink.events.lock().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].reason, CloseReason::RemoteEof);
    }

    #[test]
    fn finish_stops_the_reader_too() {
        let sink = Arc::new(Recorder::default());
        let guard = SessionGuard::default();
        assert!(!guard.is_stopped());
        guard.finish(sink.as_ref(), "s", CloseReason::User, None);
        assert!(guard.is_stopped());
    }

    #[test]
    fn reservations_reject_a_second_open() {
        let sink: Arc<dyn TerminalSink> = Arc::new(Recorder::default());
        let map = GuardMap::default();
        let first = map.begin("s", sink.clone());
        assert!(first.is_some());
        assert!(map.begin("s", sink.clone()).is_none());
        assert!(map.contains("s"));
    }

    #[test]
    fn a_stale_generation_cannot_release_a_new_one() {
        let sink: Arc<dyn TerminalSink> = Arc::new(Recorder::default());
        let map = GuardMap::default();
        let old = map.begin("s", sink.clone()).unwrap();
        map.take_any("s");
        let new = map.begin("s", sink).unwrap();
        assert!(!old.same(&new));
        assert!(map.take_if("s", &old).is_none());
        assert!(map.contains("s"), "stale reader must not free the new id");
        assert!(map.take_if("s", &new).is_some());
        assert!(!map.contains("s"));
    }

    #[test]
    fn payload_serialises_for_the_webview() {
        let payload = ClosedPayload::with_message(CloseReason::TransportError, "reset");
        let json = serde_json::to_value(&payload).unwrap();
        assert_eq!(json["reason"], "transport_error");
        assert_eq!(json["message"], "reset");
        assert!(serde_json::to_value(ClosedPayload::new(CloseReason::User))
            .unwrap()
            .get("message")
            .is_none());
    }
}
