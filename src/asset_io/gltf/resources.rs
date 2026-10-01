//! glTF URI and data-URI decoding stays in the format adapter.
use super::*;
use crate::asset_io::resources::{MAX_RESOURCE_BYTES, validate_path};
use base64::Engine;

pub(super) fn uri_bytes(uri: &str, resolver: &dyn ResourceResolver) -> Result<Vec<u8>> {
    if let Some(data) = uri.strip_prefix("data:") {
        let (header, payload) = data.split_once(',').ok_or("Malformed glTF data URI.")?;
        if !header.ends_with(";base64") {
            return Err("glTF data URIs must use base64 encoding.".into());
        }
        let mime = header.strip_suffix(";base64").unwrap();
        if !matches!(
            mime,
            "application/octet-stream" | "application/gltf-buffer" | "image/png" | "image/jpeg"
        ) {
            return Err(format!("Unsupported glTF data URI media type: {mime}"));
        }
        if payload.len() > MAX_RESOURCE_BYTES * 4 / 3 + 4 {
            return Err("glTF data URI exceeds its byte budget.".into());
        }
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(payload)
            .map_err(|error| format!("Invalid glTF base64 resource: {error}"))?;
        if bytes.len() > MAX_RESOURCE_BYTES {
            return Err("glTF resource exceeds its byte budget.".into());
        }
        return Ok(bytes);
    }
    let decoded = percent_decode(uri)?;
    validate_path(&decoded)?;
    let bytes = resolver.read(&decoded, MAX_RESOURCE_BYTES)?;
    if bytes.len() > MAX_RESOURCE_BYTES {
        return Err("Resource resolver exceeded its byte budget.".into());
    }
    Ok(bytes)
}

fn percent_decode(uri: &str) -> Result<String> {
    let bytes = uri.as_bytes();
    let mut result = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let code = bytes
                .get(index + 1..index + 3)
                .ok_or("Invalid URI escape.")?;
            let hex = |value: u8| (value as char).to_digit(16).ok_or("Invalid URI escape.");
            result.push((hex(code[0])? * 16 + hex(code[1])?) as u8);
            index += 3;
        } else {
            result.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(result).map_err(|_| "Resource URIs must decode to UTF-8 paths.".into())
}
