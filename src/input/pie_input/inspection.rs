//! Inspection and isolated fixtures for executable native tooling.
use super::*;

impl PieInput {
    pub(crate) fn for_instance(id: egui::Id) -> Self {
        Self {
            instance: Some(id),
            ..Self::default()
        }
    }

    pub fn view_active(&self) -> bool {
        matches!(self.pie, Some(ActivePie::View(_)))
    }

    pub fn shading_active(&self) -> bool {
        matches!(self.pie, Some(ActivePie::Shading(_)))
    }

    /// Hosts can constrain an isolated component while reusing the same input
    /// lifecycle. The editor supplies its full window bounds through `begin`.
    pub(crate) fn begin_in(
        &mut self,
        ctx: &Context,
        viewport: Rect,
        bounds: Rect,
        context: PieContext,
    ) -> Vec<Command> {
        self.begin_with_pointer_policy(ctx, viewport, bounds, context, |ctx, viewport, point| {
            // An isolated component has no editor gizmo. Preserve egui's
            // pointer ownership while leaving editor hit regions to `begin`.
            viewport.contains(point)
                && !ctx.egui_is_using_pointer()
                && ctx.layer_id_at(point) == Some(egui::LayerId::background())
        })
    }
}
