//! Primitive-to-mesh adaptation for the generic derived-edit session. The
//! document retains its source representation until an effective vertex edit.
use std::borrow::Cow;

use crate::document::{Document, EditableMesh, Geometry, Object, Primitive};

use super::derived_edit::{DerivedEdit, DerivedValue};

#[derive(Clone, Debug, PartialEq)]
pub(super) struct GeometryEdit {
    pub object: u64,
    representation: DerivedEdit<Primitive, EditableMesh>,
}

impl GeometryEdit {
    pub fn capture(object: &Object) -> Result<Option<Self>, String> {
        let Geometry::Primitive(source) = &object.geometry else {
            return Ok(None);
        };
        Ok(Some(Self {
            object: object.id,
            representation: DerivedEdit::new(source.clone(), source.evaluate()?),
        }))
    }

    /// Resolve only this edit session's object, before validation/publication
    /// and history comparison. IDs, topology, and positions must match exactly;
    /// this never recognizes an arbitrary mesh as a procedural shape.
    pub fn normalize(&self, document: &mut Document) -> Result<(), String> {
        let Some(object) = document
            .objects
            .iter_mut()
            .find(|object| object.id == self.object)
        else {
            return Ok(());
        };
        // A parameter edit needs a fresh evaluated baseline. The inspector
        // already keeps shape controls outside vertex mode; enforce the same
        // boundary for other callers rather than retain stale provenance.
        if let Geometry::Primitive(source) = &object.geometry
            && source != self.representation.source()
        {
            return Err("Leave vertex edit mode before changing shape parameters.".into());
        }
        if matches!(object.geometry, Geometry::Mesh(_)) {
            let original = Geometry::Primitive(self.representation.source().clone());
            let Geometry::Mesh(mesh) = std::mem::replace(&mut object.geometry, original) else {
                unreachable!();
            };
            object.geometry = match self.representation.resolve(mesh) {
                DerivedValue::Source(source) => Geometry::Primitive(source),
                DerivedValue::Edited(mesh) => Geometry::Mesh(mesh),
            };
        }
        Ok(())
    }

    pub fn evaluated<'a>(
        &'a self,
        geometry: &'a Geometry,
    ) -> Result<Cow<'a, EditableMesh>, String> {
        if let Geometry::Primitive(source) = geometry
            && source == self.representation.source()
        {
            return Ok(Cow::Borrowed(self.representation.baseline()));
        }
        evaluated(geometry)
    }
}

/// Read vertices without replacing their authored source in the document.
pub(super) fn evaluated(geometry: &Geometry) -> Result<Cow<'_, EditableMesh>, String> {
    match geometry {
        Geometry::Primitive(source) => source.evaluate().map(Cow::Owned),
        Geometry::Mesh(mesh) => Ok(Cow::Borrowed(mesh)),
    }
}

/// Only call on a private document candidate. The editor resolves representation
/// and validates the complete candidate before it becomes visible or undoable.
pub(super) fn editable(geometry: &mut Geometry) -> Result<&mut EditableMesh, String> {
    if let Geometry::Primitive(source) = geometry {
        *geometry = Geometry::Mesh(source.evaluate()?);
    }
    let Geometry::Mesh(mesh) = geometry else {
        unreachable!()
    };
    Ok(mesh)
}
