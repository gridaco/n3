//! The shared keyboard policy. Hosts supply egui events and execute host effects;
//! they do not maintain their own keymaps or decide which editor context wins.
use super::bindings::{self, Trigger};
use crate::{
    camera::View,
    editor::Tool,
    keyboard_input::{NumberKey, NumberKeyEvent},
};
use egui::{Context, Event, Key, Modifiers};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Command {
    New,
    Open,
    OpenPreferences,
    InsertMenu,
    Save {
        save_as: bool,
    },
    Quit,
    Undo,
    Redo,
    ToggleUi,
    FocusToasts,
    SelectAll,
    DuplicateSelection,
    DeleteSelection,
    MakeFace,
    CycleSelection {
        reverse: bool,
    },
    Tool(Tool),
    /// Advisory feedback only. The UI resolves the canonical binding label;
    /// showing a hint never executes the suggested command.
    ShortcutHint {
        binding: &'static str,
    },
    ToggleTransformAxis(usize),
    TransformCharacter(char),
    TransformBackspace,
    Nudge {
        horizontal: i8,
        vertical: i8,
        fast: bool,
        repeat: bool,
    },
    EndNudge {
        horizontal: i8,
        vertical: i8,
    },
    Confirm,
    LeaveEdit,
    Escape,
    Frame,
    FrameSelection,
    ToggleProjection,
    SetShading(crate::render::shading::ShadingMode),
    ToggleXray,
    TogglePlanarNavigation,
    ToggleRuler2D,
    ToggleLocalView,
    View(View),
    /// Signed 15-degree steps: positive horizontal looks right, vertical up.
    OrbitView {
        horizontal: f32,
        vertical: f32,
    },
}

/// Effects that only the native host can perform. The documentation harness
/// stops at this boundary rather than opening dialogs or quitting its process.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HostEffect {
    #[default]
    None,
    Open,
    Quit,
    CancelNavigation,
    NavigationContextChanged,
}

/// Selection/interaction eligibility comes from the editor, while this layer
/// predicts tool and axis changes in event order. Thus `R`, `Z`, `9`, `0` in one
/// native input batch has the same meaning as four separate rendered frames.
#[derive(Clone, Copy, Debug)]
pub struct TransformKeyboardContext {
    pub tool: Tool,
    pub axis: Option<usize>,
    pub can_transform: bool,
    pub numeric_active: bool,
}

impl Default for TransformKeyboardContext {
    fn default() -> Self {
        Self {
            tool: Tool::View,
            axis: None,
            can_transform: false,
            numeric_active: false,
        }
    }
}

impl TransformKeyboardContext {
    fn transform_tool(self) -> bool {
        matches!(self.tool, Tool::Move | Tool::Rotate | Tool::Scale)
    }

    fn owns_numbers(self) -> bool {
        self.can_transform && self.transform_tool() && (self.axis.is_some() || self.numeric_active)
    }

