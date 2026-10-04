//! Strict generated-tree ownership, bindings, validation and publication.
use super::*;
use executable_docs::{
    Audience,
    lifecycle::{self, Ownership},
};

pub(super) fn render_template(
    template: &str,
    bindings: &BTreeMap<Control, String>,
    values: &BTreeMap<String, String>,
    images: &Artifacts,
    animations: &BTreeSet<String>,
) -> Result<String> {
    let mut result = GENERATED.to_owned();
    let mut tail = template;
    let mut used_images = BTreeSet::new();
    while let Some(start) = tail.find("{{") {
        result.push_str(&tail[..start]);
        let rest = &tail[start + 2..];
        let end = rest.find("}}").ok_or("Unclosed documentation binding")?;
        let (kind, key) = rest[..end]
            .split_once(':')
            .ok_or("Expected {{kind:key}} binding")?;
        match kind {
            "control" => {
                let control = Control::parse(key)?;
                let label = bindings.get(&control).ok_or_else(|| {
                    format!("Documented control {key} was not witnessed by this feature's scenario")
                })?;
                result.push_str(&format!("<code>{}</code>", escape_html(label)));
            }
            "shortcut" => result.push_str(&shortcut_markup(key)?),
            "key" => {
                let key = egui::Key::from_name(key)
                    .ok_or_else(|| format!("Unknown literal documentation key: {key}"))?;
                result.push_str(&keycaps(&[key.name().to_owned()]));
            }
            "modifier" => {
                let label = match key {
                    "shift" => "Shift",
                    "command" => "Command",
                    "control" => "Control",
                    "alt" => "Option",
                    _ => return Err(format!("Unknown documentation modifier: {key}")),
                };
                result.push_str(&keycaps(&[label.to_owned()]));
            }
            "value" => result.push_str(
                values
                    .get(key)
                    .ok_or_else(|| format!("Unverified documentation value: {key}"))?,
            ),
            "image" | "animation" => {
                let path = format!("assets/{key}.webp");
                validate_path(&path)?;
                if !images.contains_key(&path) {
                    return Err(format!(
                        "Screenshot {key} was not captured by this scenario"
                    ));
                }
                if animations.contains(&path) != (kind == "animation") {
                    return Err(format!(
                        "Media {key} requires the correct image or animation binding"
                    ));
                }
                used_images.insert(path.clone());
                result.push_str(&format!("![{}]({path})", key.replace('-', " ")));
            }
            _ => return Err(format!("Unknown documentation binding kind: {kind}")),
        }
        tail = &rest[end + 2..];
    }
    if tail.contains("}}") {
        return Err("Unmatched documentation binding terminator".into());
    }
    result.push_str(tail);
    if images.keys().any(|key| !used_images.contains(key)) {
        return Err("Scenario captured an unreferenced screenshot".into());
    }
    Ok(result)
}

/// Shortcut spelling comes from the binding used by production input routing.
/// Templates name the action/variant, never the current key or a second keymap.
pub(super) fn shortcut_markup(id: &str) -> Result<String> {
    Ok(keycaps(&crate::input::bindings::binding(id)?.key_parts()))
}

fn keycaps(parts: &[String]) -> String {
    parts
        .iter()
        .map(|part| format!("<kbd>{}</kbd>", escape_html(part)))
        .collect::<Vec<_>>()
        .join(" + ")
}

fn escape_html(text: &str) -> String {
    // Pipes must also be escaped because bindings can occur in Markdown tables.
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
        .replace('|', "&#124;")
}

pub(super) fn generate() -> Result<Artifacts> {
    Ok(generate_views(false)?.reader)
}

struct Views {
    reader: Artifacts,
    contributor: Option<Artifacts>,
}

