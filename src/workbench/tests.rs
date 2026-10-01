//! The component host uses real controls and input, with deterministic fixture effects.
use super::{replay::Replay, *};
use std::time::Duration;

fn frame(replay: &mut Replay, events: Vec<egui::Event>) {
    // Every interaction is exercised through an additional egui layout pass.
    replay.retry = true;
    let mut output = replay.frame(events, Duration::from_millis(16)).unwrap();
    output.textures_delta.clear();
}

fn settle(replay: &mut Replay) {
    for _ in 0..3 {
        frame(replay, Vec::new());
    }
}

fn fixture(case: Case) -> Replay {
    let mut replay = Replay::new(case, ResolvedTheme::Light);
    settle(&mut replay);
    replay
}

fn tap(replay: &mut Replay, key: egui::Key) {
    frame(replay, vec![Replay::literal(key, true)]);
    frame(replay, vec![Replay::literal(key, false)]);
    settle(replay);
}

fn click(replay: &mut Replay, instance: usize, target: &str) {
    let point = replay.target(instance, target).unwrap().rect.center();
    click_at(replay, point, egui::PointerButton::Primary);
}

fn click_at(replay: &mut Replay, point: egui::Pos2, button: egui::PointerButton) {
    frame(replay, vec![egui::Event::PointerMoved(point)]);
    frame(replay, vec![Replay::button(point, button, true)]);
    frame(replay, vec![Replay::button(point, button, false)]);
    settle(replay);
}

fn assert_focused(replay: &Replay, instance: usize, target: &str) {
    assert_eq!(
        replay.ctx.memory(|memory| memory.focused()),
        Some(replay.target(instance, target).unwrap().id),
        "Focus belongs to the actual {target} component row"
    );
}

fn open_file(replay: &mut Replay, instance: usize) {
    click(replay, instance, "menu-trigger");
    assert_focused(replay, instance, "file-submenu");
    tap(replay, egui::Key::ArrowRight);
}

fn open_view(replay: &mut Replay, instance: usize) {
    click(replay, instance, "menu-trigger");
    tap(replay, egui::Key::ArrowDown);
    assert_focused(replay, instance, "view-submenu");
    tap(replay, egui::Key::ArrowRight);
}

#[test]
fn catalog_has_stable_unique_case_names() {
    let mut ids = std::collections::BTreeSet::new();
    for case in Case::ALL {
        assert!(ids.insert(case.id()));
        assert!(!case.page().is_empty() && !case.name().is_empty());
    }
}

#[test]
fn reset_restores_fixture_state_and_stable_component_identities() {
    let mut replay = fixture(Case::MenuIsolation);
    let original: Vec<_> = (0..2)
        .map(|index| replay.target(index, "menu-trigger").unwrap().id)
        .collect();
    assert_ne!(original[0], original[1]);
    replay.bench.appearance = ResolvedTheme::Dark;
    replay.bench.dimensions = egui::vec2(800.0, 450.0);
    replay.bench.disabled = false;
    replay.bench.unavailable = false;
    replay.bench.long_label = true;
    replay.bench.fit_enabled = false;
    replay.bench.text = "fixture owner".into();
    replay.bench.instances[0].checked = true;
    replay.bench.instances[1].shading = ShadingMode::Wireframe;
    replay.bench.events.push_back(Delivered {
        instance: 0,
        command: Command::OpenPreferences,
    });
    settle(&mut replay);
    click(&mut replay, 0, "menu-trigger");
    assert!(egui::Popup::is_any_open(&replay.ctx));
    click(&mut replay, 0, "reset");
    assert!(!egui::Popup::is_any_open(&replay.ctx));
    assert!(replay.bench.disabled && replay.bench.unavailable && replay.bench.fit_enabled);
    assert!(!replay.bench.long_label);
    assert!(replay.bench.text.is_empty() && replay.bench.events.is_empty());
    assert!(replay.bench.pending.is_empty() && replay.bench.delivered_frame.is_empty());
    assert!(replay.bench.instances.iter().all(|instance| {
        !instance.checked && instance.shading == ShadingMode::Solid && !instance.pie.active()
    }));
    assert_eq!(replay.bench.appearance, ResolvedTheme::Dark);
    assert_eq!(replay.bench.dimensions, egui::vec2(800.0, 450.0));
    let reset: Vec<_> = (0..2)
        .map(|index| replay.target(index, "menu-trigger").unwrap().id)
        .collect();
    assert_eq!(reset, original);
    replay.bench.select_case(&replay.ctx, Case::RulerDensity);
    settle(&mut replay);
    assert_eq!(replay.bench.scale, 0.04);
    assert_eq!(replay.bench.origin, 1.0e8);
    replay.bench.select_case(&replay.ctx, Case::MenuIsolation);
    settle(&mut replay);
    assert_eq!(replay.target(0, "menu-trigger").unwrap().id, original[0]);
}

