//! N3 composition root. Implementation modules are private to this application.
mod asset_io;
mod documentation;
mod editor;
mod input;
mod model;
mod native;
mod render;
mod scene;
mod scene_view;
mod settings;
mod terminal;
mod theme;
mod ui;
mod workbench;

use asset_io::document as document_io;
use documentation as doc_harness;
use documentation::{animation as doc_animation, capture as doc_capture, input as doc_input};
use editor::{edit_history, numeric_transform, snapping};
use input::{
    keyboard_input, move_input, navigation_events, navigation_input, navigation_state, pie_input,
    pointer_policy, scroll_input, shortcuts,
};
use model::{document, mesh, units};
use render::{camera, edit_feedback, object_feedback, orientation, renderer};
use std::path::PathBuf;
use ui::{axis_gizmo, controls, ruler_2d, workspace_ui};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let mut initial = args.next();
    if initial.as_deref() == Some(std::ffi::OsStr::new("--workbench")) {
        return match (args.next(), args.next()) {
            (None, None) => workbench::run(),
            (Some(mode), None) if mode == "evidence" => {
                workbench::evidence().map_err(|error| std::io::Error::other(error).into())
            }
            _ => Err("Usage: n3 --workbench [evidence]".into()),
        };
    }
    let access = if initial.as_deref() == Some(std::ffi::OsStr::new("--read-only")) {
        initial = args.next();
        editor::EditorAccess::ReadOnly
    } else {
        editor::EditorAccess::ReadWrite
    };
    if initial.as_deref() == Some(std::ffi::OsStr::new("--docs")) {
        let mode = args
            .next()
            .and_then(|mode| mode.into_string().ok())
            .ok_or("Usage: n3 --docs update|check")?;
        if !matches!(mode.as_str(), "update" | "check") || args.next().is_some() {
            return Err("Usage: n3 --docs update|check".into());
        }
        doc_harness::run(&mode).map_err(std::io::Error::other)?;
        return Ok(());
    }
    if initial.as_deref() == Some(std::ffi::OsStr::new("--check")) {
        let path = args
            .next()
            .map(PathBuf::from)
            .ok_or("Usage: n3 --check PATH")?;
        if args.next().is_some() {
            return Err("Usage: n3 --check PATH".into());
        }
        let asset = asset_io::load(&path).map_err(std::io::Error::other)?;
        asset.document.validate().map_err(std::io::Error::other)?;
        let summary = serde_json::json!({
            "valid": true, "version": asset.document.version,
            "length_unit": asset.document.length_unit, "objects": asset.document.objects.len(),
            "linked_assets": asset.assets.len(), "diagnostics": asset.diagnostics,
        });
        println!("{summary}");
        return Ok(());
    }
    if args.next().is_some() {
        return Err("Usage: n3 [--read-only] [PATH] | --check PATH | --docs update|check | --workbench [evidence]".into());
    }
    native::run(initial.map(PathBuf::from), access)
}
