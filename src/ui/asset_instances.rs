//! Imported assets contribute immutable evaluated content to the ordinary editor.
//! Placement belongs to the document; playback and resource caches do not.
use super::*;
use crate::{
    document::AssetInstance,
    scene_view::{Action, SceneView},
};
use std::sync::Arc;

pub(crate) fn prepare_views(
    document: &Document,
    assets: &BTreeMap<String, Arc<crate::scene::SceneAsset>>,
    diagnostics: &mut Vec<String>,
) -> BTreeMap<AssetInstance, SceneView> {
    let mut views: BTreeMap<AssetInstance, SceneView> = BTreeMap::new();
    let mut attempted = BTreeSet::new();
    for object in &document.objects {
        if let Geometry::Asset(instance) = &object.geometry
            && attempted.insert(instance.clone())
            && let Some(asset) = assets.get(&instance.source)
        {
            // A broken link must not prevent sibling objects from opening.
            // Use the loader's diagnostic wording so host-validated failures
            // and direct in-memory candidates share one visible report.
            let result = SceneView::from_shared(asset.clone(), instance.scene).and_then(|view| {
                crate::scene::budgets::retained_frames(
                    views
                        .values()
                        .map(|view| view.frame.as_ref())
                        .chain(std::iter::once(view.frame.as_ref())),
                )?;
                Ok(view)
            });
            // Scene evaluation can replace the source's cached rest pose.
            // Stop before evaluating more sources if retained payloads grew
            // beyond the common cache budget; the caller rejects publication.
            if let Err(error) = crate::asset_io::validate_resource_cache(assets) {
                diagnostics.push(error);
                break;
            }
            match result {
                Ok(view) => {
                    views.insert(instance.clone(), view);
                }
                Err(error) => {
                    let diagnostic = format!(
                        "Cannot evaluate linked object {:?}, scene {}: {error}",
                        object.name, instance.scene
                    );
                    if !diagnostics.contains(&diagnostic) {
                        diagnostics.push(diagnostic);
                    }
                }
            }
        }
    }
    views
}

pub(crate) fn placed_assets(
    document: &Document,
    views: &BTreeMap<AssetInstance, SceneView>,
) -> Vec<crate::renderer::PlacedScene> {
    document
        .objects
        .iter()
        .filter_map(|object| {
            let Geometry::Asset(instance) = &object.geometry else {
                return None;
            };
            let view = views.get(instance)?;
            Some(crate::renderer::PlacedScene {
                object: object.id,
                asset: view.asset.clone(),
                frame: view.frame.clone(),
                transform: object.transform.clone(),
                exposure: view.exposure,
            })
        })
        .collect()
}

impl WorkspaceUi {
    pub(super) fn navigation_gizmo_visible(&self) -> bool {
        self.show_ui
    }

    pub fn selected_asset_view(&self) -> Option<&SceneView> {
        let id = self.editor.selected_object?;
        let object = self
            .editor
            .document
            .objects
            .iter()
            .find(|object| object.id == id)?;
        let Geometry::Asset(instance) = &object.geometry else {
            return None;
        };
        self.asset_views.get(instance)
    }

    pub fn placed_scenes(&self) -> Vec<crate::renderer::PlacedScene> {
        // Visibility is a renderer draw filter, not cache membership. Isolating
        // another object must not evict these materials or instance buffers.
        placed_assets(&self.editor.document, &self.asset_views)
    }

    pub(super) fn asset_frames(&self) -> crate::document::AssetFrames {
        self.asset_views
            .iter()
            .map(|(key, view)| (key.clone(), view.frame.clone()))
            .collect()
    }

    pub fn import_loaded_document(
        &mut self,
        mut loaded: crate::asset_io::LoadedDocument,
    ) -> Result<(), String> {
        if !self.document_action_allowed() {
            return Err(pending_transform_message());
        }
        let mut resources = loaded.assets.clone();
        for (key, view) in &self.asset_views {
            resources.insert(key.source.clone(), view.asset.clone());
        }
        crate::asset_io::validate_resource_cache(&resources)?;
        let was_empty = self.editor.document.objects.is_empty();
        let mut views = prepare_views(&loaded.document, &resources, &mut loaded.diagnostics);
        crate::asset_io::validate_resource_cache(&resources)?;
        // Existing instances retain their preview state. New instances sharing
        // a source also share playback in this first placement milestone.
        for (key, view) in &self.asset_views {
            views.insert(key.clone(), view.clone());
        }
        crate::scene::budgets::retained_frames(views.values().map(|view| view.frame.as_ref()))?;
        let frames = views
            .iter()
            .map(|(key, view)| (key.clone(), view.frame.clone()))
            .collect();
        self.editor
            .import_objects(loaded.document.objects, frames)?;
        self.asset_views = views;
        self.asset_diagnostics.extend(loaded.diagnostics);
        self.inspector_key = None;
        if was_empty {
            self.camera.frame(self.aspect());
        }
        self.refresh_mesh()?;
        Ok(())
    }

