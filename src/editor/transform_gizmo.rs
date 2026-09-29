//! Shared projection, solid picking, and render data for transform affordances.
use super::*;
use crate::render::transform_gizmo::GizmoVertex;
use glam::DVec4;

// Logical UI points, so zoom and Retina density do not shrink the pointer margin.
const ENDPOINT_HIT_MARGIN: f32 = 5.0;
const SHAFT_HIT_RADIUS: f32 = 7.0;

impl Handle {
    pub(super) fn with_endpoint(
        mut self,
        projection: &Projection,
        tool: Tool,
        rotation: DQuat,
        [start, center]: [DVec3; 2],
    ) -> Self {
        let Some(unit) = projection.pixel_size(center) else {
            return self;
        };
        self.endpoint = match tool {
            Tool::Move => Some(Endpoint::cone(center, self.axis, unit)),
            Tool::Scale => Some(Endpoint::cube(center, rotation, self.axis, unit)),
            _ => None,
        };
        if let Some(endpoint) = &self.endpoint {
            self.shaft = Some([start, endpoint.stem_end]);
        }
        self
    }

    fn endpoint_hit(&self, point: Pos2, projection: &Projection) -> Option<f64> {
        let ray = projection.ray(point)?;
        self.endpoint
            .as_ref()?
            .triangles
            .iter()
            .filter_map(|triangle| ray_triangle(ray, *triangle))
            // Match GPU near/far clipping, including partially clipped solids.
            .filter(|distance| {
                projection
                    .screen(ray.origin + ray.direction * *distance)
                    .is_some()
            })
            .min_by(f64::total_cmp)
    }

    pub(super) fn hit(&self, point: Pos2, projection: &Projection) -> bool {
        self.endpoint_hit(point, projection).is_some()
            || self.endpoint_margin_distance(point, projection).is_some()
            || self.line_or_plane_hit(point)
    }

    fn endpoint_margin_distance(&self, point: Pos2, projection: &Projection) -> Option<f32> {
        if !projection.viewport.contains(point) {
            return None;
        }
        self.endpoint
            .as_ref()?
            .triangles
            .iter()
            .filter_map(|triangle| {
                // Only expand fully visible triangles; never invent a target behind
                // the camera or beyond its clipping planes. Exact hits handle clipping.
                let points = [
                    projection.screen(triangle[0])?,
                    projection.screen(triangle[1])?,
                    projection.screen(triangle[2])?,
                ];
                let distance = if inside_polygon(point, &points) {
                    0.0
                } else {
                    [(0, 1), (1, 2), (2, 0)]
                        .into_iter()
                        .map(|(a, b)| segment_distance(point, points[a], points[b]))
                        .fold(f32::INFINITY, f32::min)
                };
                (distance <= ENDPOINT_HIT_MARGIN).then_some(distance)
            })
            .min_by(f32::total_cmp)
    }

    fn line_or_plane_hit(&self, point: Pos2) -> bool {
        if self.filled {
            inside_polygon(point, &self.points)
        } else {
            self.points
                .windows(2)
                .any(|pair| segment_distance(point, pair[0], pair[1]) <= SHAFT_HIT_RADIUS)
        }
    }

    fn color(&self) -> Color32 {
        match self.kind {
            HandleKind::Axis(axis) => COLORS[axis],
            HandleKind::Plane(a, b) => COLORS[3 - a - b],
            HandleKind::Uniform => crate::object_feedback::SELECTED_COLOR,
        }
    }
}

