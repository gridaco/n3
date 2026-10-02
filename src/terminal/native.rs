//! Native terminal composition. Process, PTY, and emulator dependencies stay here.
#[path = "backend.rs"]
mod backend;
#[path = "colors.rs"]
mod colors;
#[path = "input.rs"]
mod input;
#[path = "session.rs"]
mod session;
#[path = "view.rs"]
mod view;

pub(crate) use session::{SessionStatus, TerminalSession};
pub(crate) use view::TerminalView;

const PLACEHOLDER_NOTICE: &str = "Sample output · no shell process";

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
