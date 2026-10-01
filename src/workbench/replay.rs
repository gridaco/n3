//! Component replay shares guide input cues, capture, framing, and lossless encoding.
use super::*;
use crate::{
    controls::{self, Trace},
    doc_animation::{AnimationSettings, WebpAnimation},
    doc_capture::UiCapture,
    doc_input::VirtualInput,
    documentation::annotations::{self, Annotation},
};
use egui::{Event, Key, Modifiers, PointerButton, Pos2};
use std::{path::Path, time::Duration};

const WIDTH: u32 = 1280;
const HEIGHT: u32 = 800;
const STEP: Duration = Duration::from_millis(16);

/// No Editor, camera, native dialogs or settings store. Every event reaches the
/// production component and input adapter before paint-only guide cues.
pub(super) struct Replay {
    pub ctx: Context,
    pub bench: Workbench,
    time: Duration,
    input: VirtualInput,
    focused: bool,
    pub retry: bool,
    pub trace: Trace,
    annotation: Option<Annotation>,
    traced: bool,
}
impl Replay {
    pub fn new(case: Case, appearance: ResolvedTheme) -> Self {
        let ctx = Context::default();
        crate::workspace_ui::configure_context(&ctx);
        let mut bench = Workbench::default();
        bench.select_case(&ctx, case);
        bench.appearance = appearance;
        if case.count() == 1 {
            controls::enable(&ctx);
        }
        Self {
            ctx,
            bench,
            time: Duration::ZERO,
            input: VirtualInput::default(),
            focused: true,
            retry: false,
            trace: Trace::default(),
            annotation: None,
            traced: case.count() == 1,
        }
    }
    pub fn frame(
        &mut self,
        events: Vec<Event>,
        elapsed: Duration,
    ) -> Result<egui::FullOutput, String> {
        let traced = self.bench.case.count() == 1;
        if traced != self.traced {
            if traced {
                controls::enable(&self.ctx);
            } else {
                controls::disable(&self.ctx);
            }
            self.traced = traced;
            self.trace = Trace::default();
        }
        self.time += elapsed;
        for event in &events {
            if let Event::WindowFocused(focused) = event {
                self.focused = *focused;
            }
        }
        self.input.advance(elapsed);
        self.input
            .observe(&events, &[], Modifiers::NONE, self.focused);
        let context = self.ctx.clone();
        let mut error = None;
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(
                    Pos2::ZERO,
                    egui::vec2(WIDTH as f32, HEIGHT as f32),
                )),
                time: Some(self.time.as_secs_f64()),
                predicted_dt: STEP.as_secs_f32(),
                focused: self.focused,
                events,
                ..Default::default()
            },
            |ui| {
                controls::begin_pass(ui.ctx());
                self.bench.ui(ui);
                if self.bench.case.count() == 1 {
                    self.trace = controls::snapshot(ui.ctx());
                    if let Err(problem) = self.trace.validate() {
                        error = Some(problem);
                    }
                }
                if let Some(note) = &self.annotation
                    && let Err(problem) = annotations::paint(
                        ui.ctx(),
                        ui.ctx().content_rect(),
                        &self.trace,
                        std::slice::from_ref(note),
                    )
                {
                    error = Some(problem);
                }
                self.input.paint(
                    ui.ctx(),
                    ui.ctx().content_rect(),
                    ui.ctx().output(|o| o.cursor_icon),
                );
                if self.retry && ui.ctx().current_pass_index() == 0 {
                    ui.ctx()
                        .request_discard("Exercise component delivery across repeated layout");
                }
            },
        );
        self.retry = false;
        if let Some(problem) = error {
            output.textures_delta.clear();
            return Err(problem);
        }
        Ok(output)
    }
    pub fn target(&self, instance: usize, target: &str) -> Result<&Observed, String> {
        self.bench
            .observed
            .iter()
            .find(|o| o.instance == instance && o.target == target)
            .ok_or_else(|| {
                format!(
                    "Case {} has no live instance {instance} / {target}",
                    self.bench.case.id()
                )
            })
    }
    pub fn key(binding: &str, pressed: bool) -> Event {
        let binding = bindings::required(binding);
        Event::Key {
            key: binding
                .key()
                .expect("A held-key case needs a physical key binding"),
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: binding.modifiers,
        }
    }
    pub fn literal(key: Key, pressed: bool) -> Event {
        Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: Modifiers::NONE,
        }
    }
    pub fn button(point: Pos2, button: PointerButton, pressed: bool) -> Event {
        Event::PointerButton {
            pos: point,
            button,
            pressed,
            modifiers: Modifiers::NONE,
        }
    }
}

