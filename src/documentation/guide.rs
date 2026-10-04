//! N3's adapter for Rust-authored guides. Product input, witnesses, and captures
//! stay in Session; the portable document owns composition and immutable evidence.
use super::{Result, Session};
use crate::controls::Control;
use executable_docs::{Artifact, Binding, BindingValue, Doc, Resource};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Media {
    Still,
    Animation,
}

pub(super) struct Guide {
    pub(super) doc: Doc,
    bindings: BTreeMap<String, Binding>,
    media: BTreeMap<String, Resource>,
}

impl Guide {
    pub(super) fn new(id: &str, title: &str) -> Result<Self> {
        Ok(Self {
            doc: Doc::new(id, title)?,
            bindings: BTreeMap::new(),
            media: BTreeMap::new(),
        })
    }

    fn binding(&mut self, id: String, value: BindingValue) -> Result<Binding> {
        if let Some(binding) = self.bindings.get(&id) {
            return Ok(binding.clone());
        }
        let binding = self.doc.binding(&id, value)?;
        self.bindings.insert(id, binding.clone());
        Ok(binding)
    }

    /// Labels must come from a control actually witnessed by this same replay.
    pub(super) fn control(&mut self, session: &Session<'_>, control: Control) -> Result<Binding> {
        let label = session
            .bindings
            .get(&control)
            .ok_or_else(|| format!("Authored guide control {} was not witnessed", control.id()))?;
        self.binding(
            format!("control-{}", control.id().replace('.', "-")),
            BindingValue::Code(label.clone()),
        )
    }

    /// Shortcut presentation is observed from the production catalog. It does
    /// not deliver input or assert that an action was exercised by this call.
    pub(super) fn shortcut(&mut self, id: &str) -> Result<Binding> {
        let keys = crate::input::bindings::binding(id)?.key_parts();
        self.binding(
            format!("shortcut-{}", id.replace('.', "-")),
            BindingValue::Keys(keys),
        )
    }

    pub(super) fn modifier(&mut self, id: &str) -> Result<Binding> {
        let label = match id {
            "shift" => "Shift",
            "command" => "Command",
            "control" => "Control",
            "alt" => "Option",
            _ => return Err(format!("Unknown documentation modifier: {id}")),
        };
        self.binding(
            format!("modifier-{id}"),
            BindingValue::Keys(vec![label.into()]),
        )
    }

    pub(super) fn capture_tutorial(
        &mut self,
        session: &mut Session<'_>,
        name: &str,
    ) -> Result<Resource> {
        session.capture_tutorial(name)?;
        self.media(session, name, Media::Still)
    }

    pub(super) fn capture_image(
        &mut self,
        session: &mut Session<'_>,
        name: &str,
    ) -> Result<Resource> {
        session.capture_image(name)?;
        self.media(session, name, Media::Still)
    }

    pub(super) fn capture_clip<'a>(
        &mut self,
        session: &mut Session<'a>,
        name: &str,
        spec: super::ClipSpec,
        run: impl FnOnce(&mut Session<'a>) -> Result<()>,
    ) -> Result<Resource> {
        session.capture_clip(name, spec, run)?;
        self.media(session, name, Media::Animation)
    }

    /// Register already captured bytes without another render or clock advance.
    /// N3 additionally distinguishes still/animation uses of the same WebP MIME.
    pub(super) fn media(
        &mut self,
        session: &Session<'_>,
        name: &str,
        kind: Media,
    ) -> Result<Resource> {
        let path = format!("assets/{name}.webp");
        let bytes = session.images.get(&path).ok_or_else(|| {
            format!("Authored guide media {name} was not captured by this scenario")
        })?;
        if session.animations.contains(&path) != (kind == Media::Animation) {
            return Err(format!(
                "Media {name} requires the correct still/animation reference"
            ));
        }
        if let Some(resource) = self.media.get(&path) {
            return Ok(resource.clone());
        }
        let resource = self.doc.resource_at(
            name,
            Artifact::new(
                bytes.clone(),
                "image/webp",
                "webp",
                "n3-headless",
                session.capture.renderer_profile,
            )?,
            &path,
        )?;
        self.media.insert(path, resource.clone());
        Ok(resource)
    }