#[test]
fn nested_menus_skip_fixture_availability_and_deliver_once_across_layout_retries() {
    let mut replay = fixture(Case::Dropdown);
    replay.retry = true;
    open_file(&mut replay, 0);
    assert_focused(&replay, 0, ActionId::New.id());
    assert!(replay.target(0, ActionId::Import.id()).is_err());
    assert!(!replay.target(0, ActionId::Save.id()).unwrap().enabled);
    let open_id = replay.target(0, ActionId::Open.id()).unwrap().id;
    tap(&mut replay, egui::Key::ArrowDown);
    assert_focused(&replay, 0, ActionId::Open.id());
    tap(&mut replay, egui::Key::ArrowDown);
    assert_focused(&replay, 0, ActionId::SaveAs.id());
    assert!(
        replay.bench.events.is_empty(),
        "Navigation is not activation"
    );
    tap(&mut replay, egui::Key::Escape);
    assert!(egui::Popup::is_any_open(&replay.ctx));
    assert!(replay.target(0, ActionId::Open.id()).is_err());
    assert_focused(&replay, 0, "file-submenu");
    tap(&mut replay, egui::Key::Escape);
    assert!(!egui::Popup::is_any_open(&replay.ctx));
    replay.bench.unavailable = false;
    replay.bench.disabled = false;
    settle(&mut replay);
    open_file(&mut replay, 0);
    assert_eq!(replay.target(0, ActionId::Open.id()).unwrap().id, open_id);
    assert!(replay.target(0, ActionId::Import.id()).unwrap().enabled);
    assert!(replay.target(0, ActionId::Save.id()).unwrap().enabled);
    tap(&mut replay, egui::Key::ArrowDown);
    tap(&mut replay, egui::Key::Enter);
    assert!(!egui::Popup::is_any_open(&replay.ctx));
    assert_eq!(
        replay.bench.events.iter().cloned().collect::<Vec<_>>(),
        [Delivered {
            instance: 0,
            command: ActionId::Open.command()
        }]
    );
    settle(&mut replay);
    assert_eq!(replay.bench.events.len(), 1);
}

#[test]
fn context_menu_and_long_label_use_production_rows_and_preserve_text_ownership() {
    let mut replay = fixture(Case::Context);
    let point = replay.target(0, "context-surface").unwrap().rect.center();
    click_at(&mut replay, point, egui::PointerButton::Secondary);
    assert_focused(&replay, 0, ActionId::SelectAll.id());
    assert!(!replay.target(0, ActionId::Delete.id()).unwrap().enabled);
    assert!(replay.target(0, ActionId::MakeFace.id()).is_err());
    let normal = replay.target(0, ActionId::Preferences.id()).unwrap().rect;
    tap(&mut replay, egui::Key::Escape);
    replay.bench.long_label = true;
    settle(&mut replay);
    click_at(&mut replay, point, egui::PointerButton::Secondary);
    let long = replay.target(0, ActionId::Preferences.id()).unwrap().rect;
    assert!(long.width() > normal.width());
    assert!(replay.ctx.content_rect().contains_rect(long));
    tap(&mut replay, egui::Key::Escape);
    click(&mut replay, 0, "owner-field");
    assert_focused(&replay, 0, "owner-field");
    frame(&mut replay, vec![egui::Event::Text("literal input".into())]);
    frame(&mut replay, vec![Replay::key("view.pie", true)]);
    frame(&mut replay, vec![Replay::key("view.pie", false)]);
    assert_eq!(replay.bench.text, "literal input");
    assert!(!egui::Popup::is_any_open(&replay.ctx));
    assert!(replay.bench.events.is_empty());
    assert_focused(&replay, 0, "owner-field");
}

