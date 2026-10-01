//! Terminal emulator, native PTY session, and egui view; independent of documents.
//!
//! The session/view split follows Zed's architectural guidance, without copying
//! its implementation: https://github.com/zed-industries/zed/tree/main/crates/terminal_view
//! Alacritty-specific types stay inside this feature. Only the native host opts
//! into shell creation; workbench and test consumers use deterministic sessions.
mod backend;
mod colors;
mod input;
mod session;
mod view;

pub(crate) use session::{SessionStatus, TerminalSession};
pub(crate) use view::TerminalView;

pub(crate) const PLACEHOLDER_NOTICE: &str = "Sample output · no shell process";

#[cfg(test)]
mod tests;
