//! Imported contents stay immutable while their authored placements share the editor.
use super::{Capture, Control, HEIGHT, Session, WIDTH};
use crate::{
    document::Geometry,
    editor::{EditorAccess, Tool},
    shortcuts::{Command, HostEffect, viewport_focus_id},
};
use std::{sync::Arc, time::Duration};

fn setup(s: &mut Session<'_>) {
    s.load_scene_fixture("SimpleSkin/glTF/SimpleSkin.gltf")
        .unwrap();
    s.ctx
        .memory_mut(|memory| memory.request_focus(viewport_focus_id()));
    s.hover(Control::Viewport).unwrap();
    assert!(s.state.is_dirty());
    assert_eq!(s.state.editor.document.objects.len(), 1);
    assert!(!s.state.animation_panel_is_open());
    assert!(s.trace.get(Control::ScenePlay).is_err());
}

fn open_animation(s: &mut Session<'_>) {
    s.click(Control::AnimationPanelToggle).unwrap();
    assert!(s.state.animation_panel_is_open());
    assert!(s.trace.get(Control::ScenePlay).unwrap().enabled);
}

#[test]
fn asset_placement_uses_ordinary_edits_and_save_while_contents_remain_read_only() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    setup(&mut s);
    let document = s.state.editor.document.clone();
    let asset = s.state.selected_asset_view().unwrap().asset.clone();
    for shortcut in ["edit.confirm", "mesh.make-face"] {
        s.shortcut(shortcut).unwrap();
        assert_eq!(s.state.editor.document, document);
        assert!(!s.state.editor.edit_mode);
    }
    s.shortcut("tool.move").unwrap();
    assert_eq!(s.state.editor.tool, Tool::Move);
    s.shortcut("transform.axis-x").unwrap();
    s.key(egui::Key::Num6, true, egui::Modifiers::NONE).unwrap();
    s.key(egui::Key::Num6, false, egui::Modifiers::NONE)
        .unwrap();
    s.shortcut("edit.confirm").unwrap();
    assert_eq!(
        s.state.editor.document.objects[0].transform.translation,
        [6., 0., 0.]
    );
    assert!(matches!(
        s.state.editor.document.objects[0].geometry,
        Geometry::Asset(_)
    ));
    assert!(Arc::ptr_eq(
        &asset,
        &s.state.selected_asset_view().unwrap().asset
    ));
    s.undo().unwrap();
    assert_eq!(s.state.editor.document, document);
    s.redo().unwrap();
    let placed = s.state.editor.document.clone();
    s.shortcut("selection.duplicate").unwrap();
    assert_eq!(s.state.editor.document.objects.len(), 2);
    assert_eq!(
        s.state.asset_views.len(),
        1,
        "Duplicate placements share source resources"
    );
    s.shortcut("selection.delete").unwrap();
    assert_eq!(s.state.editor.document, placed);
    s.undo().unwrap();
    assert_eq!(s.state.editor.document.objects.len(), 2);
    assert!(Arc::ptr_eq(
        &asset,
        &s.state.selected_asset_view().unwrap().asset
    ));

    s.click_path(&[Control::N3Menu, Control::FileMenu]).unwrap();
    assert!(s.trace.get(Control::Save).unwrap().enabled);
    assert!(s.trace.get(Control::SaveAs).unwrap().enabled);
    s.click(Control::Save).unwrap();
    assert!(
        s.state.request_save,
        "Save reaches its native host request boundary"
    );
    assert_eq!(
        s.state
            .dispatch(Command::Save { save_as: true }, &s.ctx, false),
        HostEffect::None
    );
    assert!(s.state.request_save_as);
    s.shortcut("document.new").unwrap();
    assert!(
        s.state.request_new,
        "Unsaved imported placements reach the normal discard confirmation boundary"
    );
    // Supply the native confirmation outcome without opening a system dialog.
    s.state.request_new = false;
    s.state.new_document();
    s.settle().unwrap();
    assert!(s.state.asset_views.is_empty());
    assert!(s.state.editor.document.objects.is_empty());
    assert!(!s.state.is_dirty());
    s.click(Control::InsertMenu).unwrap();
    s.click(Control::InsertCube).unwrap();
    assert!(matches!(
        s.state.editor.document.objects[0].geometry,
        Geometry::Primitive(_)
    ));
}