/// Both views are rendered from the same completed sessions and captured bytes.
/// Ordinary checks only render the published reader view.
fn generate_views(include_contributor: bool) -> Result<Views> {
    validate_feature_inventory()?;
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT))?;
    let (media_width, media_height) = capture.framed_dimensions();
    let mut result = Artifacts::new();
    let mut contributor = include_contributor.then(Artifacts::new);
    let mut manifest = format!(
        "n3 executable documentation v6\nrenderer: {}\nui-platform: macOS\napp-canvas: {WIDTH}x{HEIGHT}, 1 pixel/point, RGBA8Unorm\nmedia-canvas: {media_width}x{media_height}, lossless WebP\nframe-template: {}\nbaseline: Light theme; 240-point side panels\nclock: explicit scenario time; no wall clock\n",
        capture.renderer_profile,
        capture.frame_template_name(),
    );
    for binding in crate::input::bindings::BINDINGS {
        manifest.push_str(&format!(
            "binding: {} = {} ({:?})\n",
            binding.id,
            binding.label(),
            binding.trigger
        ));
    }
    println!("docs renderer: {}", capture.adapter);
    let mut index = format!(
        "{GENERATED}# N3 guide\n\nGuide to N3, the minimal mesh editor. Import OBJ geometry, insert parametric shapes, edit vertices, and save text documents.\n\n"
    );
    for feature in FEATURES {
        println!("docs: {}", feature.slug);
        let mut session = Session::new(&mut capture)?;
        (feature.scenario)(&mut session).map_err(|error| format!("{}: {error}", feature.slug))?;
        if session.facts.is_empty() {
            return Err(format!("{} has no behavioral assertions", feature.slug));
        }
        let page = |audience| match (feature.template, session.authored.as_ref()) {
            (Some(template), None) => render_template(
                template,
                &session.bindings,
                &session.values,
                &session.images,
                &session.animations,
            ),
            (None, Some(document)) if document.id() == feature.slug => Ok(format!(
                "{GENERATED}{}",
                document.render_fragment(audience)?
            )),
            _ => Err(format!(
                "{} must own exactly one narrative source",
                feature.slug
            )),
        };
        insert(
            &mut result,
            format!("{}.md", feature.slug),
            page(Audience::Reader)?.into_bytes(),
        )?;
        if let Some(contributor) = contributor.as_mut() {
            insert(
                contributor,
                format!("{}.md", feature.slug),
                page(Audience::Contributor)?.into_bytes(),
            )?;
        }
        index.push_str(&format!("- [{}]({}.md)\n", feature.title, feature.slug));
        manifest.push_str(&format!("\nfeature: {}\n", feature.slug));
        for (id, path) in session.bindings {
            manifest.push_str(&format!("control: {} = {path}\n", id.id()));
        }
        for (id, label) in session.shortcut_bindings {
            manifest.push_str(&format!("shortcut-replay: {id} = {label}\n"));
        }
        for (key, value) in session.values {
            manifest.push_str(&format!("value: {key} = {value}\n"));
        }
        for fact in session.facts {
            manifest.push_str(&format!("assert: {fact}\n"));
        }
        for (path, bytes) in session.images {
            let kind = if session.animations.contains(&path) {
                "animation"
            } else {
                "image"
            };
            manifest.push_str(&format!("{kind}: {path} ({} bytes)\n", bytes.len()));
            manifest.push_str(&format!("input: {}\n", session.image_inputs[&path]));
            if let Some(contributor) = contributor.as_mut() {
                insert(contributor, path.clone(), bytes.clone())?;
            }
            insert(&mut result, path, bytes)?;
        }
    }
    if let Some(contributor) = contributor.as_mut() {
        insert(contributor, "README.md".into(), index.as_bytes().to_vec())?;
        insert(
            contributor,
            "manifest.txt".into(),
            manifest.as_bytes().to_vec(),
        )?;
        validate_links(contributor)?;
    }
    insert(&mut result, "README.md".into(), index.into_bytes())?;
    insert(&mut result, "manifest.txt".into(), manifest.into_bytes())?;
    validate_links(&result)?;
    Ok(Views {
        reader: result,
        contributor,
    })
}

pub(super) fn validate_feature_inventory() -> Result<()> {
    let templates: BTreeSet<_> = FEATURES
        .iter()
        .filter(|feature| feature.template.is_some())
        .map(|f| format!("{}.md.in", f.slug))
        .collect();
    let scenarios: BTreeSet<_> = FEATURES
        .iter()
        .map(|f| format!("{}.rs", f.slug.replace('-', "_")))
        .collect();
    for (directory, expected) in [
        (root().join("docs/templates"), templates),
        (root().join("src/documentation/scenarios"), scenarios),
    ] {
        let current = std::fs::read_dir(&directory)
            .map_err(|e| e.to_string())?
            .map(|entry| {
                entry.map_err(|e| e.to_string()).and_then(|entry| {
                    entry
                        .file_name()
                        .into_string()
                        .map_err(|_| "Non UTF-8 scenario filename".into())
                })
            })
            .collect::<Result<BTreeSet<String>>>()?;
        check_inventory(&expected, &current)?;
    }
    Ok(())
}

