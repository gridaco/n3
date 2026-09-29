use super::{
    Settings,
    model::{KEYS, SettingsFile},
};

/// Host storage boundary. Missing data is `Ok(None)`, not an empty document.
/// CAS must fail without replacing data when the current bytes differ from
/// `expected`; `None` means the file must still be absent.
pub trait SettingsStore {
    fn read(&mut self) -> Result<Option<Vec<u8>>, String>;
    fn compare_and_swap(
        &mut self,
        expected: Option<&[u8]>,
        replacement: &[u8],
    ) -> Result<(), String>;
}

/// Three-way merge between the last successfully applied snapshot, the current
/// local preferences, and freshly read file values. Errors never advance the
/// baseline, so a pending UI edit stays pending until success or explicit reload.
pub struct SettingsController<S: SettingsStore> {
    store: S,
    defaults: Settings,
    applied: Settings,
}

impl<S: SettingsStore> SettingsController<S> {
    /// Validate fallback values, without reading or creating a global file.
    pub fn new(store: S, defaults: Settings) -> Result<Self, String> {
        defaults.validate()?;
        Ok(Self {
            store,
            applied: defaults.clone(),
            defaults,
        })
    }

    pub fn sync(&mut self, local: &Settings) -> Result<Settings, String> {
        self.merge(local, false)
    }

    /// Use before opening the JSON in a text editor. Pending local edits obey
    /// the same merge rules; an absent file is explicitly materialized with all
    /// known defaults so its editable settings are discoverable.
    pub fn ensure_file(&mut self, local: &Settings) -> Result<Settings, String> {
        self.merge(local, true)
    }

    /// Explicitly discard pending local preference edits. A missing file means
    /// defaults. Malformed/unreadable data still fails without changing baseline.
    pub fn reload(&mut self) -> Result<Settings, String> {
        let bytes = self.store.read()?;
        let effective = SettingsFile::parse(bytes.as_deref(), &self.defaults)?.effective;
        self.applied = effective.clone();
        Ok(effective)
    }

    fn merge(&mut self, local: &Settings, create_missing: bool) -> Result<Settings, String> {
        let local_values = local.values()?;
        let baseline = self.applied.values()?;
        let bytes = self.store.read()?;
        let file = SettingsFile::parse(bytes.as_deref(), &self.defaults)?;
        let external_values = file.effective.values()?;
        let mut merged = file.values;
        let mut changed = false;
        let mut conflicts = Vec::new();

        for key in KEYS {
            if local_values[key] == baseline[key] {
                continue;
            }
            // Concurrent edits that independently chose the same value converge.
            if external_values[key] == local_values[key] {
                continue;
            }
            if external_values[key] != baseline[key] {
                conflicts.push(key);
                continue;
            }
            merged.insert(
                key.to_owned(),
                serde_json::value::to_raw_value(&local_values[key])
                    .map_err(|error| error.to_string())?,
            );
            changed = true;
        }
        if !conflicts.is_empty() {
            return Err(format!(
                "Settings conflict: {} changed both locally and in the file. Reload settings to use the file, or resolve the file before retrying.",
                conflicts.join(", ")
            ));
        }

        let write = changed || (create_missing && bytes.is_none());
        let effective = if write {
            // A newly created settings.json lists all known keys. Existing files
            // keep omitted defaults and unknown fields; only edited keys change.
            if bytes.is_none() {
                let mut all = self.defaults.raw_values()?;
                all.extend(merged);
                merged = all;
            }
            let replacement = SettingsFile::encode(&merged)?;
            let effective = SettingsFile::parse(Some(&replacement), &self.defaults)?.effective;
            self.store
                .compare_and_swap(bytes.as_deref(), &replacement)?;
            effective
        } else {
            file.effective
        };
        self.applied = effective.clone();
        Ok(effective)
    }
}
