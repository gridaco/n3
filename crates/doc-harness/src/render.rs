use crate::model::{BindingValue, Content, Document, Inline, Kind, Node, Prose, Reference};
use crate::{Audience, ExportLayout, Result};
use pulldown_cmark::{BrokenLink, Event, Options, Parser, Tag};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

/// Keep provenance for the small amount of HTML emitted by typed bindings and
/// anchors. Authored Markdown never acquires that permission by sharing a page.
#[derive(Default)]
struct Rendered {
    text: String,
    html: Vec<Range<usize>>,
}

impl Rendered {
    fn text(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            html: Vec::new(),
        }
    }

    fn html(text: String) -> Self {
        let len = text.len();
        Self {
            text,
            html: std::iter::once(0..len).collect(),
        }
    }

    fn push(&mut self, other: Self) {
        let offset = self.text.len();
        self.text.push_str(&other.text);
        self.html.extend(
            other
                .html
                .into_iter()
                .map(|range| range.start + offset..range.end + offset),
        );
    }

    fn quote(self) -> Self {
        let mut result = Self::default();
        let mut offset = 0;
        for line in self.text.split_inclusive('\n') {
            let start = result.text.len() + 2;
            result.text.push_str("> ");
            result.text.push_str(line);
            for range in &self.html {
                let left = range.start.max(offset);
                let right = range.end.min(offset + line.len());
                if left < right {
                    result
                        .html
                        .push(start + left - offset..start + right - offset);
                }
            }
            offset += line.len();
        }
        // Match str::lines(), used by the original note renderer.
        if result.text.ends_with('\n') {
            result.text.pop();
        }
        result
    }
}

pub(crate) fn markdown_links(text: &str) -> Result<Vec<String>> {
    parsed_links(&Rendered::text(text), &[])
}

/// Parse the bytes that will actually be exported. Every HTML token must come
/// from a generated span, every generated tag must survive Markdown parsing,
/// and code/container blocks cannot consume the next authored node. Plain
/// paragraphs may continue across MarkdownParts nodes, whose whitespace is literal.
fn parsed_links(rendered: &Rendered, boundaries: &[usize]) -> Result<Vec<String>> {
    let text = &rendered.text;
    let mut unresolved = Vec::new();
    let mut callback = |link: BrokenLink<'_>| {
        unresolved.push(link.reference.to_string());
        None
    };
    let mut links = Vec::new();
    let mut html_ranges = Vec::new();
    for (event, range) in Parser::new_with_broken_link_callback(
        text,
        Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH,
        Some(&mut callback),
    )
    .into_offset_iter()
    {
        if matches!(
            &event,
            Event::Code(_)
                | Event::Start(
                    Tag::CodeBlock(_)
                        | Tag::HtmlBlock
                        | Tag::BlockQuote(_)
                        | Tag::List(_)
                        | Tag::Item
                        | Tag::Table(_)
                        | Tag::TableHead
                        | Tag::TableRow
                )
        ) && boundaries
            .iter()
            .any(|boundary| range.start < *boundary && *boundary < range.end)
        {
            return Err(format!(
                "Markdown block crosses a document block boundary at {range:?} ({event:?}); close fences and separate blocks explicitly"
            ));
        }
        match event {
            Event::Start(Tag::Link { dest_url, .. } | Tag::Image { dest_url, .. }) => {
                links.push(dest_url.to_string())
            }
            Event::Html(_) | Event::InlineHtml(_) => {
                if (range.clone()).any(|position| {
                    !text.as_bytes()[position].is_ascii_whitespace()
                        && !rendered
                            .html
                            .iter()
                            .any(|allowed| allowed.contains(&position))
                }) {
                    return Err(
                        "Raw HTML is unsupported in authored Markdown; use typed document blocks"
                            .into(),
                    );
                }
                html_ranges.push(range);
            }
            _ => {}
        }
    }
    if !unresolved.is_empty() {
        return Err(format!(
            "Unresolved Markdown references: {}",
            unresolved.join(", ")
        ));
    }
    if rendered.html.iter().any(|expected| {
        expected.clone().any(|position| {
            !text.as_bytes()[position].is_ascii_whitespace()
                && !html_ranges.iter().any(|actual| actual.contains(&position))
        })
    }) {
        return Err("Markdown swallowed a generated binding or block anchor; typed HTML cannot be placed inside code or HTML syntax".into());
    }
    Ok(links)
}

