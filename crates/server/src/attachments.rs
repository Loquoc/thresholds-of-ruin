//! Payload validation independent of simulation and transport authority.
use crate::Failure;
use sha2::{Digest, Sha256};
use std::io::Read;
use tor_protocol::{AttachmentMediaType, AttachmentMetadata, ErrorCode};
pub(crate) const MAX_SAVE_ATTACHMENT_BYTES: usize = 256 * 1024 * 1024;
#[cfg(test)]
thread_local! { static SAVE_ATTACHMENT_TEST_LIMIT: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) }; }
pub(crate) fn save_attachment_limit() -> usize {
    #[cfg(test)]
    if let Some(limit) = SAVE_ATTACHMENT_TEST_LIMIT.with(std::cell::Cell::get) {
        return limit;
    }
    MAX_SAVE_ATTACHMENT_BYTES
}
#[cfg(test)]
pub(crate) struct SaveAttachmentTestLimit {
    previous: Option<usize>,
    _thread: std::marker::PhantomData<std::rc::Rc<()>>,
}
#[cfg(test)]
impl SaveAttachmentTestLimit {
    pub(crate) fn new(limit: usize) -> Self {
        Self {
            previous: SAVE_ATTACHMENT_TEST_LIMIT.with(|value| value.replace(Some(limit))),
            _thread: std::marker::PhantomData,
        }
    }
}
#[cfg(test)]
impl Drop for SaveAttachmentTestLimit {
    fn drop(&mut self) {
        SAVE_ATTACHMENT_TEST_LIMIT.with(|value| value.set(self.previous));
    }
}
fn invalid() -> Failure {
    Failure::new(ErrorCode::InvalidRequest, "Invalid attachment payload")
}
pub(crate) fn validate_payload(meta: &AttachmentMetadata, bytes: &[u8]) -> Result<(), Failure> {
    if !tor_protocol::valid_attachment_metadata(meta)
        || bytes.len() != meta.byte_len as usize
        || Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
            != meta.sha256
    {
        return Err(invalid());
    }
    match meta.media_type {
        AttachmentMediaType::Text => {
            std::str::from_utf8(bytes).map_err(|_| invalid())?;
        }
        AttachmentMediaType::Png => validate_png(bytes)?,
    }
    Ok(())
}
fn crc(bytes: &[u8]) -> u32 {
    let mut c = !0u32;
    for &b in bytes {
        c ^= u32::from(b);
        for _ in 0..8 {
            c = (c >> 1) ^ (0xedb88320u32.wrapping_mul(c & 1));
        }
    }
    !c
}
fn validate_png(bytes: &[u8]) -> Result<(), Failure> {
    if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err(invalid());
    }
    let mut pos = 8usize;
    let mut dimensions = None;
    let mut compressed = Vec::new();
    let mut ended = false;
    let mut idat_ended = false;
    while pos < bytes.len() {
        let header = bytes.get(pos..pos + 8).ok_or_else(invalid)?;
        let size = u32::from_be_bytes(header[..4].try_into().unwrap()) as usize;
        let end = pos
            .checked_add(12)
            .and_then(|n| n.checked_add(size))
            .ok_or_else(invalid)?;
        let chunk = bytes.get(pos..end).ok_or_else(invalid)?;
        let kind = &header[4..];
        let data = &chunk[8..8 + size];
        if !kind.iter().all(u8::is_ascii_alphabetic) || !kind[2].is_ascii_uppercase() {
            return Err(invalid());
        }
        if crc(&chunk[4..8 + size]) != u32::from_be_bytes(chunk[8 + size..].try_into().unwrap()) {
            return Err(invalid());
        }
        if dimensions.is_none() && kind != b"IHDR" {
            return Err(invalid());
        }
        match kind {
            b"IHDR" => {
                if pos != 8 || size != 13 {
                    return Err(invalid());
                }
                let w = u32::from_be_bytes(data[..4].try_into().unwrap());
                let h = u32::from_be_bytes(data[4..8].try_into().unwrap());
                let channels = match data[9] {
                    0 => 1,
                    2 => 3,
                    4 => 2,
                    6 => 4,
                    _ => return Err(invalid()),
                };
                if w == 0
                    || h == 0
                    || w > 4096
                    || h > 4096
                    || u64::from(w) * u64::from(h) > 16 * 1024 * 1024
                    || data[8] != 8
                    || data[10..] != [0, 0, 0]
                {
                    return Err(invalid());
                }
                dimensions = Some((w as usize, h as usize, channels));
            }
            b"IDAT" => {
                if idat_ended {
                    return Err(invalid());
                }
                compressed.extend_from_slice(data);
            }
            b"IEND" => {
                if size != 0 || compressed.is_empty() || end != bytes.len() {
                    return Err(invalid());
                }
                ended = true;
            }
            _ => {
                if !compressed.is_empty() {
                    idat_ended = true;
                }
                if kind[0] & 32 == 0 {
                    return Err(invalid());
                }
            }
        }
        pos = end;
    }
    if !ended {
        return Err(invalid());
    }
    let (w, h, channels) = dimensions.ok_or_else(invalid)?;
    let stride = w * channels + 1;
    let expected = stride * h;
    let mut decoder = flate2::read::ZlibDecoder::new(compressed.as_slice());
    let mut decoded = Vec::new();
    decoder
        .by_ref()
        .take((expected + 1) as u64)
        .read_to_end(&mut decoded)
        .map_err(|_| invalid())?;
    if decoded.len() != expected
        || decoder.total_in() != compressed.len() as u64
        || decoded.chunks_exact(stride).any(|row| row[0] > 4)
    {
        return Err(invalid());
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_text_and_hash_are_required() {
        let bytes = b"  a\t\n\x1b[31mred\x1b[0m\n";
        let mut meta = AttachmentMetadata {
            media_type: AttachmentMediaType::Text,
            byte_len: bytes.len() as u32,
            sha256: Sha256::digest(bytes)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>(),
        };
        assert!(validate_payload(&meta, bytes).is_ok());
        meta.sha256.replace_range(..1, "x");
        assert!(validate_payload(&meta, bytes).is_err());
    }
    #[test]
    fn malformed_png_is_rejected_even_with_valid_hash() {
        let bytes = b"\x89PNG\r\n\x1a\n";
        let meta = AttachmentMetadata {
            media_type: AttachmentMediaType::Png,
            byte_len: 8,
            sha256: Sha256::digest(bytes)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>(),
        };
        assert!(validate_payload(&meta, bytes).is_err());
    }
    #[test]
    fn png_fixture_checks_crc_inflation_dimensions_and_trailing_data() {
        let png: &[u8] = &[
            137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1,
            8, 6, 0, 0, 0, 31, 21, 196, 137, 0, 0, 0, 13, 73, 68, 65, 84, 120, 156, 99, 248, 207,
            192, 240, 31, 0, 5, 0, 1, 255, 137, 153, 61, 29, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66,
            96, 130,
        ];
        assert!(validate_png(png).is_ok());
        let mut reserved = png[..33].to_vec();
        reserved.extend_from_slice(&0u32.to_be_bytes());
        reserved.extend_from_slice(b"abcd");
        reserved.extend_from_slice(&crc(b"abcd").to_be_bytes());
        reserved.extend_from_slice(&png[33..]);
        assert!(validate_png(&reserved).is_err());
        let mut corrupt = png.to_vec();
        corrupt[45] ^= 1;
        assert!(validate_png(&corrupt).is_err());
        let mut trailing = png.to_vec();
        trailing.push(0);
        assert!(validate_png(&trailing).is_err());
        let mut huge = png.to_vec();
        huge[16..20].copy_from_slice(&4097u32.to_be_bytes());
        let sum = crc(&huge[12..29]);
        huge[29..33].copy_from_slice(&sum.to_be_bytes());
        assert!(validate_png(&huge).is_err());
        let mut interlaced = png.to_vec();
        interlaced[28] = 1;
        let sum = crc(&interlaced[12..29]);
        interlaced[29..33].copy_from_slice(&sum.to_be_bytes());
        assert!(validate_png(&interlaced).is_err());
    }
}
