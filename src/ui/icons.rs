use egui::{Color32, Painter, Rect, Shape, Stroke};

/// Small, hand-authored UI symbols until N3 needs a broader icon source.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Icon {
    Orthographic,
    Perspective,
}

impl Icon {
    pub(crate) fn paint(self, painter: &Painter, rect: Rect, color: Color32) {
        let size = rect.width().min(rect.height());
        let origin = rect.center() - egui::Vec2::splat(size * 0.5);
        let point = |[x, y]: [f32; 2]| origin + egui::vec2(x, y) * (size / 20.0);
        let stroke = Stroke::new(size * 0.08, color);

        // A square front face distinguishes the parallel view; the centered
        // three-face cube reads as the perspective view at small button sizes.
        let (outline, junction, spokes) = match self {
            Self::Orthographic => (
                [
                    [3., 7.],
                    [8., 2.],
                    [18., 2.],
                    [18., 12.],
                    [13., 17.],
                    [3., 17.],
                ],
                [13., 7.],
                [[3., 7.], [18., 2.], [13., 17.]],
            ),
            Self::Perspective => (
                [
                    [10., 2.],
                    [18., 6.],
                    [18., 14.],
                    [10., 18.],
                    [2., 14.],
                    [2., 6.],
                ],
                [10., 10.],
                [[18., 6.], [10., 18.], [2., 6.]],
            ),
        };
        painter.add(Shape::closed_line(
            outline.into_iter().map(point).collect(),
            stroke,
        ));
        for end in spokes {
            painter.line_segment([point(junction), point(end)], stroke);
        }
    }
}