pub(super) fn check_inventory(
    expected: &BTreeSet<String>,
    current: &BTreeSet<String>,
) -> Result<()> {
    lifecycle::check_inventory(expected, current)
}

pub(super) fn validate_path(path: &str) -> Result<()> {
    lifecycle::validate_path(path)
}
pub(super) fn insert(artifacts: &mut Artifacts, path: String, bytes: Vec<u8>) -> Result<()> {
    lifecycle::insert(artifacts, path, bytes)
}
pub(super) fn validate_links(artifacts: &Artifacts) -> Result<()> {
    for (path, bytes) in artifacts.iter().filter(|(path, _)| path.ends_with(".md")) {
        let text = std::str::from_utf8(bytes).map_err(|error| error.to_string())?;
        for tail in text.split("](").skip(1) {
            let target = tail
                .split(')')
                .next()
                .unwrap_or_default()
                .split('#')
                .next()
                .unwrap_or_default();
            // User-facing navigation stays within the generated guide.
            // Repository-owned documentation may link in, never the reverse.
            if target.is_empty() {
                continue;
            }
            if !artifacts.contains_key(target) {
                return Err(format!("Broken generated link in {path}: {target}"));
            }
        }
    }
    Ok(())
}
pub(super) fn read_tree(dir: &Path) -> Result<Artifacts> {
    lifecycle::read_tree(dir)
}
pub(super) fn compare(expected: &Artifacts, current: &Artifacts) -> Result<()> {
    if let (Some(generated), Some(baseline)) =
        (renderer_profile(expected), renderer_profile(current))
        && generated != baseline
    {
        return Err(format!(
            "Documentation renderer mismatch: this run uses {generated}, but the saved guide uses {baseline}. Exact media checks require the same renderer. Run checks on the baseline renderer, or deliberately run just docs update on this host and review the new baseline. No images were ignored or updated."
        ));
    }
    lifecycle::compare(expected, current).map_err(|error| {
        format!(
            "{error}. Run just docs update and review the generated changes (including images)."
        )
    })
}

pub(super) fn renderer_profile(artifacts: &Artifacts) -> Option<&str> {
    std::str::from_utf8(artifacts.get("manifest.txt")?)
        .ok()?
        .lines()
        .find_map(|line| line.strip_prefix("renderer: "))
}
pub(super) fn publish(dir: &Path, artifacts: &Artifacts) -> Result<()> {
    // Updating is an explicit renderer-baseline decision. Keep that N3 policy
    // while sharing the same staged owned-tree transaction as other consumers.
    lifecycle::update(dir, artifacts, Ownership::Dedicated, |_, _| Ok(()))?;
    compare(artifacts, &read_tree(dir)?)
}

pub fn run(mode: &str) -> Result<()> {
    execute(mode, &root().join("docs/guide"), generate)
}

/// Build disposable reader and contributor views without accepting a baseline.
/// The destination is a fresh, dedicated generated tree; it cannot overwrite an
/// existing guide. Generation of both audiences completes before publication.
pub fn build(dir: &Path) -> Result<()> {
    let dir = candidate_destination(dir, &root())?;
    let views = lifecycle::prepare(|| generate_views(true))?;
    let mut files = Artifacts::new();
    for (audience, artifacts) in [
        ("reader", views.reader),
        (
            "contributor",
            views.contributor.ok_or("Missing contributor view")?,
        ),
    ] {
        for (path, bytes) in artifacts {
            insert(&mut files, format!("{audience}/{path}"), bytes)?;
        }
    }
    lifecycle::build(&dir, &files, Ownership::Dedicated)?;
    println!(
        "docs build: reader and contributor views at {}",
        dir.display()
    );
    Ok(())
}

fn candidate_destination(path: &Path, repository: &Path) -> Result<PathBuf> {
    let path = lifecycle::absolute(path)?;
    for retained in ["docs/guide", "docs/baselines"] {
        lifecycle::ensure_separate_paths(&path, &repository.join(retained))?;
    }
    if path.try_exists().map_err(|error| error.to_string())? {
        return Err(format!("Build output already exists: {}", path.display()));
    }
    Ok(path)
}

