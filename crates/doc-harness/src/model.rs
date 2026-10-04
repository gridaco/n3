use crate::Result;
use serde::Serialize;
use serde_json::Value;
use std::{
    collections::BTreeMap,
    marker::PhantomData,
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_OWNER: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Audience {
    Reader,
    Contributor,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Kind {
    Block,
    Claim,
    Value,
    Resource,
    Binding,
}

#[derive(Clone, Debug)]
pub struct Reference {
    pub(crate) owner: u64,
    pub(crate) id: String,
    pub(crate) kind: Kind,
}

/// A handle can only originate from this crate and the document that owns it.
pub trait Target: private::Sealed {
    #[doc(hidden)]
    fn reference(&self) -> Reference;
}

mod private {
    pub trait Sealed {}
}

macro_rules! handle {
    ($name:ident) => {
        #[derive(Clone, Debug)]
        pub struct $name(pub(crate) Reference);
        impl private::Sealed for $name {}
        impl Target for $name {
            fn reference(&self) -> Reference {
                self.0.clone()
            }
        }
        impl $name {
            pub fn id(&self) -> &str {
                &self.0.id
            }
        }
        impl IntoProse for &$name {
            fn into_prose(self) -> Prose {
                Prose(vec![Inline::Reference(self.reference())])
            }
        }
        impl IntoProse for $name {
            fn into_prose(self) -> Prose {
                (&self).into_prose()
            }
        }
    };
}

handle!(Block);
handle!(Claim);
handle!(Resource);
handle!(Binding);

impl Block {
    /// Link to this block with a reader-facing label. Reader content cannot link
    /// to contributor-only notes.
    pub fn link(&self, label: &str) -> Prose {
        Prose(vec![Inline::Link(self.reference(), label.into())])
    }
}

/// An observed application label, not a behavioral assertion. The host adapter
/// must establish that the control or canonical shortcut was actually observed.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "lowercase")]
pub enum BindingValue {
    Text(String),
    Code(String),
    Keys(Vec<String>),
}

impl Resource {
    /// Place this resource as an image inline. The owning builder checks its MIME.
    pub fn image(&self, alt: &str) -> Prose {
        Prose(vec![Inline::Image(self.reference(), alt.into())])
    }

    /// Link to this resource using a reader-facing label.
    pub fn link(&self, label: &str) -> Prose {
        Prose(vec![Inline::Link(self.reference(), label.into())])
    }
}

/// The serialized observation is frozen when the equality check succeeds.
#[derive(Clone, Debug)]
pub struct Checked<T> {
    reference: Reference,
    observed: Value,
    marker: PhantomData<fn() -> T>,
}

impl<T> Checked<T> {
    pub fn id(&self) -> &str {
        &self.reference.id
    }
    pub fn observed(&self) -> &Value {
        &self.observed
    }
}
impl<T> private::Sealed for Checked<T> {}
impl<T> Target for Checked<T> {
    fn reference(&self) -> Reference {
        self.reference.clone()
    }
}
impl<T> IntoProse for &Checked<T> {
    fn into_prose(self) -> Prose {
        Prose(vec![Inline::Reference(self.reference())])
    }
}
impl<T> IntoProse for Checked<T> {
    fn into_prose(self) -> Prose {
        (&self).into_prose()
    }
}

#[derive(Clone, Debug)]
pub(crate) enum Inline {
    Text(String),
    Reference(Reference),
    Image(Reference, String),
    Link(Reference, String),
}

#[derive(Clone, Debug)]
pub struct Prose(pub(crate) Vec<Inline>);

/// Text is escaped as literal prose; typed references are resolved by the owner.
/// Tuples permit mixed text and references without placeholder names.
pub trait IntoProse {
    fn into_prose(self) -> Prose;
}
impl IntoProse for Prose {
    fn into_prose(self) -> Prose {
        self
    }
}
impl IntoProse for &str {
    fn into_prose(self) -> Prose {
        Prose(vec![Inline::Text(self.into())])
    }
}
impl IntoProse for String {
    fn into_prose(self) -> Prose {
        Prose(vec![Inline::Text(self)])
    }
}
impl IntoProse for &String {
    fn into_prose(self) -> Prose {
        self.as_str().into_prose()
    }
}
impl<T: IntoProse> IntoProse for Vec<T> {
    fn into_prose(self) -> Prose {
        Prose(
            self.into_iter()
                .flat_map(|part| part.into_prose().0)
                .collect(),
        )
    }
}
macro_rules! tuple_prose {
    ($($ty:ident:$field:tt),+) => {
        impl<$($ty: IntoProse),+> IntoProse for ($($ty,)+) {
            fn into_prose(self) -> Prose {
                let mut parts = Vec::new();
                $(parts.extend(self.$field.into_prose().0);)+
                Prose(parts)
            }
        }
    };
}
tuple_prose!(A:0, B:1);
tuple_prose!(A:0, B:1, C:2);
tuple_prose!(A:0, B:1, C:2, D:3);
tuple_prose!(A:0, B:1, C:2, D:3, E:4);
tuple_prose!(A:0, B:1, C:2, D:3, E:4, F:5);
tuple_prose!(A:0, B:1, C:2, D:3, E:4, F:5, G:6);
tuple_prose!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7);

/// Owned bytes and producer metadata. Files are read immediately, never at render time.
#[derive(Clone, Debug)]
pub struct Artifact {
    pub(crate) bytes: Vec<u8>,
    pub(crate) mime: String,
    pub(crate) extension: String,
    pub(crate) producer: String,
    pub(crate) profile: String,
}

impl Artifact {
    pub fn new(
        bytes: impl Into<Vec<u8>>,
        mime: &str,
        extension: &str,
        producer: &str,
        profile: &str,
    ) -> Result<Self> {
        let (major, minor) = mime
            .split_once('/')
            .ok_or("MIME type requires type/subtype")?;
        let token = |s: &str| {
            !s.is_empty()
                && s.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"!#$&^_.+-".contains(&b))
        };
        if !token(major) || !token(minor) {
            return Err("MIME type must be a concrete type/subtype without parameters".into());
        }
        if extension.is_empty()
            || !extension
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        {
            return Err(
                "Artifact extension must contain lowercase letters/digits without a dot".into(),
            );
        }
        for (label, value) in [("producer", producer), ("profile", profile)] {
            if value.is_empty() || value.chars().any(char::is_control) {
                return Err(format!(
                    "Artifact {label} must be nonempty and contain no control characters"
                ));
            }
        }
        Ok(Self {
            bytes: bytes.into(),
            mime: mime.into(),
            extension: extension.into(),
            producer: producer.into(),
            profile: profile.into(),
        })
    }

    pub fn from_file(
        path: impl AsRef<Path>,
        mime: &str,
        extension: &str,
        producer: &str,
        profile: &str,
    ) -> Result<Self> {
        let bytes = std::fs::read(path.as_ref())
            .map_err(|e| format!("Cannot freeze artifact {}: {e}", path.as_ref().display()))?;
        Self::new(bytes, mime, extension, producer, profile)
    }
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct Check {
    pub id: String,
    pub kind: String,
    pub actual: Value,
    pub expected: Value,
}

#[derive(Clone, Debug)]
pub(crate) enum Content {
    Heading(u8, String),
    Paragraph(Prose),
    Markdown(String),
    MarkdownParts(Prose),
    Embed(Reference, String),
    Code(Reference, String),
    Note {
        target: Option<Reference>,
        prose: Prose,
    },
}

#[derive(Clone, Debug)]
pub(crate) struct Node {
    pub id: String,
    pub content: Content,
}

impl Node {
    pub(crate) fn is_note(&self) -> bool {
        matches!(self.content, Content::Note { .. })
    }
    pub(crate) fn references(&self) -> Vec<&Reference> {
        let mut refs = Vec::new();
        match &self.content {
            Content::Paragraph(prose)
            | Content::MarkdownParts(prose)
            | Content::Note { prose, .. } => {
                refs.extend(prose.0.iter().filter_map(|part| match part {
                    Inline::Reference(r) | Inline::Image(r, _) | Inline::Link(r, _) => Some(r),
                    _ => None,
                }));
            }
            Content::Embed(r, _) | Content::Code(r, _) => refs.push(r),
            _ => {}
        }
        if let Content::Note {
            target: Some(target),
            ..
        } = &self.content
        {
            refs.push(target);
        }
        refs
    }
}

/// An immutable successfully completed scenario/document.
#[derive(Clone, Debug)]
pub struct Document {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) nodes: Vec<Node>,
    pub(crate) checks: BTreeMap<String, Check>,
    pub(crate) artifacts: BTreeMap<String, Artifact>,
    pub(crate) resource_paths: BTreeMap<String, String>,
    pub(crate) bindings: BTreeMap<String, BindingValue>,
}

