use super::{Result, Session, pointer_with_modifiers};
use crate::{
    controls::Control,
    edit_feedback::{SELECTED, UNSELECTED},
};
use std::{collections::BTreeSet, time::Duration};

#[derive(Debug, PartialEq, Eq)]
struct FeedbackCounts {
    solid_edges: usize,
    gradient_edges: usize,
    dark_edges: usize,
    selected_faces: usize,
    selected_triangles: usize,
}

fn feedback_counts(s: &Session<'_>, object: u64) -> Result<FeedbackCounts> {
    let selection = s.state.edit_selection();
    let mesh = s
        .state
        .mesh
        .as_ref()
        .ok_or("The selection example has no render mesh")?;
    let topology = mesh
        .edit_topology
        .iter()
        .find(|topology| topology.object == object)
        .ok_or("The edited cube has no original-polygon render metadata")?;
    if topology.vertices.len() != 8
        || topology.edge_vertices.len() != 12
        || topology.faces.len() != 6
        || topology.faces.iter().any(|face| face.vertices.len() != 4)
    {
        return Err("The cube feedback must retain eight vertices, twelve real boundary edges, and six original quads".into());
    }
    let mut counts = FeedbackCounts {
        solid_edges: 0,
        gradient_edges: 0,
        dark_edges: 0,
        selected_faces: 0,
        selected_triangles: 0,
    };
    for edge in &topology.edge_vertices {
        let colors = selection.edge_colors(object, *edge);
        if colors == [SELECTED; 2] {
            counts.solid_edges += 1;
        } else if colors == [UNSELECTED; 2] {
            counts.dark_edges += 1;
        } else if colors == [SELECTED, UNSELECTED] || colors == [UNSELECTED, SELECTED] {
            counts.gradient_edges += 1;
        } else {
            return Err(
                "Boundary endpoints must use the shared selected and unselected palette".into(),
            );
        }
    }
    for face in &topology.faces {
        if selection.face_selected(object, &face.vertices) {
            counts.selected_faces += 1;
            counts.selected_triangles += face.triangles.len() / 3;
        }
    }
    Ok(counts)
}

fn click_vertex(s: &mut Session<'_>, position: egui::Pos2, additive: bool) -> Result<()> {
    let modifiers = if additive {
        egui::Modifiers::SHIFT
    } else {
        egui::Modifiers::NONE
    };
    // These are separate selection clicks. Egui detects double clicks by time
    // across the whole viewport, so keep this tutorial out of that existing
    // mode-switch gesture instead of changing its interaction policy here.
    s.frame(
        vec![egui::Event::PointerMoved(position)],
        Duration::from_millis(400),
    )?;
    s.frame(
        vec![pointer_with_modifiers(position, true, modifiers)],
        Duration::ZERO,
    )?;
    s.frame(
        vec![pointer_with_modifiers(position, false, modifiers)],
        Duration::from_millis(20),
    )?;
    s.settle()
}

fn clear_pointer_cue(s: &mut Session<'_>) -> Result<()> {
    let away = s.state.viewport_ui_rect.left_bottom() + egui::vec2(40.0, -80.0);
    s.frame(
        vec![egui::Event::PointerMoved(away)],
        crate::doc_input::CUE_LIFETIME,
    )?;
    s.settle()
}

