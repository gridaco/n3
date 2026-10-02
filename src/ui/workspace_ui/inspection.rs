//! Workspace fixture setup and observations for executable native tooling.
//! Runtime hosts install resolved snapshots through `install_loaded_document`.
use super::*;

/// The parts of a workspace arrangement that can be reused without changing
/// the document, camera, selection, or edit history. This is intentionally an
/// in-memory snapshot; there is no file format or compatibility contract yet.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SavedLayout {
    pub hierarchy_width: f32,
    pub inspector_width: f32,
}

impl SavedLayout {
    pub const DEFAULT: Self = Self {
        hierarchy_width: DEFAULT_PANEL_WIDTH,
        inspector_width: DEFAULT_PANEL_WIDTH,
    };

    /// A stable, spacious arrangement for the executable guide's HD captures.
    pub const DOCUMENTATION: Self = Self::DEFAULT;

    fn normalized(self) -> Self {
        fn width(value: f32, min: f32, max: f32) -> f32 {
            if value.is_finite() {
                value.clamp(min, max)
            } else {
                DEFAULT_PANEL_WIDTH
            }
        }
        Self {
            hierarchy_width: width(self.hierarchy_width, HIERARCHY_MIN_WIDTH, 420.0),
            inspector_width: width(self.inspector_width, INSPECTOR_MIN_WIDTH, 480.0),
        }
    }
}

impl Default for SavedLayout {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl WorkspaceUi {
    /// Capture the current panel arrangement from egui's live resize state.
    /// The snapshot is deliberately ephemeral and contains no editor state.
    pub fn save_layout(&self, ctx: &egui::Context) -> SavedLayout {
        use egui::containers::panel::PanelState;

        SavedLayout {
            hierarchy_width: PanelState::load(ctx, egui::Id::new("hierarchy"))
                .map_or(DEFAULT_PANEL_WIDTH, |state| state.outer_rect.width()),
            inspector_width: PanelState::load(ctx, egui::Id::new("inspector"))
                .map_or(DEFAULT_PANEL_WIDTH, |state| state.outer_rect.width()),
        }
        .normalized()
    }

    /// Apply a layout before the next UI pass, including panels that egui has
    /// previously resized. The normal resize handles keep working afterward.
    pub fn apply_saved_layout(&mut self, ctx: &egui::Context, layout: SavedLayout) {
        use egui::containers::panel::PanelState;

        let layout = layout.normalized();
        ctx.data_mut(|data| {
            for (id, width) in [
                ("hierarchy", layout.hierarchy_width),
                ("inspector", layout.inspector_width),
            ] {
                // Panel reads only the previous rect's width, then writes
                // the actual on-screen rect during its next layout pass.
                data.insert_persisted(
                    egui::Id::new(id),
                    PanelState {
                        outer_rect: egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(width, 0.0),
                        ),
                    },
                );
            }
        });
        ctx.request_repaint();
    }

    /// Fixture convenience: native I/O callers pass the loader's exact snapshot.
    pub fn install_document(&mut self, path: PathBuf, document: Document) -> Result<(), String> {
        let loaded = if crate::document_io::is_native_path(&path) {
            let loaded = crate::asset_io::load(&path)?;
            if loaded.document != document {
                return Err("The file changed while opening. Open it again.".into());
            }
            loaded
        } else {
            crate::asset_io::LoadedDocument {
                document,
                assets: BTreeMap::new(),
                diagnostics: Vec::new(),
                saved_bytes: None,
            }
        };
        self.install_loaded_document(path, loaded)
    }

    /// Current visible row bounds, keyed by document identity rather than order.
    pub fn layer_row_rect(&self, id: u64) -> Option<egui::Rect> {
        if !self
            .editor
            .document
            .objects
            .iter()
            .any(|object| object.id == id)
        {
            return None;
        }
        self.layer_rows
            .iter()
            .find(|row| row.id == id)
            .map(|row| row.rect)
    }

    pub(crate) fn layer_row_icon(&self, id: u64) -> Option<lucide::Icon> {
        self.layer_rows
            .iter()
            .find(|row| row.id == id)
            .map(|row| row.icon)
    }

    pub fn view_pie_active(&self) -> bool {
        self.pie_input.view_active()
    }

    pub fn shading_pie_active(&self) -> bool {
        self.pie_input.shading_active()
    }
}