impl Document {
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn render_many(
        documents: &[Self],
        audience: Audience,
    ) -> Result<BTreeMap<String, Vec<u8>>> {
        crate::render(documents, audience)
    }

    /// Export a complete bundle using project-selected page and resource paths.
    /// Checks ownership, audience visibility and links against the selected layout.
    pub fn render_many_with(
        documents: &[Self],
        audience: Audience,
        layout: &crate::ExportLayout,
    ) -> Result<BTreeMap<String, Vec<u8>>> {
        crate::render_with(documents, audience, layout)
    }

    /// Export this document as a complete bundle with a selected layout.
    pub fn render_with(
        &self,
        audience: Audience,
        layout: &crate::ExportLayout,
    ) -> Result<BTreeMap<String, Vec<u8>>> {
        Self::render_many_with(std::slice::from_ref(self), audience, layout)
    }

    /// Compose an existing host-owned Markdown page without adding a title or
    /// block anchors. The host must validate links against its complete bundle.
    /// Handles, resources, visibility and binding kinds remain strictly checked.
    pub fn render_fragment(&self, audience: Audience) -> Result<String> {
        crate::render::render_fragment(self, audience)
    }

    /// Copy the frozen resources referenced by this audience, keyed by export path.
    /// Fragment adapters must use the same audience for page text and resources.
    /// This adds no page or manifest and never reruns a resource producer.
    pub fn resource_files(&self, audience: Audience) -> Result<BTreeMap<String, Vec<u8>>> {
        crate::render::resource_files(self, audience)
    }
}

