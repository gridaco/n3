//! Project-owned bridge from the same replay/narratives to lgtmd's site host.
use super::{
    GENERATED, Result, Session,
    features::Feature,
    guide::{Guide, Media},
};
use crate::controls::Control;
use lgtmd::{
    Doc, Document, IntoProse,
    transport::{Input, Node, Part},
};

pub(super) fn document(feature: &Feature, session: &mut Session<'_>) -> Result<Document> {
    if let Some(template) = feature.template {
        let mut guide = Guide::new(feature.slug, feature.title)?;
        let mut parts = Vec::new();
        let mut tail = template;
        while let Some(start) = tail.find("{{") {
            parts.push(tail[..start].into_prose());
            let rest = &tail[start + 2..];
            let end = rest.find("}}").ok_or("Unclosed documentation binding")?;
            let (kind, key) = rest[..end]
                .split_once(':')
                .ok_or("Expected {{kind:key}} binding")?;
            parts.push(match kind {
                "control" => guide.control(session, Control::parse(key)?)?.into_prose(),
                "shortcut" => guide.shortcut(key)?.into_prose(),
                "key" => guide.key(key)?.into_prose(),
                "modifier" => guide.modifier(key)?.into_prose(),
                "value" => session
                    .values
                    .get(key)
                    .ok_or_else(|| format!("Unverified documentation value: {key}"))?
                    .into_prose(),
                "image" | "animation" => guide
                    .media(
                        session,
                        key,
                        if kind == "image" {
                            Media::Still
                        } else {
                            Media::Animation
                        },
                    )?
                    .image(&key.replace('-', " ")),
                _ => return Err(format!("Unknown documentation binding kind: {kind}")),
            });
            tail = &rest[end + 2..];
        }
        parts.push(tail.into_prose());
        guide.doc.markdown_parts(parts)?;
        // Import only assertions that the real Session already completed, and
        // require exactly the same public captures in both audience projections.
        guide.finish(session)?;
    }
    session
        .authored
        .clone()
        .ok_or_else(|| format!("{} has no completed site document", feature.slug))
}

pub(super) fn overview(index: &[u8]) -> Result<Document> {
    let text = std::str::from_utf8(index).map_err(|error| error.to_string())?;
    let mut doc = Doc::new("overview", "N3 guide")?;
    doc.markdown(
        text.strip_prefix(GENERATED)
            .ok_or("Missing guide provenance")?,
    )?;
    doc.finish()
}

/// The SDK additionally escapes Markdown punctuation inside its generated HTML
/// labels. Compare the same label spelling using N3's existing HTML convention;
/// narrative text, whitespace, resource paths, and media still compare exactly.
pub(super) fn legacy_fragment(document: &Document) -> Result<String> {
    let mut fragment = document.render_fragment(lgtmd::Audience::Reader)?;
    for (entity, literal) in [
        ("&#96;", "`"),
        ("&#42;", "*"),
        ("&#95;", "_"),
        ("&#126;", "~"),
        ("&#92;", "\\"),
        ("&#91;", "["),
        ("&#93;", "]"),
        ("&#33;", "!"),
    ] {
        for tag in ["code", "kbd"] {
            let opening = format!("<{tag}>");
            let closing = format!("</{tag}>");
            let mut offset = 0;
            while let Some(start) = fragment[offset..].find(&opening) {
                let start = offset + start + opening.len();
                let end = start
                    + fragment[start..]
                        .find(&closing)
                        .ok_or("Unclosed SDK label")?;
                let label = fragment[start..end].replace(entity, literal);
                fragment.replace_range(start..end, &label);
                offset = start + label.len() + closing.len();
            }
        }
    }
    Ok(fragment)
}

/// The full SDK exporter supplies the page title and explicit block anchors.
/// Adapt those presentation details without changing the canonical fragments.
pub(super) fn prepare(documents: Vec<Document>) -> Result<Vec<Document>> {
    let mut input = Input::from_documents(&documents)?;
    for document in &mut input.documents {
        let mut nodes = Vec::new();
        for (index, node) in std::mem::take(&mut document.nodes).into_iter().enumerate() {
            let (id, mut parts) = match node {
                Node::Markdown { id, parts } => (id, parts),
                Node::RawMarkdown { id, text } => (id, vec![Part::Markdown { text }]),
                other => {
                    nodes.push(other);
                    continue;
                }
            };
            if index == 0
                && let Some(Part::Markdown { text }) = parts.first_mut()
                && let Some(rest) = text.strip_prefix("# ")
                && let Some((title, body)) = rest.split_once('\n')
            {
                document.title = title.to_owned();
                *text = body.strip_prefix('\n').unwrap_or(body).to_owned();
            }
            split_sections(&id, parts, &mut nodes);
        }
        document.nodes = nodes;
    }
    input.thaw()
}

fn heading(line: &str) -> Option<(u8, &str)> {
    let level = line.bytes().take_while(|byte| *byte == b'#').count();
    if !(1..=6).contains(&level) {
        return None;
    }
    let text = line[level..].strip_prefix(' ')?.trim_end();
    (!text.is_empty()).then_some((level as u8, text))
}