    pub(super) fn tick_assets(&mut self, ctx: &egui::Context) {
        if ctx.current_pass_index() != 0 {
            return;
        }
        let now = ctx.input(|input| input.time);
        let previous = self.scene_tick_time.replace(now);
        // A selection/transform preview uses one stable surface and baseline.
        // Pause the preview clock during gestures; never catch up paused time.
        if self.editor.is_interacting() {
            return;
        }
        let before = self.asset_views.clone();
        let referenced: BTreeSet<_> = self
            .editor
            .document
            .objects
            .iter()
            .filter_map(|object| {
                if let Geometry::Asset(key) = &object.geometry {
                    Some(key.clone())
                } else {
                    None
                }
            })
            .collect();
        for (key, view) in &mut self.asset_views {
            if !referenced.contains(key) {
                continue;
            }
            if let Some(previous) = previous
                && let Err(error) = view.advance(now - previous)
            {
                view.playback.playing = false;
                self.error = Some(error);
            }
            if view.playback.playing {
                ctx.request_repaint_after(Duration::from_millis(16));
            }
        }
        if let Err(error) = self.editor.set_asset_frames(self.asset_frames()) {
            self.asset_views = before;
            for view in self.asset_views.values_mut() {
                view.playback.playing = false;
            }
            self.error = Some(error);
        }
    }

    fn queue_scene_action(&mut self, action: Action, ctx: &egui::Context) {
        let command = Command::Scene(action);
        if !self.pending_ui_commands.contains(&command) {
            self.pending_ui_commands.push(command);
        }
        ctx.request_repaint();
    }

    pub(super) fn dispatch_asset_action(&mut self, action: Action, ctx: &egui::Context) {
        // The authored scene reference is immutable at this milestone. Source
        // cameras remain inspectable data but never replace the editor camera.
        if self.editor.is_interacting() {
            return;
        }
        let Some(key) = self.selected_asset_key() else {
            return;
        };
        self.cancel_animation_interaction(ctx);
        let result = self.apply_asset_action(&key, action, ctx);
        self.report(result);
    }

    /// Shared, targeted runtime path for Properties and timeline requests.
    /// Timeline requests retain their source identity even if selection changes.
    pub(super) fn apply_asset_action(
        &mut self,
        key: &AssetInstance,
        action: Action,
        ctx: &egui::Context,
    ) -> Result<(), String> {
        let Some(view) = self.asset_views.get(key) else {
            return Err("Animation source is unavailable.".into());
        };
        let mut candidate = view.clone();
        candidate.apply(action)?;
        self.publish_asset_view(key, candidate)?;
        self.scene_tick_time = Some(ctx.input(|input| input.time));
        ctx.request_repaint();
        Ok(())
    }

    pub(super) fn publish_asset_view(
        &mut self,
        key: &AssetInstance,
        candidate: SceneView,
    ) -> Result<(), String> {
        let mut frames = self.asset_frames();
        frames.insert(key.clone(), candidate.frame.clone());
        self.editor.set_asset_frames(frames)?;
        self.asset_views.insert(key.clone(), candidate);
        Ok(())
    }

