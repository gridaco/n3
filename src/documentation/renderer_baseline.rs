//! Exact secondary-renderer receipts, tied to the complete canonical guide.
//! The ordinary generation pipeline owns both renderers; only explicit updates
//! write review captures and a receipt. Checks never publish either baseline.
use super::{Artifacts, Result, artifacts};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, path::Path};

pub(super) const PROFILE: &str = "linux-vulkan-lavapipe";
const VERSION: u32 = 1;
const RECEIPT: &str = "docs/baselines/linux-vulkan-lavapipe.json";
const REVIEW: &str = ".cache/docs/linux-vulkan-lavapipe";

/// Failure captures are diagnostics, never a baseline. Restrict the opt-in
/// destination to disposable caches, including the CI runner's writable mounts.
fn failure_directory(root: &Path, destination: &Path) -> Result<()> {
    use std::path::Component;
    if !destination.is_absolute()
        || destination
            .components()
            .any(|part| !matches!(part, Component::RootDir | Component::Normal(_)))
        || ![
            root.join(".cache"),
            root.join("target"),
            "/n3-cache/home".into(),
            "/n3-cache/target".into(),
        ]
        .iter()
        .any(|cache| destination != cache && destination.starts_with(cache))
    {
        return Err("Documentation failure artifacts require an absolute directory inside .cache, target, or a CI cache mount.".into());
    }
    for ancestor in destination.ancestors() {
        match std::fs::symlink_metadata(ancestor) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(format!(
                    "Documentation failure artifacts must not follow symlinks: {}",
                    ancestor.display()
                ));
            }
            Ok(metadata) if !metadata.is_dir() => {
                return Err(format!(
                    "Documentation failure artifacts require a directory: {}",
                    ancestor.display()
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.to_string()),
        }
    }
    Ok(())
}

fn export_failure(root: &Path, destination: &Path, generated: &Artifacts) -> Result<()> {
    failure_directory(root, destination)?;
    for path in generated.keys() {
        artifacts::validate_path(path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if let Ok(metadata) = std::fs::symlink_metadata(destination.join(path))
                && metadata.nlink() > 1
            {
                return Err(format!(
                    "Documentation failure artifacts must not overwrite hard links: {path}"
                ));
            }
        }
    }
    artifacts::publish(destination, generated)
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    version: u32,
    renderer: String,
    canonical_renderer: String,
    canonical_guide_sha256: String,
    artifacts: Vec<ArtifactDigest>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ArtifactDigest {
    path: String,
    bytes: u64,
    sha256: String,
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Paths, lengths and bytes are framed independently, in BTreeMap path order.
/// This pins all canonical prose, manifest evidence, stills and animations.
fn guide_digest(guide: &Artifacts) -> String {
    let mut digest = Sha256::new();
    digest.update(b"n3 canonical guide v1\0");
    digest.update((guide.len() as u64).to_be_bytes());
    for (path, bytes) in guide {
        digest.update((path.len() as u64).to_be_bytes());
        digest.update(path.as_bytes());
        digest.update((bytes.len() as u64).to_be_bytes());
        digest.update(bytes);
    }
    format!("{:x}", digest.finalize())
}

fn validate_digest(value: &str) -> Result<()> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err("Renderer baseline requires lowercase SHA-256 digests.".into());
    }
    Ok(())
}

fn inventory(expected: &BTreeSet<&str>, current: &BTreeSet<&str>) -> Result<()> {
    if expected != current {
        return Err(format!(
            "Renderer baseline artifact inventory drift. Missing: {:?}; unexpected: {:?}",
            expected.difference(current).collect::<Vec<_>>(),
            current.difference(expected).collect::<Vec<_>>()
        ));
    }
    Ok(())
}

