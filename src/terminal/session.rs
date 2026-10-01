use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, mpsc};

use alacritty_terminal::event::{Event, WindowSize};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::{Config, Osc52, Term, TermMode};
use alacritty_terminal::tty::{Options, Shell};
use alacritty_terminal::vte::ansi::{Color, NamedColor, Processor, Rgb};

use super::backend::{EventProxy, NativePty};

const HISTORY_LINES: usize = 1_000;
const INITIAL_COLUMNS: usize = 80;
const INITIAL_ROWS: usize = 12;
const SAMPLE_OUTPUT: &[u8] = concat!(
    "\x1b[?25l",
    "N3 Terminal\r\n",
    "Deterministic sample output from the terminal emulator.\r\n",
    "ANSI colors: \x1b[31mred\x1b[0m  \x1b[32mgreen\x1b[0m  \x1b[34mblue\x1b[0m\r\n",
    "\x1b[38;2;156;106;222mTrue color\x1b[0m and \x1b[4munderlined text\x1b[0m.\r\n",
    "Output preview only; commands cannot be entered."
)
.as_bytes();

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SessionStatus {
    Placeholder,
    /// Deterministic interaction source; input bytes are recorded, never executed.
    Fixture,
    Dormant,
    Running,
    Exited {
        code: Option<i32>,
    },
    Failed(String),
}

/// Transient emulator state and, only after explicit native attachment, a PTY.
/// No authored document, scripting language, or edit history participates.
pub(crate) struct TerminalSession {
    pub(super) term: Arc<FairMutex<Term<EventProxy>>>,
    processor: Processor,
    sample_pending: bool,
    status: SessionStatus,
    events: mpsc::Receiver<Event>,
    runtime: Option<NativePty>,
    fixture_input: Vec<u8>,
    cell_size: (u16, u16),
    default_colors: Option<([u8; 3], [u8; 3])>,
    pending_exit_code: Option<i32>,
}

#[derive(Clone, Copy)]
struct Size {
    columns: usize,
    rows: usize,
}
impl Dimensions for Size {
    fn total_lines(&self) -> usize {
        self.rows
    }
    fn screen_lines(&self) -> usize {
        self.rows
    }
    fn columns(&self) -> usize {
        self.columns
    }
}

fn terminal_config() -> Config {
    Config {
        scrolling_history: HISTORY_LINES,
        osc52: Osc52::Disabled,
        // The input encoder implements classic VT/xterm. Do not negotiate
        // enhanced keyboard modes until their input protocol is implemented.
        kitty_keyboard: false,
        ..Config::default()
    }
}

/// Child-only defaults; querying inherited values separately keeps this policy
/// testable without mutating the process environment or loading shell rc files.
fn child_environment(
    inherited: impl Fn(&str) -> Option<std::ffi::OsString>,
    overrides: &[(&str, &str)],
) -> HashMap<String, String> {
    let mut variables = HashMap::from([
        ("TERM".into(), "xterm-256color".into()),
        ("COLORTERM".into(), "truecolor".into()),
        ("TERM_PROGRAM".into(), "N3".into()),
    ]);
    if inherited("PATH").is_none() {
        variables.insert(
            "PATH".into(),
            "/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin".into(),
        );
    }
    if !["LANG", "LC_ALL", "LC_CTYPE"]
        .iter()
        .any(|key| inherited(key).is_some_and(|value| !value.is_empty()))
    {
        let locale = if cfg!(target_os = "macos") {
            "en_US.UTF-8"
        } else {
            "C.UTF-8"
        };
        variables.insert("LANG".into(), locale.into());
    }
    for (key, value) in overrides {
        variables.insert((*key).into(), (*value).into());
    }
    variables
}

impl Default for TerminalSession {
    fn default() -> Self {
        Self::placeholder()
    }
}

impl TerminalSession {
    pub(crate) fn placeholder() -> Self {
        let mut session = Self::empty(INITIAL_COLUMNS, INITIAL_ROWS);
        session.sample_pending = true;
        session
    }

