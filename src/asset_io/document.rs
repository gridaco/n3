//! Native document loading and persistence, independent of dialogs and rendering.
//! Pure JSON encoding/decoding stays with the versioned N3 document schema.
use crate::document::{Document, MAX_BYTES};
use std::{
    fs,
    io::{Read, Write},
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};
pub(super) fn read_text(path: &Path) -> Result<String, String> {
    let file = fs::File::open(path).map_err(|e| format!("Cannot open {}: {e}", path.display()))?;
    let mut text = String::new();
    file.take(MAX_BYTES + 1)
        .read_to_string(&mut text)
        .map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
    if text.len() as u64 > MAX_BYTES {
        return Err("File exceeds the 64 MiB input limit".into());
    }
    Ok(text)
}

/// Open native JSON or import an OBJ, selected by its case-insensitive extension.
pub(crate) fn load_path(path: &Path) -> Result<Document, String> {
    if path
        .extension()
        .and_then(|s| s.to_str())
        .is_some_and(|s| s.eq_ignore_ascii_case("obj"))
    {
        super::obj::load_path(path)
    } else {
        Document::from_json(&read_text(path)?)
    }
}

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

pub fn is_native_path(path: &Path) -> bool {
    path.file_name().is_some_and(|n| {
        n.to_string_lossy()
            .to_ascii_lowercase()
            .ends_with(".n3.json")
    })
}

/// Write a validated snapshot. `expected` is the exact last-read/saved file;
/// overwrite permits a separately chosen Save As target, never a stale baseline.
pub fn save(
    path: &Path,
    document: &Document,
    expected: Option<&[u8]>,
    overwrite: bool,
) -> Result<Vec<u8>, String> {
    if !is_native_path(path) {
        return Err("Native documents must use the .n3.json extension.".into());
    }
    // Rebase a serialization snapshot, never the editor's working document or
    // history. Live asset references are absolute; saved references are portable
    // relative paths when the destination and source share a filesystem root.
    let bytes = super::linked::snapshot_for_save(document, path)?
        .to_json()?
        .into_bytes();
    let check = || -> Result<(), String> {
        match fs::read(path) {
            Ok(current) => {
                if let Some(expected) = expected {
                    if current != expected {
                        return Err("The document changed on disk. Use Save as… to preserve your edits in another file.".into());
                    }
                } else if !overwrite {
                    return Err(
                        "The destination already exists; choose Save as… to replace it explicitly."
                            .into(),
                    );
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                if expected.is_some() {
                    return Err(
                        "The document was removed on disk. Use Save as… to choose a destination."
                            .into(),
                    );
                }
            }
            Err(e) => return Err(format!("Cannot read destination: {e}")),
        }
        Ok(())
    };
    check()?;
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let temp = parent.join(format!(
        ".n3-save-{}-{}.tmp",
        std::process::id(),
        NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
    ));
    write_snapshot(path, &temp, &bytes, check)?;
    Ok(bytes)
}

fn write_snapshot(
    path: &Path,
    temp: &Path,
    bytes: &[u8],
    check: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    // Failure to create the file gives us no ownership of this path. Return
    // before entering the cleanup path so an existing file stays untouched.
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(temp)
        .map_err(|e| format!("Cannot create save file: {e}"))?;
    let result = (|| -> Result<(), String> {
        file.write_all(bytes)
            .and_then(|_| file.sync_all())
            .map_err(|e| format!("Cannot write document: {e}"))?;
        if let Ok(metadata) = fs::metadata(path) {
            fs::set_permissions(temp, metadata.permissions()).map_err(|e| e.to_string())?;
        }
        check()?;
        fs::rename(temp, path).map_err(|e| format!("Cannot replace document: {e}"))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Scratch(std::path::PathBuf);
    impl Scratch {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!(
                "n3-save-test-{}-{}",
                std::process::id(),
                NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&p).unwrap();
            Self(p)
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn roundtrip_and_external_change_protection() {
        let dir = Scratch::new();
        let path = dir.0.join("shape.n3.json");
        let mut doc = Document::default();
        doc.insert_primitive(crate::document::PrimitiveKind::Cube)
            .unwrap();
        let first = save(&path, &doc, None, false).unwrap();
        assert_eq!(load_path(&path).unwrap(), doc);
        assert_eq!(
            Document::from_json(std::str::from_utf8(&first).unwrap()).unwrap(),
            doc
        );
        assert!(save(&path, &doc, None, false).is_err());
        fs::write(&path, b"external edit").unwrap();
        assert!(
            save(&path, &doc, Some(&first), false)
                .unwrap_err()
                .contains("changed on disk")
        );
        assert_eq!(fs::read(&path).unwrap(), b"external edit");
        assert_eq!(fs::read_dir(&dir.0).unwrap().count(), 1);
    }
    #[test]
    fn invalid_document_does_not_replace_existing_data() {
        let dir = Scratch::new();
        let path = dir.0.join("shape.n3.json");
        let doc = Document::default();
        let first = save(&path, &doc, None, false).unwrap();
        let mut invalid = doc;
        invalid.version = u32::MAX;
        assert!(save(&path, &invalid, Some(&first), false).is_err());
        assert_eq!(fs::read(&path).unwrap(), first);
        assert!(save(&dir.0.join("source.obj"), &Document::default(), None, false).is_err());
        assert!(!is_native_path(&dir.0.join("imported.json")));
        assert!(
            save(
                &dir.0.join("Uppercase.N3.JSON"),
                &Document::default(),
                None,
                false
            )
            .is_ok()
        );
    }

    #[test]
    fn temporary_file_collision_preserves_the_existing_file_and_destination() {
        let dir = Scratch::new();
        let destination = dir.0.join("shape.n3.json");
        let temp = dir.0.join("foreign-save.tmp");
        fs::write(&destination, b"original document").unwrap();
        fs::write(&temp, b"another writer's file").unwrap();

        let error = write_snapshot(&destination, &temp, b"replacement", || Ok(())).unwrap_err();

        assert!(error.contains("Cannot create save file"));
        assert_eq!(fs::read(&temp).unwrap(), b"another writer's file");
        assert_eq!(fs::read(&destination).unwrap(), b"original document");
    }

    #[test]
    fn conflict_after_temp_creation_cleans_up_only_the_owned_temp() {
        let dir = Scratch::new();
        let destination = dir.0.join("shape.n3.json");
        let temp = dir.0.join("owned-save.tmp");
        fs::write(&destination, b"external change").unwrap();

        let error = write_snapshot(&destination, &temp, b"replacement", || {
            Err("The document changed on disk".into())
        })
        .unwrap_err();

        assert!(error.contains("changed on disk"));
        assert!(!temp.exists());
        assert_eq!(fs::read(&destination).unwrap(), b"external change");
    }
}