fn validate_canonical(canonical: &Artifacts, generated: &Artifacts) -> Result<String> {
    let profile = artifacts::renderer_profile(canonical)
        .ok_or("Canonical guide is missing its renderer profile.")?;
    if artifacts::renderer_profile(generated) != Some(PROFILE) {
        return Err(format!("Secondary renderer baseline requires {PROFILE}."));
    }
    if !matches!(profile, "macos-metal" | "linux-vulkan") {
        return Err(format!(
            "Unsupported canonical guide renderer for a secondary baseline: {profile}."
        ));
    }
    inventory(
        &canonical.keys().map(String::as_str).collect(),
        &generated.keys().map(String::as_str).collect(),
    )?;
    let prose_drift: Vec<_> = canonical
        .iter()
        .filter(|(path, bytes)| path.ends_with(".md") && generated.get(*path) != Some(*bytes))
        .map(|(path, _)| path.as_str())
        .collect();
    if !prose_drift.is_empty() {
        return Err(format!(
            "Secondary renderer Markdown differs from the canonical guide: {}. Update and review the native guide first.",
            prose_drift.join(", ")
        ));
    }
    Ok(profile.to_owned())
}

impl Receipt {
    fn create(canonical: &Artifacts, generated: &Artifacts) -> Result<Self> {
        Ok(Self {
            version: VERSION,
            renderer: PROFILE.into(),
            canonical_renderer: validate_canonical(canonical, generated)?,
            canonical_guide_sha256: guide_digest(canonical),
            artifacts: generated
                .iter()
                .map(|(path, bytes)| ArtifactDigest {
                    path: path.clone(),
                    bytes: bytes.len() as u64,
                    sha256: sha256(bytes),
                })
                .collect(),
        })
    }

    fn decode(bytes: &[u8]) -> Result<Self> {
        let receipt: Self = serde_json::from_slice(bytes)
            .map_err(|error| format!("Malformed renderer baseline receipt: {error}"))?;
        if receipt.version != VERSION || receipt.renderer != PROFILE {
            return Err(format!(
                "Unsupported renderer baseline receipt version {} or renderer {:?}.",
                receipt.version, receipt.renderer
            ));
        }
        validate_digest(&receipt.canonical_guide_sha256)?;
        let mut paths = BTreeSet::new();
        for artifact in &receipt.artifacts {
            artifacts::validate_path(&artifact.path)?;
            validate_digest(&artifact.sha256)?;
            if !paths.insert(&artifact.path) {
                return Err(format!(
                    "Duplicate renderer baseline artifact: {}.",
                    artifact.path
                ));
            }
        }
        Ok(receipt)
    }

    fn check(&self, canonical: &Artifacts, generated: &Artifacts) -> Result<()> {
        let profile = validate_canonical(canonical, generated)?;
        if self.canonical_renderer != profile
            || self.canonical_guide_sha256 != guide_digest(canonical)
        {
            return Err("Stale secondary renderer baseline: the canonical guide changed. Run just ci-docs update and review the new captures and receipt.".into());
        }
        inventory(
            &self
                .artifacts
                .iter()
                .map(|artifact| artifact.path.as_str())
                .collect(),
            &generated.keys().map(String::as_str).collect(),
        )?;
        let drift: Vec<_> = self
            .artifacts
            .iter()
            .filter(|artifact| {
                let bytes = &generated[&artifact.path];
                artifact.bytes != bytes.len() as u64 || artifact.sha256 != sha256(bytes)
            })
            .map(|artifact| artifact.path.as_str())
            .collect();
        if !drift.is_empty() {
            return Err(format!(
                "Secondary renderer artifact drift: {}. Run just ci-docs update and review the new captures and receipt.",
                drift.join(", ")
            ));
        }
        Ok(())
    }
}

pub(super) fn run(
    mode: &str,
    root: &Path,
    canonical: &Artifacts,
    generated: &Artifacts,
) -> Result<()> {
    let destination = std::env::var_os("N3_DOCS_FAILURE_ARTIFACTS");
    run_with_failure_artifacts(
        mode,
        root,
        canonical,
        generated,
        destination.as_deref().map(Path::new),
    )
}