    pub(crate) fn dormant_native() -> Self {
        let mut session = Self::empty(INITIAL_COLUMNS, INITIAL_ROWS);
        session.status = SessionStatus::Dormant;
        session
    }

    pub(crate) fn interactive_fixture() -> Self {
        let mut session = Self::empty(INITIAL_COLUMNS, INITIAL_ROWS);
        session.status = SessionStatus::Fixture;
        session
    }

    pub(super) fn empty(columns: usize, rows: usize) -> Self {
        let size = Size {
            columns: columns.max(2),
            rows: rows.max(1),
        };
        let (proxy, events) = EventProxy::channel(None);
        Self {
            term: Arc::new(FairMutex::new(Term::new(terminal_config(), &size, proxy))),
            processor: Processor::new(),
            sample_pending: false,
            status: SessionStatus::Placeholder,
            events,
            runtime: None,
            fixture_input: Vec::new(),
            cell_size: (8, 16),
            default_colors: None,
            pending_exit_code: None,
        }
    }

    pub(crate) fn status(&self) -> &SessionStatus {
        &self.status
    }
    pub(crate) fn status_text(&self) -> String {
        match &self.status {
            SessionStatus::Placeholder => super::PLACEHOLDER_NOTICE.into(),
            SessionStatus::Fixture => "Interactive fixture · no shell process".into(),
            SessionStatus::Dormant => "Shell session has not started.".into(),
            SessionStatus::Running => "Shell session".into(),
            SessionStatus::Exited { code: Some(code) } => format!("Shell exited ({code})."),
            SessionStatus::Exited { code: None } => "Shell exited.".into(),
            SessionStatus::Failed(error) => error.clone(),
        }
    }

    pub(crate) fn is_interactive(&self) -> bool {
        matches!(self.status, SessionStatus::Running | SessionStatus::Fixture)
    }
    pub(super) fn mode(&self) -> TermMode {
        *self.term.lock().mode()
    }

