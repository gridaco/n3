use super::*;

#[test]
fn xray_chord_respects_input_ownership_and_stays_separate_from_plain_z() {
    let viewport = KeyboardOwner {
        viewport: true,
        ..Default::default()
    };
    assert_eq!(
        resolve(Key::Z, Modifiers::ALT, viewport),
        Some(Command::ToggleXray)
    );
    // Closing a menu may leave no widget focused. View shortcuts stay usable
    // then, while focused fields, popups, and gestures retain their ownership.
    assert_eq!(
        resolve(Key::Z, Modifiers::ALT, KeyboardOwner::default()),
        Some(Command::ToggleXray)
    );
    for modifiers in [
        Modifiers::NONE,
        Modifiers::CTRL,
        Modifiers::COMMAND,
        Modifiers {
            alt: true,
            shift: true,
            ..Modifiers::NONE
        },
        Modifiers {
            alt: true,
            command: true,
            ..Modifiers::NONE
        },
    ] {
        assert_ne!(
            resolve(Key::Z, modifiers, viewport),
            Some(Command::ToggleXray)
        );
    }
    for owner in [
        KeyboardOwner {
            text: true,
            ..viewport
        },
        KeyboardOwner {
            popup: true,
            ..viewport
        },
        KeyboardOwner {
            modal: true,
            ..viewport
        },
    ] {
        assert_ne!(
            resolve(Key::Z, Modifiers::ALT, owner),
            Some(Command::ToggleXray)
        );
    }
    for tool in [Tool::View, Tool::Move, Tool::Rotate, Tool::Scale] {
        let ctx = Context::default();
        ctx.memory_mut(|memory| memory.request_focus(viewport_focus_id()));
        let mut repeat = key(Key::Z, Modifiers::ALT);
        if let Event::Key { repeat, .. } = &mut repeat {
            *repeat = true;
        }
        let commands = transform_frame(
            &ctx,
            transform_context(tool, Some(0)),
            vec![key(Key::Z, Modifiers::ALT), repeat, Event::Text("Ω".into())],
            Vec::new(),
            |_| {},
        );
        assert_eq!(commands, vec![Command::ToggleXray], "{tool:?}");
    }
}

fn transform_context(tool: Tool, axis: Option<usize>) -> TransformKeyboardContext {
    TransformKeyboardContext {
        tool,
        axis,
        can_transform: true,
        numeric_active: false,
    }
}

fn typed_key(key: Key, pressed: bool) -> Event {
    Event::Key {
        key,
        physical_key: Some(key),
        pressed,
        repeat: false,
        modifiers: Modifiers::NONE,
    }
}

fn transform_frame(
    ctx: &Context,
    transform: TransformKeyboardContext,
    events: Vec<Event>,
    numbers: Vec<NumberKeyEvent>,
    mut during_ui: impl FnMut(&Context),
) -> Vec<Command> {
    let mut shortcuts = ShortcutFrame::with_number_events(ctx, numbers);
    ctx.run_ui(
        egui::RawInput {
            events,
            focused: true,
            ..Default::default()
        },
        |root_ui| {
            let context = root_ui.ctx().clone();
            let ctx = &context;
            shortcuts.begin_pass(ctx);
            during_ui(ctx);
            shortcuts.collect_with_transform(ctx, transform);
        },
    )
    .textures_delta
    .clear();
    shortcuts.commands()
}

#[test]
fn shortcut_hint_is_advisory_and_only_an_explicit_unassigned_key_can_trigger_it() {
    let ctx = Context::default();
    let commands = transform_frame(
        &ctx,
        TransformKeyboardContext::default(),
        vec![typed_key(Key::A, true), Event::Text("a".into())],
        Vec::new(),
        |_| {},
    );
    assert!(matches!(
        commands.as_slice(),
        [Command::ShortcutHint {
            binding: "selection.all"
        }]
    ));

    // Existing aliases and primary bindings keep their behavior. An advisory
    // does not become a second, competing command for an assigned input.
    for (key, tool) in [
        (Key::G, Tool::Move),
        (Key::R, Tool::Rotate),
        (Key::S, Tool::Scale),
    ] {
        let commands = transform_frame(
            &Context::default(),
            TransformKeyboardContext::default(),
            vec![typed_key(key, true)],
            Vec::new(),
            |_| {},
        );
        assert!(matches!(commands.as_slice(), [Command::Tool(actual)] if *actual == tool));
    }
    let commands = transform_frame(
        &Context::default(),
        TransformKeyboardContext::default(),
        vec![key(Key::A, Modifiers::COMMAND)],
        Vec::new(),
        |_| {},
    );
    assert!(matches!(commands.as_slice(), [Command::SelectAll]));
    assert!(
        transform_frame(
            &Context::default(),
            TransformKeyboardContext::default(),
            vec![typed_key(Key::B, true)],
            Vec::new(),
            |_| {},
        )
        .is_empty()
    );
    for binding in bindings::BINDINGS {
        if let Some(key) = binding.key() {
            assert!(
                resolve_shortcut_hint(
                    key,
                    binding.modifiers,
                    KeyboardOwner::default(),
                    TransformKeyboardContext::default(),
                )
                .is_none(),
                "Binding {} must retain its key",
                binding.id
            );
        }
    }
}

#[test]
fn shortcut_hint_ignores_modified_keys_releases_and_held_repeats() {
    for modifiers in [
        Modifiers::SHIFT,
        Modifiers::ALT,
        Modifiers::CTRL,
        Modifiers::MAC_CMD,
    ] {
        assert!(
            resolve_shortcut_hint(
                Key::A,
                modifiers,
                KeyboardOwner::default(),
                TransformKeyboardContext::default(),
            )
            .is_none()
        );
    }
    let ctx = Context::default();
    let frame = |events| {
        transform_frame(
            &ctx,
            TransformKeyboardContext::default(),
            events,
            Vec::new(),
            |_| {},
        )
    };
    assert!(frame(vec![typed_key(Key::A, false)]).is_empty());
    assert!(matches!(
        frame(vec![typed_key(Key::A, true)]).as_slice(),
        [Command::ShortcutHint { .. }]
    ));
    assert!(frame(vec![typed_key(Key::A, true)]).is_empty());
    assert!(frame(vec![typed_key(Key::A, false)]).is_empty());
}

#[test]
fn notification_shortcuts_respect_fields_popups_modals_claims_and_late_ownership() {
    for (key, ownership) in [Key::A, Key::F6]
        .into_iter()
        .flat_map(|key| (0..9).map(move |ownership| (key, ownership)))
    {
        let ctx = Context::default();
        let field = egui::Id::new("shortcut-hint-field");
        let popup = egui::Id::new("shortcut-hint-popup");
        if ownership == 0 {
            ctx.memory_mut(|memory| memory.request_focus(field));
        }
        if ownership == 1 {
            egui::Popup::open_id(&ctx, popup);
        }
        let mut events = vec![typed_key(key, true)];
        if ownership == 8 {
            events.push(Event::WindowFocused(false));
        }
        let commands = transform_frame(
            &ctx,
            TransformKeyboardContext::default(),
            events,
            Vec::new(),
            |ctx| match ownership {
                0 => ctx.memory_mut(|memory| memory.surrender_focus(field)),
                1 => egui::Popup::close_all(ctx),
                2 => claim_viewport_input(ctx),
                3 => {
                    egui::Modal::new(egui::Id::new("shortcut-hint-modal")).show(ctx, |ui| {
                        ui.label("Modal");
                    });
                }
                4..=7 if ctx.current_pass_index() == 0 => {
                    ctx.request_discard("Late shortcut hint ownership");
                }
                4 => ctx.memory_mut(|memory| memory.request_focus(field)),
                5 => egui::Popup::open_id(ctx, popup),
                6 => claim_viewport_input(ctx),
                7 => {
                    egui::Modal::new(egui::Id::new("late-shortcut-hint-modal")).show(ctx, |ui| {
                        ui.label("Modal");
                    });
                }
                _ => {}
            },
        );
        assert!(
            commands.is_empty(),
            "{key:?}, ownership case {ownership}: {commands:?}"
        );
    }
}

#[test]
fn toast_focus_binding_is_plain_f6_and_runs_once_across_retries_and_held_repeats() {
    let binding = bindings::required("notifications.focus");
    assert_eq!(binding.key(), Some(Key::F6));
    assert_eq!(binding.modifiers, Modifiers::NONE);
    for modifiers in [
        Modifiers::SHIFT,
        Modifiers::ALT,
        Modifiers::CTRL,
        Modifiers::COMMAND,
    ] {
        assert!(resolve(Key::F6, modifiers, KeyboardOwner::default()).is_none());
    }
    let ctx = Context::default();
    let commands = transform_frame(
        &ctx,
        TransformKeyboardContext::default(),
        vec![typed_key(Key::F6, true)],
        Vec::new(),
        |ctx| {
            if ctx.current_pass_index() == 0 {
                ctx.request_discard("Toast focus is dispatched once after layout retries");
            }
        },
    );
    assert_eq!(commands, vec![Command::FocusToasts]);
    for pressed in [true, false] {
        assert!(
            transform_frame(
                &ctx,
                TransformKeyboardContext::default(),
                vec![typed_key(Key::F6, pressed)],
                Vec::new(),
                |_| {},
            )
            .is_empty()
        );
    }
}

