//! N3 composition root. Implementation modules are private to this application.
mod documentation;
mod editor;
mod input;
mod model;
mod native;
mod render;
mod settings;
mod theme;
mod ui;

use documentation as doc_harness;
use documentation::{animation as doc_animation, capture as doc_capture, input as doc_input};
use editor::{edit_history, numeric_transform, snapping};
use input::{
    keyboard_input, move_input, navigation_events, navigation_input, navigation_state, pie_input,
    pointer_policy, scroll_input, shortcuts,
};
use model::{document, document_io, mesh, units};
use render::{camera, edit_feedback, object_feedback, orientation, renderer};
use std::path::PathBuf;
use ui::{axis_gizmo, controls, ruler_2d, workspace_ui};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let initial = args.next();
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
        let document = document::load(&path).map_err(std::io::Error::other)?;
        document.validate().map_err(std::io::Error::other)?;
        println!(
            "{}",
            serde_json::json!({"valid":true,"version":document.version,"length_unit":document.length_unit,"objects":document.objects.len()})
        );
        return Ok(());
    }
    if args.next().is_some() {
        return Err("Usage: n3 [PATH] | --check PATH | --docs update|check".into());
    }
    native::run(initial.map(PathBuf::from))
}