pub(crate) fn validate_markdown_parts(parts: &Prose) -> Result<()> {
    let template: String = parts
        .0
        .iter()
        .map(|part| match part {
            Inline::Text(text) => text.as_str(),
            _ => "EXECUTABLEDOCREFERENCE",
        })
        .collect();
    markdown_links(&template).map(|_| ())
}

pub(crate) fn validate_resource_path(path: &str) -> Result<()> {
    if path.is_empty()
        || matches!(
            path.split('/').next(),
            Some("manifest.json" | ".ownership.json")
        )
        || path
            .chars()
            .any(|c| c.is_control() || c.is_whitespace() || "\\:%?#()[]<>\"".contains(c))
        || path
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(format!("Unsafe or reserved artifact path: {path:?}"));
    }
    Ok(())
}

pub(crate) fn escape(text: &str) -> String {
    let mut result = String::new();
    for c in text.chars() {
        match c {
            '&' => result.push_str("&amp;"),
            '<' => result.push_str("&lt;"),
            '>' => result.push_str("&gt;"),
            '\\' | '`' | '~' | '*' | '_' | '[' | ']' | '(' | ')' | '#' | '+' | '-' | '!' | '|'
            | '{' | '}' | '.' => {
                result.push('\\');
                result.push(c);
            }
            _ => result.push(c),
        }
    }
    result
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub(crate) fn artifact_path(document: &Document, id: &str) -> String {
    ExportLayout::default().resource_path(document, id)
}

fn node_links(document: &Document, node: &Node, layout: &ExportLayout) -> Result<Vec<String>> {
    match &node.content {
        Content::Markdown(text) => markdown_links(text),
        Content::MarkdownParts(parts) => {
            parsed_links(&render_prose(document, parts, true, layout), &[])
        }
        _ => Ok(Vec::new()),
    }
}

fn visible_nodes(document: &Document, audience: Audience) -> impl Iterator<Item = &Node> {
    document
        .nodes
        .iter()
        .filter(move |node| audience == Audience::Contributor || !node.is_note())
}

fn visible_resources(
    document: &Document,
    audience: Audience,
    layout: &ExportLayout,
) -> Result<BTreeSet<String>> {
    let paths: BTreeMap<_, _> = document
        .artifacts
        .keys()
        .map(|id| (layout.resource_path(document, id), id))
        .collect();
    let mut used = BTreeSet::new();
    for node in visible_nodes(document, audience) {
        for reference in node.references() {
            if reference.kind == Kind::Resource {
                used.insert(reference.id.clone());
            }
        }
        for link in node_links(document, node, layout)? {
            if let Some((path, _)) = local_target(&link, &layout.page_path(&document.id))?
                && let Some(id) = paths.get(&path)
            {
                used.insert((*id).clone());
            }
        }
    }
    Ok(used)
}

fn insert_owned_path(paths: &mut BTreeSet<String>, path: String) -> Result<()> {
    validate_resource_path(&path)?;
    if paths.contains(&path) {
        return Err(format!("Duplicate artifact path: {path}"));
    }
    if paths.iter().any(|existing| {
        path.starts_with(&format!("{existing}/")) || existing.starts_with(&format!("{path}/"))
    }) {
        return Err(format!("Artifact file/directory collision: {path}"));
    }
    paths.insert(path);
    Ok(())
}

pub(crate) fn validate_document(document: &Document) -> Result<()> {
    for audience in [Audience::Reader, Audience::Contributor] {
        let ids: BTreeSet<_> = visible_nodes(document, audience)
            .map(|node| node.id.as_str())
            .collect();
        for node in visible_nodes(document, audience) {
            for reference in node.references() {
                if reference.kind == Kind::Block && !ids.contains(reference.id.as_str()) {
                    return Err(format!(
                        "Visible block {} references excluded block {}",
                        node.id, reference.id
                    ));
                }
            }
        }
    }
    Ok(())
}

fn validate_document_with(document: &Document, layout: &ExportLayout) -> Result<()> {
    validate_document(document)?;
    let mut paths = BTreeSet::from(["manifest.json".to_owned()]);
    insert_owned_path(&mut paths, layout.page_path(&document.id))?;
    for id in document.artifacts.keys() {
        insert_owned_path(&mut paths, layout.resource_path(document, id))?;
    }
    let used = visible_resources(document, Audience::Contributor, layout)?;
    let orphaned: Vec<_> = document
        .artifacts
        .keys()
        .filter(|id| !used.contains(*id))
        .collect();
    if !orphaned.is_empty() {
        return Err(format!("Unreferenced resources: {orphaned:?}"));
    }
    Ok(())
}

fn resource_url(document: &Document, reference: &Reference, layout: &ExportLayout) -> String {
    if reference.kind == Kind::Block {
        return format!("#{}", reference.id);
    }
    // Encode bytes before Markdown parses entities or punctuation. The stored
    // artifact path remains unchanged; ownership checks decode this destination.
    relative_path(
        &layout.page_path(&document.id),
        &layout.resource_path(document, &reference.id),
    )
    .bytes()
    .map(|byte| {
        if byte.is_ascii_alphanumeric() || b"/-._~".contains(&byte) {
            (byte as char).to_string()
        } else {
            format!("%{byte:02X}")
        }
    })
    .collect()
}

fn binding_html(tag: &str, text: &str) -> Rendered {
    let mut rendered = Rendered::html(format!("<{tag}>"));
    rendered.push(Rendered::text(escape_html(text)));
    rendered.push(Rendered::html(format!("</{tag}>")));
    rendered
}

fn inline(document: &Document, reference: &Reference, layout: &ExportLayout) -> Rendered {
    Rendered::text(match reference.kind {
        Kind::Resource | Kind::Block => format!(
            "[{}]({})",
            escape(&reference.id),
            resource_url(document, reference, layout)
        ),
        Kind::Claim => format!("`{}`", reference.id),
        Kind::Value => {
            let actual = &document.checks[&reference.id].actual;
            escape(
                &actual
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| actual.to_string()),
            )
        }
        Kind::Binding => match &document.bindings[&reference.id] {
            BindingValue::Text(text) => escape(text),
            BindingValue::Code(text) => return binding_html("code", text),
            BindingValue::Keys(keys) => {
                let mut rendered = Rendered::default();
                for (index, key) in keys.iter().enumerate() {
                    if index != 0 {
                        rendered.push(Rendered::text(" + "));
                    }
                    rendered.push(binding_html("kbd", key));
                }
                return rendered;
            }
        },
    })
}

fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
        .replace('|', "&#124;")
        .replace('`', "&#96;")
        .replace('*', "&#42;")
        .replace('_', "&#95;")
        .replace('~', "&#126;")
        .replace('\\', "&#92;")
        .replace('[', "&#91;")
        .replace(']', "&#93;")
        .replace('!', "&#33;")
}

fn render_prose(
    document: &Document,
    prose: &Prose,
    markdown: bool,
    layout: &ExportLayout,
) -> Rendered {
    let mut rendered = Rendered::default();
    for part in &prose.0 {
        rendered.push(match part {
            Inline::Text(text) => {
                Rendered::text(if markdown { text.clone() } else { escape(text) })
            }
            Inline::Reference(reference) => inline(document, reference, layout),
            Inline::Image(reference, alt) => Rendered::text(format!(
                "![{}]({})",
                escape(alt),
                resource_url(document, reference, layout)
            )),
            Inline::Link(reference, label) => Rendered::text(format!(
                "[{}]({})",
                escape(label),
                resource_url(document, reference, layout)
            )),
        });
    }
    rendered
}

fn code_fence(text: &str) -> String {
    let longest = text.split(|c| c != '`').map(str::len).max().unwrap_or(0);
    "`".repeat(longest.max(2) + 1)
}

fn render_page(
    document: &Document,
    audience: Audience,
    fragment: bool,
    layout: &ExportLayout,
) -> Result<(String, Vec<String>)> {
    let mut page = if fragment {
        Rendered::default()
    } else {
        Rendered::text(format!("# {}\n\n", escape(&document.title)))
    };
    let mut boundaries = Vec::new();
    for node in visible_nodes(document, audience) {
        boundaries.push(page.text.len());
        if !fragment {
            page.push(Rendered::html(format!("<a id=\"{}\"></a>", node.id)));
            page.push(Rendered::text("\n\n"));
        }
        let content = match &node.content {
            Content::Heading(level, text) => {
                Rendered::text(format!("{} {}", "#".repeat((*level).into()), escape(text)))
            }
            Content::Paragraph(text) => render_prose(document, text, false, layout),
            Content::Markdown(text) => Rendered::text(text.clone()),
            Content::MarkdownParts(parts) => render_prose(document, parts, true, layout),
            Content::Embed(reference, caption) => {
                let image = document.artifacts[&reference.id].mime.starts_with("image/");
                Rendered::text(format!(
                    "{}[{}]({})",
                    if image { "!" } else { "" },
                    escape(caption),
                    resource_url(document, reference, layout)
                ))
            }
            Content::Code(reference, language) => {
                let text = std::str::from_utf8(&document.artifacts[&reference.id].bytes)
                    .map_err(|_| "Code resource must be UTF-8")?;
                let fence = code_fence(text);
                Rendered::text(format!(
                    "{fence}{language}\n{text}{}{fence}",
                    if text.ends_with('\n') { "" } else { "\n" }
                ))
            }
            Content::Note { target, prose } => {
                let mut note = Rendered::text("**Contributor note");
                if let Some(reference) = target {
                    note.push(Rendered::text(" on "));
                    note.push(match reference.kind {
                        Kind::Claim | Kind::Value => Rendered::text(format!("`{}`", reference.id)),
                        _ => inline(document, reference, layout),
                    });
                }
                note.push(Rendered::text(":** "));
                note.push(render_prose(document, prose, false, layout));
                note.quote()
            }
        };
        page.push(content);
        if !matches!(node.content, Content::MarkdownParts(_)) {
            page.push(Rendered::text("\n\n"));
        }
    }
    let links = parsed_links(&page, &boundaries)?;
    Ok((page.text, links))
}