#[test]
fn toast_focus_does_not_interrupt_numeric_input_or_escape_its_confirmation_boundary() {
    for events in [
        vec![typed_key(Key::F6, true)],
        vec![typed_key(Key::Enter, true), typed_key(Key::F6, true)],
    ] {
        let commands = transform_frame(
            &Context::default(),
            transform_context(Tool::Move, Some(0)),
            events,
            Vec::new(),
            |_| {},
        );
        assert!(commands.iter().all(|command| *command == Command::Confirm));
    }
}

#[test]
fn toast_focus_stops_later_viewport_commands_in_the_same_input_batch() {
    for next_key in [Key::Enter, Key::Delete, Key::Tab] {
        let ctx = Context::default();
        ctx.memory_mut(|memory| memory.request_focus(viewport_focus_id()));
        let commands = transform_frame(
            &ctx,
            TransformKeyboardContext::default(),
            vec![typed_key(Key::F6, true), typed_key(next_key, true)],
            Vec::new(),
            |_| {},
        );
        assert_eq!(commands, vec![Command::FocusToasts], "{next_key:?}");
    }
}

#[test]
fn shortcut_hint_obeys_numeric_ownership_and_confirmation_order() {
    let commands = transform_frame(
        &Context::default(),
        transform_context(Tool::View, None),
        vec![
            typed_key(Key::W, true),
            typed_key(Key::X, true),
            typed_key(Key::A, true),
        ],
        Vec::new(),
        |_| {},
    );
    assert!(matches!(
        commands.as_slice(),
        [Command::Tool(Tool::Move), Command::ToggleTransformAxis(0)]
    ));
    for finish in [Key::Enter, Key::Escape] {
        let commands = transform_frame(
            &Context::default(),
            transform_context(Tool::Move, Some(0)),
            vec![typed_key(finish, true), typed_key(Key::A, true)],
            Vec::new(),
            |_| {},
        );
        assert!(matches!(
            commands.as_slice(),
            [Command::Confirm | Command::Escape]
        ));
    }
    let context = TransformKeyboardContext {
        numeric_active: true,
        ..transform_context(Tool::Move, None)
    };
    assert!(
        transform_frame(
            &Context::default(),
            context,
            vec![typed_key(Key::A, true)],
            Vec::new(),
            |_| {},
        )
        .is_empty()
    );
}

#[test]
fn notification_shortcuts_run_once_per_frame_and_a_late_numeric_owner_suppresses_them() {
    for (key, late_numeric_owner) in [Key::A, Key::F6]
        .into_iter()
        .flat_map(|key| [false, true].map(|late_numeric_owner| (key, late_numeric_owner)))
    {
        let ctx = Context::default();
        let mut shortcuts = ShortcutFrame::new(&ctx);
        ctx.run_ui(
            egui::RawInput {
                events: vec![typed_key(key, true)],
                focused: true,
                ..Default::default()
            },
            |ui| {
                let ctx = ui.ctx();
                shortcuts.begin_pass(ctx);
                if ctx.current_pass_index() == 0 {
                    ctx.request_discard("Check shortcut hint during layout retry");
                }
                let transform = if late_numeric_owner && ctx.current_pass_index() != 0 {
                    transform_context(Tool::Move, Some(0))
                } else {
                    TransformKeyboardContext::default()
                };
                shortcuts.collect_with_transform(ctx, transform);
            },
        )
        .textures_delta
        .clear();
        let commands = shortcuts.commands();
        if late_numeric_owner {
            assert!(commands.is_empty());
        } else {
            assert!(matches!(
                commands.as_slice(),
                [Command::ShortcutHint { .. } | Command::FocusToasts]
            ));
        }
    }
}

#[test]
fn numeric_transform_predicts_tool_axis_and_digits_in_order_without_text_echoes() {
    let ctx = Context::default();
    let commands = transform_frame(
        &ctx,
        transform_context(Tool::View, None),
        vec![
            typed_key(Key::R, true),
            typed_key(Key::Z, true),
            typed_key(Key::Num9, true),
            Event::Text("9".into()),
            typed_key(Key::Num0, true),
            Event::Text("0".into()),
        ],
        vec![
            NumberKeyEvent {
                event_index: 2,
                ..number(NumberKey::TopRow(9), Modifiers::NONE)
            },
            NumberKeyEvent {
                event_index: 4,
                ..number(NumberKey::TopRow(0), Modifiers::NONE)
            },
        ],
        |_| {},
    );
    assert!(matches!(
        commands.as_slice(),
        [
            Command::Tool(Tool::Rotate),
            Command::ToggleTransformAxis(2),
            Command::TransformCharacter('9'),
            Command::TransformCharacter('0')
        ]
    ));
}

#[test]
fn numeric_typing_accepts_text_only_batches_negative_decimal_and_key_repeats() {
    let ctx = Context::default();
    let context = transform_context(Tool::Move, Some(0));
    let commands = transform_frame(
        &ctx,
        context,
        vec![Event::Text("-0.25".into())],
        Vec::new(),
        |_| {},
    );
    assert!(matches!(
        commands.as_slice(),
        [
            Command::TransformCharacter('-'),
            Command::TransformCharacter('0'),
            Command::TransformCharacter('.'),
            Command::TransformCharacter('2'),
            Command::TransformCharacter('5')
        ]
    ));
    for _ in 0..2 {
        let commands = transform_frame(
            &ctx,
            context,
            vec![typed_key(Key::Num9, true), Event::Text("9".into())],
            Vec::new(),
            |_| {},
        );
        assert!(matches!(
            commands.as_slice(),
            [Command::TransformCharacter('9')]
        ));
    }
    let commands = transform_frame(
        &ctx,
        context,
        vec![
            typed_key(Key::Minus, true),
            Event::Text("-".into()),
            typed_key(Key::Period, true),
            Event::Text(".".into()),
        ],
        Vec::new(),
        |_| {},
    );
    assert!(matches!(
        commands.as_slice(),
        [
            Command::TransformCharacter('-'),
            Command::TransformCharacter('.')
        ]
    ));
}

#[test]
fn typed_decimal_release_cannot_toggle_planar_after_accept_or_cancel() {
    for finish in [Key::Enter, Key::Escape] {
        let ctx = Context::default();
        let commands = transform_frame(
            &ctx,
            transform_context(Tool::Move, Some(0)),
            vec![
                typed_key(Key::Period, true),
                Event::Text(".".into()),
                typed_key(finish, true),
            ],
            Vec::new(),
            |_| {},
        );
        assert!(matches!(
            commands.first(),
            Some(Command::TransformCharacter('.'))
        ));
        assert_eq!(commands.len(), 2);
        let commands = transform_frame(
            &ctx,
            transform_context(Tool::Move, None),
            vec![typed_key(Key::Period, false)],
            Vec::new(),
            |_| {},
        );
        assert!(commands.is_empty());
    }
}

#[test]
fn armed_axis_owns_delete_and_backspace_even_before_first_character() {
    let ctx = Context::default();
    let commands = transform_frame(
        &ctx,
        transform_context(Tool::Scale, Some(1)),
        vec![
            typed_key(Key::Backspace, true),
            typed_key(Key::Delete, true),
        ],
        Vec::new(),
        |_| {},
    );
    assert!(matches!(
        commands.as_slice(),
        [Command::TransformBackspace, Command::TransformBackspace]
    ));
    let ctx = Context::default();
    let commands = transform_frame(
        &ctx,
        transform_context(Tool::Scale, None),
        vec![typed_key(Key::Backspace, true)],
        Vec::new(),
        |_| {},
    );
    assert!(matches!(commands.as_slice(), [Command::DeleteSelection]));
}

#[test]
fn numlock_off_digits_override_logical_arrows_and_delete_in_numeric_context() {
    let ctx = Context::default();
    let commands = transform_frame(
        &ctx,
        transform_context(Tool::Rotate, Some(2)),
        vec![
            typed_key(Key::ArrowLeft, true),
            typed_key(Key::Delete, true),
        ],
        vec![
            NumberKeyEvent {
                event_index: 0,
                ..number(NumberKey::Numpad(4), Modifiers::NONE)
            },
            NumberKeyEvent {
                event_index: 1,
                ..number(NumberKey::Numpad(0), Modifiers::NONE)
            },
        ],
        |_| {},
    );
    assert!(matches!(
        commands.as_slice(),
        [
            Command::TransformCharacter('4'),
            Command::TransformCharacter('0')
        ]
    ));
    let ctx = Context::default();
    let commands = transform_frame(
        &ctx,
        transform_context(Tool::Rotate, None),
        vec![typed_key(Key::ArrowLeft, true)],
        vec![number(NumberKey::Numpad(4), Modifiers::NONE)],
        |_| {},
    );
    assert!(matches!(
        commands.as_slice(),
        [Command::OrbitView {
            horizontal: -1.0,
            vertical: 0.0
        }]
    ));
}

#[test]
fn plain_a_and_d_are_unassigned_and_command_z_keeps_undo() {
    for key in [Key::A, Key::D] {
        assert!(resolve(key, Modifiers::NONE, KeyboardOwner::default()).is_none());
    }
    assert!(
        resolve_transform_key(
            Key::Z,
            Modifiers::COMMAND,
            KeyboardOwner::default(),
            transform_context(Tool::Rotate, Some(2))
        )
        .is_none()
    );
    assert!(matches!(
        resolve(Key::Z, Modifiers::COMMAND, KeyboardOwner::default()),
        Some(Command::Undo)
    ));
}