    fn observe(&mut self, command: Command) {
        match command {
            Command::Tool(tool) => {
                if self.tool != tool {
                    self.axis = None;
                    self.numeric_active = false;
                }
                self.tool = tool;
            }
            Command::ToggleTransformAxis(axis) if self.can_transform && self.transform_tool() => {
                self.axis = (self.axis != Some(axis)).then_some(axis);
                self.numeric_active = false;
            }
            Command::TransformCharacter(_) => self.numeric_active = true,
            Command::Confirm | Command::Escape | Command::LeaveEdit => {
                self.axis = None;
                self.numeric_active = false;
            }
            _ => {}
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct KeyboardOwner {
    text: bool,
    viewport: bool,
    popup: bool,
    modal: bool,
}

impl KeyboardOwner {
    fn read(ctx: &Context) -> Self {
        let focused = ctx.memory(|memory| memory.focused());
        Self {
            // egui's wants_keyboard_input means *any* focused widget, including
            // our viewport. Every other focused control retains its own keys.
            text: focused.is_some_and(|id| id != viewport_focus_id()),
            viewport: focused == Some(viewport_focus_id()),
            popup: egui::Popup::is_any_open(ctx),
            modal: ctx.memory(|memory| memory.top_modal_layer().is_some()),
        }
    }

    fn include(self, other: Self) -> Self {
        Self {
            text: self.text || other.text,
            viewport: self.viewport && other.viewport,
            popup: self.popup || other.popup,
            modal: self.modal || other.modal,
        }
    }
}

pub fn viewport_focus_id() -> egui::Id {
    egui::Id::new("n3.viewport.keyboard_focus")
}

fn viewport_input_claim_id(ctx: &Context) -> egui::Id {
    egui::Id::new("n3.viewport.input_claim").with(ctx.viewport_id())
}

/// Capture all shortcut input through the current `Context::run`, including
/// layout retries and the frame in which a transient gesture is dismissed.
/// Call before widgets run whenever that gesture owns any part of the frame.
/// This also consumes pending key taps: a future hold gesture can claim input
/// here so releasing its key cannot run the tap action as well.
pub fn claim_viewport_input(ctx: &Context) {
    let frame = ctx.cumulative_frame_nr();
    let id = viewport_input_claim_id(ctx);
    ctx.data_mut(|data| data.insert_temp(id, frame));
    retain_captured_viewport_focus(ctx);
}

pub fn viewport_input_claimed(ctx: &Context) -> bool {
    let id = viewport_input_claim_id(ctx);
    ctx.data(|data| data.get_temp::<u64>(id)) == Some(ctx.cumulative_frame_nr())
}

fn retain_captured_viewport_focus(ctx: &Context) {
    if ctx.input(|input| input.focused) {
        ctx.memory_mut(|memory| {
            // Focus navigation is queued before our UI runs. Cancel that
            // direction, without retaining an arrow-key lock next frame.
            memory.move_focus(egui::FocusDirection::None);
            if !memory.has_focus(viewport_focus_id()) {
                memory.request_focus(viewport_focus_id());
            }
            memory.set_focus_lock_filter(viewport_focus_id(), viewport_key_filter());
        });
    }
}

/// Read-only ownership gate for held keys such as Space. Snapshot this before
/// widgets run and require it again at gesture routing, so dismissing a field
/// or popup cannot transfer the same key press into the viewport.
pub fn viewport_keys_available(ctx: &Context) -> bool {
    let owner = KeyboardOwner::read(ctx);
    ctx.input(|input| input.focused) && !owner.text && !owner.popup && !owner.modal
}

fn viewport_key_filter() -> egui::EventFilter {
    egui::EventFilter {
        tab: true,
        escape: true,
        horizontal_arrows: true,
        vertical_arrows: true,
    }
}

/// Call with the actual viewport interaction response using `viewport_focus_id`.
/// Eligibility belongs to viewport hit testing, excluding gizmo and UI overlays.
pub fn viewport_interaction(ui: &mut egui::Ui, response: &egui::Response, eligible_press: bool) {
    debug_assert_eq!(response.id, viewport_focus_id());
    if eligible_press
        && response.enabled()
        && ui.input(|input| input.focused && input.pointer.any_pressed())
        && !egui::Popup::is_any_open(ui.ctx())
        && !ui.memory(|memory| memory.top_modal_layer().is_some())
    {
        response.request_focus();
    }
    ui.memory_mut(|memory| {
        memory.set_focus_lock_filter(viewport_focus_id(), viewport_key_filter());
    });
}

/// egui ignores a focus filter on the frame focus is first acquired. Before
/// widgets run on the next pass, stop its queued Tab traversal and install the
/// filter. This protects the very first Tab after a viewport click.
#[derive(Default)]
struct ViewportFocus;

impl egui::Plugin for ViewportFocus {
    fn debug_name(&self) -> &'static str {
        "n3 viewport keyboard focus"
    }

    fn on_begin_pass(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx();
        if viewport_input_claimed(ctx) {
            retain_captured_viewport_focus(ctx);
            return;
        }
        if !ctx.input(|input| input.focused)
            || egui::Popup::is_any_open(ctx)
            || ctx.memory(|memory| memory.top_modal_layer().is_some())
        {
            return;
        }
        let (last_navigation_key, plain_escape) = ctx.input(|input| {
            let last = input.events.iter().rev().find_map(|event| match event {
                Event::Key { key, pressed: true, .. }
                    if matches!(key, Key::Tab | Key::Escape | Key::ArrowUp | Key::ArrowDown | Key::ArrowLeft | Key::ArrowRight) => Some(*key),
                _ => None,
            });
            let escape = input.events.iter().any(|event| {
                matches!(event, Event::Key { key: Key::Escape, pressed: true, modifiers, .. } if *modifiers == Modifiers::NONE)
            });
            (last, escape)
        });
        ctx.memory_mut(|memory| {
            let id = viewport_focus_id();
            // Focus::begin_pass clears newly acquired focus on Escape before
            // the filter can be installed. Keep the editor's Escape ladder in
            // the viewport instead of abandoning keyboard ownership.
            if plain_escape && memory.focused().is_none() && memory.had_focus_last_frame(id) {
                memory.request_focus(id);
            }
            if memory.has_focus(id) {
                if matches!(
                    last_navigation_key,
                    Some(
                        Key::Tab | Key::ArrowUp | Key::ArrowDown | Key::ArrowLeft | Key::ArrowRight
                    )
                ) {
                    memory.move_focus(egui::FocusDirection::None);
                }
                memory.set_focus_lock_filter(id, viewport_key_filter());
            }
        });
    }
}

fn resolve(key: Key, modifiers: Modifiers, owner: KeyboardOwner) -> Option<Command> {
    // Ownership on either side of the UI pass prevents a dismissal key from
    // falling through after its popup or field relinquishes it. Bindings choose
    // the command; context only decides whether that command can receive input.
    if owner.popup || owner.modal {
        return None;
    }
    bindings::BINDINGS.iter().find_map(|binding| {
        (binding.trigger == Trigger::Press
            && (!owner.text || binding.over_text)
            && (!binding.viewport_only || owner.viewport)
            && binding.matches_key(key, modifiers))
        .then_some(binding.command)
        .flatten()
    })
}

fn arrow_direction(key: Key) -> Option<(i8, i8)> {
    bindings::BINDINGS.iter().find_map(|binding| {
        if binding.key() == Some(key)
            && let Some(Command::Nudge {
                horizontal,
                vertical,
                ..
            }) = binding.command
        {
            Some((horizontal, vertical))
        } else {
            None
        }
    })
}

/// Input recognition is separate from the binding. A tap is one eligible
/// down/up pair; it has no timer, and a hold owner may suppress it at any point.
#[derive(Clone, Copy, Default)]
struct KeyTap {
    down: bool,
    eligible: bool,
}

impl KeyTap {
    fn suppress(&mut self) {
        self.eligible = false;
    }

    fn event(&mut self, pressed: bool, repeat: bool, eligible: bool) -> bool {
        if pressed {
            if !self.down {
                self.down = true;
                self.eligible = eligible && !repeat;
            } else if !eligible {
                self.suppress();
            }
            false
        } else {
            let tapped = self.down && self.eligible && eligible;
            *self = Self::default();
            tapped
        }
    }
}

fn period_tap_id(ctx: &Context) -> egui::Id {
    egui::Id::new("n3.viewport.period_tap").with(ctx.viewport_id())
}

fn resolve_tap(key: Key, modifiers: Modifiers, owner: KeyboardOwner) -> Option<Command> {
    if owner.text || owner.popup || owner.modal {
        return None;
    }
    let binding = bindings::required("view.planar");
    binding
        .matches_key(key, modifiers)
        .then_some(binding.command)
        .flatten()
}

fn track_arrow_repeats(ctx: &Context, events: &mut [Event], numbers: &[NumberKeyEvent]) {
    let id = egui::Id::new("n3.viewport.arrow_keys_down").with(ctx.viewport_id());
    let mut down = ctx
        .data(|data| data.get_temp::<[bool; 4]>(id))
        .unwrap_or_default();
    let window_focused = ctx.input(|input| input.focused);
    let mut focused = window_focused;
    if !focused {
        down.fill(false);
    }
    for (index, event) in events.iter_mut().enumerate() {
        if let Event::WindowFocused(value) = event {
            focused = *value;
            if !focused {
                down.fill(false);
            }
        } else if focused
            && !numbers.iter().any(|number| number.event_index == index)
            && let Event::Key {
                key,
                pressed,
                repeat,
                ..
            } = event
            && let Some(arrow) = [
                Key::ArrowLeft,
                Key::ArrowRight,
                Key::ArrowUp,
                Key::ArrowDown,
            ]
            .iter()
            .position(|arrow| arrow == key)
        {
            // egui merges a NumLock-off keypad key with its logical arrow.
            // Track only real arrow events, even when a field owns the press.
            if *pressed {
                *repeat = down[arrow];
            }
            down[arrow] = *pressed;
        }
    }
    if !window_focused {
        down.fill(false);
    }
    ctx.data_mut(|data| data.insert_temp(id, down));
}

fn resolve_number(event: NumberKeyEvent, owner: KeyboardOwner) -> Option<Command> {
    if !event.pressed || event.repeat || owner.text || owner.popup || owner.modal {
        return None;
    }
    bindings::BINDINGS.iter().find_map(|binding| {
        binding
            .matches_number(event.key, event.modifiers)
            .then_some(binding.command)
            .flatten()
    })
}

fn numeric_character(character: char) -> bool {
    character.is_ascii_digit() || matches!(character, '-' | '.')
}

/// Physical digit metadata wins over the logical egui key, especially with
/// NumLock off (a keypad digit may otherwise look like Delete or an arrow).
fn numeric_key_character(index: usize, event: &Event, numbers: &[NumberKeyEvent]) -> Option<char> {
    let Event::Key {
        key,
        pressed: true,
        modifiers,
        ..
    } = event
    else {
        return None;
    };
    if *modifiers != Modifiers::NONE {
        return None;
    }
    if let Some(number) = numbers.iter().find(|number| number.event_index == index) {
        if !number.pressed || number.modifiers != Modifiers::NONE {
            return None;
        }
        let (NumberKey::TopRow(digit) | NumberKey::Numpad(digit)) = number.key;
        return (digit <= 9).then(|| char::from(b'0' + digit));
    }
    match key {
        Key::Num0 => Some('0'),
        Key::Num1 => Some('1'),
        Key::Num2 => Some('2'),
        Key::Num3 => Some('3'),
        Key::Num4 => Some('4'),
        Key::Num5 => Some('5'),
        Key::Num6 => Some('6'),
        Key::Num7 => Some('7'),
        Key::Num8 => Some('8'),
        Key::Num9 => Some('9'),
        Key::Minus => Some('-'),
        Key::Period => Some('.'),
        _ => None,
    }
}

/// egui can carry both a Key and its printable Text event. Resolve at the key's
/// position for deterministic axis/digit ordering, then discard only its own
/// matching text echo. Text-only input still works, including batched strings.
fn unmatched_numeric_text(events: &[Event], numbers: &[NumberKeyEvent]) -> Vec<Vec<char>> {
    let mut pending = Vec::new();
    let mut result = vec![Vec::new(); events.len()];
    for (index, event) in events.iter().enumerate() {
        if let Some(character) = numeric_key_character(index, event, numbers) {
            pending.push(character);
        } else if let Event::Text(text) = event {
            // Reject unsupported text as a whole. Filtering `1e3` to `13`
            // would reinterpret the user's value instead of preserving input.
            if text.chars().all(numeric_character) {
                for character in text.chars() {
                    if let Some(paired) = pending.iter().position(|key| *key == character) {
                        pending.remove(paired);
                    } else {
                        result[index].push(character);
                    }
                }
            }
            pending.clear();
        } else {
            // Native printable echoes immediately follow their key events.
            // Never pair across an axis change, release, or unrelated input.
            pending.clear();
        }
    }
    result
}

fn transform_keys_available(owner: KeyboardOwner) -> bool {
    !owner.text && !owner.popup && !owner.modal
}

fn resolve_transform_key(
    key: Key,
    modifiers: Modifiers,
    owner: KeyboardOwner,
    transform: TransformKeyboardContext,
) -> Option<Command> {
    if modifiers != Modifiers::NONE || !transform_keys_available(owner) {
        return None;
    }
    if transform.owns_numbers() && matches!(key, Key::Delete | Key::Backspace) {
        return Some(Command::TransformBackspace);
    }
    None
}

fn resolve_shortcut_hint(
    key: Key,
    modifiers: Modifiers,
    owner: KeyboardOwner,
    transform: TransformKeyboardContext,
) -> Option<Command> {
    if modifiers != Modifiers::NONE
        || !transform_keys_available(owner)
        || transform.owns_numbers()
        // Even a context-ineligible or held binding owns its input. Hints are
        // explicit fallbacks, never an alternative route around normal policy.
        || bindings::BINDINGS
            .iter()
            .any(|binding| binding.matches_key(key, modifiers))
    {
        return None;
    }
    match key {
        Key::A => Some(Command::ShortcutHint {
            binding: "selection.all",
        }),
        _ => None,
    }
}

/// One frame, including egui layout retries. Create before `Context::run`,
/// collect after drawing UI, then dispatch once after `run` returns.
pub struct ShortcutFrame {
    context: Context,
    before: KeyboardOwner,
    commands: Vec<Command>,
    number_events: Vec<NumberKeyEvent>,
    events: Option<Vec<Event>>,
}

impl ShortcutFrame {
    pub fn new(ctx: &Context) -> Self {
        ctx.plugin_or_default::<ViewportFocus>();
        Self {
            context: ctx.clone(),
            before: KeyboardOwner::read(ctx),
            commands: Vec::new(),
            number_events: Vec::new(),
            events: None,
        }
    }

    pub fn with_number_events(ctx: &Context, number_events: Vec<NumberKeyEvent>) -> Self {
        let mut frame = Self::new(ctx);
        frame.number_events = number_events;
        frame
    }

    /// Snapshot after egui normalizes input but before widgets consume keys.
    /// Native and headless callers invoke this before drawing the first UI pass.
    pub fn begin_pass(&mut self, ctx: &Context) {
        if ctx.current_pass_index() != 0 || self.events.is_some() {
            return;
        }
        let mut events = ctx.input(|input| input.events.clone());
        track_arrow_repeats(ctx, &mut events, &self.number_events);
        // With NumLock off, winit can report a numpad digit as a logical arrow.
        // In viewport context it is a camera key, not egui focus navigation.
        if !self.before.text && !self.before.popup && !self.before.modal {
            let last_navigation = events.iter().enumerate().rev().find_map(|(index, event)| {
                matches!(
                    event,
                    Event::Key {
                        key: Key::Tab
                            | Key::ArrowUp
                            | Key::ArrowDown
                            | Key::ArrowLeft
                            | Key::ArrowRight,
                        pressed: true,
                        ..
                    }
                )
                .then_some(index)
            });
            if last_navigation.is_some_and(|index| {
                self.number_events
                    .iter()
                    .any(|event| event.event_index == index)
            }) {
                ctx.memory_mut(|memory| memory.move_focus(egui::FocusDirection::None));
            }
        }
        self.events = Some(events);
    }

    #[cfg(test)]
    pub fn collect(&mut self, ctx: &Context) {
        self.collect_with_transform(ctx, TransformKeyboardContext::default());
    }

    pub fn collect_with_transform(
        &mut self,
        ctx: &Context,
        mut transform: TransformKeyboardContext,
    ) {
        let claimed = viewport_input_claimed(ctx);
        let owner = self.before.include(KeyboardOwner::read(ctx));
        let tap_id = period_tap_id(ctx);
        let planar_binding = bindings::required("view.planar");
        let planar_key = planar_binding.key().expect("planar toggle uses a key tap");
        let mut tap = ctx
            .data(|data| data.get_temp::<KeyTap>(tap_id))
            .unwrap_or_default();
        let (focused, modifiers) = ctx.input(|input| (input.focused, input.modifiers));
        if claimed
            || !focused
            || transform.owns_numbers()
            || resolve_tap(planar_key, modifiers, owner).is_none()
        {
            tap.suppress();
            // Ownership can change on a retry after a release was collected.
            self.commands
                .retain(|command| !matches!(command, Command::TogglePlanarNavigation));
        }
        if !focused {
            tap = KeyTap::default();
        }
        if !focused || owner.popup || owner.modal {
            self.commands.clear();
        } else if owner.text {
            // A late field on a layout retry owns the entire frame. Keep only
            // the application shortcuts that `resolve` permits over fields.
            self.commands.retain(|command| {
                matches!(
                    command,
                    Command::New
                        | Command::Open
                        | Command::OpenPreferences
                        | Command::Save { .. }
                        | Command::Quit
                        | Command::ToggleUi
                        | Command::EndNudge { .. }
                )
            });
        }
        if claimed {
            self.commands.clear();
        }
        if transform.owns_numbers() {
            // A numeric session may begin during a layout retry, after the
            // first pass collected notification input. Keep focus in its owner.
            self.commands.retain(|command| {
                !matches!(command, Command::ShortcutHint { .. } | Command::FocusToasts)
            });
        }
        if ctx.current_pass_index() != 0 {
            ctx.data_mut(|data| data.insert_temp(tap_id, tap));
            return;
        }
        ctx.input(|input| {
            let mut event_focused = input.focused;
            let mut context_boundary = false;
            let events = self.events.as_ref().unwrap_or(&input.raw.events);
            let text = unmatched_numeric_text(events, &self.number_events);
            for (index, event) in events.iter().enumerate() {
                if let Event::WindowFocused(value) = event {
                    event_focused = *value;
                    if !event_focused {
                        tap = KeyTap::default();
                        self.commands.clear();
                    }
                }
                if context_boundary {
                    // The editor may reject an incomplete numeric expression
                    // on Enter. Defer new semantic input to the next frame,
                    // where its actual accepted/cancelled state is available.
                    // Toast focus similarly transfers ownership only when
                    // dispatched; later keys must wait for that new context.
                    // Still finish period key lifecycles, preventing a later
                    // release from resurrecting a camera tap.
                    if let Event::Key {
                        key,
                        pressed,
                        repeat,
                        ..
                    } = event
                        && *key == planar_key
                    {
                        tap.event(*pressed, *repeat, false);
                    }
                    continue;
                }
                let numeric_owned = transform.owns_numbers()
                    && transform_keys_available(owner)
                    && input.focused
                    && event_focused
                    && !claimed;
                if numeric_owned
                    && let Some(character) =
                        numeric_key_character(index, event, &self.number_events)
                {
                    if let Event::Key { key, repeat, .. } = event
                        && *key == planar_key
                    {
                        // Record an ineligible down/up pair, so Enter or
                        // Escape before release cannot resurrect the tap.
                        tap.event(true, *repeat, false);
                    }
                    let command = Command::TransformCharacter(character);
                    transform.observe(command);
                    self.commands.push(command);
                    continue;
                }
                if matches!(event, Event::Text(_)) {
                    if numeric_owned && input.modifiers == Modifiers::NONE {
                        for character in &text[index] {
                            if *character == '.' {
                                tap.suppress();
                            }
                            let command = Command::TransformCharacter(*character);
                            transform.observe(command);
                            self.commands.push(command);
                        }
                    }
                    continue;
                }
                if let Event::Key {
                    key,
                    modifiers,
                    pressed,
                    repeat,
                    ..
                } = event
                    && *key == planar_key
                {
                    let binding = resolve_tap(planar_key, *modifiers, owner);
                    let eligible = input.focused
                        && event_focused
                        && !claimed
                        && !transform.owns_numbers()
                        && planar_binding.matches_modifiers(input.modifiers)
                        && binding.is_some();
                    if tap.event(*pressed, *repeat, eligible) {
                        self.commands.push(binding.unwrap());
                    }
                    continue;
                }
                if !input.focused || !event_focused || claimed {
                    continue;
                }
                if let Some(number) = self
                    .number_events
                    .iter()
                    .find(|event| event.event_index == index)
                {
                    if matches!(event, Event::Key { .. })
                        && let Some(command) = resolve_number(*number, owner)
                    {
                        self.commands.push(command);
                    }
                    continue;
                }
                if let Event::Key {
                    key,
                    modifiers,
                    pressed,
                    repeat,
                    ..
                } = event
                {
                    if !pressed {
                        if let Some((horizontal, vertical)) = arrow_direction(*key) {
                            self.commands.push(Command::EndNudge {
                                horizontal,
                                vertical,
                            });
                        }
                    } else if let Some(mut command) =
                        resolve_transform_key(*key, *modifiers, owner, transform)
                            .or_else(|| resolve(*key, *modifiers, owner))
                            .or_else(|| resolve_shortcut_hint(*key, *modifiers, owner, transform))
                    {
                        if transform.owns_numbers() && matches!(command, Command::FocusToasts) {
                            continue;
                        }
                        if let Command::Nudge {
                            repeat: repeating, ..
                        } = &mut command
                        {
                            *repeating = *repeat;
                        } else if *repeat && !matches!(command, Command::TransformBackspace) {
                            continue;
                        }
                        if matches!(command, Command::FocusToasts)
                            || (transform.owns_numbers()
                                && matches!(
                                    command,
                                    Command::Confirm | Command::Escape | Command::LeaveEdit
                                ))
                        {
                            context_boundary = true;
                        }
                        transform.observe(command);
                        if transform.owns_numbers() {
                            tap.suppress();
                        }
                        self.commands.push(command);
                    }
                }
            }
        });
        ctx.data_mut(|data| data.insert_temp(tap_id, tap));
    }

    pub fn commands(mut self) -> Vec<Command> {
        // egui promotes a newly opened modal to top_modal_layer only at the
        // end of its pass. Dispatch happens after Context::run, so recheck here
        // to include a modal's very first frame instead of leaking its input.
        let owner = self.before.include(KeyboardOwner::read(&self.context));
        if !self.context.input(|input| input.focused) || owner.popup || owner.modal {
            self.commands.clear();
        } else if owner.text {
            self.commands.retain(|command| {
                matches!(
                    command,
                    Command::New
                        | Command::Open
                        | Command::Save { .. }
                        | Command::Quit
                        | Command::ToggleUi
                        | Command::EndNudge { .. }
                )
            });
        }
        if !transform_keys_available(owner) {
            let id = period_tap_id(&self.context);
            self.context.data_mut(|data| {
                if let Some(mut tap) = data.get_temp::<KeyTap>(id) {
                    tap.suppress();
                    data.insert_temp(id, tap);
                }
            });
        }
        self.commands
    }
}

#[cfg(test)]
#[path = "shortcuts_tests.rs"]
mod tests;
