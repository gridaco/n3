//! Strict generated-tree ownership, bindings, validation and publication.
use super::*;

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
    validate_feature_inventory()?;
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT))?;
    let (media_width, media_height) = capture.framed_dimensions();
    let mut result = Artifacts::new();
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
        let page = render_template(
            feature.template,
            &session.bindings,
            &session.values,
            &session.images,
            &session.animations,
        )?;
        insert(
            &mut result,
            format!("{}.md", feature.slug),
            page.into_bytes(),
        )?;
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
            insert(&mut result, path, bytes)?;
        }
    }
    insert(&mut result, "README.md".into(), index.into_bytes())?;
    insert(&mut result, "manifest.txt".into(), manifest.into_bytes())?;
    validate_links(&result)?;
    Ok(result)
}

pub(super) fn validate_feature_inventory() -> Result<()> {
    let templates: BTreeSet<_> = FEATURES
        .iter()
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
    if expected == current {
        Ok(())
    } else {
        Err(format!(
            "Every feature must own exactly one registered scenario and template. Missing: {:?}; unregistered: {:?}",
            expected.difference(current).collect::<Vec<_>>(),
            current.difference(expected).collect::<Vec<_>>()
        ))
    }
}

pub(super) fn validate_path(path: &str) -> Result<()> {
    if path.is_empty()
        || path.contains('\\')
        || path
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
        || Path::new(path)
            .components()
            .any(|part| !matches!(part, std::path::Component::Normal(_)))
    {
        return Err(format!("Invalid documentation artifact path: {path}"));
    }
    Ok(())
}
pub(super) fn insert(artifacts: &mut Artifacts, path: String, bytes: Vec<u8>) -> Result<()> {
    validate_path(&path)?;
    match artifacts.entry(path) {
        std::collections::btree_map::Entry::Vacant(entry) => {
            entry.insert(bytes);
        }
        std::collections::btree_map::Entry::Occupied(entry) => {
            return Err(format!("Duplicate artifact owner: {}", entry.key()));
        }
    }
    Ok(())
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
    fn visit(base: &Path, dir: &Path, files: &mut Artifacts) -> Result<()> {
        if dir
            .symlink_metadata()
            .map_err(|e| e.to_string())?
            .file_type()
            .is_symlink()
        {
            return Err("Documentation tree must not contain symlinks".into());
        }
        for item in std::fs::read_dir(dir).map_err(|e| e.to_string())? {
            let item = item.map_err(|e| e.to_string())?;
            let kind = item.file_type().map_err(|e| e.to_string())?;
            if kind.is_symlink() {
                return Err(format!(
                    "Documentation symlink is not supported: {}",
                    item.path().display()
                ));
            }
            if kind.is_dir() {
                visit(base, &item.path(), files)?;
            } else if kind.is_file() {
                let key = item
                    .path()
                    .strip_prefix(base)
                    .unwrap()
                    .to_str()
                    .ok_or("Non UTF-8 documentation path")?
                    .to_owned();
                insert(
                    files,
                    key,
                    std::fs::read(item.path()).map_err(|e| e.to_string())?,
                )?;
            }
        }
        Ok(())
    }
    let mut files = Artifacts::new();
    if dir.exists() {
        visit(dir, dir, &mut files)?;
    }
    Ok(files)
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
    let paths: BTreeSet<_> = expected.keys().chain(current.keys()).collect();
    let drift: Vec<_> = paths
        .into_iter()
        .filter(|path| expected.get(*path) != current.get(*path))
        .cloned()
        .collect();
    if drift.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "Documentation drift: {}. Run just docs update and review the generated changes (including images).",
            drift.join(", ")
        ))
    }
}

fn renderer_profile(artifacts: &Artifacts) -> Option<&str> {
    std::str::from_utf8(artifacts.get("manifest.txt")?)
        .ok()?
        .lines()
        .find_map(|line| line.strip_prefix("renderer: "))
}
pub(super) fn publish(dir: &Path, artifacts: &Artifacts) -> Result<()> {
    let current = read_tree(dir)?;
    let orphans: Vec<_> = current
        .keys()
        .filter(|key| !artifacts.contains_key(*key))
        .collect();
    if !orphans.is_empty() {
        return Err(format!(
            "Unowned documentation files: {orphans:?}; explicitly remove obsolete files before updating"
        ));
    }
    for (path, bytes) in artifacts {
        let destination = dir.join(path);
        std::fs::create_dir_all(destination.parent().unwrap()).map_err(|e| e.to_string())?;
        std::fs::write(destination, bytes).map_err(|e| e.to_string())?;
    }
    compare(artifacts, &read_tree(dir)?)
}

pub fn run(mode: &str) -> Result<()> {
    execute(mode, &root().join("docs/guide"), generate)
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
    let artifacts = generate()?;
    match mode {
        "update" => publish(dir, &artifacts)?,
        _ => compare(&artifacts, &read_tree(dir)?)?,
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