#[test]
fn read_only_is_an_editor_capability_independent_of_imported_format() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    setup(&mut s);
    s.state.editor.set_access(EditorAccess::ReadOnly);
    s.settle().unwrap();
    let before = s.state.editor.document.clone();
    for shortcut in [
        "selection.duplicate",
        "selection.delete",
        "edit.confirm",
        "mesh.make-face",
    ] {
        s.shortcut(shortcut).unwrap();
        assert_eq!(s.state.editor.document, before, "{shortcut}");
        assert!(!s.state.editor.edit_mode);
    }
    open_animation(&mut s);
    s.click(Control::ScenePlay).unwrap();
    s.wait(Duration::from_millis(650)).unwrap();
    assert!(s.state.selected_asset_view().unwrap().playback.position > 0.6);
    assert_eq!(s.state.editor.document, before);
    s.click(Control::ScenePlay).unwrap();
    s.hover(Control::Viewport).unwrap();
    let camera = s.state.camera.view_projection(s.state.aspect());
    s.pinch(0.3).unwrap();
    assert_ne!(s.state.camera.view_projection(s.state.aspect()), camera);
}

#[test]
fn scene_playback_controls_seek_pause_and_restore_without_duplicate_layout_actions() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    setup(&mut s);
    open_animation(&mut s);
    let rest = s.state.selected_asset_view().unwrap().frame.clone();
    s.hover(Control::ScenePlay).unwrap();
    s.pointer_button(egui::PointerButton::Primary, true)
        .unwrap();
    // Retry the actual release frame, rather than consuming the retry on the
    // pointer movement before the control receives its click.
    s.extra_layout_pass = true;
    s.pointer_button(egui::PointerButton::Primary, false)
        .unwrap();
    assert!(s.state.selected_asset_view().unwrap().playback.playing);
    s.wait(Duration::from_millis(950)).unwrap();
    let playing = s.state.selected_asset_view().unwrap();
    assert!(
        playing.playback.position > 0.9,
        "Playback after 950 ms: {:?}; error: {:?}",
        playing.playback,
        s.state.error
    );
    assert_ne!(playing.frame.node_world, rest.node_world);
    assert!(!Arc::ptr_eq(&playing.frame, &rest));
    s.hover(Control::ScenePlay).unwrap();
    s.pointer_button(egui::PointerButton::Primary, true)
        .unwrap();
    s.extra_layout_pass = true;
    s.pointer_button(egui::PointerButton::Primary, false)
        .unwrap();
    let paused = s.state.selected_asset_view().unwrap().frame.clone();
    let time = s.state.selected_asset_view().unwrap().playback.position;
    assert!(!s.state.selected_asset_view().unwrap().playback.playing);
    s.wait(Duration::from_millis(400)).unwrap();
    assert_eq!(
        s.state.selected_asset_view().unwrap().playback.position,
        time
    );
    assert!(Arc::ptr_eq(
        &paused,
        &s.state.selected_asset_view().unwrap().frame
    ));

    let ruler = s.trace.get(Control::AnimationRuler).unwrap().rect;
    s.click_at(egui::pos2(
        egui::lerp(ruler.x_range(), 0.8),
        ruler.center().y,
    ))
    .unwrap();
    let seeked = s.state.selected_asset_view().unwrap();
    assert!(seeked.playback.position > time);
    assert!(!seeked.playback.playing);
    assert_ne!(seeked.frame.node_world, paused.node_world);
    s.click(Control::SceneRest).unwrap();
    let restored = s.state.selected_asset_view().unwrap();
    assert!(restored.playback.clip.is_none());
    assert_eq!(restored.playback.position, 0.0);
    assert!(!restored.playback.playing);
    assert!(Arc::ptr_eq(&restored.frame, &rest));
    assert!(
        !s.state.editor.undo(),
        "Playback must not create an Undo entry"
    );
    assert_eq!(s.state.editor.document.objects.len(), 1);
    assert!(s.state.is_dirty());
}