fn run_with_failure_artifacts(
    mode: &str,
    root: &Path,
    canonical: &Artifacts,
    generated: &Artifacts,
    failure_artifacts: Option<&Path>,
) -> Result<()> {
    let receipt_path = root.join(RECEIPT);
    match mode {
        "update" => {
            let receipt = Receipt::create(canonical, generated)?;
            let mut bytes =
                serde_json::to_vec_pretty(&receipt).map_err(|error| error.to_string())?;
            bytes.push(b'\n');
            artifacts::publish(&root.join(REVIEW), generated)?;
            std::fs::create_dir_all(receipt_path.parent().unwrap()).map_err(|e| e.to_string())?;
            std::fs::write(&receipt_path, bytes).map_err(|e| e.to_string())?;
            println!("docs update: review {REVIEW} and {RECEIPT}; canonical guide preserved");
        }
        "check" => {
            let result = (|| {
                let bytes = std::fs::read(&receipt_path).map_err(|error| {
                    format!("Cannot read secondary renderer baseline {}: {error}. Run just ci-docs update and review its captures and receipt.", receipt_path.display())
                })?;
                Receipt::decode(&bytes)?.check(canonical, generated)
            })();
            if result.is_err()
                && let Some(destination) = failure_artifacts
            {
                match export_failure(root, destination, generated) {
                    Ok(()) => eprintln!(
                        "docs check: actual failure artifacts exported to {}",
                        destination.display()
                    ),
                    Err(error) => eprintln!(
                        "docs check: could not export failure artifacts to {}: {error}",
                        destination.display()
                    ),
                }
            }
            // A diagnostic export cannot replace or suppress the strict check.
            result?;
        }
        _ => return Err("Usage: --docs update|check".into()),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(mode: &str, root: &Path, canonical: &Artifacts, generated: &Artifacts) -> Result<()> {
        run_with_failure_artifacts(mode, root, canonical, generated, None)
    }

    fn guide(profile: &str) -> Artifacts {
        Artifacts::from([
            ("README.md".into(), b"# Same guide\n".to_vec()),
            (
                "manifest.txt".into(),
                format!("renderer: {profile}\n").into_bytes(),
            ),
            ("assets/still.webp".into(), vec![1, 2, 3]),
            ("assets/animation.webp".into(), vec![4, 5, 6]),
        ])
    }

    #[test]
    fn receipt_checks_every_artifact_without_comparing_different_renderer_pixels() {
        let canonical = guide("macos-metal");
        let mut generated = guide(PROFILE);
        generated.insert("assets/still.webp".into(), vec![7, 8, 9]);
        let receipt = Receipt::create(&canonical, &generated).unwrap();
        let bytes = serde_json::to_vec(&receipt).unwrap();
        let receipt = Receipt::decode(&bytes).unwrap();
        receipt.check(&canonical, &generated).unwrap();
        for path in generated.keys() {
            let mut changed = generated.clone();
            changed.get_mut(path).unwrap().push(0);
            assert!(
                receipt
                    .check(&canonical, &changed)
                    .unwrap_err()
                    .contains(path)
            );
        }
        let mut length = Receipt::decode(&bytes).unwrap();
        length.artifacts[0].bytes += 1;
        assert!(length.check(&canonical, &generated).is_err());
    }

    #[test]
    fn complete_canonical_guide_changes_make_the_receipt_stale() {
        let canonical = guide("macos-metal");
        let generated = guide(PROFILE);
        let receipt = Receipt::create(&canonical, &generated).unwrap();
        for path in ["assets/still.webp", "assets/animation.webp", "manifest.txt"] {
            let mut changed = canonical.clone();
            changed.get_mut(path).unwrap().push(0);
            assert!(
                receipt
                    .check(&changed, &generated)
                    .unwrap_err()
                    .contains("Stale")
            );
        }
        let mut changed = canonical.clone();
        changed.insert("README.md".into(), b"# Reviewed new prose\n".to_vec());
        let mut matching = generated.clone();
        matching.insert("README.md".into(), changed["README.md"].clone());
        assert!(
            receipt
                .check(&changed, &matching)
                .unwrap_err()
                .contains("Stale")
        );
    }

    #[test]
    fn inventory_and_canonical_prose_cannot_be_blessed_by_secondary_update() {
        let canonical = guide("macos-metal");
        let generated = guide(PROFILE);
        for added in [false, true] {
            let mut changed = generated.clone();
            if added {
                changed.insert("orphan.webp".into(), vec![0]);
            } else {
                changed.remove("assets/still.webp");
            }
            assert!(
                Receipt::create(&canonical, &changed)
                    .unwrap_err()
                    .contains("inventory")
            );
        }
        let mut changed = generated.clone();
        changed.insert("README.md".into(), b"# Drift\n".to_vec());
        assert!(
            Receipt::create(&canonical, &changed)
                .unwrap_err()
                .contains("Markdown")
        );
        let mut receipt = Receipt::create(&canonical, &generated).unwrap();
        receipt.artifacts.pop();
        assert!(
            receipt
                .check(&canonical, &generated)
                .unwrap_err()
                .contains("inventory")
        );
    }

    #[test]
    fn malformed_unsupported_and_duplicate_receipts_fail_visibly() {
        let receipt = Receipt::create(&guide("macos-metal"), &guide(PROFILE)).unwrap();
        let value = serde_json::to_value(&receipt).unwrap();
        assert!(
            Receipt::decode(b"not JSON")
                .unwrap_err()
                .contains("Malformed")
        );
        for (field, replacement) in [
            ("version", serde_json::json!(2)),
            ("renderer", serde_json::json!("linux-vulkan")),
            ("canonical_guide_sha256", serde_json::json!("bad hash")),
        ] {
            let mut changed = value.clone();
            changed[field] = replacement;
            assert!(Receipt::decode(&serde_json::to_vec(&changed).unwrap()).is_err());
        }
        let mut unknown = value.clone();
        unknown["skip_images"] = serde_json::json!(true);
        assert!(Receipt::decode(&serde_json::to_vec(&unknown).unwrap()).is_err());
        for path in [
            "../escaped.webp",
            "assets/../escaped.webp",
            "/absolute.webp",
        ] {
            let mut changed = value.clone();
            changed["artifacts"][0]["path"] = serde_json::json!(path);
            assert!(Receipt::decode(&serde_json::to_vec(&changed).unwrap()).is_err());
        }
        let mut duplicate = value;
        let entry = duplicate["artifacts"][0].clone();
        duplicate["artifacts"].as_array_mut().unwrap().push(entry);
        assert!(
            Receipt::decode(&serde_json::to_vec(&duplicate).unwrap())
                .unwrap_err()
                .contains("Duplicate")
        );
    }

    #[test]
    fn renderer_and_framing_are_part_of_the_contract() {
        let canonical = guide("macos-metal");
        assert!(Receipt::create(&canonical, &guide("linux-vulkan")).is_err());
        assert!(Receipt::create(&guide(PROFILE), &guide(PROFILE)).is_err());
        let receipt = Receipt::create(&canonical, &guide(PROFILE)).unwrap();
        assert!(
            receipt
                .check(&guide("linux-vulkan"), &guide(PROFILE))
                .unwrap_err()
                .contains("Stale")
        );
        assert_ne!(
            guide_digest(&Artifacts::from([("a".into(), b"bc".to_vec())])),
            guide_digest(&Artifacts::from([("ab".into(), b"c".to_vec())]))
        );
        assert_eq!(
            sha256(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn update_preserves_canonical_files_and_check_never_writes_review_outputs() {
        let root = std::env::temp_dir().join(format!(
            "n3-renderer-baseline-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let canonical = guide("macos-metal");
        let generated = guide(PROFILE);
        // Resolve the test-owned temp root; production publication still rejects
        // symlink components, including macOS's /var alias.
        std::fs::create_dir(&root).unwrap();
        let root = root.canonicalize().unwrap();
        artifacts::publish(&root.join("docs/guide"), &canonical).unwrap();
        assert!(
            run("check", &root, &canonical, &generated)
                .unwrap_err()
                .contains("Cannot read")
        );
        assert!(!root.join(REVIEW).exists());
        run("update", &root, &canonical, &generated).unwrap();
        assert_eq!(
            artifacts::read_tree(&root.join("docs/guide")).unwrap(),
            canonical
        );
        assert_eq!(artifacts::read_tree(&root.join(REVIEW)).unwrap(), generated);
        // CI needs only the committed receipt, never a previously written cache.
        std::fs::remove_dir_all(root.join(REVIEW)).unwrap();
        let before = artifacts::read_tree(&root).unwrap();
        run("check", &root, &canonical, &generated).unwrap();
        let mut changed = generated.clone();
        changed.get_mut("assets/still.webp").unwrap().push(0);
        assert!(run("check", &root, &canonical, &changed).is_err());
        assert_eq!(artifacts::read_tree(&root).unwrap(), before);
        changed.insert("README.md".into(), b"# Unreviewed prose\n".to_vec());
        assert!(run("update", &root, &canonical, &changed).is_err());
        assert_eq!(artifacts::read_tree(&root).unwrap(), before);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn failure_exports_actual_captures_and_preserves_the_original_error_and_baselines() {
        let root = std::env::temp_dir().join(format!(
            "n3-failure-artifacts-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let canonical = guide("macos-metal");
        let expected = guide(PROFILE);
        artifacts::publish(&root.join("docs/guide"), &canonical).unwrap();
        run("update", &root, &canonical, &expected).unwrap();
        let baseline = artifacts::read_tree(&root.join("docs")).unwrap();
        let review = artifacts::read_tree(&root.join(REVIEW)).unwrap();
        let destination = root.join(".cache/failure-artifacts");
        run_with_failure_artifacts("check", &root, &canonical, &expected, Some(&destination))
            .unwrap();
        assert!(!destination.exists(), "Successful checks do not publish");

        let mut actual = expected.clone();
        actual.get_mut("assets/still.webp").unwrap().push(42);
        let error = run("check", &root, &canonical, &actual).unwrap_err();
        assert!(error.contains("Secondary renderer artifact drift"));
        assert_eq!(
            run_with_failure_artifacts("check", &root, &canonical, &actual, Some(&destination),)
                .unwrap_err(),
            error
        );
        assert_eq!(artifacts::read_tree(&destination).unwrap(), actual);

        // Rejected destinations and a publication failure retain the same error.
        let blocked = root.join(".cache/blocked");
        std::fs::create_dir_all(&blocked).unwrap();
        std::fs::write(blocked.join("unowned.txt"), b"retain this file").unwrap();
        let blocked_before = artifacts::read_tree(&blocked).unwrap();
        for rejected in [
            root.join("docs/guide"),
            root.join("docs/baselines"),
            root.join(".cache"),
            root.join(".cache/../docs/guide"),
            ".cache/relative".into(),
            blocked.clone(),
        ] {
            assert_eq!(
                run_with_failure_artifacts("check", &root, &canonical, &actual, Some(&rejected),)
                    .unwrap_err(),
                error
            );
        }
        assert_eq!(artifacts::read_tree(&blocked).unwrap(), blocked_before);
        assert_eq!(artifacts::read_tree(&root.join("docs")).unwrap(), baseline);
        assert_eq!(artifacts::read_tree(&root.join(REVIEW)).unwrap(), review);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn failure_export_rejects_link_ancestors_and_artifacts() {
        let root = std::env::temp_dir().join(format!(
            "n3-failure-symlinks-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join(".cache/actual")).unwrap();
        let root = root.canonicalize().unwrap();
        let outside = root.join("untouched");
        std::fs::create_dir(&outside).unwrap();
        std::fs::write(outside.join("still.webp"), b"preserved").unwrap();
        std::os::unix::fs::symlink(&outside, root.join(".cache/link")).unwrap();
        assert!(
            export_failure(&root, &root.join(".cache/link/captures"), &guide(PROFILE))
                .unwrap_err()
                .contains("symlink")
        );
        let artifacts = root.join(".cache/actual/assets");
        std::fs::create_dir(&artifacts).unwrap();
        std::os::unix::fs::symlink(outside.join("still.webp"), artifacts.join("still.webp"))
            .unwrap();
        assert!(
            export_failure(&root, &root.join(".cache/actual"), &guide(PROFILE))
                .unwrap_err()
                .contains("symlink")
        );
        assert_eq!(
            std::fs::read(outside.join("still.webp")).unwrap(),
            b"preserved"
        );
        assert!(!outside.join("captures").exists());
        std::fs::remove_file(artifacts.join("still.webp")).unwrap();
        std::fs::hard_link(outside.join("still.webp"), artifacts.join("still.webp")).unwrap();
        assert!(
            export_failure(&root, &root.join(".cache/actual"), &guide(PROFILE))
                .unwrap_err()
                .contains("hard links")
        );
        assert_eq!(
            std::fs::read(outside.join("still.webp")).unwrap(),
            b"preserved"
        );
        assert!(!root.join(".cache/actual/README.md").exists());
        std::fs::remove_dir_all(root).unwrap();
    }
}
