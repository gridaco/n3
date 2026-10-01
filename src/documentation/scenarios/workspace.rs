use super::{Result, Session, pointer};
use crate::{
    controls::Control, document::Geometry, theme, ui::lucide::Icon, workspace_ui::SavedLayout,
};
use std::time::Duration;

fn width(s: &Session<'_>) -> Result<f64> {
    let object = s
        .state
        .editor
        .document
        .objects
        .first()
        .ok_or("Missing cube")?;
    match &object.geometry {
        Geometry::Primitive(p) => Ok(p.size[0]),
        _ => Err("Cube lost its primitive geometry".into()),
    }
}

fn assert_transform_layout(s: &mut Session<'_>) -> Result<()> {
    let inspector = s.trace.get(Control::Inspector)?.rect;
    let layers = s.trace.get(Control::ObjectList)?.rect;
    let viewport = s.trace.get(Control::Viewport)?.rect;
    let menu = s.trace.get(Control::N3Menu)?.rect;
    let border = s
        .ctx
        .global_style()
        .visuals
        .widgets
        .noninteractive
        .bg_stroke
        .width
        .round();
    s.require(
        layers.right() + border == viewport.left()
            && inspector.left() - border == viewport.right()
            && menu.left() - layers.left() == theme::space::LG,
        "Panel roots reach the viewport borders while their section controls retain an inset",
    )?;
    let mut previous_bottom = f32::NEG_INFINITY;
    for (name, axes) in [
        (
            "Position",
            [Control::PositionX, Control::PositionY, Control::PositionZ],
        ),
        ("Scale", [Control::ScaleX, Control::ScaleY, Control::ScaleZ]),
        (
            "Rotation",
            [Control::RotationX, Control::RotationY, Control::RotationZ],
        ),
    ] {
        let rects = [
            s.trace.get(axes[0])?.rect,
            s.trace.get(axes[1])?.rect,
            s.trace.get(axes[2])?.rect,
        ];
        s.require(
            rects.iter().all(|rect| {
                inspector.contains_rect(*rect)
                    && rect.left() >= inspector.left() + theme::space::LG
                    && rect.right() <= inspector.right() - theme::space::LG
                    && rect.width() >= 40.0
                    && rect.height() <= 26.0
            }) && rects[0].left() < rects[1].left()
                && rects[1].left() < rects[2].left()
                && (rects[0].center().y - rects[1].center().y).abs() < 2.0
                && (rects[1].center().y - rects[2].center().y).abs() < 2.0
                && rects[0].top() > previous_bottom,
            &format!("{name} has three compact X/Y/Z fields on one row inside Properties (inspector={inspector:?}, fields={rects:?}, previous_bottom={previous_bottom})"),
        )?;
        previous_bottom = rects[0].bottom();
    }
    s.require(
        s.trace.get(Control::PrimitiveX)?.rect.top() > previous_bottom,
        "Existing primitive controls appear in Shape after Transform",
    )
}

