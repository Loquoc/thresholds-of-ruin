//! Shared test/example-only reader for the sharded recorded wire fixture.
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::io::{self, Read};
use std::path::Path;

const MAX_PART_BYTES: usize = 100_000;
const MAX_PARTS: usize = 1000;
const MAX_SAMPLES: usize = 100_000;

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

fn read_bounded(path: &Path) -> io::Result<Vec<u8>> {
    let file = std::fs::File::open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() > MAX_PART_BYTES as u64 {
        return Err(invalid(
            "wire fixture file exceeds byte bound or is not a file",
        ));
    }
    let mut bytes = Vec::new();
    file.take(MAX_PART_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_PART_BYTES {
        return Err(invalid("wire fixture file exceeds byte bound"));
    }
    Ok(bytes)
}

pub fn load(path: impl AsRef<Path>, expected_protocol: u32) -> io::Result<Value> {
    let path = path.as_ref();
    let manifest = read_bounded(path)?;
    let root = path
        .parent()
        .ok_or_else(|| invalid("fixture has no parent"))?
        .canonicalize()?;
    load_parts(&manifest, expected_protocol, |part| {
        let path = root.join(part).canonicalize()?;
        if !path.starts_with(&root) {
            return Err(invalid("fixture part escapes its root"));
        }
        read_bounded(&path)
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    fixture_format: String,
    protocol: u32,
    client_count: usize,
    server_count: usize,
    client_parts: Vec<String>,
    server_parts: Vec<String>,
}

fn load_parts(
    manifest: &[u8],
    expected_protocol: u32,
    mut read: impl FnMut(&str) -> io::Result<Vec<u8>>,
) -> io::Result<Value> {
    if manifest.len() > MAX_PART_BYTES {
        return Err(invalid("wire fixture manifest exceeds byte bound"));
    }
    let object: Value = serde_json::from_slice(manifest).map_err(io::Error::other)?;
    if !object.is_object() {
        return Err(invalid("wire fixture manifest must be an object"));
    }
    let manifest: Manifest = serde_json::from_value(object).map_err(io::Error::other)?;
    if manifest.fixture_format != "wire-parts-v1" || manifest.protocol != expected_protocol {
        return Err(invalid("wrong wire fixture format or protocol"));
    }
    let client = read_kind(
        &manifest.client_parts,
        manifest.client_count,
        "client",
        expected_protocol,
        &mut read,
    )?;
    let server = read_kind(
        &manifest.server_parts,
        manifest.server_count,
        "server",
        expected_protocol,
        &mut read,
    )?;
    Ok(serde_json::json!({"protocol": manifest.protocol, "client": client, "server": server}))
}

fn read_kind(
    parts: &[String],
    count: usize,
    kind: &str,
    protocol: u32,
    read: &mut impl FnMut(&str) -> io::Result<Vec<u8>>,
) -> io::Result<Vec<Value>> {
    if count > MAX_SAMPLES || parts.len() > MAX_PARTS || parts.len() > count {
        return Err(invalid("wire fixture count exceeds bound"));
    }
    let mut samples = Vec::with_capacity(count);
    for (index, path) in parts.iter().enumerate() {
        let prefix = format!("wire-v{protocol}/{kind}-{index:03}-");
        let digest = path
            .strip_prefix(&prefix)
            .and_then(|name| name.strip_suffix(".json"))
            .filter(|digest| {
                digest.len() == 64
                    && digest
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            })
            .ok_or_else(|| invalid("invalid, reordered or escaping fixture part path"))?;
        let bytes = read(path)?;
        if bytes.len() > MAX_PART_BYTES {
            return Err(invalid("wire fixture part exceeds byte bound"));
        }
        let actual: String = Sha256::digest(&bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        if actual != digest {
            return Err(invalid("wire fixture part checksum mismatch"));
        }
        let part: Vec<Value> = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
        if part.is_empty() || part.len() > count.saturating_sub(samples.len()) {
            return Err(invalid("wire fixture sample count mismatch"));
        }
        samples.extend(part);
    }
    if samples.len() != count {
        return Err(invalid("wire fixture sample count mismatch"));
    }
    Ok(samples)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::BTreeMap;

    use sha2::{Digest, Sha256};
    fn path(kind: &str, index: usize, bytes: &[u8]) -> String {
        let hash: String = Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        format!("wire-v69/{kind}-{index:03}-{hash}.json")
    }
    fn fixture() -> (Value, BTreeMap<String, Vec<u8>>) {
        let a = br#"[{"sample":1},"a"]"#.to_vec();
        let b = br#"[{"sample":2}]"#.to_vec();
        let c = br#"[{"nested":[2,1],"exact":"  x\n"}]"#.to_vec();
        let a_path = path("client", 0, &a);
        let b_path = path("client", 1, &b);
        let c_path = path("server", 0, &c);
        let manifest = json!({"fixture_format":"wire-parts-v1","protocol":69,"client_count":3,"server_count":1,"client_parts":[a_path,b_path],"server_parts":[c_path]});
        (
            manifest,
            BTreeMap::from([(a_path, a), (b_path, b), (c_path, c)]),
        )
    }
    fn read(
        manifest: &Value,
        parts: &BTreeMap<String, Vec<u8>>,
        protocol: u32,
    ) -> io::Result<Value> {
        load_parts(&serde_json::to_vec(manifest).unwrap(), protocol, |path| {
            parts
                .get(path)
                .cloned()
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "missing fixture part"))
        })
    }
    fn replace_part(manifest: &mut Value, parts: &mut BTreeMap<String, Vec<u8>>, bytes: Vec<u8>) {
        parts.remove(manifest["client_parts"][0].as_str().unwrap());
        let path = path("client", 0, &bytes);
        manifest["client_parts"][0] = json!(path);
        parts.insert(path, bytes);
    }
    #[test]
    fn parts_reconstruct_exact_values_in_declared_order() {
        let (manifest, parts) = fixture();
        let loaded = read(&manifest, &parts, 69).unwrap();
        assert_eq!(
            loaded,
            json!({"protocol":69,"client":[{"sample":1},"a",{"sample":2}],"server":[{"nested":[2,1],"exact":"  x\n"}]})
        );
    }
    #[test]
    fn rejects_wrong_versions_counts_missing_parts_and_nonarrays() {
        let (manifest, parts) = fixture();
        assert!(read(&manifest, &parts, 68).is_err());
        for field in [
            "protocol",
            "client_count",
            "server_count",
            "client_parts",
            "server_parts",
        ] {
            let mut invalid = manifest.clone();
            invalid.as_object_mut().unwrap().remove(field);
            assert!(read(&invalid, &parts, 69).is_err(), "missing {field}");
        }
        for (field, value) in [
            ("fixture_format", json!("other")),
            ("client_count", json!(2)),
            ("server_count", json!(2)),
            ("client_count", json!("3")),
        ] {
            let mut invalid = manifest.clone();
            invalid[field] = value;
            assert!(read(&invalid, &parts, 69).is_err(), "invalid {field}");
        }
        let mut missing = parts.clone();
        missing.remove(manifest["client_parts"][1].as_str().unwrap());
        assert!(read(&manifest, &missing, 69).is_err());
        let mut invalid_manifest = manifest;
        let mut invalid_parts = parts;
        replace_part(&mut invalid_manifest, &mut invalid_parts, b"{}".to_vec());
        assert!(read(&invalid_manifest, &invalid_parts, 69).is_err());
    }
    #[test]
    fn rejects_reordered_duplicate_and_escaping_part_paths() {
        let (manifest, parts) = fixture();
        let a = manifest["client_parts"][0].clone();
        let b = manifest["client_parts"][1].clone();
        let mut reordered = manifest.clone();
        reordered["client_parts"] = json!([b, a]);
        assert!(read(&reordered, &parts, 69).is_err());
        let mut duplicate = manifest.clone();
        duplicate["client_count"] = json!(4);
        duplicate["client_parts"] = json!([a, a]);
        assert!(read(&duplicate, &parts, 69).is_err());
        for path in [
            "../outside.json",
            "/tmp/outside.json",
            "wire-v69/../outside.json",
        ] {
            let mut invalid = manifest.clone();
            invalid["client_count"] = json!(2);
            invalid["client_parts"] = json!([path]);
            assert!(
                load_parts(&serde_json::to_vec(&invalid).unwrap(), 69, |_| panic!(
                    "escaping path reached reader"
                ))
                .is_err()
            );
        }
    }
    #[test]
    fn rejects_oversized_parts_and_tampered_digest() {
        let (mut manifest, mut parts) = fixture();
        let mut large = parts[manifest["client_parts"][0].as_str().unwrap()].clone();
        large.resize(100001, b' ');
        replace_part(&mut manifest, &mut parts, large);
        assert!(read(&manifest, &parts, 69).is_err());
        let (manifest, mut parts) = fixture();
        parts.insert(
            manifest["client_parts"][0].as_str().unwrap().into(),
            br#"[{"tampered":true},"a"]"#.to_vec(),
        );
        assert!(read(&manifest, &parts, 69).is_err());
    }
    #[test]
    fn empty_sides_preserve_empty_arrays_and_manifest_shape_is_required() {
        let manifest = json!({"fixture_format":"wire-parts-v1","protocol":69,"client_count":0,"server_count":0,"client_parts":[],"server_parts":[]});
        assert_eq!(
            read(&manifest, &BTreeMap::new(), 69).unwrap(),
            json!({"protocol":69,"client":[],"server":[]})
        );
        assert!(read(
            &json!(["wire-parts-v1", 69, 0, 0, [], []]),
            &BTreeMap::new(),
            69
        )
        .is_err());
        let mut extra = manifest.clone();
        extra["unexpected"] = json!(true);
        assert!(read(&extra, &BTreeMap::new(), 69).is_err());
        let mut too_many = manifest;
        too_many["client_count"] = json!(100001);
        assert!(read(&too_many, &BTreeMap::new(), 69).is_err());
    }
    #[test]
    fn oversized_manifest_is_rejected_before_reading_any_part() {
        let (manifest, _) = fixture();
        let mut bytes = serde_json::to_vec(&manifest).unwrap();
        bytes.resize(100001, b' ');
        assert!(load_parts(&bytes, 69, |_| panic!(
            "oversized manifest must reject before part IO"
        ))
        .is_err());
    }
}