pub struct Doc {
    owner: u64,
    ids: BTreeMap<String, Kind>,
    document: Document,
    errors: Vec<String>,
}

pub(crate) fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 100
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

impl Doc {
    pub fn new(id: &str, title: &str) -> Result<Self> {
        if !valid_id(id) {
            return Err(format!("Invalid document ID: {id:?}"));
        }
        if title.trim().is_empty() || title.chars().any(char::is_control) {
            return Err("Document title must be nonempty single-line text".into());
        }
        Ok(Self {
            owner: NEXT_OWNER.fetch_add(1, Ordering::Relaxed),
            ids: BTreeMap::new(),
            document: Document {
                id: id.into(),
                title: title.into(),
                nodes: Vec::new(),
                checks: BTreeMap::new(),
                artifacts: BTreeMap::new(),
                resource_paths: BTreeMap::new(),
                bindings: BTreeMap::new(),
            },
            errors: Vec::new(),
        })
    }

    fn guarded<T>(&mut self, result: Result<T>) -> Result<T> {
        result.map_err(|error| {
            let contextual = format!("Document {}: {error}", self.document.id);
            self.errors.push(contextual.clone());
            contextual
        })
    }

    fn reserve(&mut self, id: &str, kind: Kind) -> Result<Reference> {
        if !valid_id(id) {
            return Err(format!("Invalid evidence ID: {id:?}"));
        }
        if self.ids.contains_key(id) {
            return Err(format!("Duplicate evidence ID: {id}"));
        }
        self.ids.insert(id.into(), kind.clone());
        Ok(Reference {
            owner: self.owner,
            id: id.into(),
            kind,
        })
    }