pub(crate) fn render_fragment(document: &Document, audience: Audience) -> Result<String> {
    validate_document_with(document, &ExportLayout::default())?;
    for node in visible_nodes(document, audience) {
        if node
            .references()
            .iter()
            .any(|reference| reference.kind == Kind::Block)
        {
            return Err(
                "Fragment rendering omits block anchors; block references are unsupported".into(),
            );
        }
    }
    render_page(document, audience, true, &ExportLayout::default()).map(|(text, _)| text)
}

pub(crate) fn resource_files(
    document: &Document,
    audience: Audience,
) -> Result<BTreeMap<String, Vec<u8>>> {
    validate_document_with(document, &ExportLayout::default())?;
    Ok(
        visible_resources(document, audience, &ExportLayout::default())?
            .into_iter()
            .map(|id| {
                (
                    artifact_path(document, &id),
                    document.artifacts[&id].bytes.clone(),
                )
            })
            .collect(),
    )
}

/// Decode URL escapes before checking local ownership, so encoded traversal and
/// encoded audience-hidden resource links receive the same checks as plain ones.
fn decode_path(text: &str) -> Result<String> {
    let bytes = text.as_bytes();
    let mut decoded = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if i + 2 >= bytes.len() {
                return Err(format!("Malformed percent escape in link: {text}"));
            }
            let hex =
                std::str::from_utf8(&bytes[i + 1..i + 3]).map_err(|_| "Invalid URL escape")?;
            decoded.push(
                u8::from_str_radix(hex, 16)
                    .map_err(|_| format!("Malformed percent escape in link: {text}"))?,
            );
            i += 3;
        } else {
            decoded.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(decoded).map_err(|_| "Local link is not UTF-8".into())
}

/// Produce a page-relative URL path while keeping ownership paths bundle-relative.
fn relative_path(page: &str, target: &str) -> String {
    let mut directory: Vec<_> = page.split('/').collect();
    directory.pop();
    let target: Vec<_> = target.split('/').collect();
    let common = directory
        .iter()
        .zip(&target)
        .take_while(|(a, b)| a == b)
        .count();
    std::iter::repeat_n("..", directory.len() - common)
        .chain(target[common..].iter().copied())
        .collect::<Vec<_>>()
        .join("/")
}

