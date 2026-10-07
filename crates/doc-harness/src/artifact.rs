//! Owned artifact payloads and the explicit file-reading boundary.

use crate::Result;
use std::path::Path;

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
