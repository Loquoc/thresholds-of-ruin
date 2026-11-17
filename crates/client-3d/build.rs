//! Embeds the shipped scenario packages, so a device without the repository
//! can install them; see `src/embedded.rs`. Test fixtures are left out.
use std::{
    fmt::Write as _,
    path::{Path, PathBuf},
};

fn files(root: &Path, directory: &Path, found: &mut Vec<PathBuf>) {
    let mut entries: Vec<_> = std::fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("reading {}: {error}", directory.display()))
        .map(|entry| entry.unwrap().path())
        .collect();
    entries.sort();
    for path in entries {
        let name = path.file_name().unwrap().to_string_lossy();
        if name.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            files(root, &path, found);
        } else {
            found.push(path.strip_prefix(root).unwrap().to_path_buf());
        }
    }
}

/// The certificate's content hash, without a JSON dependency here.
fn content_hash(certificate: &str) -> Option<String> {
    let start = certificate.find("\"content_hash\"")?;
    let rest = &certificate[start + "\"content_hash\"".len()..];
    let value = rest.split('"').nth(1)?;
    Some(value.to_owned())
}

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let scenarios = manifest.join("../../scenarios");
    println!("cargo:rerun-if-changed={}", scenarios.display());
    let mut source = String::from("pub(crate) static PACKAGES: &[Package] = &[\n");
    let mut directories: Vec<_> = std::fs::read_dir(&scenarios)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.join("scenario.toml").is_file() && !path.ends_with("tests"))
        .collect();
    directories.sort();
    for directory in directories {
        println!("cargo:rerun-if-changed={}", directory.display());
        let name = directory
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let certificate = std::fs::read_to_string(directory.join("validation.json"))
            .unwrap_or_else(|_| panic!("{name} has no validation certificate"));
        let hash = content_hash(&certificate)
            .unwrap_or_else(|| panic!("{name}'s certificate has no content hash"));
        let mut found = Vec::new();
        files(&directory, &directory, &mut found);
        writeln!(
            source,
            "    Package {{ name: {name:?}, content_hash: {hash:?}, files: &["
        )
        .unwrap();
        for file in found {
            let absolute = directory.join(&file);
            println!("cargo:rerun-if-changed={}", absolute.display());
            let relative = file.to_string_lossy().replace('\\', "/");
            writeln!(
                source,
                "        ({relative:?}, include_bytes!({:?})),",
                absolute.canonicalize().unwrap()
            )
            .unwrap();
        }
        source.push_str("    ] },\n");
    }
    source.push_str("];\n");
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("embedded_scenarios.rs");
    std::fs::write(out, source).unwrap();
}