fn render(
    s: &mut Replay,
    capture: &mut UiCapture,
    events: Vec<Event>,
    elapsed: Duration,
) -> Result<(), String> {
    let output = s.frame(events, elapsed)?;
    capture.render(&s.ctx, output)
}
fn settle(s: &mut Replay, capture: &mut UiCapture) -> Result<(), String> {
    for _ in 0..4 {
        render(s, capture, Vec::new(), STEP)?;
    }
    Ok(())
}
fn click(
    s: &mut Replay,
    capture: &mut UiCapture,
    instance: usize,
    target: &str,
    button: PointerButton,
) -> Result<(), String> {
    let point = s.target(instance, target)?.rect.center();
    render(s, capture, vec![Event::PointerMoved(point)], STEP)?;
    render(s, capture, vec![Replay::button(point, button, true)], STEP)?;
    render(s, capture, vec![Replay::button(point, button, false)], STEP)?;
    settle(s, capture)
}
fn tap(s: &mut Replay, capture: &mut UiCapture, key: Key) -> Result<(), String> {
    render(s, capture, vec![Replay::literal(key, true)], STEP)?;
    render(s, capture, vec![Replay::literal(key, false)], STEP)?;
    settle(s, capture)
}
fn prepare(
    case: Case,
    appearance: ResolvedTheme,
    capture: &mut UiCapture,
) -> Result<Replay, String> {
    let mut s = Replay::new(case, appearance);
    settle(&mut s, capture)?;
    match case {
        Case::Dropdown => {
            click(&mut s, capture, 0, "menu-trigger", PointerButton::Primary)?;
            tap(&mut s, capture, Key::ArrowRight)?;
            if !s.target(0, ActionId::Open.id())?.enabled
                || s.target(0, ActionId::Save.id())?.enabled
            {
                return Err("File fixture availability was not presented.".into());
            }
            s.annotation = Some(Annotation {
                target: Control::Save,
                caption: "Disabled by this fixture; shortcuts still come from the binding catalog."
                    .into(),
            });
        }
        Case::Context => {
            s.bench.long_label = true;
            click(
                &mut s,
                capture,
                0,
                "context-surface",
                PointerButton::Secondary,
            )?;
            let point = s.target(0, ActionId::SelectAll.id())?.rect.center();
            render(&mut s, capture, vec![Event::PointerMoved(point)], STEP)?;
        }
        Case::MenuIsolation => {
            click(&mut s, capture, 0, "menu-trigger", PointerButton::Primary)?;
            tap(&mut s, capture, Key::ArrowDown)?;
            tap(&mut s, capture, Key::ArrowRight)?;
            click(
                &mut s,
                capture,
                0,
                ActionId::Xray.id(),
                PointerButton::Primary,
            )?;
            click(&mut s, capture, 1, "menu-trigger", PointerButton::Primary)?;
            tap(&mut s, capture, Key::ArrowDown)?;
            tap(&mut s, capture, Key::ArrowRight)?;
            if !s.bench.instances[0].checked || s.bench.instances[1].checked {
                return Err("Independent menu fixture state leaked.".into());
            }
        }
        Case::ViewPie | Case::ShadingPie | Case::PieIsolation => {
            let index = usize::from(case == Case::PieIsolation);
            let bounds = s.bench.instances[index].bounds;
            let anchor = if case == Case::ViewPie {
                bounds.left_top() + Vec2::splat(8.0)
            } else {
                bounds.center()
            };
            let binding = if case == Case::ShadingPie {
                "shading.pie"
            } else {
                "view.pie"
            };
            render(
                &mut s,
                capture,
                vec![Event::PointerMoved(anchor), Replay::key(binding, true)],
                STEP,
            )?;
            if !s.bench.instances[index].pie.active() {
                return Err(format!("{} did not open its held pie", case.id()));
            }
            let point = if case == Case::ShadingPie {
                let pie = crate::ui::shading_pie::Pie::open(&s.ctx, anchor, bounds)
                    .ok_or("Cannot plan shading fixture")?;
                pie.item_rect(crate::ui::shading_pie::Action::MaterialPreview)
                    .center()
            } else {
                let pie = crate::ui::view_pie::Pie::open(&s.ctx, anchor, bounds)
                    .ok_or("Cannot plan view fixture")?;
                pie.item_rect(crate::ui::view_pie::Action::Selection)
                    .center()
            };
            render(&mut s, capture, vec![Event::PointerMoved(point)], STEP)?;
            render(&mut s, capture, Vec::new(), Duration::from_millis(1000))?;
        }
        Case::TimelineSmall | Case::TimelineHierarchy => {
            let key = s
                .bench
                .observed
                .iter()
                .find(|o| o.target.starts_with("timeline-key-"))
                .ok_or("No visible timeline key")?
                .target
                .clone();
            click(&mut s, capture, 0, &key, PointerButton::Primary)?;
            if s.bench.timeline.as_ref().unwrap().timelines[0]
                .selection()
                .is_none()
            {
                return Err("Timeline key inspection did not select a key".into());
            }
        }
        Case::TimelineMarquee => {
            let (start, end) = marquee_points(&s, 0, "timeline-key-1-1", "timeline-key-3-2")?;
            render(
                &mut s,
                capture,
                vec![
                    Event::PointerMoved(start),
                    Replay::button(start, PointerButton::Primary, true),
                ],
                STEP,
            )?;
            render(&mut s, capture, vec![Event::PointerMoved(end)], STEP)?;
            let fixture = s.bench.timeline.as_ref().unwrap();
            if !fixture.timelines[0].is_marquee_active()
                || fixture.timelines[0].selection().is_some()
                || !fixture.events.is_empty()
            {
                return Err(
                    "Held marquee must show its bounds without selecting or seeking".into(),
                );
            }
        }
        Case::TimelineDense => {
            s.bench.dimensions = egui::vec2(450.0, 350.0);
            settle(&mut s, capture)?;
            let canvas = s.target(0, "timeline-canvas")?.rect;
            render(
                &mut s,
                capture,
                vec![
                    Event::PointerMoved(canvas.center()),
                    wheel(egui::vec2(0.0, -150.0), Modifiers::NONE),
                ],
                STEP,
            )?;
            if s.bench.timeline.as_ref().unwrap().metrics[0].rows >= 1000 {
                return Err("Dense timeline rows were not virtualized".into());
            }
        }
        Case::TimelineIsolation => {
            let ruler = s.target(1, "timeline-ruler")?.rect;
            let point = ruler.center();
            render(
                &mut s,
                capture,
                vec![
                    Event::PointerMoved(point),
                    Replay::button(point, PointerButton::Primary, true),
                ],
                STEP,
            )?;
            render(
                &mut s,
                capture,
                vec![Replay::button(point, PointerButton::Primary, false)],
                STEP,
            )?;
            let fixture = s.bench.timeline.as_ref().unwrap();
            if fixture.hosts[0].accepted_time == fixture.hosts[1].accepted_time {
                return Err("Timeline instance seek isolation failed".into());
            }
        }
        Case::TimelineReject => {
            let point = s.target(0, "timeline-ruler")?.rect.center();
            render(
                &mut s,
                capture,
                vec![
                    Event::PointerMoved(point),
                    Replay::button(point, PointerButton::Primary, true),
                ],
                STEP,
            )?;
            render(
                &mut s,
                capture,
                vec![Replay::button(point, PointerButton::Primary, false)],
                STEP,
            )?;
            let fixture = s.bench.timeline.as_ref().unwrap();
            if fixture.hosts[0].accepted_time != 0.0
                || fixture.events.iter().any(|event| event.accepted)
            {
                return Err("Rejected seek moved accepted playback".into());
            }
        }
        Case::TerminalInput => {
            click(&mut s, capture, 0, "terminal-tui", PointerButton::Primary)?;
            click(
                &mut s,
                capture,
                0,
                "terminal-content",
                PointerButton::Primary,
            )?;
            tap(&mut s, capture, Key::ArrowUp)?;
            tap(&mut s, capture, Key::Tab)?;
            tap(&mut s, capture, Key::Escape)?;
            render(
                &mut s,
                capture,
                vec![
                    Event::Text("hello ✓".into()),
                    Event::Paste("one\ntwo".into()),
                ],
                STEP,
            )?;
            let fixture = s.bench.terminal.as_ref().unwrap();
            if fixture.input_bytes[0] != "\x1bOA\t\x1bhello ✓\x1b[200~one\ntwo\x1b[201~".as_bytes()
                || !fixture.views[0].is_focused(&s.ctx)
            {
                return Err("Interactive terminal must encode keys, UTF-8 and bracketed paste once while retaining focus".into());
            }
        }
        Case::TerminalPlaceholder => {
            click(
                &mut s,
                capture,
                0,
                "terminal-content",
                PointerButton::Primary,
            )?;
            if !s.bench.terminal.as_ref().unwrap().views[0].is_focused(&s.ctx) {
                return Err("Terminal content must own focus after clicking it".into());
            }
            let original = s.bench.terminal.as_ref().unwrap().sessions[0].visible_text();
            render(
                &mut s,
                capture,
                vec![
                    Event::Text("cannot execute".into()),
                    Replay::literal(Key::Enter, true),
                ],
                STEP,
            )?;
            render(
                &mut s,
                capture,
                vec![Replay::literal(Key::Enter, false)],
                STEP,
            )?;
            if s.bench.terminal.as_ref().unwrap().sessions[0].visible_text() != original {
                return Err("Terminal placeholder accepted executable input".into());
            }
        }
        Case::TerminalNarrow => {
            let original = s.bench.terminal.as_ref().unwrap().sessions[0].columns();
            drag_dimension(&mut s, capture, "available-width", -400.0)?;
            drag_dimension(&mut s, capture, "available-height", -180.0)?;
            if s.bench.terminal.as_ref().unwrap().sessions[0].columns() >= original {
                return Err("Terminal did not resize with the available width".into());
            }
            click(
                &mut s,
                capture,
                0,
                "terminal-append",
                PointerButton::Primary,
            )?;
            let point = s.target(0, "terminal-content")?.rect.center();
            render(
                &mut s,
                capture,
                vec![
                    Event::PointerMoved(point),
                    wheel(egui::vec2(0.0, 90.0), Modifiers::NONE),
                ],
                STEP,
            )?;
            if s.bench.terminal.as_ref().unwrap().sessions[0].display_offset() == 0 {
                return Err("Terminal wheel navigation did not reveal scrollback".into());
            }
        }
        Case::TerminalIsolation => {
            let original = s.bench.terminal.as_ref().unwrap().sessions[0].visible_text();
            click(
                &mut s,
                capture,
                1,
                "terminal-append",
                PointerButton::Primary,
            )?;
            click(
                &mut s,
                capture,
                1,
                "terminal-content",
                PointerButton::Primary,
            )?;
            let fixture = s.bench.terminal.as_ref().unwrap();
            if fixture.sessions[0].visible_text() != original || fixture.batches != [0, 1] {
                return Err("Terminal byte ingestion leaked between instances".into());
            }
        }
        Case::RulerRanges | Case::RulerDensity | Case::TimelineEmpty | Case::TimelineZero => {}
    }
    settle(&mut s, capture)?;
    Ok(s)
}