#[test]
fn numeric_input_obeys_ownership_on_both_sides_of_the_ui_pass_and_retries() {
    for ownership in 0..5 {
        let ctx = Context::default();
        if ownership == 0 {
            ctx.memory_mut(|memory| memory.request_focus(egui::Id::new("numeric-field")));
        }
        if ownership == 1 {
            egui::Popup::open_id(&ctx, egui::Id::new("numeric-popup"));
        }
        let commands = transform_frame(
            &ctx,
            transform_context(Tool::Move, Some(0)),
            vec![
                typed_key(Key::Num9, true),
                Event::Text("9".into()),
                typed_key(Key::Backspace, true),
            ],
            Vec::new(),
            |ctx| match ownership {
                0 => {
                    ctx.memory_mut(|memory| memory.surrender_focus(egui::Id::new("numeric-field")));
                }
                1 => {
                    egui::Popup::close_all(ctx);
                }
                2 => {
                    claim_viewport_input(ctx);
                }
                3 => {
                    egui::Modal::new(egui::Id::new("numeric-modal")).show(ctx, |ui| {
                        ui.label("Modal");
                    });
                }
                4 if ctx.current_pass_index() == 0 => {
                    ctx.request_discard("Late numeric ownership");
                }
                4 => {
                    ctx.memory_mut(|memory| {
                        memory.request_focus(egui::Id::new("late-numeric-field"))
                    });
                }
                _ => {}
            },
        );
        assert!(
            commands.is_empty(),
            "ownership case {ownership}: {commands:?}"
        );
    }
}

#[test]
fn numeric_input_runs_once_across_layout_retries() {
    let ctx = Context::default();
    let commands = transform_frame(
        &ctx,
        transform_context(Tool::Move, None),
        vec![
            typed_key(Key::X, true),
            typed_key(Key::Num9, true),
            Event::Text("9".into()),
            typed_key(Key::Num0, true),
            Event::Text("0".into()),
        ],
        Vec::new(),
        |ctx| {
            if ctx.current_pass_index() == 0 {
                ctx.request_discard("Do not replay numeric input");
            }
        },
    );
    assert!(matches!(
        commands.as_slice(),
        [
            Command::ToggleTransformAxis(0),
            Command::TransformCharacter('9'),
            Command::TransformCharacter('0')
        ]
    ));
}

#[test]
fn numeric_text_is_never_reinterpreted_by_filtering_or_stale_key_echoes() {
    let ctx = Context::default();
    for text in ["1e3", "1+2", "2 cm", "abc9"] {
        let commands = transform_frame(
            &ctx,
            transform_context(Tool::Move, Some(0)),
            vec![Event::Text(text.into())],
            Vec::new(),
            |_| {},
        );
        assert!(
            commands.is_empty(),
            "Unsupported text {text} must remain unhandled"
        );
    }
    let ctx = Context::default();
    let commands = transform_frame(
        &ctx,
        transform_context(Tool::Move, None),
        vec![
            typed_key(Key::Num9, true),
            typed_key(Key::X, true),
            Event::Text("9".into()),
            typed_key(Key::Num0, true),
            typed_key(Key::Num0, false),
            Event::Text("0".into()),
        ],
        Vec::new(),
        |_| {},
    );
    assert!(matches!(
        commands.as_slice(),
        [
            Command::ToggleTransformAxis(0),
            Command::TransformCharacter('9'),
            Command::TransformCharacter('0'),
            Command::TransformCharacter('0')
        ]
    ));
}

#[test]
fn numeric_confirmation_stops_same_frame_fallthrough_until_actual_editor_state_is_known() {
    for finish in [Key::Enter, Key::Escape] {
        let ctx = Context::default();
        let commands = transform_frame(
            &ctx,
            transform_context(Tool::Move, Some(0)),
            vec![
                typed_key(Key::Minus, true),
                typed_key(finish, true),
                typed_key(Key::Num2, true),
                Event::Text("2".into()),
                typed_key(Key::Period, true),
                typed_key(Key::Period, false),
            ],
            vec![NumberKeyEvent {
                event_index: 2,
                ..number(NumberKey::TopRow(2), Modifiers::NONE)
            }],
            |_| {},
        );
        assert_eq!(commands.len(), 2);
        assert!(matches!(
            commands.first(),
            Some(Command::TransformCharacter('-'))
        ));
        assert!(matches!(
            commands.last(),
            Some(Command::Confirm | Command::Escape)
        ));
    }
}

#[test]
fn focus_loss_discards_the_whole_pending_transform_batch() {
    let ctx = Context::default();
    let commands = transform_frame(
        &ctx,
        transform_context(Tool::View, None),
        vec![
            typed_key(Key::R, true),
            typed_key(Key::Z, true),
            typed_key(Key::Num9, true),
            typed_key(Key::Enter, true),
            Event::WindowFocused(false),
        ],
        Vec::new(),
        |_| {},
    );
    assert!(commands.is_empty());
}

fn period(pressed: bool, repeat: bool, modifiers: Modifiers) -> Event {
    Event::Key {
        key: Key::Period,
        physical_key: Some(Key::Period),
        pressed,
        repeat,
        modifiers,
    }
}

fn tap_frame(
    ctx: &Context,
    input: egui::RawInput,
    mut during_ui: impl FnMut(&Context),
) -> Vec<Command> {
    let mut shortcuts = ShortcutFrame::new(ctx);
    ctx.run_ui(input, |root_ui| {
        let context = root_ui.ctx().clone();
        let ctx = &context;
        shortcuts.begin_pass(ctx);
        during_ui(ctx);
        shortcuts.collect(ctx);
    })
    .textures_delta
    .clear();
    shortcuts.commands()
}

#[test]
fn period_tap_toggles_once_on_release_and_rejects_bare_releases_and_repeats() {
    let ctx = Context::default();
    let run = |events| {
        tap_frame(
            &ctx,
            egui::RawInput {
                events,
                ..Default::default()
            },
            |_| {},
        )
    };
    assert!(run(vec![period(false, false, Modifiers::NONE)]).is_empty());
    assert!(run(vec![period(true, false, Modifiers::NONE)]).is_empty());
    for _ in 0..3 {
        assert!(run(vec![period(true, true, Modifiers::NONE)]).is_empty());
    }
    assert!(matches!(
        run(vec![period(false, false, Modifiers::NONE)]).as_slice(),
        [Command::TogglePlanarNavigation]
    ));
    assert!(run(vec![period(false, false, Modifiers::NONE)]).is_empty());
    // egui derives repeat flags from its own down-key set; test an orphan
    // repeat at the recognizer boundary, before that normalization.
    let mut orphan = KeyTap::default();
    assert!(!orphan.event(true, true, true));
    assert!(!orphan.event(false, false, true));
    assert!(matches!(
        run(vec![
            period(true, false, Modifiers::NONE),
            period(false, false, Modifiers::NONE),
        ])
        .as_slice(),
        [Command::TogglePlanarNavigation]
    ));
}

#[test]
fn period_binding_is_separate_from_press_commands_and_respects_ownership_and_modifiers() {
    assert!(resolve(Key::Period, Modifiers::NONE, KeyboardOwner::default()).is_none());
    for owner in [
        KeyboardOwner {
            text: true,
            ..Default::default()
        },
        KeyboardOwner {
            popup: true,
            ..Default::default()
        },
        KeyboardOwner {
            modal: true,
            ..Default::default()
        },
    ] {
        assert!(resolve_tap(Key::Period, Modifiers::NONE, owner).is_none());
    }
    for modifiers in [
        Modifiers::SHIFT,
        Modifiers::CTRL,
        Modifiers::ALT,
        Modifiers::COMMAND,
        Modifiers::MAC_CMD,
    ] {
        let ctx = Context::default();
        assert!(
            tap_frame(
                &ctx,
                egui::RawInput {
                    events: vec![period(true, false, modifiers)],
                    ..Default::default()
                },
                |_| {}
            )
            .is_empty()
        );
        assert!(
            tap_frame(
                &ctx,
                egui::RawInput {
                    events: vec![period(false, false, Modifiers::NONE)],
                    ..Default::default()
                },
                |_| {}
            )
            .is_empty(),
            "Releasing modifiers cannot adopt a modified press"
        );
    }
}

#[test]
fn period_tap_cancels_when_ownership_focus_or_modifiers_change_before_release() {
    for interruption in 0..7 {
        let ctx = Context::default();
        tap_frame(
            &ctx,
            egui::RawInput {
                events: vec![period(true, false, Modifiers::NONE)],
                ..Default::default()
            },
            |_| {},
        );
        let field = egui::Id::new("period-field");
        if interruption == 0 {
            ctx.memory_mut(|memory| memory.request_focus(field));
        } else if interruption == 1 {
            egui::Popup::open_id(&ctx, egui::Id::new("period-popup"));
        }
        assert!(
            tap_frame(
                &ctx,
                egui::RawInput {
                    focused: interruption != 3,
                    events: if interruption == 5 {
                        vec![Event::ModifiersChanged(Modifiers::SHIFT)]
                    } else if interruption == 6 {
                        vec![Event::WindowFocused(false), Event::WindowFocused(true)]
                    } else {
                        vec![]
                    },
                    ..Default::default()
                },
                |ctx| {
                    if interruption == 2 {
                        egui::Modal::new(egui::Id::new("period-modal")).show(ctx, |ui| {
                            ui.label("Modal owns this frame");
                        });
                    } else if interruption == 4 {
                        claim_viewport_input(ctx);
                    }
                }
            )
            .is_empty()
        );
        egui::Popup::close_all(&ctx);
        ctx.memory_mut(|memory| memory.surrender_focus(field));
        // Let transient UI disappear, then release in an otherwise eligible
        // viewport. Ownership must not transfer back to this old press.
        tap_frame(&ctx, egui::RawInput::default(), |_| {});
        assert!(
            tap_frame(
                &ctx,
                egui::RawInput {
                    events: vec![period(false, false, Modifiers::NONE)],
                    ..Default::default()
                },
                |_| {}
            )
            .is_empty(),
            "Interruption {interruption} must consume the tap"
        );
    }
}