pub(super) fn execute(
    mode: &str,
    dir: &Path,
    generate: impl FnOnce() -> Result<Artifacts>,
) -> Result<()> {
    if !matches!(mode, "update" | "check") {
        return Err("Usage: --docs update|check".into());
    }
    // All UI actions, assertions, rendering, encoding, and template validation
    // complete before any documentation is written. A failing run cannot bless it.
    let artifacts = lifecycle::prepare(generate)?;
    let canonical = read_tree(dir)?;
    if std::env::var("N3_DOCS_RENDERER").as_deref() == Ok("lavapipe")
        && renderer_profile(&artifacts) == Some(renderer_baseline::PROFILE)
        && renderer_profile(&canonical) != Some(renderer_baseline::PROFILE)
    {
        renderer_baseline::run(mode, &root(), &canonical, &artifacts)?;
    } else {
        match mode {
            "update" => publish(dir, &artifacts)?,
            _ => compare(&artifacts, &canonical)?,
        }
    }
    println!(
        "docs {mode}: {} verified features, {} artifacts",
        FEATURES.len(),
        artifacts.len()
    );
    Ok(())
}

#[cfg(test)]
mod renderer_tests {
    use super::*;

    #[test]
    fn candidate_output_cannot_write_into_or_around_a_retained_baseline() {
        let repository = root();
        for relative in [
            "docs",
            "docs/guide",
            "docs/guide/candidate",
            "docs/baselines/candidate",
            "docs/guide/./candidate",
            "docs/guide/../candidate",
            ".",
        ] {
            assert!(
                candidate_destination(&repository.join(relative), &repository).is_err(),
                "{relative}"
            );
        }
        assert!(candidate_destination(&repository.join("Cargo.toml"), &repository).is_err());
        // This is a read-only preflight: the fresh destination remains absent.
        let fresh = repository.join(".cache/executable-docs-never-created-by-preflight");
        assert_eq!(candidate_destination(&fresh, &repository).unwrap(), fresh);
        assert!(!fresh.exists());
    }

    #[cfg(unix)]
    #[test]
    fn candidate_preflight_rejects_case_aliases_of_retained_directories() {
        use std::os::unix::fs::MetadataExt;

        let repository = root();
        for (retained, alias) in [
            ("docs/guide", "DOCS/GUIDE"),
            ("docs/baselines", "DOCS/BASELINES"),
        ] {
            let baseline = repository.join(retained);
            let alias = repository.join(alias);
            let Ok(alias_metadata) = std::fs::metadata(&alias) else {
                continue;
            };
            let baseline_metadata = std::fs::metadata(&baseline).unwrap();
            if (alias_metadata.dev(), alias_metadata.ino())
                != (baseline_metadata.dev(), baseline_metadata.ino())
            {
                continue;
            }
            let candidate = alias.join("case-alias-preflight-must-not-create");
            assert!(!candidate.exists());
            let before = read_tree(&baseline).unwrap();
            assert!(
                candidate_destination(&candidate, &repository)
                    .unwrap_err()
                    .contains("overlap")
            );
            assert!(!candidate.exists());
            assert_eq!(read_tree(&baseline).unwrap(), before);
        }
    }

    fn baseline(profile: &str) -> Artifacts {
        Artifacts::from([
            (
                "manifest.txt".into(),
                format!("renderer: {profile}\n").into_bytes(),
            ),
            ("assets/example.webp".into(), vec![1, 2, 3]),
        ])
    }

    #[test]
    fn different_renderers_fail_explicitly_without_relaxing_media_checks() {
        let native = baseline("macos-metal");
        let linux = baseline("linux-vulkan-lavapipe");
        let error = compare(&native, &linux).unwrap_err();
        assert!(error.contains("renderer mismatch"));
        assert!(error.contains("macos-metal"));
        assert!(error.contains("linux-vulkan-lavapipe"));
        assert!(compare(&native, &native).is_ok());
        let mut changed = native.clone();
        changed.insert("assets/example.webp".into(), vec![4, 5, 6]);
        assert!(
            compare(&native, &changed)
                .unwrap_err()
                .contains("assets/example.webp")
        );
    }
}