pub fn run(s: &mut Session<'_>) -> Result<()> {
    s.load_fixture("cube-quads.obj")?;
    s.click_path(&[Control::N3Menu, Control::ViewMenu, Control::Frame])?;
    s.click_path(&[Control::N3Menu, Control::ViewMenu])?;
    s.click(Control::ViewFront)?;
    s.frame(
        Vec::new(),
        Duration::from_millis(s.state.view_duration_ms.into()),
    )?;
    s.settle()?;
    // Keep all four selection targets below the gizmo and its navigation tabs.
    // Geometric visibility alone does not imply a vertex is outside UI overlays.
    s.hover(Control::Viewport)?;
    s.pinch(-0.2)?;
    s.settle()?;
    let id = s.state.editor.document.objects[0].id;
    s.click_at(s.state.viewport.center())?;
    s.click(Control::ToolView)?;
    s.shortcut("edit.confirm")?;
    s.require(
        s.state.editor.edit_mode && s.state.editor.selected_object == Some(id),
        "The selection feedback example enters vertex editing on the canonical quad cube",
    )?;
    s.witness(Control::EditToolbar)?;
    s.witness(Control::LeaveEdit)?;
    s.witness(Control::Viewport)?;
    let document = s.state.editor.document.clone();
    let revision = s.state.editor.revision;
    let render_revision = s.state.mesh_revision;
    let mut visible =
        s.state
            .editor
            .selectable_vertices(s.state.viewport, &s.state.camera, s.state.z_up)?;
    visible.sort_by(|(_, a), (_, b)| a.y.total_cmp(&b.y).then_with(|| a.x.total_cmp(&b.x)));
    s.require(
        visible.len() == 4,
        "Only the four front vertices are selectable; the rear corners remain occluded",
    )?;
    s.require(
        (visible[0].1.y - visible[1].1.y).abs() < 0.01,
        "The first two visible vertices share the top boundary of the front quad",
    )?;
    s.require(
        visible
            .iter()
            .all(|(_, point)| !crate::axis_gizmo::bounds(s.state.viewport).contains(*point)),
        "Every tutorial vertex is directly clickable outside the gizmo overlay",
    )?;

    // This controls ordinary object edges. Editing still shows the active
    // object's real boundaries; it does not depend on that display preference.
    if s.state.show_edges {
        s.click_path(&[Control::N3Menu, Control::ViewMenu])?;
        s.click_path(&[Control::N3Menu, Control::ViewMenu, Control::Edges])?;
        if s.trace.get(Control::Edges).is_ok() {
            s.shortcut("cancel")?;
        }
    }
    s.require(!s.state.show_edges, "The edited object's boundary feedback is exercised with the general edge display switched off")?;
    s.require(
        s.trace.get(Control::Edges).is_err(),
        "The display menu closes before clicking any vertex",
    )?;
    click_vertex(s, visible[0].1, false)?;
    s.require(
        s.state.editor.selected_vertices == BTreeSet::from([visible[0].0]),
        &format!(
            "The first plain click selects exactly its visible vertex (selected={:?}, target={})",
            s.state.editor.selected_vertices, visible[0].0
        ),
    )?;
    click_vertex(s, visible[0].1 + egui::vec2(6.0, 0.0), false)?;
    s.require(
        s.state.editor.selected_vertices == BTreeSet::from([visible[0].0]),
        "A click outside the small painted marker still picks the nearby vertex",
    )?;
    click_vertex(s, visible[1].1, true)?;
    let partial = BTreeSet::from([visible[0].0, visible[1].0]);
    let counts = feedback_counts(s, id)?;
    s.require(
        s.state.editor.selected_vertices == partial
            && s.state.edit_selection().vertices == partial
            && counts == FeedbackCounts { solid_edges: 1, gradient_edges: 4, dark_edges: 7, selected_faces: 0, selected_triangles: 0 },
        &format!("Selecting two adjacent front vertices produces one solid selected boundary and four endpoint gradients, with no face tint or triangulation diagonal (selected={:?}, expected={partial:?}, counts={counts:?})", s.state.editor.selected_vertices),
    )?;
    clear_pointer_cue(s)?;
    s.capture_tutorial("edit-selection-partial")?;

    click_vertex(s, visible[2].1, true)?;
    let counts = feedback_counts(s, id)?;
    s.require(
        s.state.editor.selected_vertices.len() == 3 && counts.selected_faces == 0 && counts.selected_triangles == 0,
        "Three selected corners cannot tint either derived triangle of the original four-corner polygon",
    )?;
    click_vertex(s, visible[3].1, true)?;
    let complete: BTreeSet<_> = visible.iter().map(|(id, _)| *id).collect();
    let counts = feedback_counts(s, id)?;
    s.require(
        s.state.editor.selected_vertices == complete
            && counts == FeedbackCounts { solid_edges: 4, gradient_edges: 4, dark_edges: 4, selected_faces: 1, selected_triangles: 2 },
        "Selecting all four front vertices highlights exactly that original quad and its two render triangles, preserving dark rear boundaries and shared edge gradients",
    )?;
    // Inspect the same selection across depth, using the normal right-drag
    // navigation path rather than changing render state for the screenshot.
    let start = s.state.viewport_ui_rect.left_bottom() + egui::vec2(160.0, -100.0);
    let end = start + egui::vec2(-75.0, 45.0);
    let camera = s
        .state
        .camera
        .view_projection(s.state.viewport.aspect_ratio());
    s.frame(vec![egui::Event::PointerMoved(start)], Duration::ZERO)?;
    s.frame(
        vec![egui::Event::PointerButton {
            pos: start,
            button: egui::PointerButton::Secondary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        }],
        Duration::ZERO,
    )?;
    s.frame(vec![egui::Event::PointerMoved(end)], Duration::ZERO)?;
    s.frame(
        vec![egui::Event::PointerButton {
            pos: end,
            button: egui::PointerButton::Secondary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }],
        Duration::ZERO,
    )?;
    s.require(
        s.state.camera.view_projection(s.state.viewport.aspect_ratio()) != camera
            && s.state.editor.selected_vertices == complete
            && feedback_counts(s, id)? == counts,
        "Right-drag orbits the fully selected front face into an oblique view without altering its vertex, edge, or original-face selection",
    )?;
    clear_pointer_cue(s)?;
    s.capture_tutorial("edit-selection-face")?;
    s.click_path(&[Control::N3Menu, Control::ViewMenu])?;
    s.click(Control::ViewFront)?;
    s.frame(
        Vec::new(),
        Duration::from_millis(s.state.view_duration_ms.into()),
    )?;
    s.settle()?;

    click_vertex(s, visible[3].1, true)?;
    s.require(feedback_counts(s, id)?.selected_faces == 0, "Shift-clicking one selected corner removes its selection and immediately clears the face tint")?;
    s.require(
        s.state.editor.document == document
            && s.state.editor.revision == revision
            && s.state.mesh_revision == render_revision,
        "Selection feedback changes no authored geometry or mesh revision",
    )?;
    s.click(Control::LeaveEdit)?;
    s.require(!s.state.editor.edit_mode && s.state.edit_selection().object.is_none() && s.state.editor.document == document, "Leaving vertex editing clears the component-rendering payload while retaining the object and geometry")?;
    s.undo()?;
    s.require(
        s.state.editor.document == document,
        "Changing the selection and its colors creates no geometry undo entry",
    )?;
    Ok(())
}
