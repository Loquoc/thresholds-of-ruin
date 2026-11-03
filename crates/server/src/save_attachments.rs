//! Save-owned descriptor DTOs; no wire serializer defines these stored bytes.
use serde::{Deserialize, Serialize};
use tor_protocol as wire;

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum MediaType {
    Png,
    Text,
}

#[derive(Serialize)]
struct Metadata {
    media_type: MediaType,
    byte_len: u32,
    sha256: String,
}
impl<'de> Deserialize<'de> for Metadata {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            media_type: MediaType,
            byte_len: u32,
            sha256: String,
        }
        let Fields {
            media_type,
            byte_len,
            sha256,
        } = super::named_map(deserializer)?;
        let value = Self {
            media_type,
            byte_len,
            sha256,
        };
        if !wire::valid_attachment_metadata(&value.to_runtime()) {
            return Err(serde::de::Error::custom(
                "invalid saved attachment descriptor",
            ));
        }
        Ok(value)
    }
}
impl Metadata {
    fn from_runtime(value: &wire::AttachmentMetadata) -> Self {
        Self {
            media_type: match value.media_type {
                wire::AttachmentMediaType::Png => MediaType::Png,
                wire::AttachmentMediaType::Text => MediaType::Text,
            },
            byte_len: value.byte_len,
            sha256: value.sha256.clone(),
        }
    }
    fn to_runtime(&self) -> wire::AttachmentMetadata {
        wire::AttachmentMetadata {
            media_type: match self.media_type {
                MediaType::Png => wire::AttachmentMediaType::Png,
                MediaType::Text => wire::AttachmentMediaType::Text,
            },
            byte_len: self.byte_len,
            sha256: self.sha256.clone(),
        }
    }
}
pub(crate) fn serialize<S: serde::Serializer>(
    values: &[wire::AttachmentMetadata],
    serializer: S,
) -> Result<S::Ok, S::Error> {
    if values.len() > 1
        || values
            .iter()
            .any(|value| !wire::valid_attachment_metadata(value))
    {
        return Err(serde::ser::Error::custom(
            "invalid saved attachment descriptor list",
        ));
    }
    use serde::ser::SerializeSeq;
    let mut sequence = serializer.serialize_seq(Some(values.len()))?;
    for value in values {
        sequence.serialize_element(&Metadata::from_runtime(value))?;
    }
    sequence.end()
}
pub(crate) fn deserialize<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<wire::AttachmentMetadata>, D::Error> {
    struct Descriptors;
    impl<'de> serde::de::Visitor<'de> for Descriptors {
        type Value = Vec<wire::AttachmentMetadata>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("at most one saved attachment descriptor")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut sequence: A,
        ) -> Result<Self::Value, A::Error> {
            if sequence.size_hint().is_some_and(|size| size > 1) {
                return Err(serde::de::Error::custom("too many saved attachments"));
            }
            let mut values = Vec::new();
            if let Some(value) = sequence.next_element::<Metadata>()? {
                values.push(value.to_runtime());
            }
            if sequence.next_element::<serde::de::IgnoredAny>()?.is_some() {
                return Err(serde::de::Error::custom("too many saved attachments"));
            }
            Ok(values)
        }
    }
    deserializer.deserialize_seq(Descriptors)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Serialize, Deserialize)]
    struct Stored(#[serde(with = "super")] Vec<wire::AttachmentMetadata>);
    #[test]
    fn saved_descriptors_have_required_named_fields_numeric_lengths_and_one_row() {
        let row = serde_json::json!({"media_type":"text","byte_len":3,"sha256":"a".repeat(64)});
        let json = serde_json::json!([row.clone()]);
        let decoded: Stored = serde_json::from_value(json.clone()).unwrap();
        assert_eq!(decoded.0[0].byte_len, 3);
        assert_eq!(serde_json::to_value(decoded).unwrap(), json);
        assert!(serde_json::from_value::<Stored>(serde_json::json!([])).is_ok());
        for missing in ["media_type", "byte_len", "sha256"] {
            let mut bad = row.clone();
            bad.as_object_mut().unwrap().remove(missing);
            assert!(serde_json::from_value::<Stored>(serde_json::json!([bad])).is_err());
        }
        for bad_length in [
            serde_json::json!("3"),
            serde_json::json!(-1),
            serde_json::json!(3.5),
            serde_json::json!(0),
            serde_json::json!(1048577),
        ] {
            let mut bad = row.clone();
            bad["byte_len"] = bad_length;
            assert!(serde_json::from_value::<Stored>(serde_json::json!([bad])).is_err());
        }
        let mut extra = row.clone();
        extra["path"] = serde_json::json!("external.png");
        assert!(serde_json::from_value::<Stored>(serde_json::json!([extra])).is_err());
        assert!(serde_json::from_value::<Stored>(serde_json::json!([row.clone(), row])).is_err());
        assert!(
            serde_json::from_value::<Stored>(serde_json::json!([["text", 3, "a".repeat(64)]]))
                .is_err()
        );
    }
}