    /// Start a login shell only on explicit native-host request. Environment
    /// overrides belong to the child; Alacritty's global setup_env is not used.
    pub(crate) fn start_shell(&mut self, ctx: &egui::Context) -> Result<(), String> {
        // A standalone shell starts in the user's valid home directory. If HOME
        // is unavailable, inherit the process directory; documents do not own it.
        let directory = std::env::var_os("HOME")
            .map(std::path::PathBuf::from)
            .filter(|path| path.is_dir());
        #[cfg(unix)]
        {
            let fallback = if cfg!(target_os = "macos") {
                "/bin/zsh"
            } else {
                "/bin/sh"
            };
            let program = std::env::var("SHELL")
                .ok()
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| fallback.into());
            self.start_program(ctx, &program, &["-l"], directory.as_deref(), &[])
        }
        #[cfg(not(unix))]
        {
            let program = std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".into());
            self.start_program(ctx, &program, &[], directory.as_deref(), &[])
        }
    }

    /// Controlled native launch for hosts and executable guides. Tests supply
    /// /bin/sh and explicit ENV/BASH_ENV/PS1; they never run user shell rc files.
    pub(crate) fn start_program(
        &mut self,
        ctx: &egui::Context,
        program: &str,
        args: &[&str],
        working_directory: Option<&Path>,
        env: &[(&str, &str)],
    ) -> Result<(), String> {
        if self.runtime.is_some() && matches!(self.status, SessionStatus::Running) {
            return Err("The terminal session is already running.".into());
        }
        if let Some(directory) = working_directory
            && !directory.is_dir()
        {
            let error = "The terminal working directory does not exist.".to_owned();
            self.status = SessionStatus::Failed(error.clone());
            return Err(error);
        }
        self.runtime = None;
        let size = Size {
            columns: self.columns(),
            rows: self.rows(),
        };
        let (proxy, events) = EventProxy::channel(Some(ctx.clone()));
        let term = Arc::new(FairMutex::new(Term::new(
            terminal_config(),
            &size,
            proxy.clone(),
        )));
        let variables = child_environment(|key| std::env::var_os(key), env);
        let options = Options {
            shell: Some(Shell::new(
                program.into(),
                args.iter().map(|arg| (*arg).into()).collect(),
            )),
            working_directory: working_directory.map(Path::to_owned),
            drain_on_exit: true,
            env: variables,
            #[cfg(target_os = "windows")]
            escape_args: true,
        };
        match NativePty::start(term.clone(), proxy, &options, self.window_size()) {
            Ok(runtime) => {
                self.term = term;
                self.events = events;
                self.processor = Processor::new();
                self.sample_pending = false;
                self.fixture_input.clear();
                self.pending_exit_code = None;
                self.runtime = Some(runtime);
                self.status = SessionStatus::Running;
                ctx.request_repaint();
                Ok(())
            }
            Err(error) => {
                let error = format!("Could not start terminal: {error}");
                self.status = SessionStatus::Failed(error.clone());
                Err(error)
            }
        }
    }

    /// Feed deterministic output. A live native session instead gets bytes from
    /// Alacritty's I/O thread, which owns its own streaming parser.
    pub(crate) fn ingest(&mut self, bytes: &[u8]) {
        self.flush_sample();
        self.processor.advance(&mut *self.term.lock(), bytes);
    }

    fn flush_sample(&mut self) {
        if std::mem::take(&mut self.sample_pending) {
            self.processor
                .advance(&mut *self.term.lock(), SAMPLE_OUTPUT);
        }
    }

    pub(crate) fn send_input(&mut self, bytes: Vec<u8>) -> bool {
        if bytes.is_empty() {
            return true;
        }
        match &self.status {
            SessionStatus::Fixture => {
                self.fixture_input.extend(bytes);
                true
            }
            SessionStatus::Running => {
                let sent = self
                    .runtime
                    .as_ref()
                    .is_some_and(|runtime| runtime.input(bytes));
                if !sent {
                    self.status =
                        SessionStatus::Failed("The terminal input channel closed.".into());
                }
                sent
            }
            _ => false,
        }
    }

    pub(crate) fn take_fixture_input(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.fixture_input)
    }

    /// Drain emulator replies even when the panel is hidden. Wakeups schedule an
    /// egui frame; this method contains all host-directed terminal effects.
    pub(crate) fn poll(&mut self, ctx: &egui::Context) {
        let mut drained = false;
        for _ in 0..1_024 {
            let Ok(event) = self.events.try_recv() else {
                drained = true;
                break;
            };
            match event {
                Event::PtyWrite(bytes) => {
                    self.send_input(bytes.into_bytes());
                }
                Event::ColorRequest(index, format) => {
                    let rgb = self.query_color(index, ctx);
                    self.send_input(format(rgb).into_bytes());
                }
                Event::TextAreaSizeRequest(format) => {
                    self.send_input(format(self.window_size()).into_bytes());
                }
                Event::ChildExit(status) => {
                    // Alacritty reports the child status before draining final
                    // PTY output. Publish Exited only at its subsequent Exit.
                    self.pending_exit_code = status.code();
                }
                Event::Exit if matches!(self.status, SessionStatus::Running) => {
                    self.status = SessionStatus::Exited {
                        code: self.pending_exit_code.take(),
                    };
                }
                // OSC 52 is disabled; do not execute title, clipboard, or browser effects.
                _ => {}
            }
        }
        if !drained {
            ctx.request_repaint();
        }
        if drained && self.runtime.as_ref().is_some_and(NativePty::is_finished) {
            self.runtime = None;
            if matches!(self.status, SessionStatus::Running) {
                self.status = SessionStatus::Failed("Terminal I/O closed unexpectedly.".into());
            }
        }
    }

    fn query_color(&self, index: usize, ctx: &egui::Context) -> Rgb {
        let term = self.term.lock();
        if let Some(rgb) = term.colors()[index] {
            return rgb;
        }
        if let Some((foreground, background)) = self.default_colors {
            let rgb = match index {
                value if value == NamedColor::Foreground as usize => Some(foreground),
                value if value == NamedColor::Background as usize => Some(background),
                _ => None,
            };
            if let Some([r, g, b]) = rgb {
                return Rgb { r, g, b };
            }
        }
        let palette =
            crate::theme::Palette::from_context(ctx, crate::settings::AccentColor::DEFAULT);
        let color = if index < 256 {
            Color::Indexed(index as u8)
        } else if index == NamedColor::Background as usize {
            Color::Named(NamedColor::Background)
        } else {
            Color::Named(NamedColor::Foreground)
        };
        let resolved = super::colors::resolve_color(
            color,
            palette,
            ctx.theme() == egui::Theme::Dark,
            term.colors(),
        );
        Rgb {
            r: resolved.r(),
            g: resolved.g(),
            b: resolved.b(),
        }
    }

    pub(crate) fn set_default_colors(&mut self, foreground: [u8; 3], background: [u8; 3]) {
        self.default_colors = Some((foreground, background));
    }

    pub(crate) fn set_cell_size(&mut self, width: u16, height: u16) {
        let next = (width.max(1), height.max(1));
        if self.cell_size != next {
            self.cell_size = next;
            if let Some(runtime) = &self.runtime {
                runtime.resize(self.window_size());
            }
        }
    }

    fn window_size(&self) -> WindowSize {
        WindowSize {
            num_cols: self.columns() as u16,
            num_lines: self.rows() as u16,
            cell_width: self.cell_size.0,
            cell_height: self.cell_size.1,
        }
    }

    pub(crate) fn resize(&mut self, columns: usize, rows: usize) {
        let size = Size {
            columns: columns.clamp(2, 1_024),
            rows: rows.clamp(1, 512),
        };
        let changed = {
            let mut term = self.term.lock();
            let changed =
                size.columns != term.grid().columns() || size.rows != term.grid().screen_lines();
            if changed {
                term.resize(size);
            }
            changed
        };
        if changed && let Some(runtime) = &self.runtime {
            runtime.resize(self.window_size());
        }
        self.flush_sample();
    }

    pub(crate) fn columns(&self) -> usize {
        self.term.lock().grid().columns()
    }
    pub(crate) fn rows(&self) -> usize {
        self.term.lock().grid().screen_lines()
    }
    pub(crate) fn history_size(&self) -> usize {
        self.term.lock().grid().history_size()
    }
    pub(crate) fn display_offset(&self) -> usize {
        self.term.lock().grid().display_offset()
    }
    pub(super) fn scroll_lines(&mut self, lines: i32) {
        self.term.lock().scroll_display(Scroll::Delta(lines));
    }
    pub(super) fn scroll_to(&mut self, offset: usize) {
        let mut term = self.term.lock();
        let current = term.grid().display_offset() as i32;
        let next = offset.min(term.grid().history_size()) as i32;
        term.scroll_display(Scroll::Delta(next - current));
    }

    pub(crate) fn visible_text(&self) -> String {
        use alacritty_terminal::term::cell::Flags;
        let term = self.term.lock();
        let mut result = String::new();
        let mut line = None;
        for indexed in term.grid().display_iter() {
            if line != Some(indexed.point.line) {
                if line.is_some() {
                    result.push('\n');
                }
                line = Some(indexed.point.line);
            }
            if !indexed
                .cell
                .flags
                .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
            {
                result.push(indexed.cell.c);
                if let Some(combining) = indexed.cell.zerowidth() {
                    result.extend(combining);
                }
            }
        }
        result
            .lines()
            .map(str::trim_end)
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[cfg(test)]
#[path = "runtime_tests.rs"]
mod runtime_tests;
