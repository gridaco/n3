//! User-level macOS settings persistence. The settings core owns schema validation.
//!
//! Cooperating N3 instances serialize writes with a create-new sidecar lock.
//! Locks are never stolen: a lock left after a crash produces an explicit error.
//! Existing files are compared again immediately before atomic replacement, but
//! comparison plus rename is not a filesystem CAS against noncooperating editors.
//! First creation uses a no-clobber hard link, so even that external race cannot
//! replace a file that appeared after the missing-file comparison.
use crate::settings::{MAX_SETTINGS_BYTES, SettingsStore};
use std::{
    fs::{self, File, Metadata, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

const MAX_BYTES: u64 = MAX_SETTINGS_BYTES as u64;
static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

pub(super) struct FileSettingsStore {
    path: PathBuf,
}

impl FileSettingsStore {
    /// Resolve the location only; constructing the store never touches disk.
    pub(super) fn global() -> Result<Self, String> {
        let home = std::env::var_os("HOME")
            .ok_or("Cannot locate N3 settings: the home directory is unavailable.")?;
        Ok(Self {
            path: settings_path(Path::new(&home))?,
        })
    }

    pub(super) fn path(&self) -> &Path {
        &self.path
    }
}

/// Keep home resolution separate from environment access for deterministic tests.
pub(super) fn settings_path(home: &Path) -> Result<PathBuf, String> {
    if !home.is_absolute() || home.as_os_str().is_empty() {
        return Err("Cannot locate N3 settings: the home directory must be absolute.".into());
    }
    Ok(home.join("Library/Application Support/N3/settings.json"))
}

impl SettingsStore for FileSettingsStore {
    fn read(&mut self) -> Result<Option<Vec<u8>>, String> {
        Ok(read_snapshot(&self.path)?.map(|snapshot| snapshot.bytes))
    }

    fn compare_and_swap(
        &mut self,
        expected: Option<&[u8]>,
        replacement: &[u8],
    ) -> Result<(), String> {
        if replacement.len() as u64 > MAX_BYTES {
            return Err("N3 settings exceed the 1 MiB file limit.".into());
        }
        let parent = self
            .path
            .parent()
            .ok_or("N3 settings have no parent directory.")?;
        check_ancestors(parent)?;
        fs::create_dir_all(parent)
            .map_err(|error| format!("Cannot create the N3 settings directory: {error}"))?;
        check_ancestors(parent)?;

        let lock_path = parent.join(".settings.json.lock");
        let (_lock_file, _lock) = OwnedPath::create(&lock_path)
            .map_err(|error| format!("Cannot lock N3 settings; another writer or an existing lock may own the file: {error}"))?;
        let original = check_expected(&self.path, expected)?;
        if original
            .as_ref()
            .is_some_and(|snapshot| snapshot.metadata.permissions().readonly())
        {
            return Err("N3 settings are read-only; the existing file was not replaced.".into());
        }
        let temporary = parent.join(format!(
            ".settings-{}-{}.tmp",
            std::process::id(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed),
        ));
        replace_snapshot(
            &self.path,
            &temporary,
            expected,
            replacement,
            original,
            || Ok(()),
        )
    }
}

struct Snapshot {
    bytes: Vec<u8>,
    metadata: Metadata,
}

/// Reject both leaf and ancestor symlinks instead of changing an unexpected target.
fn check_ancestors(parent: &Path) -> Result<(), String> {
    for path in parent.ancestors() {
        match fs::symlink_metadata(path) {
            Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
            Ok(_) => {
                return Err(format!(
                    "N3 settings directory is not a regular directory: {}",
                    path.display()
                ));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("Cannot inspect the N3 settings directory: {error}")),
        }
    }
    Ok(())
}

fn regular_file(metadata: &Metadata) -> Result<(), String> {
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err("N3 settings must be a regular file, not a symlink or directory.".into());
    }
    if metadata.len() > MAX_BYTES {
        return Err("N3 settings exceed the 1 MiB file limit.".into());
    }
    Ok(())
}

fn identity(metadata: &Metadata) -> (u64, u64) {
    (metadata.dev(), metadata.ino())
}

fn read_snapshot(path: &Path) -> Result<Option<Snapshot>, String> {
    if let Some(parent) = path.parent() {
        check_ancestors(parent)?;
    }
    let before = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("Cannot inspect N3 settings: {error}")),
    };
    regular_file(&before)?;
    let file = File::open(path).map_err(|error| format!("Cannot read N3 settings: {error}"))?;
    let opened = file
        .metadata()
        .map_err(|error| format!("Cannot inspect open N3 settings: {error}"))?;
    regular_file(&opened)?;
    if identity(&before) != identity(&opened) {
        return Err("N3 settings changed while opening the file; retry after reviewing it.".into());
    }
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("Cannot read N3 settings: {error}"))?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err("N3 settings exceed the 1 MiB file limit.".into());
    }
    let after = fs::symlink_metadata(path)
        .map_err(|error| format!("Cannot recheck N3 settings after reading: {error}"))?;
    regular_file(&after)?;
    if identity(&opened) != identity(&after)
        || opened.len() != after.len()
        || opened.modified().ok() != after.modified().ok()
    {
        return Err("N3 settings changed while reading the file; retry after reviewing it.".into());
    }
    Ok(Some(Snapshot {
        bytes,
        metadata: after,
    }))
}