pub fn run(s: &mut Session<'_>) -> Result<()> {
    s.require(
        s.state.editor.tool == crate::editor::Tool::View,
        "A fresh workspace starts with Cursor selected",
    )?;
    let empty_insert = s.trace.get(Control::EmptyStateInsert)?.rect;
    s.require(
        s.state.editor.document.objects.is_empty()
            && !s.state.is_planar_navigation()
            && s.state.viewport.contains_rect(empty_insert)
            && (empty_insert.center().x - s.state.viewport.center().x).abs() < 1.0,
        "An empty 3D document shows a centered Insert action",
    )?;
    s.witness(Control::EmptyStateInsert)?;
    s.capture_image("workspace-empty-3d")?;
    s.hover(Control::EmptyStateInsert)?;
    s.require(
        s.trace.get(Control::EmptyStateInsert)?.rect == empty_insert,
        "The empty-state button keeps its bounds when its hover background appears",
    )?;
    s.click(Control::EmptyStateInsert)?;
    s.require(
        s.trace.get(Control::InsertCube)?.parents == [Control::InsertMenu],
        "The empty-state action opens the existing Insert menu",
    )?;
    s.click(Control::InsertCube)?;
    s.require(
        !s.trace.controls.contains_key(&Control::EmptyStateInsert),
        "The empty-state action disappears after a shape is inserted",
    )?;
    let id = s.state.editor.selected_object.ok_or("No selected cube")?;
    assert_transform_layout(s)?;
    let saved_layout = s.state.save_layout(&s.ctx);
    s.require(
        saved_layout == SavedLayout::DOCUMENTATION,
        "The documentation layout starts with both side panels exactly 240 points wide",
    )?;
    s.value("panel-width", saved_layout.hierarchy_width);
    let left = s.trace.get(Control::ObjectList)?.rect;
    let right = s.trace.get(Control::Inspector)?.rect;
    // Use the panel side of the native resize band. Panel control rectangles
    // now describe the full unpadded content, not an inset body.
    let left_border = egui::pos2(left.right() - 1.0, left.center().y);
    s.frame(vec![egui::Event::PointerMoved(left_border)], Duration::ZERO)?;
    s.require(
        s.cursor == egui::CursorIcon::ResizeHorizontal,
        "The Layers border exposes native horizontal resizing",
    )?;
    s.drag_at(left_border, egui::vec2(65.0, 0.0))?;
    let actual_left = s.trace.get(Control::ObjectList)?.rect;
    if actual_left.width() <= left.width() + 30.0 {
        return Err(format!(
            "Layers resize failed: before={left:?} after={actual_left:?} viewport={:?}",
            s.state.viewport
        ));
    }
    let right_border = egui::pos2(right.left() + 1.0, right.center().y);
    s.frame(
        vec![egui::Event::PointerMoved(right_border)],
        Duration::ZERO,
    )?;
    s.require(
        s.cursor == egui::CursorIcon::ResizeHorizontal,
        "The Properties border exposes native horizontal resizing",
    )?;
    s.drag_at(right_border, egui::vec2(-55.0, 0.0))?;
    s.require(
        s.trace.get(Control::Inspector)?.rect.width() > right.width() + 25.0,
        "Dragging the Properties divider makes the right panel wider",
    )?;
    assert_transform_layout(s)?;
    let document = s.state.editor.document.clone();
    let selection = s.state.editor.selected_objects.clone();
    let camera = s.state.camera.view_projection(1.0);
    s.state.apply_saved_layout(&s.ctx, saved_layout);
    s.settle()?;
    s.require(
        s.state.save_layout(&s.ctx) == saved_layout
            && s.state.editor.document == document
            && s.state.editor.selected_objects == selection
            && s.state.camera.view_projection(1.0) == camera,
        "Applying a saved layout restores both resized panels without changing the document, selection, or camera",
    )?;
    assert_transform_layout(s)?;
    s.require(
        s.trace.get(Control::ViewportToolbar)?.rect.min.y > s.state.viewport.min.y
            && s.trace.get(Control::SceneInfo)?.rect.max.y < s.state.viewport.max.y
            && s.trace.get(Control::ToolMove)?.parents == [Control::ViewportToolbar],
        "Model tools and scene information float within the viewport",
    )?;
    let status = s.trace.get(Control::StatusBar)?.rect;
    s.require(
        (status.height() - theme::size::XL_4).abs() < 0.1
            && (status.top() - s.state.viewport.bottom()).abs() < 0.1,
        "The Status bar is 32 points tall beneath the viewport",
    )?;
    s.value("status-bar-height", theme::size::XL_4);
    s.witness(Control::StatusBar)?;
    let insert = s.trace.get(Control::InsertMenu)?.rect;
    let toolbar = s.trace.get(Control::ViewportToolbar)?.rect;
    let view_tool = s.trace.get(Control::ToolView)?.rect;
    let move_tool = s.trace.get(Control::ToolMove)?.rect;
    s.require(
        insert.left() >= s.state.viewport.left()
            && insert.top() >= s.state.viewport.top()
            && view_tool.top() > insert.bottom()
            && move_tool.top() > view_tool.bottom()
            && (insert.left() - toolbar.left()).abs() < 1.0
            && (insert.right() - toolbar.right()).abs() < 1.0
            && (toolbar.top() - insert.bottom() - theme::space::LG).abs() < 1.0
            && (insert.center().x - view_tool.center().x).abs() < 1.0
            && (view_tool.center().x - move_tool.center().x).abs() < 1.0,
        "The floating Insert button leads the vertical tool stack at the viewport's upper left",
    )?;
    for control in [
        Control::ToolView,
        Control::ToolMove,
        Control::ToolRotate,
        Control::ToolScale,
    ] {
        let before = s.trace.get(control)?.rect;
        s.require(
            (before.width() - 32.0).abs() < 0.1 && (before.height() - 32.0).abs() < 0.1,
            &format!("{control:?} is a 32-point square: {before:?}"),
        )?;
        s.hover(control)?;
        let after = s.trace.get(control)?.rect;
        s.require(
            before == after,
            &format!(
                "Hovering {control:?} keeps its button size: before={before:?} after={after:?}"
            ),
        )?;
    }
    let style = s.ctx.style_of(s.ctx.theme());
    s.require(
        style.visuals.popup_shadow == theme::shadow::MD.resolve(style.visuals.dark_mode)
            && style.visuals.window_shadow == style.visuals.popup_shadow,
        "Menus and windows share the compact surface shadow",
    )?;
    s.require(
        style.visuals.widgets.inactive.corner_radius == egui::CornerRadius::same(theme::radius::MD)
            && style.visuals.widgets.hovered.corner_radius
                == egui::CornerRadius::same(theme::radius::MD)
            && style.visuals.window_corner_radius == egui::CornerRadius::same(theme::radius::LG)
            && style.visuals.menu_corner_radius == egui::CornerRadius::same(theme::radius::MD),
        "Buttons and menus share the rounded-corner scale across states",
    )?;
    s.witness(Control::ObjectList)?;
    s.require(
        s.state.layer_row_icon(id) == Some(Icon::Box),
        "The inserted cube has a Box icon in Layers",
    )?;
    s.witness(Control::Inspector)?;
    let n3 = s.trace.get(Control::N3Menu)?.rect;
    let layers = s.trace.get(Control::ObjectList)?.rect;
    s.require(
        layers.contains_rect(n3)
            && (n3.width() - theme::size::STEP_8).abs() < 1.0
            && (n3.width() - n3.height()).abs() < 1.0,
        "The N3 menu is a compact rounded square in the Layers panel header",
    )?;
    for (control, before) in [(Control::N3Menu, n3), (Control::InsertMenu, insert)] {
        s.hover(control)?;
        s.require(
            s.trace.get(control)?.rect == before,
            &format!("Hovering {control:?} preserves its square bounds"),
        )?;
    }
    s.hover(Control::Viewport)?;
    s.witness(Control::InsertMenu)?;
    s.witness(Control::ViewportToolbar)?;
    s.witness(Control::SceneInfo)?;
    s.witness(Control::ToolDockTabBar)?;
    s.witness(Control::AnimationPanelToggle)?;
    let stats = s.trace.get(Control::SceneInfo)?.rect;
    let tabs = s.trace.get(Control::ToolDockTabBar)?.rect;
    let floating_tabs = s
        .state
        .tool_dock
        .floating_tabs_rect
        .ok_or("Closed workspace is missing its floating Tool Dock tab bar")?;
    s.require(
        (stats.left() - s.state.viewport.left() - theme::space::XL).abs() < 1.0
            && (stats.left() - floating_tabs.left()).abs() < 1.0
            && (floating_tabs.top() - stats.bottom() - theme::space::LG).abs() < 1.0
            && s.state.viewport.contains_rect(stats),
        "Scene stats sit at the viewport's lower left, aligned above the floating Tool Dock tab bar",
    )?;
    s.require(
        floating_tabs.contains_rect(tabs)
            && s.state.viewport.contains_rect(floating_tabs)
            && (s.state.viewport.bottom() - floating_tabs.bottom() - theme::space::XL).abs() < 1.0
            && floating_tabs.bottom() < status.top()
            && s.trace.get(Control::AnimationPanelToggle)?.parents == [Control::ToolDockTabBar],
        "The Tool Dock tab bar floats beneath the scene stats and above the Status bar, with Animation inside the shared tab bar",
    )?;
    s.capture_image("workspace-layout")?;
    s.click(Control::N3Menu)?;
    s.witness(Control::FileMenu)?;
    s.witness(Control::ViewMenu)?;
    s.require(
        s.trace.get(Control::FileMenu)?.parents == [Control::N3Menu]
            && s.trace.get(Control::ViewMenu)?.parents == [Control::N3Menu]
            && s.trace.get(Control::FileMenu)?.rect.width() >= theme::size::STEP_52
            && s.trace.get(Control::ViewMenu)?.rect.width() >= theme::size::STEP_52,
        "File and View are full-width submenus of N3",
    )?;
    let focused_row = |s: &Session<'_>| {
        s.ctx
            .memory(|memory| memory.focused())
            .and_then(|id| s.ctx.read_response(id))
            .map(|response| response.rect)
    };
    s.require(
        focused_row(s) == Some(s.trace.get(Control::FileMenu)?.rect),
        "A mouse-opened menu focuses its first available row",
    )?;
    s.key(egui::Key::ArrowDown, true, egui::Modifiers::NONE)?;
    s.key(egui::Key::ArrowDown, false, egui::Modifiers::NONE)?;
    s.require(
        focused_row(s) == Some(s.trace.get(Control::ViewMenu)?.rect),
        "Arrow navigation moves focus to View without changing the document",
    )?;
    s.key(egui::Key::ArrowUp, true, egui::Modifiers::NONE)?;
    s.key(egui::Key::ArrowUp, false, egui::Modifiers::NONE)?;
    s.key(egui::Key::ArrowRight, true, egui::Modifiers::NONE)?;
    s.key(egui::Key::ArrowRight, false, egui::Modifiers::NONE)?;
    s.require(
        focused_row(s) == Some(s.trace.get(Control::New)?.rect),
        "Right enters File and focuses its first available command",
    )?;
    s.require(
        s.trace.get(Control::New)?.rect.width() >= theme::size::STEP_52,
        "File actions use the same minimum menu width",
    )?;
    let new_row = s.trace.get(Control::New)?.rect;
    let open_row = s.trace.get(Control::Open)?.rect;
    s.require(
        (new_row.bottom() - open_row.top()).abs() < 0.1,
        "Adjacent menu actions share a boundary without an empty hover gap",
    )?;
    s.hover(Control::New)?;
    s.require(
        s.trace.get(Control::New)?.rect == new_row,
        "Menu hover preserves the padded row bounds",
    )?;
    // Menu controls exist in the trace before their fade-in is visible. Advance
    // the scenario clock so the guide shows the open menu at full opacity.
    s.wait(Duration::from_millis(250))?;
    s.witness(Control::SaveAs)?;
    s.shortcut_label("document.save-as")?;
    let save_as = crate::ui::menu::Shortcut::for_action(crate::input::actions::ActionId::SaveAs)
        .ok_or("Save as has no shortcut presenter")?;
    s.value("save-as-menu-hint", save_as.presentation(s.ctx.os()).visual);
    s.value(
        "save-as-windows-hint",
        save_as
            .presentation(egui::os::OperatingSystem::Windows)
            .visual,
    );
    s.capture_image("workspace-menu")?;
    s.key(egui::Key::ArrowLeft, true, egui::Modifiers::NONE)?;
    s.key(egui::Key::ArrowLeft, false, egui::Modifiers::NONE)?;
    s.require(
        s.trace.get(Control::New).is_err()
            && focused_row(s) == Some(s.trace.get(Control::FileMenu)?.rect),
        "Left closes only the submenu and returns focus to File",
    )?;
    s.key(egui::Key::Escape, true, egui::Modifiers::NONE)?;
    s.key(egui::Key::Escape, false, egui::Modifiers::NONE)?;
    s.require(
        !egui::Popup::is_any_open(&s.ctx) && s.state.editor.document == document,
        "Escape dismisses the root menu without editing the document",
    )?;

    assert_transform_layout(s)?;
    s.hover(Control::Viewport)?;
    s.frame(Vec::new(), crate::doc_input::CUE_LIFETIME)?;
    s.capture_image("workspace-transform-layout")?;

    let row_rect = s.state.layer_row_rect(id).ok_or("No Layers row")?;
    let row = row_rect.center();
    // Two clicks on the same row, close together, open its inline editor.
    s.frame(
        vec![egui::Event::PointerMoved(row)],
        Duration::from_millis(500),
    )?;
    s.frame(vec![pointer(row, true)], Duration::ZERO)?;
    s.frame(vec![pointer(row, false)], Duration::from_millis(20))?;
    s.frame(Vec::new(), Duration::from_millis(80))?;
    s.frame(vec![pointer(row, true)], Duration::ZERO)?;
    s.frame(vec![pointer(row, false)], Duration::from_millis(20))?;
    s.settle()?;
    s.require(
        s.trace.get(Control::LayerRename).is_ok(),
        "Double-clicking a Layers row opens inline renaming",
    )?;
    s.witness(Control::LayerRename)?;
    let edit_rect = s.trace.get(Control::LayerRename)?.rect;
    s.require(
        (edit_rect.left() - row_rect.left() - theme::space::XL_4).abs() < 0.1
            && (edit_rect.center().y - row_rect.center().y).abs() < 0.1
            && (edit_rect.height() - row_rect.height()).abs() < 0.1,
        "Inline editing keeps the row's text position and height",
    )?;
    let edit_id = egui::Id::new(("n3.layer.rename", id));
    let selection = egui::TextEdit::load_state(&s.ctx, edit_id)
        .and_then(|state| state.cursor.char_range())
        .ok_or("Inline rename has no text selection")?;
    s.require(
        selection.as_sorted_char_range()
            == (egui::text::CharIndex(0)..egui::text::CharIndex("Cube".chars().count())),
        "Opening inline rename selects the whole name",
    )?;
    s.frame(
        vec![egui::Event::Text("Studio cube".into())],
        Duration::ZERO,
    )?;
    s.key(egui::Key::Enter, true, egui::Modifiers::NONE)?;
    s.key(egui::Key::Enter, false, egui::Modifiers::NONE)?;
    s.require(
        s.state.editor.document.objects[0].name == "Studio cube"
            && s.trace.get(Control::LayerRename).is_err(),
        "Enter commits the inline name",
    )?;
    s.undo()?;
    s.require(
        s.state.editor.document.objects[0].name != "Studio cube",
        "One Undo restores the previous layer name",
    )?;
    s.redo()?;
    s.require(
        s.state.editor.document.objects[0].name == "Studio cube",
        "Redo restores the new layer name",
    )?;

    let original_width = width(s)?;
    let field = s.target(Control::PrimitiveX)?;
    s.frame(vec![egui::Event::PointerMoved(field)], Duration::ZERO)?;
    s.frame(vec![pointer(field, true)], Duration::ZERO)?;
    s.frame(
        vec![egui::Event::PointerMoved(field + egui::vec2(22.0, 0.0))],
        Duration::ZERO,
    )?;
    s.require(
        width(s)? != original_width && s.state.editor.has_property_edit(),
        "A Properties drag updates geometry before pointer release within one edit session",
    )?;
    s.capture_tutorial("workspace-live-property")?;
    s.frame(
        vec![egui::Event::PointerMoved(field + egui::vec2(40.0, 0.0))],
        Duration::ZERO,
    )?;
    s.frame(
        vec![pointer(field + egui::vec2(40.0, 0.0), false)],
        Duration::ZERO,
    )?;
    s.settle()?;
    let accepted_width = width(s)?;
    s.require(
        accepted_width != original_width && !s.state.editor.has_property_edit(),
        "Releasing the field accepts its latest preview",
    )?;
    s.undo()?;
    s.require(
        width(s)? == original_width && s.state.editor.document.objects[0].name == "Studio cube",
        "One Undo reverses the entire dimension drag without reversing the rename",
    )?;
    s.redo()?;
    s.require(
        width(s)? == accepted_width,
        "Redo restores the accepted dimension",
    )?;

    let original_x = s.state.editor.document.objects[0].transform.translation[0];
    // Cross a centimeter-grid step while scrubbing the position.
    s.drag(Control::PositionX, egui::vec2(90.0, 0.0))?;
    let moved_x = s.state.editor.document.objects[0].transform.translation[0];
    s.require(
        moved_x != original_x,
        "Object transform fields also apply immediately",
    )?;
    s.undo()?;
    s.require(
        s.state.editor.document.objects[0].transform.translation[0] == original_x,
        "One Undo restores the object position after a field drag",
    )?;

    let before_hide = s.state.editor.document.clone();
    let selected = s.state.editor.selected_objects.clone();
    let viewport_width = s.state.viewport.width();
    s.shortcut("ui.toggle")?;
    s.require(!s.state.show_ui && s.state.viewport.width() > viewport_width
        && s.trace.get(Control::ObjectList).is_err()
        && s.trace.get(Control::ViewportToolbar).is_err()
        && s.trace.get(Control::ToolDockTabBar).is_err()
        && s.trace.get(Control::Gizmo).is_err()
        && s.state.editor.document == before_hide
        && s.state.editor.selected_objects == selected,
        r"Toggle UI hides the chrome and expands the viewport without changing the model or selection")?;
    s.capture_image("workspace-hidden-ui")?;
    s.shortcut("ui.toggle")?;
    s.require(
        s.state.show_ui
            && s.state.viewport.width() == viewport_width
            && s.trace.get(Control::ObjectList).is_ok()
            && s.trace.get(Control::ViewportToolbar).is_ok()
            && s.trace.get(Control::ToolDockTabBar).is_ok()
            && s.state.editor.document == before_hide,
        r"Repeating Toggle UI restores the workspace with the same document",
    )?;
    Ok(())
}
