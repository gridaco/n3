//! Synthetic byte sources and observations for the production terminal view.
//! No shell, Editor, or document participates in this component fixture.
use super::{Case, Observed, instance_id};
use crate::terminal::{TerminalSession, TerminalView};
use egui::Ui;
use std::collections::VecDeque;

#[derive(Clone, Debug, PartialEq)]
pub(super) enum Event {
    InputSent {
        instance: usize,
        bytes: Vec<u8>,
    },
    ApplicationScreen {
        instance: usize,
    },
    SampleAppended {
        instance: usize,
        batch: usize,
    },
    FocusChanged {
        instance: usize,
        focused: bool,
    },
    Resized {
        instance: usize,
        columns: usize,
        rows: usize,
    },
    Scrolled {
        instance: usize,
        offset: usize,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Snapshot {
    focused: bool,
    columns: usize,
    rows: usize,
    offset: usize,
}

pub(super) struct Fixture {
    pub sessions: [TerminalSession; 2],
    pub views: [TerminalView; 2],
    pub batches: [usize; 2],
    pub input_bytes: [Vec<u8>; 2],
    interactive: bool,
    application_frame: Option<u64>,
    pub events: VecDeque<Event>,
    previous: [Option<Snapshot>; 2],
    appended_frame: [Option<u64>; 2],
}

impl Fixture {
    pub fn new(case: Case) -> Self {
        Self {
            sessions: std::array::from_fn(|_| {
                if case == Case::TerminalInput {
                    let mut session = TerminalSession::interactive_fixture();
                    session.ingest(b"N3 interactive input fixture\r\nTyped bytes are recorded in the inspector; no shell process runs.\r\n> ");
                    session
                } else {
                    TerminalSession::placeholder()
                }
            }),
            views: std::array::from_fn(|index| {
                TerminalView::new(instance_id(case, index).with("terminal"))
            }),
            batches: [0; 2],
            input_bytes: std::array::from_fn(|_| Vec::new()),
            interactive: case == Case::TerminalInput,
            application_frame: None,
            events: VecDeque::new(),
            previous: [None; 2],
            appended_frame: [None; 2],
        }
    }

    fn record(&mut self, event: Event) {
        self.events.push_back(event);
        if self.events.len() > 32 {
            self.events.pop_front();
        }
    }

    pub fn controls(&mut self, ui: &mut Ui, count: usize) -> Vec<Observed> {
        let mut observed = Vec::new();
        ui.horizontal_wrapped(|ui| {
            for instance in 0..count {
                let response = ui.button(if count == 1 {
                    "Append sample output".to_owned()
                } else {
                    format!("Append output to {}", instance + 1)
                });
                observed.push(Observed {
                    instance,
                    target: "terminal-append".into(),
                    id: response.id,
                    rect: response.rect,
                    enabled: response.enabled(),
                });
                let frame = ui.ctx().cumulative_frame_nr();
                if response.clicked() && self.appended_frame[instance] != Some(frame) {
                    self.appended_frame[instance] = Some(frame);
                    self.batches[instance] += 1;
                    let batch = self.batches[instance];
                    for line in 1..=50 {
                        self.sessions[instance].ingest(
                            format!("\r\nSample {batch}.{line:02}: deterministic terminal output")
                                .as_bytes(),
                        );
                    }
                    self.sessions[instance].ingest(b"\r\n\x1b[32mSample complete\x1b[0m\r\n");
                    self.record(Event::SampleAppended { instance, batch });
                }
            }
            if self.interactive {
                let response = ui.button("Show TUI sample");
                observed.push(Observed {
                    instance: 0,
                    target: "terminal-tui".into(),
                    id: response.id,
                    rect: response.rect,
                    enabled: true,
                });
                let frame = ui.ctx().cumulative_frame_nr();
                if response.clicked() && self.application_frame != Some(frame) {
                    self.application_frame = Some(frame);
                    // Deterministic bytes exercise the emulator's alternate
                    // screen, application cursors, and bracketed paste modes.
                    self.sessions[0].ingest(
                        concat!(
                            "\x1b[?1049h\x1b[2J\x1b[H\x1b[?1h\x1b[?2004h",
                            "ANSI application screen\r\n\r\n",
                            "\x1b[7m Name                  State    \x1b[0m\r\n",
                            " \x1b[32mSample project\x1b[0m        Ready\r\n",
                            " \x1b[34mScene inspection\x1b[0m      Ready\r\n\r\n",
                            "Arrows, Tab, Escape and control keys reach input.\r\n",
                            "The fixture records bytes; no process runs."
                        )
                        .as_bytes(),
                    );
                    self.record(Event::ApplicationScreen { instance: 0 });
                }
                ui.small("Input is recorded as bytes; no shell process runs.");
            } else {
                ui.small("Fixture output only; typing cannot execute commands.");
            }
        });
        observed
    }

    pub fn show(&mut self, ui: &mut Ui, instance: usize) -> Vec<Observed> {
        let output = self.views[instance].show(ui, &mut self.sessions[instance]);
        let bytes = self.sessions[instance].take_fixture_input();
        if !bytes.is_empty() {
            self.input_bytes[instance].extend_from_slice(&bytes);
            self.record(Event::InputSent { instance, bytes });
        }
        let current = Snapshot {
            focused: output.focused,
            columns: output.columns,
            rows: output.rows,
            offset: output.display_offset,
        };
        if let Some(previous) = self.previous[instance] {
            if current.focused != previous.focused {
                self.record(Event::FocusChanged {
                    instance,
                    focused: current.focused,
                });
            }
            if (current.columns, current.rows) != (previous.columns, previous.rows) {
                self.record(Event::Resized {
                    instance,
                    columns: current.columns,
                    rows: current.rows,
                });
            }
            if current.offset != previous.offset {
                self.record(Event::Scrolled {
                    instance,
                    offset: current.offset,
                });
            }
        }
        self.previous[instance] = Some(current);
        let id = self.views[instance].focus_id();
        [
            ("terminal-view", output.rect, id.with("view")),
            ("terminal-content", output.content_rect, id),
            ("terminal-notice", output.notice_rect, id.with("notice")),
        ]
        .into_iter()
        .filter(|(_, rect, _)| rect.is_positive())
        .map(|(target, rect, id)| Observed {
            instance,
            target: target.into(),
            id,
            rect,
            enabled: true,
        })
        .collect()
    }

    pub fn inspector(&self, ui: &mut Ui, count: usize) {
        ui.label("Terminal state");
        for (instance, session) in self.sessions.iter().enumerate().take(count) {
            ui.small(format!(
                "{}: {} × {} cells · {} history lines · offset {}",
                instance + 1,
                session.columns(),
                session.rows(),
                session.history_size(),
                session.display_offset(),
            ));
        }
        ui.separator();
        ui.label("Terminal events (last 32)");
        for event in self.events.iter().rev().take(8) {
            ui.small(format!("{event:?}"));
        }
    }
}
