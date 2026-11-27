use std::io::{Cursor, Read};

use zip::ZipArchive;

use super::*;

/// Largest compressed file accepted, matching the XLSX limit.
const MAX_COMPRESSED_BYTES: usize = 64 * 1024 * 1024;
/// Largest expanded XML part read into memory.
const MAX_PART_BYTES: u64 = 64 * 1024 * 1024;
/// Most ZIP entries accepted before any part is read.
const MAX_PARTS: usize = 4096;
/// The media type every OpenDocument spreadsheet declares.
const SPREADSHEET_MIMETYPE: &str = "application/vnd.oasis.opendocument.spreadsheet";

/// The two XML parts the import reads.
pub(super) struct OdsParts {
    pub(super) content: Vec<u8>,
    pub(super) styles: Vec<u8>,
}

/// Opens the ZIP package, checks that it is an unencrypted spreadsheet, and reads its XML.
pub(super) fn read_package(bytes: &[u8]) -> Result<OdsParts, Message> {
    if bytes.len() > MAX_COMPRESSED_BYTES {
        return Err(Message::new("status-ods-file-too-large"));
    }
    let mut archive = ZipArchive::new(Cursor::new(bytes)).map_err(|error| {
        Message::new("status-ods-archive-invalid").with_text("detail", error.to_string())
    })?;
    if archive.len() > MAX_PARTS {
        return Err(Message::new("status-ods-file-too-large"));
    }
    // A missing mimetype is tolerated; a different one means another OpenDocument kind.
    if let Some(mimetype) = read_optional_part(&mut archive, "mimetype")?
        && String::from_utf8_lossy(&mimetype).trim() != SPREADSHEET_MIMETYPE
    {
        return Err(Message::new("status-ods-not-spreadsheet"));
    }
    if let Some(manifest) = read_optional_part(&mut archive, "META-INF/manifest.xml")?
        && contains_bytes(&manifest, b"encryption-data")
    {
        return Err(Message::new("status-ods-encrypted"));
    }
    let content = read_optional_part(&mut archive, "content.xml")?
        .ok_or_else(|| Message::new("status-ods-content-missing"))?;
    let styles = read_optional_part(&mut archive, "styles.xml")?.unwrap_or_default();
    Ok(OdsParts { content, styles })
}

/// Reads one named part with a size cap, or `None` when the package lacks it.
fn read_optional_part(
    archive: &mut ZipArchive<Cursor<&[u8]>>,
    name: &str,
) -> Result<Option<Vec<u8>>, Message> {
    let mut part = match archive.by_name(name) {
        Ok(part) => part,
        Err(zip::result::ZipError::FileNotFound) => return Ok(None),
        Err(error) => {
            return Err(
                Message::new("status-ods-archive-invalid").with_text("detail", error.to_string())
            );
        }
    };
    if part.encrypted() {
        return Err(Message::new("status-ods-encrypted"));
    }
    if part.size() > MAX_PART_BYTES {
        return Err(Message::new("status-ods-file-too-large"));
    }
    // The declared size can lie, so the read itself is capped as well.
    let mut bytes = Vec::new();
    (&mut part)
        .take(MAX_PART_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| {
            Message::new("status-ods-archive-invalid").with_text("detail", error.to_string())
        })?;
    if bytes.len() as u64 > MAX_PART_BYTES {
        return Err(Message::new("status-ods-file-too-large"));
    }
    Ok(Some(bytes))
}

/// Checks whether `needle` occurs anywhere in `haystack`.
fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}