#[test]
fn period_release_runs_once_across_layout_retries_and_late_capture_can_suppress_it() {
    for capture in 0..4 {
        let ctx = Context::default();
        tap_frame(
            &ctx,
            egui::RawInput {
                events: vec![period(true, false, Modifiers::NONE)],
                ..Default::default()
            },
            |_| {},
        );
        let mut passes = 0;
        let commands = tap_frame(
            &ctx,
            egui::RawInput {
                events: vec![period(false, false, Modifiers::NONE)],
                ..Default::default()
            },
            |ctx| {
                let pass = ctx.current_pass_index();
                if (capture == 1 && pass == 0) || (capture == 2 && pass == 1) {
                    claim_viewport_input(ctx);
                } else if capture == 3 && pass == 1 {
                    ctx.memory_mut(|memory| {
                        memory.request_focus(egui::Id::new("late-period-field"))
                    });
                }
                if pass == 0 {
                    ctx.request_discard("A tap belongs to the frame, not each layout pass");
                }
                passes += 1;
            },
        );
        assert_eq!(passes, 2);
        if capture == 0 {
            assert!(matches!(
                commands.as_slice(),
                [Command::TogglePlanarNavigation]
            ));
        } else {
            assert!(
                commands.is_empty(),
                "A claimed tap must not escape on release"
            );
        }
    }
}

#[test]
fn preferences_shortcut_is_global_but_respects_popup_ownership_and_modifiers() {
    for owner in [
        KeyboardOwner::default(),
        KeyboardOwner {
            text: true,
            ..Default::default()
        },
    ] {
        assert!(matches!(
            resolve(Key::Comma, Modifiers::COMMAND, owner),
            Some(Command::OpenPreferences)
        ));
        for modifiers in [
            Modifiers::NONE,
            Modifiers::COMMAND | Modifiers::SHIFT,
            Modifiers::COMMAND | Modifiers::ALT,
        ] {
            assert!(resolve(Key::Comma, modifiers, owner).is_none());
        }
    }
    for owner in [
        KeyboardOwner {
            popup: true,
            ..Default::default()
        },
        KeyboardOwner {
            modal: true,
            ..Default::default()
        },
    ] {
        assert!(resolve(Key::Comma, Modifiers::COMMAND, owner).is_none());
    }
    let ctx = Context::default();
    let mut event = typed_key(Key::Comma, true);
    if let Event::Key { modifiers, .. } = &mut event {
        *modifiers = Modifiers::COMMAND;
    }
    let commands = transform_frame(
        &ctx,
        TransformKeyboardContext::default(),
        vec![event],
        Vec::new(),
        |ctx| {
            ctx.memory_mut(|memory| {
                memory.request_focus(egui::Id::new("preferences-shortcut-field"))
            });
            if ctx.current_pass_index() == 0 {
                ctx.request_discard(
                    "Verify application shortcut survives a focused-field layout retry",
                );
            }
        },
    );
    assert!(matches!(commands.as_slice(), [Command::OpenPreferences]));
}

#[test]
fn application_bindings_survive_registered_field_and_timeline_focus_at_final_dispatch() {
    for timeline in [false, true] {
        for binding in bindings::BINDINGS
            .iter()
            .filter(|binding| binding.over_text)
        {
            let ctx = Context::default();
            let focus_id = if timeline {
                crate::input::timeline_input::timeline_focus_id()
            } else {
                egui::Id::new("registered-application-shortcut-field")
            };
            let mut text = String::new();
            let mut control = |ui: &mut egui::Ui| {
                if timeline {
                    ui.interact(
                        egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(200.0, 100.0)),
                        focus_id,
                        egui::Sense::click(),
                    )
                } else {
                    ui.add(egui::TextEdit::singleline(&mut text).id(focus_id))
                }
            };
            ctx.run_ui(egui::RawInput::default(), |ui| {
                control(ui).request_focus();
            })
            .textures_delta
            .clear();
            let mut shortcuts = ShortcutFrame::new(&ctx);
            ctx.run_ui(
                egui::RawInput {
                    events: vec![key(binding.key().unwrap(), binding.modifiers)],
                    ..Default::default()
                },
                |ui| {
                    shortcuts.begin_pass(ui.ctx());
                    control(ui);
                    shortcuts.collect(ui.ctx());
                    if ui.ctx().current_pass_index() == 0 {
                        ui.ctx()
                            .request_discard("Test registered shortcut owner through layout retry");
                    }
                },
            )
            .textures_delta
            .clear();
            assert_eq!(ctx.memory(|memory| memory.focused()), Some(focus_id));
            assert_eq!(
                shortcuts.commands(),
                vec![binding.command.unwrap()],
                "{} timeline={timeline}",
                binding.id
            );
        }
    }
}

#[test]
fn ui_toggle_is_a_global_command_without_a_plain_backslash_binding() {
    for owner in [
        KeyboardOwner {
            viewport: true,
            ..Default::default()
        },
        KeyboardOwner {
            text: true,
            ..Default::default()
        },
        KeyboardOwner::default(),
    ] {
        assert!(matches!(
            resolve(Key::Backslash, Modifiers::COMMAND, owner),
            Some(Command::ToggleUi)
        ));
        assert!(resolve(Key::Backslash, Modifiers::NONE, owner).is_none());
    }
}

#[test]
fn axis_locks_and_nudges_respect_modifiers_and_keyboard_owner() {
    let viewport = KeyboardOwner {
        viewport: true,
        ..Default::default()
    };
    for (key, axis) in [(Key::X, 0), (Key::Y, 1), (Key::Z, 2)] {
        assert!(
            matches!(resolve(key, Modifiers::NONE, viewport), Some(Command::ToggleTransformAxis(actual)) if actual == axis)
        );
        assert!(resolve(key, Modifiers::SHIFT, viewport).is_none());
    }
    assert!(matches!(
        resolve(Key::A, Modifiers::COMMAND, viewport),
        Some(Command::SelectAll)
    ));
    assert!(matches!(
        resolve(Key::S, Modifiers::COMMAND, viewport),
        Some(Command::Save { save_as: false })
    ));
    for (key, direction) in [
        (Key::ArrowUp, (0, 1)),
        (Key::ArrowDown, (0, -1)),
        (Key::ArrowLeft, (-1, 0)),
        (Key::ArrowRight, (1, 0)),
    ] {
        for modifiers in [Modifiers::NONE, Modifiers::SHIFT] {
            assert!(
                matches!(resolve(key, modifiers, viewport), Some(Command::Nudge { horizontal, vertical, fast, repeat: false }) if (horizontal, vertical) == direction && fast == modifiers.shift)
            );
            for owner in [
                KeyboardOwner::default(),
                KeyboardOwner {
                    text: true,
                    ..viewport
                },
                KeyboardOwner {
                    popup: true,
                    ..viewport
                },
                KeyboardOwner {
                    modal: true,
                    ..viewport
                },
            ] {
                assert!(resolve(key, modifiers, owner).is_none());
            }
        }
        for modifiers in [Modifiers::ALT, Modifiers::CTRL, Modifiers::COMMAND] {
            assert!(resolve(key, modifiers, viewport).is_none());
        }
    }
}

#[test]
fn viewport_retains_arrows_and_repeats_but_numpad_remains_camera_input() {
    let ctx = Context::default();
    focus_frame(&ctx, vec![], false);
    ctx.memory_mut(|memory| memory.request_focus(viewport_focus_id()));
    let (commands, _) = focus_frame(&ctx, vec![key(Key::ArrowUp, Modifiers::NONE)], false);
    assert!(matches!(
        commands.as_slice(),
        [Command::Nudge {
            vertical: 1,
            repeat: false,
            ..
        }]
    ));
    assert!(ctx.memory(|memory| memory.has_focus(viewport_focus_id())));
    let mut repeated = key(Key::ArrowUp, Modifiers::SHIFT);
    if let Event::Key { repeat, .. } = &mut repeated {
        *repeat = true;
    }
    let (commands, _) = focus_frame(&ctx, vec![repeated], false);
    assert!(matches!(
        commands.as_slice(),
        [Command::Nudge {
            vertical: 1,
            fast: true,
            repeat: true,
            ..
        }]
    ));
    let (commands, _) = focus_frame_with_numbers(
        &ctx,
        vec![key(Key::ArrowRight, Modifiers::NONE)],
        false,
        vec![number(NumberKey::Numpad(6), Modifiers::NONE)],
    );
    assert!(matches!(
        commands.as_slice(),
        [Command::OrbitView {
            horizontal: 1.0,
            vertical: 0.0
        }]
    ));
    assert!(ctx.memory(|memory| memory.has_focus(viewport_focus_id())));
}