fn drag_dimension(
    s: &mut Replay,
    capture: &mut UiCapture,
    target: &str,
    delta: f32,
) -> Result<(), String> {
    let start = s.target(0, target)?.rect.center();
    let end = start + egui::vec2(delta, 0.0);
    render(s, capture, vec![Event::PointerMoved(start)], STEP)?;
    render(
        s,
        capture,
        vec![Replay::button(start, PointerButton::Primary, true)],
        STEP,
    )?;
    render(s, capture, vec![Event::PointerMoved(end)], STEP)?;
    render(
        s,
        capture,
        vec![Replay::button(end, PointerButton::Primary, false)],
        STEP,
    )?;
    settle(s, capture)
}

pub(super) fn wheel(delta: Vec2, modifiers: Modifiers) -> Event {
    Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta,
        modifiers,
        phase: egui::TouchPhase::Move,
    }
}

/// Points derive from live marker bounds: the origin is empty key space, and
/// the endpoint includes keys in another row without depending on a screen size.
pub(super) fn marquee_points(
    replay: &Replay,
    instance: usize,
    first: &str,
    last: &str,
) -> Result<(Pos2, Pos2), String> {
    let first = replay.target(instance, first)?.rect;
    let last = replay.target(instance, last)?.rect;
    Ok((
        first.left_top() - egui::vec2(2.0, 2.0),
        last.right_bottom() + egui::vec2(2.0, 2.0),
    ))
}