    fn validate_reference(&self, reference: &Reference) -> Result<()> {
        if reference.owner != self.owner {
            return Err(format!("Foreign document/run handle: {}", reference.id));
        }
        if self.ids.get(&reference.id) != Some(&reference.kind) {
            return Err(format!("Missing or wrong-kind reference: {}", reference.id));
        }
        Ok(())
    }

    fn append(&mut self, content: Content) -> Result<Block> {
        self.append_with_id(None, content)
    }

    fn append_with_id(&mut self, id: Option<&str>, content: Content) -> Result<Block> {
        let result = (|| {
            let note = matches!(content, Content::Note { .. });
            let count = self
                .document
                .nodes
                .iter()
                .filter(|node| node.is_note() == note)
                .count();
            let prefix = if note { "note" } else { "block" };
            let node = Node {
                id: id
                    .map(str::to_owned)
                    .unwrap_or_else(|| format!("{prefix}-{:04}", count + 1)),
                content,
            };
            for reference in node.references() {
                self.validate_reference(reference)?;
            }
            if let Content::Paragraph(prose)
            | Content::MarkdownParts(prose)
            | Content::Note { prose, .. } = &node.content
            {
                for part in &prose.0 {
                    if let Inline::Image(reference, _) = part
                        && !self.document.artifacts[&reference.id]
                            .mime
                            .starts_with("image/")
                    {
                        return Err(format!(
                            "Resource {} is not declared as an image",
                            reference.id
                        ));
                    }
                }
            }
            let reference = self.reserve(&node.id, Kind::Block)?;
            self.document.nodes.push(node);
            Ok(Block(reference))
        })();
        self.guarded(result)
    }

    pub fn heading(&mut self, level: u8, text: &str) -> Result<Block> {
        self.append_heading(None, level, text)
    }

    /// Give a section an explicit anchor that survives inserting other blocks.
    /// IDs share the document's evidence namespace; duplicates fail. Other
    /// headings retain their positional IDs unless explicitly named here.
    pub fn heading_with_id(&mut self, id: &str, level: u8, text: &str) -> Result<Block> {
        self.append_heading(Some(id), level, text)
    }

    fn append_heading(&mut self, id: Option<&str>, level: u8, text: &str) -> Result<Block> {
        if !(1..=6).contains(&level) || text.trim().is_empty() || text.chars().any(char::is_control)
        {
            return self.guarded(Err(
                "Heading requires level 1..6 and nonempty single-line text".into(),
            ));
        }
        self.append_with_id(id, Content::Heading(level, text.into()))
    }

    pub fn paragraph(&mut self, prose: impl IntoProse) -> Result<Block> {
        self.append(Content::Paragraph(prose.into_prose()))
    }

    /// CommonMark plus tables and strikethrough. Reference links and code are parsed
    /// by pulldown-cmark; unresolved references and raw HTML fail explicitly.
    /// Local links target emitted files or explicit block anchors. External
    /// http(s)/mailto links are not network-verified.
    pub fn markdown(&mut self, text: &str) -> Result<Block> {
        let parsed = crate::render::markdown_links(text);
        if let Err(error) = parsed {
            return self.guarded(Err(error));
        }
        self.append(Content::Markdown(text.into()))
    }

    /// Preserve literal Markdown bytes while inserting typed values, bindings and
    /// resource views. No separator is appended. Literal HTML is still rejected;
    /// only typed Code/Keys bindings can generate the corresponding safe HTML.
    pub fn markdown_parts(&mut self, parts: impl IntoProse) -> Result<Block> {
        let parts = parts.into_prose();
        let validation = crate::render::validate_markdown_parts(&parts);
        self.guarded(validation)?;
        self.append(Content::MarkdownParts(parts))
    }