#[test]
fn scene_navigation_and_frame_use_imported_bounds_without_document_mutation() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    setup(&mut s);
    s.state.animate_views = false;
    s.shortcut("view.front").unwrap();
    let frame = s.state.selected_asset_view().unwrap().frame.clone();
    let pose = s.state.camera.view_projection(s.state.aspect());
    s.pinch(0.35).unwrap();
    assert_ne!(s.state.camera.view_projection(s.state.aspect()), pose);
    s.shortcut("view.frame").unwrap();
    let projection = s.state.camera.view_projection(s.state.aspect());
    let transform = &s.state.editor.frame;
    assert!(
        frame
            .draws
            .iter()
            .flat_map(|draw| &draw.vertices)
            .all(|vertex| {
                let point = transform
                    .world_to_display(glam::DVec3::from_array(vertex.position))
                    .as_vec3();
                let clip = projection * point.extend(1.0);
                let ndc = clip.truncate() / clip.w;
                clip.w > 0.0 && ndc.x.abs() <= 1.0 && ndc.y.abs() <= 1.0
            }),
        "Frame includes imported evaluated geometry in the ordinary document"
    );
    assert!(Arc::ptr_eq(
        &frame,
        &s.state.selected_asset_view().unwrap().frame
    ));
    assert_eq!(s.state.editor.document.objects.len(), 1);
    assert!(s.state.is_dirty());
}

#[test]
fn failed_parse_or_document_install_preserves_the_current_document_and_assets() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    setup(&mut s);
    let asset = s.state.selected_asset_view().unwrap().asset.clone();
    let frame = s.state.selected_asset_view().unwrap().frame.clone();
    let document = s.state.editor.document.clone();
    let path = s.state.path.clone();
    let camera = s.state.camera.view_projection(s.state.aspect());
    let resolver =
        crate::asset_io::FileResolver::new(&super::root().join("fixtures/gltf")).unwrap();
    assert!(crate::asset_io::gltf::load("broken.gltf", b"not JSON or GLB", &resolver).is_err());
    let mut invalid = document.clone();
    invalid.objects[0].transform.scale[0] = 0.;
    assert!(
        s.state
            .install_loaded_document(
                "invalid.n3.json".into(),
                crate::asset_io::LoadedDocument {
                    document: invalid,
                    assets: Default::default(),
                    diagnostics: Vec::new(),
                    saved_bytes: None,
                }
            )
            .is_err()
    );
    s.settle().unwrap();
    assert!(Arc::ptr_eq(
        &asset,
        &s.state.selected_asset_view().unwrap().asset
    ));
    assert!(Arc::ptr_eq(
        &frame,
        &s.state.selected_asset_view().unwrap().frame
    ));
    assert_eq!(s.state.editor.document, document);
    assert_eq!(s.state.path, path);
    assert_eq!(s.state.camera.view_projection(s.state.aspect()), camera);
}

#[test]
fn scene_viewer_guide_replays_and_resolves_its_live_bindings() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    super::scene_viewer::run(&mut s).unwrap();
    let page = super::artifacts::render_template(
        include_str!("../../../docs/templates/scene-viewer.md.in"),
        &s.bindings,
        &s.values,
        &s.images,
        &s.animations,
    )
    .unwrap();
    assert!(!page.contains("{{"));
    assert_eq!(s.images.len(), 2);
    assert_eq!(s.animations.len(), 1);
    assert!(!s.facts.is_empty());
}