    pub(super) fn finish(mut self, session: &mut Session<'_>) -> Result<()> {
        let registered: BTreeSet<_> = self.media.keys().cloned().collect();
        let captured: BTreeSet<_> = session.images.keys().cloned().collect();
        executable_docs::lifecycle::check_inventory(&captured, &registered)?;
        if session.authored.is_some() {
            return Err("A scenario must finish exactly one authored guide".into());
        }
        // These entries exist only after Session::require succeeds. Import the
        // already-executed legacy checks; never replace replay with assertions
        // about a prepared fixture. The existing text manifest keeps its facts.
        for (index, _) in session.facts.iter().enumerate() {
            self.doc
                .require(&format!("replay-assert-{index:04}"), true)?;
        }
        let document = self.doc.finish()?;
        // N3's legacy manifest describes every Session capture. This bridge
        // supports public captured media plus private prose notes; a private
        // capture must fail instead of leaking through that legacy inventory.
        // Full portable exports support private resources without this limit.
        for audience in [
            executable_docs::Audience::Reader,
            executable_docs::Audience::Contributor,
        ] {
            executable_docs::lifecycle::compare(
                &session.images,
                &document.resource_files(audience)?,
            ).map_err(|error| format!("N3 authored guides require every capture in the reader narrative and no extra resource files: {error}"))?;
        }
        session.authored = Some(document);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::documentation::{Capture, GENERATED, HEIGHT, WIDTH, hand_tool, root};
    use executable_docs::Audience;

    #[test]
    fn authored_hand_tool_reuses_frozen_evidence_and_keeps_notes_private() {
        let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
        let mut session = Session::new(&mut capture).unwrap();
        hand_tool::run(&mut session).unwrap();
        let clock = session.time;
        let state = session.state.editor.document.clone();
        let captured = session.images.clone();
        let document = session.authored.as_ref().unwrap();
        let reader = document.render_fragment(Audience::Reader).unwrap();
        let contributor = document.render_fragment(Audience::Contributor).unwrap();
        let baseline = std::fs::read_to_string(root().join("docs/guide/hand-tool.md")).unwrap();
        assert_eq!(format!("{GENERATED}{reader}"), baseline);
        let note = "The sampled replay verifies held input";
        assert!(!reader.contains(note));
        assert!(contributor.contains(note));
        assert_eq!(document.render_fragment(Audience::Reader).unwrap(), reader);
        assert_eq!(session.time, clock, "rendering must not advance replay");
        assert_eq!(session.state.editor.document, state);
        assert_eq!(session.images, captured);

        let mut probe = Guide::new("invalid-evidence", "Invalid evidence").unwrap();
        assert!(probe.control(&session, Control::PreferencesClose).is_err());
        assert!(probe.media(&session, "missing", Media::Still).is_err());
        assert!(
            probe
                .media(&session, "hand-tool-ready", Media::Animation)
                .is_err()
        );
        assert!(
            probe
                .media(&session, "hand-tool-pan-and-release", Media::Still)
                .is_err()
        );

        session.authored = None;
        let mut private = Guide::new("private-capture", "Private capture").unwrap();
        for name in [
            "hand-tool-ready",
            "hand-tool-drag",
            "hand-tool-pan-and-release",
        ] {
            let kind = if name == "hand-tool-pan-and-release" {
                Media::Animation
            } else {
                Media::Still
            };
            let resource = private.media(&session, name, kind).unwrap();
            if name == "hand-tool-ready" {
                private.doc.note(&resource).unwrap();
            } else {
                private.doc.embed(&resource, name).unwrap();
            }
        }
        assert!(
            private
                .finish(&mut session)
                .unwrap_err()
                .contains("every capture in the reader narrative")
        );
        assert!(
            session.authored.is_none(),
            "reject private media before any export"
        );
    }
}