fn check_expected(path: &Path, expected: Option<&[u8]>) -> Result<Option<Snapshot>, String> {
    let current = read_snapshot(path)?;
    if current.as_ref().map(|snapshot| snapshot.bytes.as_slice()) != expected {
        return Err("N3 settings changed on disk; reload before saving this preference.".into());
    }
    Ok(current)
}

/// Clean up only the inode created by this operation. A collision, replacement,
/// or symlink at that name never grants ownership of somebody else's file.
struct OwnedPath {
    path: PathBuf,
    identity: (u64, u64),
}

impl OwnedPath {
    fn create(path: &Path) -> Result<(File, Self), String> {
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        let metadata = file
            .metadata()
            .map_err(|error| format!("Cannot inspect newly created settings file: {error}"))?;
        Ok((
            file,
            Self {
                path: path.to_owned(),
                identity: identity(&metadata),
            },
        ))
    }
}

impl Drop for OwnedPath {
    fn drop(&mut self) {
        if let Ok(metadata) = fs::symlink_metadata(&self.path)
            && metadata.is_file()
            && !metadata.file_type().is_symlink()
            && identity(&metadata) == self.identity
        {
            let _ = fs::remove_file(&self.path);
        }
    }
}

fn replace_snapshot(
    path: &Path,
    temporary: &Path,
    expected: Option<&[u8]>,
    replacement: &[u8],
    original: Option<Snapshot>,
    before_recheck: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    let (mut file, _temporary) = OwnedPath::create(temporary)
        .map_err(|error| format!("Cannot create temporary N3 settings: {error}"))?;
    file.write_all(replacement)
        .map_err(|error| format!("Cannot write N3 settings: {error}"))?;
    if let Some(snapshot) = &original {
        file.set_permissions(snapshot.metadata.permissions())
            .map_err(|error| format!("Cannot preserve N3 settings permissions: {error}"))?;
    }
    file.sync_all()
        .map_err(|error| format!("Cannot flush N3 settings: {error}"))?;
    before_recheck()?;
    let current = check_expected(path, expected)?;
    if original.as_ref().map(|snapshot| snapshot.metadata.mode())
        != current.as_ref().map(|snapshot| snapshot.metadata.mode())
    {
        return Err(
            "N3 settings permissions changed during save; the file was not replaced.".into(),
        );
    }
    install_snapshot(path, temporary, expected.is_none())
}