pub(super) fn pick_handle(
    handles: &[Handle],
    projection: &Projection,
    point: Pos2,
) -> Option<usize> {
    // Plane controls keep their existing explicit priority. On overlapping solid
    // endpoints, choose the nearest triangle, exactly like the overlay depth pass.
    handles
        .iter()
        .position(|h| h.filled && h.line_or_plane_hit(point))
        .or_else(|| {
            handles
                .iter()
                .enumerate()
                .filter_map(|(i, h)| Some((i, h.endpoint_hit(point, projection)?)))
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(i, _)| i)
        })
        // A direct shaft/ring hit wins over another endpoint's invisible margin.
        .or_else(|| {
            handles.iter().position(|h| {
                !h.filled
                    && h.points
                        .windows(2)
                        .any(|pair| segment_distance(point, pair[0], pair[1]) <= 1.25)
            })
        })
        .or_else(|| {
            handles
                .iter()
                .enumerate()
                .filter_map(|(i, h)| Some((i, h.endpoint_margin_distance(point, projection)?)))
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(i, _)| i)
        })
        .or_else(|| handles.iter().position(|h| h.line_or_plane_hit(point)))
}

impl Editor {
    pub(crate) fn transform_gizmo_vertices(
        &mut self,
        viewport: Rect,
        camera: &Camera,
        z_up: bool,
    ) -> Result<Vec<GizmoVertex>, String> {
        if self.tool == Tool::View
            || self.selected_objects.is_empty()
            || (self.edit_mode && self.selected_vertices.is_empty())
        {
            return Ok(Vec::new());
        }
        let projection = Projection::new(viewport, camera, z_up)?;
        self.prepare(&projection)?;
        let mut vertices = Vec::new();
        if let Some(handle) = self.locked_transform_handle(&projection)
            && let Some(center) = projection.screen(handle.pivot)
            && let Some((direction, _)) = projection.axis_direction(handle.pivot, handle.axis)
            && let Some(line) = clipped_axis_line(viewport, center, direction)
        {
            let clip = line.map(|p| screen_clip(p, DVec4::W, viewport));
            // The long axis guide sits behind every endpoint, independent of scene depth.
            let clip = clip.map(|p| DVec4::new(p.x, p.y, 1.0, 1.0));
            ribbon(&mut vertices, clip, viewport, 2.0, handle.color());
        }
        let ray = projection
            .ray(viewport.center())
            .ok_or("Invalid gizmo view ray")?;
        let center = projection
            .unproject(viewport.center(), 0.5)
            .ok_or("Invalid gizmo projection")?;
        let up = (projection
            .unproject(viewport.center() - egui::vec2(0.0, 1.0), 0.5)
            .ok_or("Invalid gizmo projection")?
            - center)
            .normalize();
        let right = (projection
            .unproject(viewport.center() + egui::vec2(1.0, 0.0), 0.5)
            .ok_or("Invalid gizmo projection")?
            - center)
            .normalize();
        let light = (-ray.direction + up * 0.7 - right * 0.4).normalize();
        for handle in self.handles(&projection) {
            let color = handle.color();
            if let Some(shaft) = handle.shaft {
                let clip = shaft.map(|point| projection.matrix * point.extend(1.0));
                if clip.iter().all(|p| p.is_finite() && p.w > 0.0) {
                    ribbon(&mut vertices, clip, viewport, 2.5, color);
                }
            }
            if let Some(endpoint) = handle.endpoint {
                for triangle in endpoint.triangles {
                    let normal = (triangle[1] - triangle[0])
                        .cross(triangle[2] - triangle[0])
                        .normalize();
                    let shade = (0.55 + 0.45 * normal.dot(light).max(0.0)) as f32;
                    let color = rgba(color, shade);
                    vertices.extend(triangle.map(|point| GizmoVertex {
                        position: (projection.matrix * point.extend(1.0)).as_vec4().to_array(),
                        color,
                    }));
                }
            }
        }
        Ok(vertices)
    }
}

fn rgba(color: Color32, shade: f32) -> [f32; 4] {
    [
        f32::from(color.r()) / 255.0 * shade,
        f32::from(color.g()) / 255.0 * shade,
        f32::from(color.b()) / 255.0 * shade,
        1.0,
    ]
}