#[test]
fn keypad_release_cannot_turn_a_physical_arrow_repeat_into_a_new_nudge() {
    let ctx = Context::default();
    focus_frame(&ctx, vec![], false);
    ctx.memory_mut(|memory| memory.request_focus(viewport_focus_id()));
    let arrow = |physical_key, pressed| Event::Key {
        key: Key::ArrowRight,
        physical_key: Some(physical_key),
        pressed,
        repeat: false,
        modifiers: Modifiers::NONE,
    };
    let mut viewer = crate::workspace_ui::WorkspaceUi::new(egui::TextureId::Managed(0));
    viewer
        .editor
        .insert(crate::document::PrimitiveKind::Cube)
        .unwrap();
    viewer.editor.set_tool(Tool::Move);
    viewer.editor.toggle_transform_axis(0).unwrap();
    viewer.camera.set_view(View::Front);
    let (commands, _) = focus_frame(&ctx, vec![arrow(Key::ArrowRight, true)], false);
    assert!(matches!(
        commands.as_slice(),
        [Command::Nudge { repeat: false, .. }]
    ));
    for command in commands {
        viewer.dispatch(command, &ctx, false);
    }
    let moved = viewer.editor.document.clone();
    assert_eq!(moved.objects[0].transform.translation, [1., 0., 0.]);

    for pressed in [true, false] {
        let (commands, _) = focus_frame_with_numbers(
            &ctx,
            vec![arrow(Key::Num6, pressed)],
            false,
            vec![NumberKeyEvent {
                pressed,
                ..number(NumberKey::Numpad(6), Modifiers::NONE)
            }],
        );
        if pressed {
            assert!(matches!(commands.as_slice(), [Command::OrbitView { .. }]));
        } else {
            assert!(
                commands.is_empty(),
                "A keypad release is not an arrow release"
            );
        }
        for command in commands {
            viewer.dispatch(command, &ctx, false);
        }
    }
    let mut shortcuts = ShortcutFrame::new(&ctx);
    let mut passes = 0;
    ctx.run_ui(
        egui::RawInput {
            events: vec![arrow(Key::ArrowRight, true)],
            ..Default::default()
        },
        |root_ui| {
            let context = root_ui.ctx().clone();
            let ctx = &context;
            if ctx.current_pass_index() == 0 {
                assert!(
                    ctx.input(|input| matches!(input.events[0], Event::Key { repeat: false, .. })),
                    "egui loses the logical hold when the overlapping keypad key is released"
                );
                ctx.request_discard("Physical arrow provenance survives layout retries");
            }
            shortcuts.begin_pass(ctx);
            egui::CentralPanel::default().show(root_ui, |ui| {
                let response = ui.interact(
                    ui.available_rect_before_wrap(),
                    viewport_focus_id(),
                    egui::Sense::click_and_drag(),
                );
                viewport_interaction(ui, &response, false);
            });
            shortcuts.collect(ctx);
            passes += 1;
        },
    )
    .textures_delta
    .clear();
    assert_eq!(passes, 2);
    let commands = shortcuts.commands();
    assert!(matches!(
        commands.as_slice(),
        [Command::Nudge { repeat: true, .. }]
    ));
    for command in commands {
        viewer.dispatch(command, &ctx, false);
    }
    assert_eq!(
        viewer.editor.document, moved,
        "The camera command ended nudge ownership; its old hold cannot restart movement"
    );

    let (release, _) = focus_frame(&ctx, vec![arrow(Key::ArrowRight, false)], false);
    assert!(matches!(
        release.as_slice(),
        [Command::EndNudge {
            horizontal: 1,
            vertical: 0
        }]
    ));
    let (commands, _) = focus_frame(&ctx, vec![arrow(Key::ArrowRight, true)], false);
    assert!(matches!(
        commands.as_slice(),
        [Command::Nudge { repeat: false, .. }]
    ));
    for command in commands {
        viewer.dispatch(command, &ctx, false);
    }
    assert_eq!(
        viewer.editor.document.objects[0].transform.translation,
        [2., 0., 0.]
    );
}

#[test]
fn arrow_provenance_tracks_field_owned_holds_and_resets_on_focus_loss() {
    let ctx = Context::default();
    let field_id = egui::Id::new("arrow-provenance-field");
    ctx.memory_mut(|memory| memory.request_focus(field_id));
    let mut field = String::from("123");
    let mut shortcuts = ShortcutFrame::new(&ctx);
    ctx.run_ui(
        egui::RawInput {
            events: vec![key(Key::ArrowRight, Modifiers::NONE)],
            ..Default::default()
        },
        |root_ui| {
            let context = root_ui.ctx().clone();
            let ctx = &context;
            shortcuts.begin_pass(ctx);
            egui::CentralPanel::default().show(root_ui, |ui| {
                ui.add(egui::TextEdit::singleline(&mut field).id(field_id));
            });
            shortcuts.collect(ctx);
        },
    )
    .textures_delta
    .clear();
    assert!(shortcuts.commands().is_empty());
    ctx.memory_mut(|memory| memory.request_focus(viewport_focus_id()));
    let (commands, _) = focus_frame(&ctx, vec![key(Key::ArrowRight, Modifiers::NONE)], false);
    assert!(
        matches!(commands.as_slice(), [Command::Nudge { repeat: true, .. }]),
        "Moving focus cannot turn a field-owned hold into a new viewport press"
    );

    let (commands, _) = focus_frame(
        &ctx,
        vec![
            Event::WindowFocused(false),
            Event::WindowFocused(true),
            key(Key::ArrowRight, Modifiers::NONE),
        ],
        false,
    );
    assert!(
        matches!(commands.as_slice(), [Command::Nudge { repeat: false, .. }]),
        "An ordered loss/return resets provenance even when final RawInput is focused"
    );
    let mut shortcuts = ShortcutFrame::new(&ctx);
    ctx.run_ui(
        egui::RawInput {
            focused: false,
            ..Default::default()
        },
        |root_ui| {
            let context = root_ui.ctx().clone();
            let ctx = &context;
            shortcuts.begin_pass(ctx);
            shortcuts.collect(ctx);
        },
    )
    .textures_delta
    .clear();
    assert!(shortcuts.commands().is_empty());
    ctx.memory_mut(|memory| memory.request_focus(viewport_focus_id()));
    let (commands, _) = focus_frame(&ctx, vec![key(Key::ArrowRight, Modifiers::NONE)], false);
    assert!(
        matches!(commands.as_slice(), [Command::Nudge { repeat: false, .. }]),
        "RawInput focus loss without a separate event also clears held arrows"
    );
}

fn key(key: Key, modifiers: Modifiers) -> Event {
    Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    }
}

fn focus_frame(
    ctx: &Context,
    events: Vec<Event>,
    eligible_press: bool,
) -> (Vec<Command>, [egui::Id; 2]) {
    focus_frame_with_numbers(ctx, events, eligible_press, Vec::new())
}

fn focus_frame_with_numbers(
    ctx: &Context,
    events: Vec<Event>,
    eligible_press: bool,
    numbers: Vec<NumberKeyEvent>,
) -> (Vec<Command>, [egui::Id; 2]) {
    let mut shortcuts = ShortcutFrame::with_number_events(ctx, numbers);
    let mut buttons = [egui::Id::NULL; 2];
    ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(500.0, 300.0),
            )),
            events,
            ..Default::default()
        },
        |root_ui| {
            let context = root_ui.ctx().clone();
            let ctx = &context;
            shortcuts.begin_pass(ctx);
            egui::CentralPanel::default().show(root_ui, |ui| {
                buttons[0] = ui.button("First control").id;
                buttons[1] = ui.button("Second control").id;
                let response = ui.interact(
                    egui::Rect::from_min_max(egui::pos2(10.0, 80.0), egui::pos2(490.0, 290.0)),
                    viewport_focus_id(),
                    egui::Sense::click_and_drag(),
                );
                viewport_interaction(ui, &response, eligible_press);
            });
            shortcuts.collect(ctx);
        },
    )
    .textures_delta
    .clear();
    (shortcuts.commands(), buttons)
}

fn number(key: NumberKey, modifiers: Modifiers) -> NumberKeyEvent {
    NumberKeyEvent {
        event_index: 0,
        key,
        pressed: true,
        repeat: false,
        modifiers,
    }
}

#[test]
fn transient_capture_blocks_shortcuts_and_focus_traversal_through_layout_retries() {
    let ctx = Context::default();
    let command = Modifiers {
        command: true,
        mac_cmd: true,
        ..Modifiers::NONE
    };
    let mut shortcuts = ShortcutFrame::with_number_events(
        &ctx,
        vec![NumberKeyEvent {
            event_index: 9,
            ..number(NumberKey::TopRow(2), Modifiers::NONE)
        }],
    );
    let mut passes = 0;
    ctx.run_ui(
        egui::RawInput {
            events: vec![
                key(Key::Delete, Modifiers::NONE),
                key(Key::Backspace, Modifiers::NONE),
                key(Key::N, command),
                key(Key::O, command),
                key(Key::S, command),
                key(Key::Q, command),
                key(Key::A, command),
                key(Key::Z, command),
                key(Key::Escape, Modifiers::NONE),
                key(Key::Num2, Modifiers::NONE),
                key(Key::Tab, Modifiers::NONE),
                key(Key::ArrowDown, Modifiers::NONE),
            ],
            ..Default::default()
        },
        |root_ui| {
            let context = root_ui.ctx().clone();
            let ctx = &context;
            shortcuts.begin_pass(ctx);
            if ctx.current_pass_index() == 0 {
                // The owning gesture may close later in this pass. Its
                // claim must persist without being renewed on a retry.
                claim_viewport_input(ctx);
                ctx.request_discard("Exercise transient capture across layout retry");
            }
            assert!(viewport_input_claimed(ctx));
            assert!(viewport_keys_available(ctx));
            egui::CentralPanel::default().show(root_ui, |ui| {
                ui.button("Another focus target").clicked();
                let response = ui.interact(
                    ui.available_rect_before_wrap(),
                    viewport_focus_id(),
                    egui::Sense::click_and_drag(),
                );
                viewport_interaction(ui, &response, false);
            });
            shortcuts.collect(ctx);
            assert_eq!(
                ctx.memory(|memory| memory.focused()),
                Some(viewport_focus_id())
            );
            passes += 1;
        },
    )
    .textures_delta
    .clear();
    assert_eq!(passes, 2);
    assert!(shortcuts.commands().is_empty());
    assert!(!viewport_input_claimed(&ctx));
    assert_eq!(
        ctx.memory(|memory| memory.focused()),
        Some(viewport_focus_id())
    );

    let (commands, _) = focus_frame(&ctx, vec![key(Key::W, Modifiers::NONE)], false);
    assert!(matches!(commands.as_slice(), [Command::Tool(Tool::Move)]));
}