fn local_target(link: &str, page: &str) -> Result<Option<(String, Option<String>)>> {
    if link.chars().any(char::is_control) {
        return Err("Link contains control characters".into());
    }
    for scheme in ["https://", "http://"] {
        if let Some(rest) = link.strip_prefix(scheme) {
            if rest.is_empty() || rest.starts_with('/') || rest.chars().any(char::is_whitespace) {
                return Err(format!("Malformed external link: {link}"));
            }
            return Ok(None);
        }
    }
    if let Some(rest) = link.strip_prefix("mailto:") {
        if rest.is_empty() || rest.chars().any(char::is_whitespace) {
            return Err(format!("Malformed mail link: {link}"));
        }
        return Ok(None);
    }
    let (path, fragment) = link
        .split_once('#')
        .map_or((link, None), |(path, fragment)| (path, Some(fragment)));
    let path = decode_path(path)?;
    if path.contains(['\\', ':', '?'])
        || path.starts_with('/')
        || (!path.is_empty() && path.split('/').any(str::is_empty))
        || path.chars().any(char::is_control)
    {
        return Err(format!("Unsafe or unsupported local link: {link}"));
    }
    let fragment = fragment.map(decode_path).transpose()?;
    if path.is_empty() && fragment.as_deref().is_none_or(str::is_empty) {
        return Err("An empty link does not identify evidence".into());
    }
    let resolved = if path.is_empty() {
        page.to_owned()
    } else {
        let mut components: Vec<_> = page.split('/').collect();
        components.pop();
        for component in path.split('/') {
            match component {
                "." => {}
                ".." => {
                    if components.pop().is_none() {
                        return Err(format!("Local link escapes the export bundle: {link}"));
                    }
                }
                _ => components.push(component),
            }
        }
        if components.is_empty() {
            return Err(format!("Local link does not identify a file: {link}"));
        }
        components.join("/")
    };
    Ok(Some((resolved, fragment)))
}

/// Render ordinary Markdown, immutable resources and a deterministic JSON manifest.
/// Reader output contains neither contributor blocks nor their exclusive resources.
/// Local link fragments must refer to generated block IDs; raw heading slugs are
/// intentionally not assumed to be identical across downstream site renderers.
pub fn render(documents: &[Document], audience: Audience) -> Result<BTreeMap<String, Vec<u8>>> {
    render_with(documents, audience, &ExportLayout::default())
}