#[test]
fn menu_instances_keep_distinct_focus_and_checked_state() {
    let mut replay = fixture(Case::MenuIsolation);
    replay.retry = true;
    open_view(&mut replay, 0);
    let first_id = replay.target(0, ActionId::Edges.id()).unwrap().id;
    tap(&mut replay, egui::Key::Enter);
    assert!(replay.bench.instances[0].checked);
    assert!(!replay.bench.instances[1].checked);
    open_view(&mut replay, 1);
    let second_id = replay.target(1, ActionId::Edges.id()).unwrap().id;
    assert_ne!(first_id, second_id);
    assert_focused(&replay, 1, ActionId::Edges.id());
    tap(&mut replay, egui::Key::Enter);
    assert!(replay.bench.instances[0].checked && replay.bench.instances[1].checked);
    assert_eq!(replay.bench.events.len(), 2);
    assert_eq!(replay.bench.events[0].instance, 0);
    assert_eq!(replay.bench.events[1].instance, 1);
}

#[test]
fn held_pie_release_and_cancellation_keep_one_input_owner() {
    let mut replay = fixture(Case::PieIsolation);
    replay.retry = true;
    let right = replay.bench.instances[1].bounds;
    frame(
        &mut replay,
        vec![
            egui::Event::PointerMoved(right.center()),
            Replay::key("view.pie", true),
        ],
    );
    assert!(!replay.bench.instances[0].pie.active());
    assert!(replay.bench.instances[1].pie.view_active());
    let target = right.center() + egui::vec2(80.0, 0.0);
    frame(
        &mut replay,
        vec![
            egui::Event::PointerMoved(target),
            Replay::key("view.pie", false),
        ],
    );
    assert!(
        !replay
            .bench
            .instances
            .iter()
            .any(|instance| instance.pie.active())
    );
    assert_eq!(
        replay.bench.events.iter().cloned().collect::<Vec<_>>(),
        [Delivered {
            instance: 1,
            command: ActionId::ViewRight.command()
        }]
    );
    settle(&mut replay);
    assert_eq!(replay.bench.events.len(), 1);
    let left = replay.bench.instances[0].bounds;
    frame(
        &mut replay,
        vec![
            egui::Event::PointerMoved(left.center()),
            Replay::key("view.pie", true),
        ],
    );
    assert!(replay.bench.instances[0].pie.view_active());
    frame(&mut replay, vec![Replay::literal(egui::Key::Escape, true)]);
    frame(&mut replay, vec![Replay::key("view.pie", false)]);
    assert!(!replay.bench.instances[0].pie.active());
    assert_eq!(
        replay.bench.events.len(),
        1,
        "Cancellation does not emit a choice"
    );
}