#[test]
fn transient_capture_on_retry_discards_already_collected_commands() {
    let ctx = Context::default();
    let mut shortcuts = ShortcutFrame::new(&ctx);
    ctx.run_ui(
        egui::RawInput {
            events: vec![key(Key::Delete, Modifiers::NONE)],
            ..Default::default()
        },
        |root_ui| {
            let context = root_ui.ctx().clone();
            let ctx = &context;
            shortcuts.begin_pass(ctx);
            if ctx.current_pass_index() == 0 {
                shortcuts.collect(ctx);
                assert!(matches!(
                    shortcuts.commands.as_slice(),
                    [Command::DeleteSelection]
                ));
                ctx.request_discard("Capture begins on the extra layout pass");
            } else {
                claim_viewport_input(ctx);
                shortcuts.collect(ctx);
            }
        },
    )
    .textures_delta
    .clear();
    assert!(shortcuts.commands().is_empty());
    assert!(!viewport_input_claimed(&ctx));
}

#[test]
fn physical_number_maps_are_explicit_and_respect_modifiers_and_ownership() {
    let owner = KeyboardOwner::default();
    for (key, expected) in [
        (NumberKey::TopRow(1), View::Perspective),
        (NumberKey::TopRow(2), View::Front),
        (NumberKey::TopRow(3), View::Right),
        (NumberKey::TopRow(4), View::Back),
        (NumberKey::TopRow(5), View::Left),
        (NumberKey::TopRow(6), View::Top),
        (NumberKey::TopRow(7), View::Bottom),
        (NumberKey::Numpad(0), View::Perspective),
        (NumberKey::Numpad(1), View::Front),
        (NumberKey::Numpad(3), View::Right),
        (NumberKey::Numpad(7), View::Top),
    ] {
        let Some(Command::View(actual)) = resolve_number(number(key, Modifiers::NONE), owner)
        else {
            panic!("Missing view command for {key:?}");
        };
        assert_eq!(
            std::mem::discriminant(&actual),
            std::mem::discriminant(&expected)
        );
    }
    for (digit, expected) in [(2, (0., -1.)), (4, (-1., 0.)), (6, (1., 0.)), (8, (0., 1.))] {
        let Some(Command::OrbitView {
            horizontal,
            vertical,
        }) = resolve_number(number(NumberKey::Numpad(digit), Modifiers::NONE), owner)
        else {
            panic!("Missing orbit command for numpad {digit}");
        };
        assert_eq!((horizontal, vertical), expected);
    }
    assert!(matches!(
        resolve_number(number(NumberKey::TopRow(0), Modifiers::NONE), owner),
        Some(Command::ToggleProjection)
    ));
    assert!(matches!(
        resolve_number(number(NumberKey::Numpad(5), Modifiers::NONE), owner),
        Some(Command::ToggleProjection)
    ));
    assert!(matches!(
        resolve_number(number(NumberKey::TopRow(1), Modifiers::SHIFT), owner),
        Some(Command::Frame)
    ));
    assert!(matches!(
        resolve_number(number(NumberKey::TopRow(2), Modifiers::SHIFT), owner),
        Some(Command::FrameSelection)
    ));
    for digit in 0..10 {
        assert!(
            resolve_number(number(NumberKey::Numpad(digit), Modifiers::SHIFT), owner).is_none()
        );
    }
    for key in [
        NumberKey::TopRow(8),
        NumberKey::TopRow(9),
        NumberKey::Numpad(9),
    ] {
        assert!(resolve_number(number(key, Modifiers::NONE), owner).is_none());
    }
    for key in [
        NumberKey::TopRow(0),
        NumberKey::TopRow(1),
        NumberKey::Numpad(1),
        NumberKey::Numpad(4),
    ] {
        for owner in [
            KeyboardOwner {
                text: true,
                ..Default::default()
            },
            KeyboardOwner {
                popup: true,
                ..Default::default()
            },
            KeyboardOwner {
                modal: true,
                ..Default::default()
            },
        ] {
            assert!(resolve_number(number(key, Modifiers::NONE), owner).is_none());
        }
        for modifiers in [
            Modifiers::CTRL,
            Modifiers::ALT,
            Modifiers::COMMAND,
            Modifiers::MAC_CMD,
        ] {
            assert!(resolve_number(number(key, modifiers), KeyboardOwner::default()).is_none());
        }
        let mut event = number(key, Modifiers::NONE);
        event.repeat = true;
        assert!(resolve_number(event, KeyboardOwner::default()).is_none());
        event.repeat = false;
        event.pressed = false;
        assert!(resolve_number(event, KeyboardOwner::default()).is_none());
    }
    assert!(
        resolve(Key::Num3, Modifiers::NONE, owner).is_none(),
        "merged digits cannot run a second shortcut"
    );
    assert!(resolve(Key::P, Modifiers::NONE, owner).is_none());
}

#[test]
fn number_metadata_preserves_event_order_source_repeat_and_shifted_text() {
    let ctx = Context::default();
    let mut native = crate::keyboard_input::NumberKeyInput::default();
    native.record(0, NumberKey::TopRow(0), true, false, Modifiers::NONE);
    native.record(1, NumberKey::TopRow(3), true, false, Modifiers::NONE);
    native.record(2, NumberKey::Numpad(3), true, false, Modifiers::NONE);
    native.record(3, NumberKey::TopRow(2), true, false, Modifiers::SHIFT);
    let mut shortcuts = ShortcutFrame::with_number_events(&ctx, native.take());
    ctx.run_ui(
        egui::RawInput {
            events: vec![
                key(Key::Num0, Modifiers::NONE),
                key(Key::Num3, Modifiers::NONE),
                key(Key::Num3, Modifiers::NONE),
                key(Key::A, Modifiers::SHIFT),
                Event::Text("@".into()),
            ],
            ..Default::default()
        },
        |root_ui| {
            let context = root_ui.ctx().clone();
            let ctx = &context;
            shortcuts.begin_pass(ctx);
            if ctx.current_pass_index() == 0 {
                assert!(
                    ctx.input(|input| matches!(input.events[2], Event::Key { repeat: true, .. })),
                    "egui merges the two logical 3 keys"
                );
                // Simulate a UI consumer removing a preceding event: source
                // indices belong to the immutable first-pass snapshot.
                ctx.input_mut(|input| {
                    input.consume_key(Modifiers::NONE, Key::Num0);
                });
                assert!(ctx.input(|input| input.events.contains(&Event::Text("@".into()))));
                ctx.request_discard("Verify source metadata is not replayed on layout retry");
            }
            shortcuts.collect(ctx);
        },
    )
    .textures_delta
    .clear();
    assert!(matches!(
        shortcuts.commands().as_slice(),
        [
            Command::ToggleProjection,
            Command::View(View::Right),
            Command::View(View::Right),
            Command::FrameSelection
        ]
    ));
}

#[test]
fn numlock_arrow_stays_a_view_command_and_inactive_window_ignores_sources() {
    let ctx = Context::default();
    focus_frame(&ctx, vec![], false);
    ctx.memory_mut(|memory| memory.request_focus(viewport_focus_id()));
    let (commands, _) = focus_frame_with_numbers(
        &ctx,
        vec![key(Key::ArrowLeft, Modifiers::NONE)],
        false,
        vec![number(NumberKey::Numpad(4), Modifiers::NONE)],
    );
    assert!(matches!(
        commands.as_slice(),
        [Command::OrbitView {
            horizontal: -1.0,
            vertical: 0.0
        }]
    ));
    assert!(ctx.memory(|memory| memory.has_focus(viewport_focus_id())));

    let mut shortcuts = ShortcutFrame::with_number_events(
        &ctx,
        vec![number(NumberKey::TopRow(1), Modifiers::NONE)],
    );
    ctx.run_ui(
        egui::RawInput {
            focused: false,
            events: vec![key(Key::Num1, Modifiers::NONE)],
            ..Default::default()
        },
        |root_ui| {
            let context = root_ui.ctx().clone();
            let ctx = &context;
            shortcuts.begin_pass(ctx);
            shortcuts.collect(ctx);
        },
    )
    .textures_delta
    .clear();
    assert!(shortcuts.commands().is_empty());
}