fn heading_id(text: &str) -> String {
    text.to_ascii_lowercase()
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

fn split_sections(id: &str, parts: Vec<Part>, nodes: &mut Vec<Node>) {
    let mut current = Vec::new();
    let mut segment = 0;
    let mut fence: Option<&str> = None;
    let mut line_start = true;
    let flush = |current: &mut Vec<Part>, segment: &mut usize, nodes: &mut Vec<Node>| {
        if current.is_empty() {
            return;
        }
        nodes.push(Node::Markdown {
            id: if *segment == 0 {
                id.to_owned()
            } else {
                format!("{id}-site-{segment}")
            },
            parts: std::mem::take(current),
        });
        *segment += 1;
    };
    for part in parts {
        let Part::Markdown { text } = part else {
            current.push(part);
            line_start = false;
            continue;
        };
        for line in text.split_inclusive('\n') {
            let marker = if line.starts_with("```") {
                Some("```")
            } else if line.starts_with("~~~") {
                Some("~~~")
            } else {
                None
            };
            if line_start && let Some(marker) = marker {
                fence = if fence == Some(marker) {
                    None
                } else {
                    fence.or(Some(marker))
                };
            }
            if line_start
                && fence.is_none()
                && let Some((level, text)) = heading(line)
            {
                flush(&mut current, &mut segment, nodes);
                nodes.push(Node::Heading {
                    id: heading_id(text),
                    level,
                    text: text.to_owned(),
                });
            } else {
                // README.md is N3's canonical index. Preserve literal fenced
                // examples while routing the existing guide backlink to the site.
                current.push(Part::Markdown {
                    text: if fence.is_none() {
                        line.replace("[Back to guide](README.md)", "[Back to guide](overview.md)")
                    } else {
                        line.to_owned()
                    },
                });
            }
            line_start = line.ends_with('\n');
        }
    }
    flush(&mut current, &mut segment, nodes);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::documentation::{artifacts, features::FEATURES};
    use lgtmd::{Audience, BindingValue, prose, site};

    #[test]
    fn fragment_parity_accounts_only_for_sdk_label_entity_spelling() {
        let mut doc = Doc::new("labels", "Labels").unwrap();
        let key = doc
            .binding("key", BindingValue::Keys(vec!["\\".into()]))
            .unwrap();
        doc.markdown_parts(("&#92; outside ", key)).unwrap();
        let document = doc.finish().unwrap();
        assert_eq!(
            legacy_fragment(&document).unwrap(),
            "&#92; outside <kbd>\\</kbd>"
        );
    }

    #[test]
    fn site_routes_index_and_sections_without_changing_literal_code_or_typed_keys() {
        let mut doc = Doc::new("gizmo", "Axis gizmo").unwrap();
        let key = doc
            .binding("projection-key", BindingValue::Keys(vec!["0".into()]))
            .unwrap();
        doc.markdown_parts(prose!("# Axis gizmo\n\n[Back to guide](README.md)\n\n## Projection\n\nPress {key}.\n\n```text\n## Literal code\n[Back to guide](README.md)\n```\n", key = key).unwrap()).unwrap();
        let original = doc.finish().unwrap();
        let original_fragment = original.render_fragment(Audience::Reader).unwrap();
        let mut index = Doc::new("overview", "N3 guide").unwrap();
        index.markdown("[Projection](gizmo.md#projection)").unwrap();
        let documents = prepare(vec![index.finish().unwrap(), original.clone()]).unwrap();
        let site = site::compile(
            &Input::from_documents(&documents).unwrap(),
            "N3 guide",
            None,
        )
        .unwrap();
        let gizmo = site
            .reader
            .bundle
            .pages
            .iter()
            .find(|page| page.id == "gizmo")
            .unwrap();
        assert_eq!(gizmo.title, "Axis gizmo");
        assert!(gizmo.anchors.iter().any(|anchor| anchor == "projection"));
        let markdown = std::str::from_utf8(&site.markdown.reader["gizmo.md"]).unwrap();
        assert_eq!(markdown.matches("# Axis gizmo").count(), 1);
        assert!(markdown.contains("## Literal code"));
        assert!(markdown.contains("<kbd>0</kbd>"));
        assert!(markdown.contains("](overview.md)"));
        assert!(markdown.contains("[Back to guide](README.md)\n```"));
        assert_eq!(
            original.render_fragment(Audience::Reader).unwrap(),
            original_fragment
        );
    }

    #[test]
    fn complete_site_replay_preserves_guide_and_filters_contributor_notes() {
        let views = artifacts::generate_outputs(false, true).unwrap();
        artifacts::compare(
            &views.reader,
            &artifacts::read_tree(&super::super::root().join("docs/guide")).unwrap(),
        )
        .unwrap();
        let documents = prepare(views.documents).unwrap();
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../../lgtmd.config.json")).unwrap();
        let navigation: Vec<String> = serde_json::from_value(config["navigation"].clone()).unwrap();
        assert_eq!(
            navigation,
            std::iter::once("overview".to_owned())
                .chain(FEATURES.iter().map(|feature| feature.slug.to_owned()))
                .collect::<Vec<_>>()
        );
        let site = site::compile(
            &Input::from_documents(&documents).unwrap(),
            "N3 guide",
            Some(&navigation),
        )
        .unwrap();
        for (path, bytes) in &views.reader {
            if path.starts_with("assets/") {
                assert_eq!(&site.reader.files[path], bytes, "{path}");
            }
        }
        assert_eq!(site.reader.bundle.pages.len(), FEATURES.len() + 1);
        assert!(
            site.reader
                .bundle
                .pages
                .iter()
                .all(|page| page.notes.is_empty())
        );
        assert!(
            site.contributor
                .bundle
                .pages
                .iter()
                .any(|page| !page.notes.is_empty())
        );
        assert!(
            site.reader
                .bundle
                .pages
                .iter()
                .any(|page| page.id == "overview")
        );
    }
}