#[test]
fn pies_respect_text_popups_focus_loss_and_resized_available_bounds() {
    let mut replay = fixture(Case::ViewPie);
    click(&mut replay, 0, "owner-field");
    let bounds = replay.bench.instances[0].bounds;
    frame(
        &mut replay,
        vec![
            egui::Event::PointerMoved(bounds.center()),
            Replay::key("view.pie", true),
        ],
    );
    assert!(!replay.bench.instances[0].pie.active());
    frame(&mut replay, vec![Replay::key("view.pie", false)]);
    click_at(&mut replay, bounds.center(), egui::PointerButton::Primary);
    frame(&mut replay, vec![Replay::key("view.pie", true)]);
    assert!(replay.bench.instances[0].pie.view_active());
    frame(&mut replay, vec![egui::Event::WindowFocused(false)]);
    frame(&mut replay, vec![Replay::key("view.pie", false)]);
    assert!(!replay.bench.instances[0].pie.active());
    assert!(replay.bench.events.is_empty());
    frame(&mut replay, vec![egui::Event::WindowFocused(true)]);
    replay.bench.dimensions = egui::vec2(200.0, 120.0);
    settle(&mut replay);
    let small = replay.bench.instances[0].bounds;
    click_at(&mut replay, small.center(), egui::PointerButton::Primary);
    frame(&mut replay, vec![Replay::key("view.pie", true)]);
    assert!(replay.bench.instances[0].pie.view_active());
    replay.bench.dimensions = egui::vec2(220.0, 140.0);
    settle(&mut replay);
    frame(&mut replay, vec![Replay::key("view.pie", false)]);
    assert!(!replay.bench.instances[0].pie.active());
    assert!(
        replay.bench.events.is_empty(),
        "Resize cancels pending choice"
    );
    click(&mut replay, 0, "owner-popup");
    let canvas = replay.bench.instances[0].bounds;
    frame(
        &mut replay,
        vec![egui::Event::PointerMoved(canvas.center())],
    );
    frame(&mut replay, vec![Replay::key("view.pie", true)]);
    frame(&mut replay, vec![Replay::key("view.pie", false)]);
    assert!(egui::Popup::is_any_open(&replay.ctx));
    assert!(!replay.bench.instances[0].pie.active());
    assert!(replay.bench.events.is_empty());
}

#[test]
fn ruler_fixture_clips_ranges_and_large_coordinate_labels_to_small_strips() {
    let mut replay = fixture(Case::RulerDensity);
    replay.bench.dimensions = egui::vec2(240.0, 160.0);
    replay.bench.range = [-400.0, 350.0];
    settle(&mut replay);
    replay.retry = true;
    let mut output = replay.frame(Vec::new(), Duration::from_millis(16)).unwrap();
    output.textures_delta.clear();
    let bounds = replay.target(0, "ruler-canvas").unwrap().rect;
    let content = ruler_2d::content_rect(bounds);
    let strips = [
        Rect::from_min_max(
            egui::pos2(content.left(), bounds.top()),
            content.right_top(),
        ),
        Rect::from_min_max(
            egui::pos2(bounds.left(), content.top()),
            content.left_bottom(),
        ),
    ];
    let selected = crate::object_feedback::SELECTED_COLOR.gamma_multiply(0.18);
    for strip in strips {
        assert!(bounds.contains_rect(strip) && strip.is_positive());
        let painted: Vec<_> = output
            .shapes
            .iter()
            .filter(|shape| shape.clip_rect == strip)
            .collect();
        assert!(
            !painted.is_empty(),
            "The production ruler paints each clipped strip"
        );
        let range = painted
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Rect(rect) if rect.fill == selected => Some(rect),
                _ => None,
            })
            .expect("An oversized selected range remains visible inside its strip");
        assert_eq!(range.rect, strip);
        assert_eq!(
            range.stroke.width, 0.0,
            "Selection fill has no added border"
        );
        let labels: Vec<_> = painted
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) => Some(&text.galley),
                _ => None,
            })
            .collect();
        assert!(
            !labels.is_empty(),
            "Large-coordinate fixtures retain readable graduations"
        );
        assert!(labels.iter().all(|label| label.size().is_finite()));
        assert!(labels.iter().any(|label| label.text().contains('e')));
    }
    assert!(replay.bench.events.is_empty());
}