#[test]
fn held_key_ownership_gate_is_read_only_and_blocks_ui_and_inactive_windows() {
    let ctx = Context::default();
    focus_frame(&ctx, vec![], false);
    assert!(viewport_keys_available(&ctx));
    assert_eq!(ctx.memory(|memory| memory.focused()), None);
    ctx.memory_mut(|memory| memory.request_focus(viewport_focus_id()));
    assert!(viewport_keys_available(&ctx));
    assert_eq!(
        ctx.memory(|memory| memory.focused()),
        Some(viewport_focus_id())
    );

    let field = egui::Id::new("ownership-test-field");
    ctx.memory_mut(|memory| memory.request_focus(field));
    let before_widgets = viewport_keys_available(&ctx);
    assert!(!before_widgets);
    assert_eq!(ctx.memory(|memory| memory.focused()), Some(field));
    ctx.memory_mut(|memory| memory.surrender_focus(field));
    assert!(viewport_keys_available(&ctx));
    assert!(!(before_widgets && viewport_keys_available(&ctx)));

    egui::Popup::open_id(&ctx, egui::Id::new("ownership-test-popup"));
    assert!(!viewport_keys_available(&ctx));
    egui::Popup::close_all(&ctx);
    ctx.run_ui(
        egui::RawInput {
            focused: false,
            ..Default::default()
        },
        |_| {},
    )
    .textures_delta
    .clear();
    assert!(!viewport_keys_available(&ctx));

    ctx.run_ui(egui::RawInput::default(), |root_ui| {
        let context = root_ui.ctx().clone();
        let ctx = &context;
        egui::Modal::new(egui::Id::new("ownership-test-modal")).show(ctx, |ui| {
            ui.label("Modal owns the keyboard");
        });
    })
    .textures_delta
    .clear();
    assert!(!viewport_keys_available(&ctx));
}

#[test]
fn space_and_repeats_stay_out_of_one_shot_commands_and_pointer_clicks() {
    let ctx = Context::default();
    focus_frame(&ctx, vec![], false);
    ctx.memory_mut(|memory| memory.request_focus(viewport_focus_id()));
    for repeat in [false, true] {
        let mut space = key(Key::Space, Modifiers::NONE);
        if let Event::Key {
            repeat: repeated, ..
        } = &mut space
        {
            *repeated = repeat;
        }
        let (commands, _) = focus_frame(&ctx, vec![space], false);
        assert!(commands.is_empty());
        assert!(viewport_keys_available(&ctx));
        let response = ctx.read_response(viewport_focus_id()).unwrap();
        assert!(
            response.clicked(),
            "egui offers keyboard activation to focused widgets"
        );
        assert!(!response.clicked_by(egui::PointerButton::Primary));
        assert_eq!(
            ctx.memory(|memory| memory.focused()),
            Some(viewport_focus_id())
        );
    }
}

#[test]
fn duplicate_uses_command_d_without_stealing_axis_lock_or_ui_keys() {
    let owner = KeyboardOwner::default();
    for command in [
        Modifiers::COMMAND,
        Modifiers {
            mac_cmd: true,
            ..Modifiers::NONE
        },
    ] {
        assert!(matches!(
            resolve(Key::D, command, owner),
            Some(Command::DuplicateSelection)
        ));
        for modifier in [Modifiers::SHIFT, Modifiers::ALT] {
            assert!(resolve(Key::D, command | modifier, owner).is_none());
        }
        for owner in [
            KeyboardOwner {
                text: true,
                ..Default::default()
            },
            KeyboardOwner {
                popup: true,
                ..Default::default()
            },
            KeyboardOwner {
                modal: true,
                ..Default::default()
            },
        ] {
            assert!(resolve(Key::D, command, owner).is_none());
        }
    }
    assert!(resolve(Key::D, Modifiers::NONE, owner).is_none());
    assert!(resolve(Key::D, Modifiers::CTRL, owner).is_none());
}

#[test]
fn duplicate_dispatches_once_across_layout_retry_and_held_repeat() {
    let ctx = Context::default();
    let mut shortcuts = ShortcutFrame::new(&ctx);
    let mut passes = 0;
    ctx.run_ui(
        egui::RawInput {
            events: vec![key(Key::D, Modifiers::COMMAND)],
            ..Default::default()
        },
        |root_ui| {
            let context = root_ui.ctx().clone();
            let ctx = &context;
            shortcuts.begin_pass(ctx);
            if ctx.current_pass_index() == 0 {
                ctx.request_discard("Duplicate is dispatched once after layout retries");
            }
            shortcuts.collect(ctx);
            passes += 1;
        },
    )
    .textures_delta
    .clear();
    assert_eq!(passes, 2);
    assert!(matches!(
        shortcuts.commands().as_slice(),
        [Command::DuplicateSelection]
    ));
    let mut repeat = key(Key::D, Modifiers::COMMAND);
    if let Event::Key { repeat, .. } = &mut repeat {
        *repeat = true;
    }
    let (commands, _) = focus_frame(&ctx, vec![repeat], false);
    assert!(commands.is_empty());
}

#[test]
fn semantic_selection_keys_respect_ui_ownership_and_modifiers() {
    let command = Modifiers {
        command: true,
        mac_cmd: true,
        ..Modifiers::NONE
    };
    for viewport in [false, true] {
        let owner = KeyboardOwner {
            viewport,
            ..Default::default()
        };
        assert!(matches!(
            resolve(Key::A, command, owner),
            Some(Command::SelectAll)
        ));
        for key in [Key::Delete, Key::Backspace] {
            assert!(matches!(
                resolve(key, Modifiers::NONE, owner),
                Some(Command::DeleteSelection)
            ));
            for modifiers in [Modifiers::SHIFT, Modifiers::ALT, Modifiers::CTRL, command] {
                assert!(resolve(key, modifiers, owner).is_none());
            }
        }
    }
    let viewport = KeyboardOwner {
        viewport: true,
        ..Default::default()
    };
    assert!(matches!(
        resolve(Key::Tab, Modifiers::NONE, viewport),
        Some(Command::CycleSelection { reverse: false })
    ));
    assert!(matches!(
        resolve(Key::Tab, Modifiers::SHIFT, viewport),
        Some(Command::CycleSelection { reverse: true })
    ));
    for owner in [
        KeyboardOwner {
            text: true,
            viewport: true,
            ..Default::default()
        },
        KeyboardOwner {
            popup: true,
            viewport: true,
            ..Default::default()
        },
        KeyboardOwner {
            modal: true,
            viewport: true,
            ..Default::default()
        },
    ] {
        assert!(resolve(Key::A, command, owner).is_none());
        assert!(resolve(Key::Delete, Modifiers::NONE, owner).is_none());
        assert!(resolve(Key::Tab, Modifiers::NONE, owner).is_none());
    }
    assert!(!viewport.include(KeyboardOwner::default()).viewport);
    assert!(
        viewport
            .include(KeyboardOwner {
                text: true,
                ..Default::default()
            })
            .text
    );
}

#[test]
fn first_tab_after_viewport_press_is_claimed_without_a_settling_frame() {
    let ctx = Context::default();
    focus_frame(&ctx, vec![], false);
    let pos = egui::pos2(250.0, 180.0);
    focus_frame(
        &ctx,
        vec![
            Event::PointerMoved(pos),
            Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
        ],
        true,
    );
    assert!(ctx.memory(|memory| memory.has_focus(viewport_focus_id())));
    let (commands, _) = focus_frame(&ctx, vec![key(Key::Tab, Modifiers::NONE)], false);
    assert!(matches!(
        commands.as_slice(),
        [Command::CycleSelection { reverse: false }]
    ));
    assert!(ctx.memory(|memory| memory.has_focus(viewport_focus_id())));
    let mut released = key(Key::Tab, Modifiers::NONE);
    if let Event::Key { pressed, .. } = &mut released {
        *pressed = false;
    }
    let (commands, _) = focus_frame(&ctx, vec![released, key(Key::Tab, Modifiers::SHIFT)], false);
    assert!(matches!(
        commands.as_slice(),
        [Command::CycleSelection { reverse: true }]
    ));
    assert!(ctx.memory(|memory| memory.has_focus(viewport_focus_id())));
    let mut repeated = key(Key::Tab, Modifiers::NONE);
    if let Event::Key { repeat, .. } = &mut repeated {
        *repeat = true;
    }
    assert!(focus_frame(&ctx, vec![repeated], false).0.is_empty());
}

#[test]
fn focused_controls_keep_normal_tab_and_viewport_hover_does_not_claim_it() {
    let ctx = Context::default();
    let (_, buttons) = focus_frame(&ctx, vec![], false);
    ctx.memory_mut(|memory| memory.request_focus(buttons[0]));
    let (commands, _) = focus_frame(
        &ctx,
        vec![Event::PointerMoved(egui::pos2(250.0, 180.0))],
        false,
    );
    assert!(commands.is_empty());
    assert!(ctx.memory(|memory| memory.has_focus(buttons[0])));
    let (commands, _) = focus_frame(&ctx, vec![key(Key::Tab, Modifiers::NONE)], false);
    assert!(commands.is_empty());
    assert!(ctx.memory(|memory| memory.has_focus(buttons[1])));
}

#[test]
fn first_escape_after_viewport_press_retains_viewport_ownership() {
    let ctx = Context::default();
    focus_frame(&ctx, vec![], false);
    let pos = egui::pos2(250.0, 180.0);
    focus_frame(
        &ctx,
        vec![
            Event::PointerMoved(pos),
            Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
        ],
        true,
    );
    let (commands, _) = focus_frame(&ctx, vec![key(Key::Escape, Modifiers::NONE)], false);
    assert!(matches!(commands.as_slice(), [Command::Escape]));
    assert!(ctx.memory(|memory| memory.has_focus(viewport_focus_id())));
}