/// Render with project-selected bundle paths. Typed links are relative to their
/// containing page; authored links resolve from that same page directory.
/// The complete inventory remains strict across both audiences.
pub fn render_with(
    documents: &[Document],
    audience: Audience,
    layout: &ExportLayout,
) -> Result<BTreeMap<String, Vec<u8>>> {
    layout.validate_documents(documents)?;
    if documents.is_empty() {
        return Err("At least one document is required".into());
    }
    let mut sorted: Vec<_> = documents.iter().collect();
    sorted.sort_by(|a, b| a.id.cmp(&b.id));
    // Ownership is a property of the completed bundle, including contributor-only
    // resources. Selecting Reader must not hide a duplicate declaration.
    let mut owned_paths = BTreeSet::from(["manifest.json".to_owned()]);
    for document in &sorted {
        for path in std::iter::once(layout.page_path(&document.id)).chain(
            document
                .artifacts
                .keys()
                .map(|id| layout.resource_path(document, id)),
        ) {
            insert_owned_path(&mut owned_paths, path)?;
        }
    }
    let mut files = BTreeMap::new();
    let mut ids = BTreeSet::new();
    let mut profiles = BTreeSet::new();
    let mut manifest_documents = Vec::new();
    let mut manifest_artifacts = Vec::new();
    let mut anchors = BTreeMap::new();
    let mut page_links = BTreeMap::new();
    for document in &sorted {
        if !ids.insert(&document.id) {
            return Err(format!("Duplicate document ID: {}", document.id));
        }
        validate_document_with(document, layout)?;
        let path = layout.page_path(&document.id);
        let (page, links) = render_page(document, audience, false, layout)?;
        let page = page.into_bytes();
        page_links.insert(&document.id, links);
        manifest_artifacts.push(json!({"path":path,"kind":"page","mime":"text/markdown","bytes":page.len(),"sha256":sha256(&page),"document":document.id}));
        if files.insert(path.clone(), page).is_some() {
            return Err(format!("Duplicate artifact path: {path}"));
        }
        anchors.insert(
            path,
            visible_nodes(document, audience)
                .map(|node| node.id.clone())
                .collect::<BTreeSet<_>>(),
        );
        let resources = visible_resources(document, audience, layout)?;
        for id in resources {
            let artifact = &document.artifacts[&id];
            let path = layout.resource_path(document, &id);
            manifest_artifacts.push(json!({"path":path,"id":id,"document":document.id,"kind":"resource","mime":artifact.mime,"producer":artifact.producer,"profile":artifact.profile,"bytes":artifact.bytes.len(),"sha256":sha256(&artifact.bytes)}));
            profiles.insert((artifact.producer.clone(), artifact.profile.clone()));
            if files.insert(path.clone(), artifact.bytes.clone()).is_some() {
                return Err(format!("Duplicate artifact path: {path}"));
            }
        }
        let reader_refs: BTreeSet<_> = document
            .nodes
            .iter()
            .filter(|node| !node.is_note())
            .flat_map(Node::references)
            .map(|r| &r.id)
            .collect();
        let checks: Vec<_> = document.checks.values().map(|check| {
            if audience == Audience::Contributor || reader_refs.contains(&check.id) {
                json!({"id":check.id,"kind":check.kind,"passed":true,"actual":check.actual,"expected":check.expected})
            } else {
                json!({"id":check.id,"kind":check.kind,"passed":true})
            }
        }).collect();
        let blocks: Vec<_> = visible_nodes(document, audience)
            .map(|node| {
                let kind = match &node.content {
                    Content::Heading(..) => "heading",
                    Content::Paragraph(..) => "paragraph",
                    Content::Markdown(..) => "markdown",
                    Content::MarkdownParts(..) => "markdown-parts",
                    Content::Embed(..) => "embed",
                    Content::Code(..) => "code",
                    Content::Note { .. } => "note",
                };
                let references: Vec<_> = node
                    .references()
                    .into_iter()
                    .map(|r| json!({"id":r.id,"kind":r.kind}))
                    .collect();
                json!({"id":node.id,"kind":kind,"references":references})
            })
            .collect();
        let visible_refs: BTreeSet<_> = visible_nodes(document, audience)
            .flat_map(Node::references)
            .map(|r| &r.id)
            .collect();
        let bindings: BTreeMap<_, _> = document
            .bindings
            .iter()
            .filter(|(id, _)| visible_refs.contains(id))
            .collect();
        let mut record =
            json!({"id":document.id,"title":document.title,"checks":checks,"blocks":blocks});
        // Preserve existing document exports exactly when no bindings are used.
        if !bindings.is_empty() {
            record["bindings"] = serde_json::to_value(bindings).map_err(|e| e.to_string())?;
        }
        manifest_documents.push(record);
    }
    // Validate against the actual filtered bundle, never against all collected resources.
    for document in &sorted {
        for link in &page_links[&document.id] {
            if let Some((path, fragment)) = local_target(link, &layout.page_path(&document.id))? {
                if !files.contains_key(&path) {
                    return Err(format!(
                        "Broken or excluded local link in {}: {link}",
                        document.id
                    ));
                }
                if let Some(fragment) = fragment
                    && !anchors
                        .get(&path)
                        .is_some_and(|ids| ids.contains(&fragment))
                {
                    return Err(format!(
                        "Unknown generated block anchor in {}: {link}",
                        document.id
                    ));
                }
            }
        }
    }
    manifest_artifacts.sort_by(|a, b| a["path"].as_str().cmp(&b["path"].as_str()));
    let profiles: Vec<_> = profiles
        .into_iter()
        .map(|(producer, profile)| json!({"producer":producer,"profile":profile}))
        .collect();
    let mut manifest = json!({"schema_version":1,"audience":audience,"profiles":profiles,"documents":manifest_documents,"artifacts":manifest_artifacts});
    manifest.sort_all_objects();
    let mut bytes = serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?;
    bytes.push(b'\n');
    files.insert("manifest.json".into(), bytes);
    Ok(files)
}