    pub fn binding(&mut self, id: &str, value: BindingValue) -> Result<Binding> {
        let result = (|| {
            match &value {
                BindingValue::Text(text) | BindingValue::Code(text) => {
                    if text.is_empty() || text.chars().any(char::is_control) {
                        return Err("Binding labels must be nonempty single-line text".into());
                    }
                }
                BindingValue::Keys(keys) => {
                    if keys.is_empty()
                        || keys
                            .iter()
                            .any(|text| text.is_empty() || text.chars().any(char::is_control))
                    {
                        return Err("Key bindings require nonempty single-line key labels".into());
                    }
                }
            }
            let reference = self.reserve(id, Kind::Binding)?;
            self.document.bindings.insert(id.into(), value);
            Ok(Binding(reference))
        })();
        self.guarded(result)
    }

    pub fn note(&mut self, prose: impl IntoProse) -> Result<Block> {
        self.append(Content::Note {
            target: None,
            prose: prose.into_prose(),
        })
    }

    pub fn note_on(&mut self, target: &impl Target, prose: impl IntoProse) -> Result<Block> {
        self.append(Content::Note {
            target: Some(target.reference()),
            prose: prose.into_prose(),
        })
    }

    pub fn expect_eq<T: PartialEq + Serialize>(
        &mut self,
        id: &str,
        actual: T,
        expected: T,
    ) -> Result<Checked<T>> {
        let result = (|| {
            let reference = self.reserve(id, Kind::Value)?;
            let mut actual_json = serde_json::to_value(&actual)
                .map_err(|e| format!("Cannot freeze observed value {id}: {e}"))?;
            let mut expected_json = serde_json::to_value(&expected)
                .map_err(|e| format!("Cannot freeze expected value {id}: {e}"))?;
            // Cargo can unify serde_json's preserve_order feature through the
            // host. Freeze all object keys in canonical order in either mode.
            actual_json.sort_all_objects();
            expected_json.sort_all_objects();
            if actual != expected {
                return Err(format!(
                    "Check {id} failed: observed {actual_json}, expected {expected_json}"
                ));
            }
            self.document.checks.insert(
                id.into(),
                Check {
                    id: id.into(),
                    kind: "equality".into(),
                    actual: actual_json.clone(),
                    expected: expected_json,
                },
            );
            Ok(Checked {
                reference,
                observed: actual_json,
                marker: PhantomData,
            })
        })();
        self.guarded(result)
    }

    pub fn require(&mut self, id: &str, condition: bool) -> Result<Claim> {
        let result = (|| {
            let reference = self.reserve(id, Kind::Claim)?;
            if !condition {
                return Err(format!("Claim {id} failed"));
            }
            self.document.checks.insert(
                id.into(),
                Check {
                    id: id.into(),
                    kind: "predicate".into(),
                    actual: Value::Bool(true),
                    expected: Value::Bool(true),
                },
            );
            Ok(Claim(reference))
        })();
        self.guarded(result)
    }

    pub fn resource(&mut self, id: &str, artifact: Artifact) -> Result<Resource> {
        let result = (|| {
            let reference = self.reserve(id, Kind::Resource)?;
            self.document.artifacts.insert(id.into(), artifact);
            Ok(Resource(reference))
        })();
        self.guarded(result)
    }

    /// Register a resource at a safe host-owned relative path. The complete
    /// renderer also rejects collisions across documents and generated pages.
    pub fn resource_at(&mut self, id: &str, artifact: Artifact, path: &str) -> Result<Resource> {
        let result = (|| {
            crate::render::validate_resource_path(path)?;
            if self
                .document
                .resource_paths
                .values()
                .any(|existing| existing == path)
            {
                return Err(format!("Duplicate artifact path: {path}"));
            }
            let reference = self.reserve(id, Kind::Resource)?;
            self.document.resource_paths.insert(id.into(), path.into());
            self.document.artifacts.insert(id.into(), artifact);
            Ok(Resource(reference))
        })();
        self.guarded(result)
    }

