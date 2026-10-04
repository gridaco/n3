//! Structured SDK manifest policy over the shared artifact lifecycle.
use crate::{
    Result,
    lifecycle::{self, Ownership},
};
pub use lifecycle::Files;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

pub(crate) fn absolute(path: &Path) -> Result<PathBuf> {
    lifecycle::absolute(path)
}

fn manifest(files: &Files) -> Result<serde_json::Value> {
    let bytes = files.get("manifest.json").ok_or("Missing manifest.json")?;
    let value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|e| format!("Invalid manifest.json: {e}"))?;
    if value["schema_version"] != 1 {
        return Err(format!(
            "Unsupported manifest schema: {}",
            value["schema_version"]
        ));
    }
    if !matches!(value["audience"].as_str(), Some("reader" | "contributor")) {
        return Err("Manifest audience must be reader or contributor".into());
    }
    if !value["profiles"].is_array() {
        return Err("Manifest profiles must be an array".into());
    }
    Ok(value)
}

fn compatible(baseline: &Files, candidate: &Files) -> Result<()> {
    validate_tree(baseline, None)?;
    compatible_profiles(baseline, candidate)
}

fn compatible_update(
    baseline: &Files,
    candidate: &Files,
    missing_owned: &BTreeSet<String>,
) -> Result<()> {
    // Explicit retirement removes an old file before update. Its old receipt is
    // necessarily stale until this transaction publishes the complete new tree.
    // Only a missing old artifact absent from the candidate qualifies; changed
    // retained bytes, unrecorded files and missing retained files still fail.
    validate_tree(baseline, Some(candidate))?;
    // A removed file is intentional retirement only when both retained records
    // identify it. Phantom ownership entries must never be silently rewritten;
    // nor may retirement conceal omissions from the ownership marker.
    let old = manifest(baseline)?;
    let mut retired = BTreeSet::new();
    for artifact in array(&old, "artifacts")? {
        let path = field(artifact, "path")?;
        if !baseline.contains_key(path) {
            retired.insert(path.to_owned());
        }
    }
    if &retired != missing_owned {
        return Err(format!(
            "Ownership inventory differs from retained manifest: missing owned {missing_owned:?}; retired artifacts {retired:?}"
        ));
    }
    compatible_profiles(baseline, candidate)
}

fn compatible_profiles(baseline: &Files, candidate: &Files) -> Result<()> {
    let old = manifest(baseline)?;
    let new = manifest(candidate)?;
    if old["audience"] != new["audience"] {
        return Err(format!(
            "Incompatible audience: baseline={}, candidate={}; use a separately reviewed baseline",
            old["audience"], new["audience"]
        ));
    }
    // Compatibility belongs to retained resource identities, not the aggregate
    // profile set: new features may introduce a producer, and two retained
    // resources can swap profiles without changing that aggregate set.
    let previous = resource_profiles(&old)?;
    let next = resource_profiles(&new)?;
    for (identity, profile) in previous {
        if let Some(candidate) = next.get(&identity)
            && *candidate != profile
        {
            return Err(format!(
                "Incompatible producer/comparison profiles for {}/{}: baseline={profile:?}, candidate={candidate:?}; use a separately reviewed baseline",
                identity.0, identity.1,
            ));
        }
    }
    Ok(())
}

fn resource_profiles(
    manifest: &serde_json::Value,
) -> Result<BTreeMap<(String, String), (String, String)>> {
    let mut profiles = BTreeMap::new();
    for artifact in manifest["artifacts"]
        .as_array()
        .ok_or("Manifest artifacts must be an array")?
    {
        if artifact["kind"] != "resource" {
            continue;
        }
        let field = |name: &str| -> Result<String> {
            artifact[name]
                .as_str()
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
                .ok_or_else(|| format!("Resource manifest requires {name}"))
        };
        let identity = (field("document")?, field("id")?);
        let profile = (field("producer")?, field("profile")?);
        if profiles.insert(identity.clone(), profile).is_some() {
            return Err(format!(
                "Duplicate resource manifest identity: {identity:?}"
            ));
        }
    }
    Ok(profiles)
}

fn validate(files: &Files) -> Result<()> {
    validate_tree(files, None)
}