    pub(super) fn asset_inspector(&mut self, ui: &mut egui::Ui) {
        let Some(id) = self.editor.selected_object else {
            return;
        };
        let Some(object) = self
            .editor
            .document
            .objects
            .iter()
            .find(|object| object.id == id)
        else {
            return;
        };
        let Geometry::Asset(instance) = &object.geometry else {
            return;
        };
        let source = instance.source.clone();
        let ctx = ui.ctx().clone();
        let asset = workspace_panel_section(ui, |ui| {
            let response = ui.label(theme::strong("Imported asset"));
            controls::record(
                &ctx,
                Control::SceneDetails,
                Control::SceneDetails.label(),
                response.rect,
                true,
            );
            ui.weak(if self.editor.can_edit() {
                "Placement editable · contents read-only"
            } else {
                "Read-only document"
            });
            ui.small(filename(Path::new(&source)))
                .on_hover_text(&source);
            let Some(scene) = self.selected_asset_view() else {
                ui.colored_label(
                    ui.visuals().warn_fg_color,
                    "Source unavailable. Restore the source file and reopen this document.",
                );
                for diagnostic in &self.asset_diagnostics {
                    ui.small(diagnostic);
                }
                return None;
            };
            let asset = scene.asset.clone();
            ui.weak(format!(
                "{} nodes · {} meshes · {} materials",
                asset.nodes.len(),
                asset.meshes.len(),
                asset.materials.len()
            ));
            ui.small(format!(
                "{} · {}",
                asset.name, asset.scenes[scene.scene].name
            ));
            Some(asset)
        })
        .inner;
        let Some(asset) = asset else {
            return;
        };
        inspector_section(ui, "Preview lighting", |ui| {
            let mut exposure = self.selected_asset_view().unwrap().exposure;
            let response = ui
                .add_enabled(
                    self.shading == crate::render::shading::ShadingMode::MaterialPreview,
                    egui::Slider::new(&mut exposure, -8. ..=8.)
                        .text("Exposure")
                        .suffix(" EV"),
                )
                .on_disabled_hover_text("Available in Material Preview");
            controls::record(
                &ctx,
                Control::SceneExposure,
                Control::SceneExposure.label(),
                response.rect,
                response.enabled(),
            );
            if response.changed() {
                self.queue_scene_action(Action::Exposure(exposure), &ctx);
            }
            ui.weak(if asset.lights.is_empty() {
                "Studio environment lighting"
            } else {
                "Asset lights + studio environment"
            });
        });
        workspace_panel_section(ui, |ui| {
            let response = ui.collapsing(Control::SceneHierarchy.label(), |ui| {
                let mut rows: Vec<_> = asset.scenes[instance_scene(self)]
                    .roots
                    .iter()
                    .rev()
                    .map(|id| (*id, 0usize))
                    .collect();
                while let Some((id, depth)) = rows.pop() {
                    ui.horizontal(|ui| {
                        ui.add_space((depth as f32 * 10.).min(60.));
                        ui.weak(&asset.nodes[id].name);
                    });
                    rows.extend(
                        asset.nodes[id]
                            .children
                            .iter()
                            .rev()
                            .map(|id| (*id, depth + 1)),
                    );
                }
            });
            controls::record(
                &ctx,
                Control::SceneHierarchy,
                Control::SceneHierarchy.label(),
                response.header_response.rect,
                true,
            );
            let response = ui.collapsing(Control::SceneDiagnostics.label(), |ui| {
                if asset.warnings.is_empty() {
                    ui.weak("No unsupported optional features reported.");
                }
                for warning in &asset.warnings {
                    ui.label(warning);
                }
                ui.collapsing("Source details", |ui| {
                    for mesh in &asset.meshes {
                        ui.small(format!("Mesh: {}", mesh.name));
                    }
                    for material in &asset.materials {
                        ui.small(format!("Material: {}", material.name));
                    }
                    for skin in &asset.skins {
                        ui.small(format!(
                            "Skin: {} · {} joints",
                            skin.name,
                            skin.joints.len()
                        ));
                    }
                    for camera in &asset.cameras {
                        ui.small(format!("Camera: {}", camera.name));
                    }
                    for light in &asset.lights {
                        ui.small(format!("Light: {}", light.name));
                    }
                    let scene = self.selected_asset_view().unwrap();
                    for camera in &scene.frame.cameras {
                        ui.small(format!("Camera node: {}", asset.nodes[camera.node].name));
                    }
                    for light in &scene.frame.lights {
                        ui.small(format!("Light node: {}", asset.nodes[light.node].name));
                    }
                });
                ui.weak("Source cameras are retained. This viewport uses N3's navigation camera.");
            });
            controls::record(
                &ctx,
                Control::SceneDiagnostics,
                Control::SceneDiagnostics.label(),
                response.header_response.rect,
                true,
            );
        });
    }
}

fn instance_scene(ui: &WorkspaceUi) -> usize {
    ui.selected_asset_view().unwrap().scene
}
