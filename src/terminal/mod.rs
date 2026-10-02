//! Terminal emulator, native PTY session, and egui view; independent of documents.
//!
//! The session/view split follows Zed's architectural guidance, without copying
//! its implementation: https://github.com/zed-industries/zed/tree/main/crates/terminal_view
//! Alacritty-specific types stay inside this feature. Only the native host opts
//! into shell creation; workbench and test consumers use deterministic sessions.
#[cfg_attr(target_arch = "wasm32", path = "browser.rs")]
#[cfg_attr(not(target_arch = "wasm32"), path = "native.rs")]
mod host;

#[cfg(not(target_arch = "wasm32"))]
pub(crate) use host::SessionStatus;
pub(crate) use host::{TerminalSession, TerminalView};

// Exercise the unavailable-host contract in the ordinary native test suite.
#[cfg(all(test, not(target_arch = "wasm32")))]
mod browser;
