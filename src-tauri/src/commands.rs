//! Tauri command surface, split by domain. The re-exports keep the
//! `commands::name` paths that `lib.rs` and the agent service rely on.

mod backup;
mod keys;
mod monitor;
mod plugin;
mod serial;
mod session;
mod store;
mod sync;
mod terminal;
mod transfer;
mod vault;

pub use backup::*;
pub use keys::*;
pub use monitor::*;
pub use plugin::*;
pub use serial::*;
pub use session::*;
pub use store::*;
pub use sync::*;
pub use terminal::*;
pub use transfer::*;
pub use vault::*;