fn screen_clip(point: Pos2, depth: DVec4, viewport: Rect) -> DVec4 {
    DVec4::new(
        f64::from(2.0 * (point.x - viewport.left()) / viewport.width() - 1.0) * depth.w,
        f64::from(1.0 - 2.0 * (point.y - viewport.top()) / viewport.height()) * depth.w,
        depth.z,
        depth.w,
    )
}

fn ribbon(
    vertices: &mut Vec<GizmoVertex>,
    clip: [DVec4; 2],
    viewport: Rect,
    width: f32,
    color: Color32,
) {
    let point = |p: DVec4| {
        egui::pos2(
            viewport.left() + (p.x / p.w + 1.0) as f32 * viewport.width() * 0.5,
            viewport.top() + (1.0 - p.y / p.w) as f32 * viewport.height() * 0.5,
        )
    };
    let points = clip.map(point);
    let delta = points[1] - points[0];
    if delta.length_sq() < 1e-8 {
        return;
    }
    let offset = egui::vec2(-delta.y, delta.x).normalized() * width * 0.5;
    let corners = [
        screen_clip(points[0] + offset, clip[0], viewport),
        screen_clip(points[0] - offset, clip[0], viewport),
        screen_clip(points[1] - offset, clip[1], viewport),
        screen_clip(points[1] + offset, clip[1], viewport),
    ];
    vertices.extend([0, 1, 2, 0, 2, 3].map(|i| GizmoVertex {
        position: corners[i].as_vec4().to_array(),
        color: rgba(color, 1.0),
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(tool: Tool) -> Editor {
        let mut document = Document::default();
        let id = document.insert_primitive(PrimitiveKind::Cube).unwrap();
        let mut editor = Editor::new(document).unwrap();
        editor.select_object(id).unwrap();
        editor.set_tool(tool);
        editor
    }

    fn viewport() -> Rect {
        Rect::from_min_size(egui::pos2(240.0, 20.0), egui::vec2(1200.0, 800.0))
    }

    #[test]
    fn solid_corners_outside_the_old_line_target_begin_real_undoable_transforms() {
        for tool in [Tool::Move, Tool::Scale] {
            let mut editor = fixture(tool);
            editor.snapping.enabled = false;
            let projection = Projection::new(viewport(), &Camera::default(), false).unwrap();
            editor.prepare(&projection).unwrap();
            let handles = editor.handles(&projection);
            let candidate = handles
                .iter()
                .enumerate()
                .find_map(|(index, handle)| {
                    handle
                        .endpoint
                        .as_ref()?
                        .triangles
                        .iter()
                        .find_map(|triangle| {
                            let center = (triangle[0] + triangle[1] + triangle[2]) / 3.0;
                            triangle.iter().find_map(|corner| {
                                let point = projection.screen(*corner * 0.95 + center * 0.05)?;
                                (!handle.line_or_plane_hit(point)
                                    && pick_handle(&handles, &projection, point) == Some(index))
                                .then_some((index, point))
                            })
                        })
                })
                .expect("A solid endpoint must be pickable beyond the old shaft-only target");
            let (index, point) = candidate;
            let handle = &handles[index];
            let before = editor.document.clone();
            editor.begin_transform(&projection, handle, point).unwrap();
            let direction = (handle.points[1] - handle.points[0]).normalized();
            editor.preview_transform(point + direction * 25.0).unwrap();
            editor.finish_gesture().unwrap();
            assert_ne!(editor.document, before);
            assert!(matches!(
                editor.document.objects[0].geometry,
                Geometry::Primitive(_)
            ));
            assert!(editor.undo());
            assert_eq!(editor.document, before);
            assert!(!editor.undo());
        }
    }

    #[test]
    fn projected_solids_keep_reachable_targets_and_no_history_across_views_and_up_axes() {
        use crate::camera::View;
        for z_up in [false, true] {
            for tool in [Tool::Move, Tool::Scale] {
                let mut editor = fixture(tool);
                let baseline = editor.document.clone();
                for view in [
                    View::Perspective,
                    View::Front,
                    View::Back,
                    View::Left,
                    View::Right,
                    View::Top,
                    View::Bottom,
                ] {
                    let mut camera = Camera::default();
                    camera.set_view(view);
                    for zoom in [-1.0, 0.0, 1.0] {
                        let mut camera = camera.clone();
                        camera.zoom(zoom);
                        let vertices = editor
                            .transform_gizmo_vertices(viewport(), &camera, z_up)
                            .unwrap();
                        assert!(!vertices.is_empty() && vertices.len() <= 324);
                        assert!(
                            vertices
                                .iter()
                                .all(|v| v.position.iter().all(|x| x.is_finite()))
                        );
                        let projection = Projection::new(viewport(), &camera, z_up).unwrap();
                        let handles = editor.handles(&projection);
                        for (i, handle) in handles.iter().enumerate() {
                            assert_eq!(
                                pick_handle(&handles, &projection, handle.target.center()),
                                Some(i)
                            );
                        }
                    }
                }
                assert_eq!(editor.document, baseline);
                assert!(!editor.undo());
                editor.set_tool(Tool::View);
                assert!(
                    editor
                        .transform_gizmo_vertices(viewport(), &Camera::default(), z_up)
                        .unwrap()
                        .is_empty()
                );
            }
        }
    }

    #[test]
    fn endpoint_margin_accepts_near_misses_but_not_distant_clicks_or_other_visible_handles() {
        for z_up in [false, true] {
            for (tool, near, far) in [(Tool::Move, 10.5, 15.0), (Tool::Scale, 9.0, 12.0)] {
                let mut editor = fixture(tool);
                let mut camera = Camera::default();
                camera.set_view(crate::camera::View::Front);
                let projection = Projection::new(viewport(), &camera, z_up).unwrap();
                editor.prepare(&projection).unwrap();
                let mut handles = editor.handles(&projection);
                let index = handles
                    .iter()
                    .position(|h| h.kind == HandleKind::Axis(0))
                    .unwrap();
                let center = handles[index].target.center();
                let point = center + egui::vec2(near, 0.0);
                assert!(handles[index].endpoint_hit(point, &projection).is_none());
                assert!(!handles[index].line_or_plane_hit(point));
                assert_eq!(pick_handle(&handles, &projection, point), Some(index));
                assert!(!handles[index].hit(center + egui::vec2(far, 0.0), &projection));

                let other = handles
                    .iter()
                    .position(|h| h.kind == HandleKind::Axis(if z_up { 2 } else { 1 }))
                    .unwrap();
                let clip = projection.matrix * handles[index].pivot.extend(1.0);
                let center = projection.unproject(point, clip.z / clip.w).unwrap();
                handles[other].endpoint = Some(Endpoint::cube(
                    center,
                    DQuat::IDENTITY,
                    DVec3::X,
                    projection.pixel_size(center).unwrap(),
                ));
                assert_eq!(
                    pick_handle(&handles, &projection, point),
                    Some(other),
                    "The invisible margin must not steal a direct hit on another solid"
                );
            }
        }
    }

    #[test]
    fn overlapping_solids_pick_nearest_face_independent_of_handle_order() {
        let mut editor = fixture(Tool::Scale);
        let projection = Projection::new(viewport(), &Camera::default(), false).unwrap();
        editor.prepare(&projection).unwrap();
        let mut handles = editor.handles(&projection);
        handles.truncate(2);
        let ray = projection.ray(viewport().center()).unwrap();
        for (handle, depth) in handles.iter_mut().zip([2.0, 3.0]) {
            handle.endpoint = Some(Endpoint::cube(
                ray.origin + ray.direction * depth,
                DQuat::IDENTITY,
                DVec3::X,
                0.02,
            ));
        }
        let nearest = handles[0].kind;
        for _ in 0..2 {
            let i = pick_handle(&handles, &projection, viewport().center()).unwrap();
            assert_eq!(handles[i].kind, nearest);
            handles.reverse();
        }
    }
}