#[test]
fn ownership_and_modifiers_keep_keys_in_their_context() {
    assert!(matches!(
        resolve(Key::I, Modifiers::SHIFT, KeyboardOwner::default()),
        Some(Command::InsertMenu)
    ));
    for modifiers in [
        Modifiers::NONE,
        Modifiers::SHIFT | Modifiers::CTRL,
        Modifiers::SHIFT | Modifiers::ALT,
        Modifiers::SHIFT | Modifiers::COMMAND,
    ] {
        assert!(resolve(Key::I, modifiers, KeyboardOwner::default()).is_none());
    }
    assert_eq!(
        resolve(Key::F, Modifiers::NONE, KeyboardOwner::default()),
        Some(Command::MakeFace)
    );
    for modifiers in [
        Modifiers::SHIFT,
        Modifiers::COMMAND,
        Modifiers::ALT,
        Modifiers::CTRL,
    ] {
        assert!(resolve(Key::F, modifiers, KeyboardOwner::default()).is_none());
    }
    let command = Modifiers {
        command: true,
        mac_cmd: true,
        ..Modifiers::NONE
    };
    let typing = KeyboardOwner {
        text: true,
        ..Default::default()
    };
    assert!(resolve(Key::Escape, Modifiers::NONE, typing).is_none());
    assert!(resolve(Key::W, Modifiers::NONE, typing).is_none());
    assert!(resolve(Key::I, Modifiers::SHIFT, typing).is_none());
    assert!(resolve(Key::V, Modifiers::NONE, typing).is_none());
    // The cursor alias must not intercept Paste or modified text input.
    for modifiers in [command, Modifiers::CTRL, Modifiers::ALT, Modifiers::SHIFT] {
        assert!(resolve(Key::V, modifiers, KeyboardOwner::default()).is_none());
    }
    assert!(resolve(Key::Z, command, typing).is_none());
    assert!(matches!(
        resolve(Key::S, command, typing),
        Some(Command::Save { save_as: false })
    ));
    for owner in [
        KeyboardOwner {
            popup: true,
            ..Default::default()
        },
        KeyboardOwner {
            modal: true,
            ..Default::default()
        },
    ] {
        assert!(resolve(Key::Escape, Modifiers::NONE, owner).is_none());
        assert!(resolve(Key::Z, command, owner).is_none());
        assert!(resolve(Key::V, Modifiers::NONE, owner).is_none());
        assert!(resolve(Key::I, Modifiers::SHIFT, owner).is_none());
    }
    assert!(resolve(Key::W, Modifiers::ALT, KeyboardOwner::default()).is_none());
    assert!(resolve(Key::W, Modifiers::CTRL, KeyboardOwner::default()).is_none());
    assert!(resolve(Key::Tab, Modifiers::NONE, KeyboardOwner::default()).is_none());
    assert!(matches!(
        resolve(Key::Enter, Modifiers::SHIFT, KeyboardOwner::default()),
        Some(Command::LeaveEdit)
    ));
    assert!(matches!(
        resolve(Key::Enter, Modifiers::NONE, KeyboardOwner::default()),
        Some(Command::Confirm)
    ));
    assert!(matches!(
        resolve(Key::Q, command, KeyboardOwner::default()),
        Some(Command::Quit)
    ));
    for key in [Key::Q, Key::V] {
        assert!(matches!(
            resolve(key, Modifiers::NONE, KeyboardOwner::default()),
            Some(Command::Tool(Tool::View))
        ));
    }
    assert!(matches!(
        resolve(Key::R, Modifiers::NONE, KeyboardOwner::default()),
        Some(Command::Tool(Tool::Rotate))
    ));
    assert!(matches!(
        resolve(Key::E, Modifiers::NONE, KeyboardOwner::default()),
        Some(Command::Tool(Tool::Scale))
    ));
}

#[test]
fn local_view_slash_resolves_once_and_respects_keyboard_ownership() {
    assert!(matches!(
        resolve(Key::Slash, Modifiers::NONE, KeyboardOwner::default()),
        Some(Command::ToggleLocalView)
    ));
    for modifiers in [
        Modifiers::SHIFT,
        Modifiers::CTRL,
        Modifiers::ALT,
        Modifiers::COMMAND,
    ] {
        assert!(resolve(Key::Slash, modifiers, KeyboardOwner::default()).is_none());
    }
    for owner in [
        KeyboardOwner {
            text: true,
            ..Default::default()
        },
        KeyboardOwner {
            popup: true,
            ..Default::default()
        },
        KeyboardOwner {
            modal: true,
            ..Default::default()
        },
    ] {
        assert!(resolve(Key::Slash, Modifiers::NONE, owner).is_none());
    }

    let ctx = Context::default();
    let mut repeated = key(Key::Slash, Modifiers::NONE);
    if let Event::Key { repeat, .. } = &mut repeated {
        *repeat = true;
    }
    let commands = transform_frame(
        &ctx,
        TransformKeyboardContext::default(),
        vec![
            key(Key::Slash, Modifiers::NONE),
            repeated,
            Event::Text("/".into()),
        ],
        Vec::new(),
        |_| {},
    );
    assert!(matches!(commands.as_slice(), [Command::ToggleLocalView]));

    let commands = transform_frame(
        &ctx,
        TransformKeyboardContext::default(),
        vec![key(Key::Slash, Modifiers::NONE)],
        Vec::new(),
        claim_viewport_input,
    );
    assert!(commands.is_empty(), "an active gesture owns its frame");
}

#[test]
fn documented_bindings_dispatch_their_registered_commands_and_respect_ownership() {
    use crate::input::bindings::{BINDINGS, BindingInput, Trigger};
    let viewport = KeyboardOwner {
        viewport: true,
        ..Default::default()
    };
    for binding in BINDINGS {
        if binding.scope == bindings::Scope::Timeline {
            assert_eq!(
                resolve(binding.key().unwrap(), binding.modifiers, viewport),
                None
            );
            continue;
        }
        let actual = match (binding.trigger, binding.input) {
            (Trigger::Press, BindingInput::Key(key)) => resolve(key, binding.modifiers, viewport),
            (Trigger::Press, BindingInput::Number(key)) => {
                resolve_number(number(key, binding.modifiers), viewport)
            }
            (Trigger::Tap, BindingInput::Key(key)) => resolve_tap(key, binding.modifiers, viewport),
            (Trigger::Hold, _) => continue,
            _ => panic!("unsupported binding {}", binding.id),
        };
        assert_eq!(
            format!("{actual:?}"),
            format!("{:?}", binding.command),
            "{}",
            binding.id
        );
        for owner in [
            KeyboardOwner {
                popup: true,
                ..viewport
            },
            KeyboardOwner {
                modal: true,
                ..viewport
            },
            KeyboardOwner {
                text: true,
                ..viewport
            },
        ] {
            let blocked = match (binding.trigger, binding.input) {
                (Trigger::Press, BindingInput::Key(key)) => resolve(key, binding.modifiers, owner),
                (Trigger::Press, BindingInput::Number(key)) => {
                    resolve_number(number(key, binding.modifiers), owner)
                }
                (Trigger::Tap, BindingInput::Key(key)) => {
                    resolve_tap(key, binding.modifiers, owner)
                }
                _ => unreachable!(),
            };
            assert_eq!(
                blocked.is_some(),
                owner.text && binding.over_text,
                "{} with {owner:?}",
                binding.id
            );
        }
    }
}

#[test]
fn canonical_cursor_aliases_are_equivalent_without_reclaiming_text_input() {
    let owner = KeyboardOwner::default();
    for id in ["tool.cursor", "tool.cursor-alternate"] {
        let binding = bindings::required(id);
        let key = binding.key().unwrap();
        assert!(matches!(
            resolve(key, binding.modifiers, owner),
            Some(Command::Tool(Tool::View))
        ));
        assert!(
            resolve(
                key,
                binding.modifiers,
                KeyboardOwner {
                    text: true,
                    ..owner
                }
            )
            .is_none()
        );
    }
}

#[test]
fn blender_transform_aliases_use_the_same_commands_without_reclaiming_ui_keys() {
    let owner = KeyboardOwner::default();
    for (primary, alternate, tool) in [
        ("tool.move", "tool.move-alternate", Tool::Move),
        ("tool.scale", "tool.scale-alternate", Tool::Scale),
    ] {
        for id in [primary, alternate] {
            let binding = bindings::required(id);
            let key = binding.key().unwrap();
            assert!(matches!(
                resolve(key, binding.modifiers, owner),
                Some(Command::Tool(actual)) if actual == tool
            ));
            for blocked in [
                KeyboardOwner {
                    text: true,
                    ..owner
                },
                KeyboardOwner {
                    popup: true,
                    ..owner
                },
                KeyboardOwner {
                    modal: true,
                    ..owner
                },
            ] {
                assert!(resolve(key, binding.modifiers, blocked).is_none());
            }
            for modifiers in [Modifiers::SHIFT, Modifiers::ALT, Modifiers::CTRL] {
                assert!(resolve(key, modifiers, owner).is_none());
            }
        }
    }
    assert!(matches!(
        resolve(Key::R, Modifiers::NONE, owner),
        Some(Command::Tool(Tool::Rotate))
    ));
    assert!(matches!(
        resolve(Key::S, Modifiers::MAC_CMD, owner),
        Some(Command::Save { save_as: false })
    ));
}