    /// Register and place a text resource without replacing its producer metadata.
    /// The returned handle can be referenced or annotated again without executing
    /// the producer or copying the artifact into the document a second time.
    pub fn resource_code(
        &mut self,
        id: &str,
        artifact: Artifact,
        language: &str,
    ) -> Result<Resource> {
        let resource = self.resource(id, artifact)?;
        self.code(&resource, language)?;
        Ok(resource)
    }

    /// Freeze UTF-8 text with the built-in text producer.
    pub fn text(&mut self, id: &str, text: impl Into<String>) -> Result<Resource> {
        let artifact = Artifact::new(
            text.into().into_bytes(),
            "text/plain",
            "txt",
            "executable-docs/text",
            "utf8-v1",
        );
        let artifact = self.guarded(artifact)?;
        self.resource(id, artifact)
    }

    /// Freeze a JSON value with the built-in deterministic JSON producer.
    pub fn json(&mut self, id: &str, value: &impl Serialize) -> Result<Resource> {
        let result = (|| {
            // Value alone is not sorted when a host enables preserve_order.
            // Sort recursively, including objects inside arrays, before freezing.
            let mut value = serde_json::to_value(value).map_err(|e| e.to_string())?;
            value.sort_all_objects();
            let mut bytes = serde_json::to_vec_pretty(&value).map_err(|e| e.to_string())?;
            bytes.push(b'\n');
            Artifact::new(
                bytes,
                "application/json",
                "json",
                "executable-docs/json",
                "json-pretty-v1",
            )
        })();
        let artifact = self.guarded(result)?;
        self.resource(id, artifact)
    }

    pub fn text_code(
        &mut self,
        id: &str,
        text: impl Into<String>,
        language: &str,
    ) -> Result<Resource> {
        let resource = self.text(id, text)?;
        self.code(&resource, language)?;
        Ok(resource)
    }

    pub fn json_code(&mut self, id: &str, value: &impl Serialize) -> Result<Resource> {
        let resource = self.json(id, value)?;
        self.code(&resource, "json")?;
        Ok(resource)
    }

    pub fn embed(&mut self, resource: &Resource, caption: &str) -> Result<Block> {
        self.append(Content::Embed(resource.reference(), caption.into()))
    }

    pub fn code(&mut self, resource: &Resource, language: &str) -> Result<Block> {
        let result = (|| {
            self.validate_reference(&resource.0)?;
            if !language
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_+-".contains(&b))
            {
                return Err("Code language must be an ASCII identifier".into());
            }
            let artifact = &self.document.artifacts[resource.id()];
            if !(artifact.mime.starts_with("text/") || artifact.mime == "application/json") {
                return Err(format!(
                    "Resource {} is not declared as text/JSON",
                    resource.id()
                ));
            }
            std::str::from_utf8(&artifact.bytes)
                .map_err(|_| format!("Resource {} is not UTF-8", resource.id()))?;
            Ok(())
        })();
        self.guarded(result)?;
        self.append(Content::Code(resource.reference(), language.into()))
    }

    /// Freeze successful execution and intrinsic references. Export validates
    /// layout-dependent resource ownership and local links against its complete
    /// bundle; finishing alone does not mean the document is publishable.
    pub fn finish(self) -> Result<Document> {
        if !self.errors.is_empty() {
            return Err(format!(
                "Document {} is tainted by failed operations: {}",
                self.document.id,
                self.errors.join("; ")
            ));
        }
        if self.document.checks.is_empty() {
            return Err(format!(
                "Document {} has no executed checks",
                self.document.id
            ));
        }
        // Publication paths and cross-document links depend on the selected export layout.
        crate::render::validate_document(&self.document)?;
        Ok(self.document)
    }
}