/// Internal artifacts are disposable review evidence, never user-guide baselines.
/// Replay every still twice and require exact encoded bytes before writing it.
pub(super) fn evidence() -> Result<(), String> {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join(".cache/workbench");
    std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    let mut capture = pollster::block_on(UiCapture::new(WIDTH, HEIGHT))?;
    let mut records = Vec::new();
    for appearance in [ResolvedTheme::Light, ResolvedTheme::Dark] {
        for case in Case::ALL {
            let s = prepare(case, appearance, &mut capture)?;
            let bytes = capture.framed_webp()?;
            let events: Vec<_> = s
                .bench
                .events
                .iter()
                .map(|e| format!("{}: {:?}", e.instance + 1, e.command))
                .collect();
            let timeline_events: Vec<_> = s
                .bench
                .timeline
                .as_ref()
                .map(|f| {
                    f.events
                        .iter()
                        .map(|e| {
                            format!(
                                "{}: {:?} accepted={}",
                                e.instance + 1,
                                e.request,
                                e.accepted
                            )
                        })
                        .collect()
                })
                .unwrap_or_default();
            let terminal_events: Vec<_> = s
                .bench
                .terminal
                .as_ref()
                .map(|fixture| {
                    fixture
                        .events
                        .iter()
                        .map(|event| format!("{event:?}"))
                        .collect()
                })
                .unwrap_or_default();
            let _repeat = prepare(case, appearance, &mut capture)?;
            if bytes != capture.framed_webp()? {
                return Err(format!(
                    "{} / {appearance:?} UI replay is not deterministic",
                    case.id()
                ));
            }
            let name = format!(
                "{}-{}.webp",
                case.id(),
                if appearance == ResolvedTheme::Light {
                    "light"
                } else {
                    "dark"
                }
            );
            std::fs::write(directory.join(&name), &bytes).map_err(|e| e.to_string())?;
            records.push(serde_json::json!({
                "file": name, "case": case.id(), "page": case.page(), "name": case.name(),
                "appearance": format!("{appearance:?}"), "bytes": bytes.len(),
                "exact_repeat": true, "delivered": events, "timeline_requests":timeline_events,
                "terminal_events": terminal_events,
            }));
        }
    }
    let mut s = Replay::new(Case::ViewPie, ResolvedTheme::Light);
    settle(&mut s, &mut capture)?;
    let (width, height) = capture.framed_dimensions();
    let mut clip = WebpAnimation::new(width, height, AnimationSettings::default())?;
    clip.push(&capture.framed_frame()?, Duration::from_millis(400))?;
    let bounds = s.bench.instances[0].bounds;
    let anchor = bounds.center();
    render(
        &mut s,
        &mut capture,
        vec![Event::PointerMoved(anchor), Replay::key("view.pie", true)],
        STEP,
    )?;
    clip.push(&capture.framed_frame()?, Duration::from_millis(400))?;
    let pie = crate::ui::view_pie::Pie::open(&s.ctx, anchor, bounds)
        .ok_or("Cannot plan pie animation")?;
    let target = pie.item_rect(crate::ui::view_pie::Action::Top).center();
    render(
        &mut s,
        &mut capture,
        vec![Event::PointerMoved(target)],
        Duration::from_millis(400),
    )?;
    clip.push(&capture.framed_frame()?, Duration::from_millis(600))?;
    render(
        &mut s,
        &mut capture,
        vec![Replay::key("view.pie", false)],
        STEP,
    )?;
    settle(&mut s, &mut capture)?;
    if s.bench.events.len() != 1 || s.bench.instances[0].pie.active() {
        return Err("Held pie animation must deliver exactly one command on release.".into());
    }
    clip.push(&capture.framed_frame()?, Duration::from_millis(600))?;
    std::fs::write(directory.join("pies-hold-release.webp"), clip.finish()?)
        .map_err(|e| e.to_string())?;
    timeline_animation(&directory, &mut capture)?;
    timeline_marquee_animation(&directory, &mut capture)?;
    let measurement = super::timeline_measure::measure()?;
    println!("Timeline navigation measurement: {measurement}");
    let manifest = serde_json::json!({
        "purpose": "internal UI component development evidence",
        "renderer_profile": capture.renderer_profile, "adapter": capture.adapter,
        "frame_template": capture.frame_template_name(), "dimensions": [width, height],
        "stills": records, "animations": ["pies-hold-release.webp","timeline-scrub-navigation.webp", "timeline-marquee.webp"],
        "timeline_navigation": measurement,
    });
    std::fs::write(
        directory.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    println!("Component evidence: {}", directory.display());
    Ok(())
}

fn timeline_marquee_animation(directory: &Path, capture: &mut UiCapture) -> Result<(), String> {
    let mut s = Replay::new(Case::TimelineMarquee, ResolvedTheme::Light);
    settle(&mut s, capture)?;
    let (width, height) = capture.framed_dimensions();
    let mut clip = WebpAnimation::new(width, height, AnimationSettings::default())?;
    clip.push(&capture.framed_frame()?, Duration::from_millis(450))?;
    let (start, end) = marquee_points(&s, 0, "timeline-key-1-1", "timeline-key-3-2")?;
    render(
        &mut s,
        capture,
        vec![
            Event::PointerMoved(start),
            Replay::button(start, PointerButton::Primary, true),
        ],
        STEP,
    )?;
    for step in 1..=10 {
        render(
            &mut s,
            capture,
            vec![Event::PointerMoved(start.lerp(end, step as f32 / 10.0))],
            Duration::from_millis(70),
        )?;
        let fixture = s.bench.timeline.as_ref().unwrap();
        if fixture.timelines[0].selection().is_some() || !fixture.events.is_empty() {
            return Err("Marquee preview must not select keys or evaluate time".into());
        }
        clip.push(&capture.framed_frame()?, Duration::from_millis(70))?;
    }
    if !s.bench.timeline.as_ref().unwrap().timelines[0].is_marquee_active() {
        return Err("Marquee animation never crossed the drag threshold".into());
    }
    clip.push(&capture.framed_frame()?, Duration::from_millis(450))?;
    render(
        &mut s,
        capture,
        vec![Replay::button(end, PointerButton::Primary, false)],
        STEP,
    )?;
    settle(&mut s, capture)?;
    let fixture = s.bench.timeline.as_ref().unwrap();
    let selection = fixture.timelines[0]
        .selection()
        .ok_or("Marquee release did not select any keys")?;
    if selection.keys.len() != 6
        || !selection
            .keys
            .iter()
            .all(|key| (1..=3).contains(&key.track.0) && (1..=2).contains(&key.key.0))
        || fixture.timelines[0].has_pointer_gesture()
        || !fixture.events.is_empty()
    {
        return Err("Marquee must select six track-qualified keys without seeking".into());
    }
    clip.push(&capture.framed_frame()?, Duration::from_millis(1400))?;
    std::fs::write(directory.join("timeline-marquee.webp"), clip.finish()?)
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn timeline_animation(directory: &Path, capture: &mut UiCapture) -> Result<(), String> {
    let mut s = Replay::new(Case::TimelineSmall, ResolvedTheme::Light);
    s.bench.dimensions.y = 320.0;
    settle(&mut s, capture)?;
    let (width, height) = capture.framed_dimensions();
    let mut clip = WebpAnimation::new(width, height, AnimationSettings::default())?;
    clip.push(&capture.framed_frame()?, Duration::from_millis(400))?;
    click(
        &mut s,
        capture,
        0,
        "timeline-playpause",
        PointerButton::Primary,
    )?;
    for _ in 0..5 {
        render(&mut s, capture, Vec::new(), Duration::from_millis(100))?;
        clip.push(&capture.framed_frame()?, Duration::from_millis(100))?;
    }
    click(
        &mut s,
        capture,
        0,
        "timeline-playpause",
        PointerButton::Primary,
    )?;
    let ruler = s.target(0, "timeline-ruler")?.rect;
    let start = egui::pos2(ruler.left() + ruler.width() * 0.2, ruler.center().y);
    render(
        &mut s,
        capture,
        vec![
            Event::PointerMoved(start),
            Replay::button(start, PointerButton::Primary, true),
        ],
        STEP,
    )?;
    if !s.bench.timeline.as_ref().unwrap().timelines[0].is_scrubbing() {
        return Err("Animation did not begin scrub".into());
    }
    clip.push(&capture.framed_frame()?, Duration::from_millis(200))?;
    for step in 1..=8 {
        let point = egui::pos2(start.x + ruler.width() * 0.055 * step as f32, start.y);
        render(
            &mut s,
            capture,
            vec![Event::PointerMoved(point)],
            Duration::from_millis(90),
        )?;
        // Render the host's accepted state after processing the request.
        render(&mut s, capture, Vec::new(), Duration::ZERO)?;
        clip.push(&capture.framed_frame()?, Duration::from_millis(90))?;
    }
    let end = egui::pos2(start.x + ruler.width() * 0.44, start.y);
    render(
        &mut s,
        capture,
        vec![Replay::button(end, PointerButton::Primary, false)],
        STEP,
    )?;
    settle(&mut s, capture)?;
    let fixture = s.bench.timeline.as_ref().unwrap();
    if fixture.timelines[0].is_scrubbing()
        || !matches!(
            fixture.events.back().map(|e| e.request),
            Some(crate::ui::timeline::Request::ScrubEnd { .. })
        )
    {
        return Err("Animation scrub did not terminate once".into());
    }
    clip.push(&capture.framed_frame()?, Duration::from_millis(350))?;
    render(&mut s, capture, vec![Event::Zoom(2.0)], STEP)?;
    clip.push(&capture.framed_frame()?, Duration::from_millis(400))?;
    render(
        &mut s,
        capture,
        vec![wheel(egui::vec2(0.0, -80.0), Modifiers::SHIFT)],
        STEP,
    )?;
    clip.push(&capture.framed_frame()?, Duration::from_millis(400))?;
    std::fs::write(
        directory.join("timeline-navigation-review.webp"),
        capture.framed_webp()?,
    )
    .map_err(|e| e.to_string())?;
    click(&mut s, capture, 0, "timeline-fit", PointerButton::Primary)?;
    clip.push(&capture.framed_frame()?, Duration::from_millis(400))?;
    std::fs::write(
        directory.join("timeline-scrub-navigation.webp"),
        clip.finish()?,
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
