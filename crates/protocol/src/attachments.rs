//! Bounded canonical base64 transport for save-owned attachments.
use crate::{
    AttachmentMediaType, AttachmentMetadata, ATTACHMENT_CHUNK_BYTES, MAX_ATTACHMENT_BYTES,
    MAX_TEXT_ATTACHMENT_BYTES,
};
impl<'de> serde::Deserialize<'de> for AttachmentMetadata {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            media_type: AttachmentMediaType,
            byte_len: u32,
            #[serde(deserialize_with = "read_sha256")]
            sha256: String,
        }
        let Fields {
            media_type,
            byte_len,
            sha256,
        } = crate::bounded::named_map(deserializer)?;
        let value = Self {
            media_type,
            byte_len,
            sha256,
        };
        if !valid_attachment_metadata(&value) {
            return Err(serde::de::Error::custom("invalid attachment metadata"));
        }
        Ok(value)
    }
}
fn read_sha256<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    struct Sha;
    impl serde::de::Visitor<'_> for Sha {
        type Value = String;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("64 lowercase hexadecimal digits")
        }
        fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<String, E> {
            if value.len() != 64
                || !value
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            {
                return Err(E::custom("invalid attachment checksum"));
            }
            Ok(value.to_owned())
        }
    }
    deserializer.deserialize_str(Sha)
}
pub(crate) fn read_metadata_list<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<AttachmentMetadata>, D::Error> {
    crate::bounded::bounded::<D, AttachmentMetadata, 1>(deserializer)
}
pub fn valid_attachment_metadata(meta: &AttachmentMetadata) -> bool {
    meta.byte_len > 0
        && meta.byte_len as usize
            <= match meta.media_type {
                AttachmentMediaType::Png => MAX_ATTACHMENT_BYTES,
                AttachmentMediaType::Text => MAX_TEXT_ATTACHMENT_BYTES,
            }
        && meta.sha256.len() == 64
        && meta
            .sha256
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub fn attachment_base64_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for c in bytes.chunks(3) {
        let n = (u32::from(c[0]) << 16)
            | (u32::from(*c.get(1).unwrap_or(&0)) << 8)
            | u32::from(*c.get(2).unwrap_or(&0));
        out.push(ALPHABET[((n >> 18) & 63) as usize] as char);
        out.push(ALPHABET[((n >> 12) & 63) as usize] as char);
        out.push(if c.len() > 1 {
            ALPHABET[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if c.len() > 2 {
            ALPHABET[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}
pub fn attachment_base64_decode(text: &str) -> Option<Vec<u8>> {
    if text.is_empty()
        || text.len() > ATTACHMENT_CHUNK_BYTES.div_ceil(3) * 4
        || !text.len().is_multiple_of(4)
    {
        return None;
    }
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    for c in text.as_bytes().as_chunks::<4>().0 {
        let mut n = 0u32;
        for &b in c {
            n = (n << 6)
                | match b {
                    b'A'..=b'Z' => u32::from(b - b'A'),
                    b'a'..=b'z' => u32::from(b - b'a' + 26),
                    b'0'..=b'9' => u32::from(b - b'0' + 52),
                    b'+' => 62,
                    b'/' => 63,
                    b'=' => 0,
                    _ => return None,
                };
        }
        out.push((n >> 16) as u8);
        if c[2] != b'=' {
            out.push((n >> 8) as u8);
        }
        if c[3] != b'=' {
            out.push(n as u8);
        }
    }
    (out.len() <= ATTACHMENT_CHUNK_BYTES && attachment_base64_encode(&out) == text).then_some(out)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn canonical_chunks() {
        for (raw, encoded) in [("f", "Zg=="), ("fo", "Zm8="), ("foo", "Zm9v")] {
            assert_eq!(attachment_base64_encode(raw.as_bytes()), encoded);
            assert_eq!(attachment_base64_decode(encoded).unwrap(), raw.as_bytes());
        }
        for bad in ["", "Zh==", "Zg=Z", "Zg==Zg==", "Zg==\n"] {
            assert!(attachment_base64_decode(bad).is_none());
        }
    }
    #[test]
    fn wire_metadata_and_history_attachment_lists_are_bounded_and_strict() {
        let descriptor =
            serde_json::json!({"media_type":"text","byte_len":3,"sha256":"a".repeat(64)});
        let meta: AttachmentMetadata = serde_json::from_value(descriptor.clone()).unwrap();
        assert_eq!(meta.byte_len, 3);
        for key in ["media_type", "byte_len", "sha256"] {
            let mut bad = descriptor.clone();
            bad.as_object_mut().unwrap().remove(key);
            assert!(serde_json::from_value::<AttachmentMetadata>(bad).is_err());
        }
        for value in [
            serde_json::json!(0),
            serde_json::json!(1048577),
            serde_json::json!("3"),
            serde_json::json!(3.5),
        ] {
            let mut bad = descriptor.clone();
            bad["byte_len"] = value;
            assert!(serde_json::from_value::<AttachmentMetadata>(bad).is_err());
        }
        for value in ["A".repeat(64), "a".repeat(65), "z".repeat(64)] {
            let mut bad = descriptor.clone();
            bad["sha256"] = serde_json::json!(value);
            assert!(serde_json::from_value::<AttachmentMetadata>(bad).is_err());
        }
        assert!(
            serde_json::from_value::<AttachmentMetadata>(serde_json::json!([
                "text",
                3,
                "a".repeat(64)
            ]))
            .is_err()
        );
        let mut note = serde_json::json!({"type":"annotation","anchor":{"type":"entry","id":"entry"},"category":"note","text":"Text capture","attachments":[descriptor.clone()]});
        assert!(serde_json::from_value::<crate::HistoryContent>(note.clone()).is_ok());
        note["attachments"] = serde_json::json!([descriptor.clone(), descriptor]);
        assert!(serde_json::from_value::<crate::HistoryContent>(note).is_err());
    }
}