fn field<'a>(record: &'a Value, name: &str) -> Result<&'a str> {
    record[name]
        .as_str()
        .filter(|text| !text.is_empty())
        .ok_or_else(|| format!("Manifest requires nonempty string field {name}"))
}

fn array<'a>(record: &'a Value, name: &str) -> Result<&'a Vec<Value>> {
    record[name]
        .as_array()
        .ok_or_else(|| format!("Manifest {name} must be an array"))
}

fn identity(identities: &mut BTreeMap<String, String>, id: &str, kind: &str) -> Result<()> {
    if !crate::model::valid_id(id) {
        return Err(format!("Invalid manifest evidence ID: {id}"));
    }
    if identities.insert(id.into(), kind.into()).is_some() {
        return Err(format!("Duplicate manifest evidence identity: {id}"));
    }
    Ok(())
}

fn validate_tree(files: &Files, retired_from: Option<&Files>) -> Result<()> {
    // Check map paths before reading metadata, so no malformed candidate can
    // reach staging merely by omitting that path from its artifact inventory.
    for path in files.keys() {
        lifecycle::validate_path(path)?;
        if path.split('/').next() == Some(".ownership.json") {
            return Err(format!("Reserved ownership path: {path}"));
        }
        for parent in Path::new(path)
            .ancestors()
            .skip(1)
            .filter(|p| !p.as_os_str().is_empty())
        {
            if files.contains_key(parent.to_str().ok_or("Non UTF-8 output path")?) {
                return Err(format!(
                    "Output file conflicts with a parent directory: {path}"
                ));
            }
        }
    }
    let manifest = manifest(files)?;
    let documents = array(&manifest, "documents")?;
    if documents.is_empty() {
        return Err("Manifest must contain at least one document".into());
    }
    let mut identities = BTreeMap::new();
    for document in documents {
        let id = field(document, "id")?;
        if !crate::model::valid_id(id) {
            return Err(format!("Invalid manifest document ID: {id}"));
        }
        field(document, "title")?;
        let mut local = BTreeMap::new();
        let checks = array(document, "checks")?;
        if checks.is_empty() {
            return Err(format!("Manifest document {id} has no checks"));
        }
        for check in checks {
            let kind = match field(check, "kind")? {
                "equality" => "value",
                "predicate" => "claim",
                other => return Err(format!("Unknown manifest check kind: {other}")),
            };
            identity(&mut local, field(check, "id")?, kind)?;
            if check["passed"] != true {
                return Err(format!(
                    "Manifest document {id} contains a failed or missing check result"
                ));
            }
            if check.get("actual").is_some() != check.get("expected").is_some() {
                return Err(
                    "Manifest check must include both actual and expected payloads, or neither"
                        .into(),
                );
            }
        }
        for block in array(document, "blocks")? {
            let kind = field(block, "kind")?;
            if !matches!(
                kind,
                "heading" | "paragraph" | "markdown" | "markdown-parts" | "embed" | "code" | "note"
            ) {
                return Err(format!("Unknown manifest block kind: {kind}"));
            }
            if manifest["audience"] == "reader" && kind == "note" {
                return Err("Reader manifest cannot contain contributor notes".into());
            }
            identity(&mut local, field(block, "id")?, "block")?;
            array(block, "references")?;
        }
        if let Some(bindings) = document.get("bindings") {
            for (binding, value) in bindings
                .as_object()
                .ok_or("Manifest bindings must be an object")?
            {
                identity(&mut local, binding, "binding")?;
                let valid_label =
                    |label: &str| !label.is_empty() && !label.chars().any(char::is_control);
                let valid = match field(value, "kind")? {
                    "text" | "code" => value["value"].as_str().is_some_and(valid_label),
                    "keys" => value["value"].as_array().is_some_and(|keys| {
                        !keys.is_empty()
                            && keys.iter().all(|key| key.as_str().is_some_and(valid_label))
                    }),
                    _ => false,
                };
                if !valid {
                    return Err(format!("Invalid manifest binding: {binding}"));
                }
            }
        }
        if identities.insert(id.to_owned(), local).is_some() {
            return Err(format!("Duplicate manifest document identity: {id}"));
        }
    }
    let mut paths = BTreeSet::new();
    let mut pages = BTreeSet::new();
    let mut profiles = BTreeSet::new();
    for artifact in array(&manifest, "artifacts")? {
        let path = field(artifact, "path")?;
        crate::render::validate_resource_path(path)?;
        if path.split('/').next() == Some(".ownership.json") {
            return Err(format!("Reserved ownership path in manifest: {path}"));
        }
        if !paths.insert(path.to_owned()) {
            return Err(format!("Duplicate manifest artifact path: {path}"));
        }
        let document = field(artifact, "document")?;
        let local = identities
            .get_mut(document)
            .ok_or_else(|| format!("Artifact {path} references unknown document {document}"))?;
        let mime = field(artifact, "mime")?;
        match field(artifact, "kind")? {
            "page" => {
                // The manifest binds each document to its exported page. Page
                // placement belongs to the adopter, not to the document ID.
                if mime != "text/markdown" {
                    return Err(format!("Invalid document page MIME: {path}"));
                }
                if !pages.insert(document.to_owned()) {
                    return Err(format!("Duplicate document page identity: {document}"));
                }
            }
            "resource" => {
                identity(local, field(artifact, "id")?, "resource")?;
                let producer = field(artifact, "producer")?;
                let profile = field(artifact, "profile")?;
                crate::Artifact::new(Vec::new(), mime, "bin", producer, profile)?;
                profiles.insert((producer.to_owned(), profile.to_owned()));
            }
            other => return Err(format!("Unknown manifest artifact kind: {other}")),
        }
        let size = artifact["bytes"]
            .as_u64()
            .ok_or_else(|| format!("Manifest artifact {path} requires a byte length"))?;
        let digest = field(artifact, "sha256")?;
        if digest.len() != 64
            || !digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(format!(
                "Manifest artifact {path} requires a lowercase SHA-256"
            ));
        }
        if let Some(bytes) = files.get(path) {
            if size != bytes.len() as u64 || digest != format!("{:x}", Sha256::digest(bytes)) {
                return Err(format!("Manifest artifact size/hash mismatch: {path}"));
            }
        } else if !retired_from.is_some_and(|candidate| !candidate.contains_key(path)) {
            return Err(format!("Manifest artifact inventory: missing {path}"));
        }
    }
    if pages != identities.keys().cloned().collect() {
        return Err("Manifest must own exactly one page for every document".into());
    }
    let extra: Vec<_> = files
        .keys()
        .filter(|path| path.as_str() != "manifest.json" && !paths.contains(*path))
        .collect();
    if !extra.is_empty() {
        return Err(format!(
            "Manifest artifact inventory: unrecorded paths {extra:?}"
        ));
    }
    let mut declared_profiles = BTreeSet::new();
    for profile in array(&manifest, "profiles")? {
        let pair = (
            field(profile, "producer")?.to_owned(),
            field(profile, "profile")?.to_owned(),
        );
        if !declared_profiles.insert(pair) {
            return Err("Duplicate manifest producer/profile pair".into());
        }
    }
    if profiles != declared_profiles {
        return Err(
            "Manifest producer/profile inventory does not match its resource metadata".into(),
        );
    }
    for document in documents {
        let id = field(document, "id")?;
        let local = &identities[id];
        for block in array(document, "blocks")? {
            for reference in array(block, "references")? {
                let target = field(reference, "id")?;
                if local.get(target).map(String::as_str) != Some(field(reference, "kind")?) {
                    return Err(format!(
                        "Manifest reference from {id} is missing or has the wrong kind: {target}"
                    ));
                }
            }
        }
    }
    Ok(())
}

/// Read-only exact check with structured manifest compatibility.
pub fn check(path: &Path, files: &Files) -> Result<()> {
    validate(files)?;
    lifecycle::check(path, files, Ownership::Recorded, compatible)
}

/// Generate a new, independently owned candidate directory.
pub fn build(path: &Path, files: &Files) -> Result<()> {
    validate(files)?;
    lifecycle::build(path, files, Ownership::Recorded)
}

/// Replace a compatible owned baseline only after staging and validation.
pub fn update(path: &Path, files: &Files) -> Result<()> {
    validate(files)?;
    lifecycle::update_with_retirement(path, files, compatible_update)
}

/// Explicitly restore a complete valid backup when its destination is absent.
pub fn recover_backup(path: &Path) -> Result<()> {
    lifecycle::recover_backup(path, Ownership::Recorded, validate)
}