fn install_snapshot(path: &Path, temporary: &Path, was_missing: bool) -> Result<(), String> {
    if was_missing {
        fs::hard_link(temporary, path).map_err(|error| {
            format!("Cannot create N3 settings without replacing an existing file: {error}")
        })?;
    } else {
        fs::rename(temporary, path)
            .map_err(|error| format!("Cannot replace N3 settings: {error}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{PermissionsExt, symlink};

    struct Scratch(PathBuf);
    impl Scratch {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "n3-settings-store-test-{}-{}",
                std::process::id(),
                NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path.canonicalize().unwrap())
        }
        fn store(&self) -> FileSettingsStore {
            FileSettingsStore {
                path: self.0.join("N3/settings.json"),
            }
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn resolves_user_location_without_workspace_or_environment_access() {
        assert_eq!(
            settings_path(Path::new("/Users/example")).unwrap(),
            Path::new("/Users/example/Library/Application Support/N3/settings.json")
        );
        assert!(settings_path(Path::new("")).is_err());
        assert!(settings_path(Path::new("workspace")).is_err());
    }

    #[test]
    fn absent_read_creates_nothing_and_roundtrip_uses_private_new_file() {
        let dir = Scratch::new();
        let mut store = dir.store();
        assert_eq!(store.read().unwrap(), None);
        assert!(!store.path.parent().unwrap().exists());
        store.compare_and_swap(None, b"first").unwrap();
        assert_eq!(store.read().unwrap(), Some(b"first".to_vec()));
        assert_eq!(
            fs::metadata(store.path()).unwrap().permissions().mode() & 0o777,
            0o600
        );
        store.compare_and_swap(Some(b"first"), b"second").unwrap();
        assert_eq!(dir.store().read().unwrap(), Some(b"second".to_vec()));
        assert_eq!(
            fs::read_dir(store.path.parent().unwrap()).unwrap().count(),
            1
        );
    }

    #[test]
    fn collisions_and_external_edits_never_overwrite_or_leave_owned_files() {
        let dir = Scratch::new();
        let mut store = dir.store();
        store.compare_and_swap(None, b"original").unwrap();
        for expected in [None, Some(b"stale".as_slice())] {
            assert!(
                store
                    .compare_and_swap(expected, b"replacement")
                    .unwrap_err()
                    .contains("changed on disk")
            );
            assert_eq!(store.read().unwrap(), Some(b"original".to_vec()));
            assert_eq!(
                fs::read_dir(store.path.parent().unwrap()).unwrap().count(),
                1
            );
        }
        fs::remove_file(store.path()).unwrap();
        assert!(
            store
                .compare_and_swap(Some(b"original"), b"replacement")
                .is_err()
        );
        assert_eq!(
            fs::read_dir(store.path.parent().unwrap()).unwrap().count(),
            0
        );
    }

    #[test]
    fn existing_permissions_survive_and_readonly_settings_are_not_replaced() {
        let dir = Scratch::new();
        let mut store = dir.store();
        store.compare_and_swap(None, b"original").unwrap();
        fs::set_permissions(store.path(), fs::Permissions::from_mode(0o640)).unwrap();
        store
            .compare_and_swap(Some(b"original"), b"updated")
            .unwrap();
        assert_eq!(
            fs::metadata(store.path()).unwrap().permissions().mode() & 0o777,
            0o640
        );
        fs::set_permissions(store.path(), fs::Permissions::from_mode(0o440)).unwrap();
        assert!(
            store
                .compare_and_swap(Some(b"updated"), b"replacement")
                .unwrap_err()
                .contains("read-only")
        );
        assert_eq!(store.read().unwrap(), Some(b"updated".to_vec()));
        assert_eq!(
            fs::read_dir(store.path.parent().unwrap()).unwrap().count(),
            1
        );
    }

    #[test]
    fn symlink_leaf_parent_and_directory_targets_are_rejected() {
        let dir = Scratch::new();
        let mut store = dir.store();
        let target = dir.0.join("foreign.json");
        fs::write(&target, b"foreign").unwrap();
        fs::create_dir(store.path.parent().unwrap()).unwrap();
        symlink(&target, store.path()).unwrap();
        assert!(store.read().is_err());
        assert!(
            store
                .compare_and_swap(Some(b"foreign"), b"replacement")
                .is_err()
        );
        assert_eq!(fs::read(&target).unwrap(), b"foreign");
        fs::remove_file(store.path()).unwrap();
        fs::create_dir(store.path()).unwrap();
        assert!(store.read().is_err());
        assert!(store.compare_and_swap(None, b"replacement").is_err());
        fs::remove_dir_all(store.path.parent().unwrap()).unwrap();
        symlink(&dir.0, store.path.parent().unwrap()).unwrap();
        assert!(store.read().is_err());
        assert!(store.compare_and_swap(None, b"replacement").is_err());
    }

    #[test]
    fn oversized_reads_and_writes_are_bounded_without_mutation() {
        let dir = Scratch::new();
        let mut store = dir.store();
        assert!(
            store
                .compare_and_swap(None, &vec![0; MAX_BYTES as usize + 1])
                .is_err()
        );
        assert!(!store.path.parent().unwrap().exists());
        fs::create_dir(store.path.parent().unwrap()).unwrap();
        let file = File::create(store.path()).unwrap();
        file.set_len(MAX_BYTES + 1).unwrap();
        assert!(store.read().unwrap_err().contains("1 MiB"));
        assert!(store.compare_and_swap(None, b"replacement").is_err());
        assert_eq!(fs::metadata(store.path()).unwrap().len(), MAX_BYTES + 1);
        assert_eq!(
            fs::read_dir(store.path.parent().unwrap()).unwrap().count(),
            1
        );
    }

    #[test]
    fn foreign_lock_and_temporary_files_are_never_removed() {
        let dir = Scratch::new();
        let mut store = dir.store();
        fs::create_dir(store.path.parent().unwrap()).unwrap();
        let lock = store.path.parent().unwrap().join(".settings.json.lock");
        fs::write(&lock, b"other writer").unwrap();
        assert!(
            store
                .compare_and_swap(None, b"replacement")
                .unwrap_err()
                .contains("Cannot lock")
        );
        assert_eq!(fs::read(&lock).unwrap(), b"other writer");
        fs::remove_file(&lock).unwrap();
        let temporary = store.path.parent().unwrap().join("collision.tmp");
        fs::write(&temporary, b"other temporary").unwrap();
        assert!(
            replace_snapshot(store.path(), &temporary, None, b"replacement", None, || Ok(
                ()
            ))
            .is_err()
        );
        assert_eq!(fs::read(&temporary).unwrap(), b"other temporary");
        assert!(!store.path().exists());
    }

    #[test]
    fn late_conflict_and_failed_write_clean_up_only_the_owned_temporary() {
        let dir = Scratch::new();
        let mut store = dir.store();
        store.compare_and_swap(None, b"original").unwrap();
        let temporary = store.path.parent().unwrap().join("owned.tmp");
        let original = check_expected(store.path(), Some(b"original")).unwrap();
        let result = replace_snapshot(
            store.path(),
            &temporary,
            Some(b"original"),
            b"replacement",
            original,
            || {
                fs::write(store.path(), b"external change").unwrap();
                Ok(())
            },
        );
        assert!(result.unwrap_err().contains("changed on disk"));
        assert_eq!(fs::read(store.path()).unwrap(), b"external change");
        assert!(!temporary.exists());
        let original = check_expected(store.path(), Some(b"external change")).unwrap();
        let result = replace_snapshot(
            store.path(),
            &temporary,
            Some(b"external change"),
            b"replacement",
            original,
            || Err("injected I/O failure".into()),
        );
        assert!(result.is_err());
        assert!(!temporary.exists());
        assert_eq!(fs::read(store.path()).unwrap(), b"external change");
    }

    #[test]
    fn first_creation_does_not_clobber_a_file_created_after_the_final_comparison() {
        let dir = Scratch::new();
        let store = dir.store();
        fs::create_dir(store.path.parent().unwrap()).unwrap();
        let temporary = store.path.parent().unwrap().join("owned.tmp");
        let (mut file, owned) = OwnedPath::create(&temporary).unwrap();
        file.write_all(b"replacement").unwrap();
        assert!(check_expected(store.path(), None).unwrap().is_none());
        // A noncooperating editor creates the destination after our last read.
        fs::write(store.path(), b"external creation").unwrap();
        assert!(install_snapshot(store.path(), &temporary, true).is_err());
        drop(owned);
        assert!(!temporary.exists());
        assert_eq!(fs::read(store.path()).unwrap(), b"external creation");
    }

    #[test]
    fn unreadable_file_and_inaccessible_directory_fail_without_mutation() {
        let dir = Scratch::new();
        let mut store = dir.store();
        store.compare_and_swap(None, b"original").unwrap();
        fs::set_permissions(store.path(), fs::Permissions::from_mode(0o000)).unwrap();
        let read = store.read();
        let write = store.compare_and_swap(Some(b"original"), b"replacement");
        fs::set_permissions(store.path(), fs::Permissions::from_mode(0o600)).unwrap();
        assert!(read.is_err());
        assert!(write.is_err());
        assert_eq!(store.read().unwrap(), Some(b"original".to_vec()));

        let parent = store.path.parent().unwrap().to_owned();
        fs::set_permissions(&parent, fs::Permissions::from_mode(0o000)).unwrap();
        let read = store.read();
        let write = store.compare_and_swap(Some(b"original"), b"replacement");
        fs::set_permissions(&parent, fs::Permissions::from_mode(0o700)).unwrap();
        assert!(read.is_err());
        assert!(write.is_err());
        assert_eq!(store.read().unwrap(), Some(b"original".to_vec()));
        assert_eq!(fs::read_dir(parent).unwrap().count(), 1);
    }

    #[test]
    fn cleanup_does_not_remove_a_replacement_at_the_owned_path() {
        let dir = Scratch::new();
        let path = dir.0.join("owned.tmp");
        let (_file, owned) = OwnedPath::create(&path).unwrap();
        fs::remove_file(&path).unwrap();
        let target = dir.0.join("external");
        fs::write(&target, b"foreign").unwrap();
        symlink(&target, &path).unwrap();
        drop(owned);
        assert!(
            fs::symlink_metadata(&path)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read(&target).unwrap(), b"foreign");
    }

    #[test]
    fn permissions_changed_during_save_are_not_restored_from_an_old_snapshot() {
        let dir = Scratch::new();
        let mut store = dir.store();
        store.compare_and_swap(None, b"original").unwrap();
        let temporary = store.path.parent().unwrap().join("owned.tmp");
        let original = check_expected(store.path(), Some(b"original")).unwrap();
        let result = replace_snapshot(
            store.path(),
            &temporary,
            Some(b"original"),
            b"replacement",
            original,
            || {
                fs::set_permissions(store.path(), fs::Permissions::from_mode(0o400)).unwrap();
                Ok(())
            },
        );
        assert!(result.unwrap_err().contains("permissions changed"));
        assert!(!temporary.exists());
        assert_eq!(fs::read(store.path()).unwrap(), b"original");
        assert_eq!(
            fs::metadata(store.path()).unwrap().permissions().mode() & 0o777,
            0o400
        );
    }
}
