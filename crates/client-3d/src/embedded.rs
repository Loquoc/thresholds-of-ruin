//! The shipped scenario packages, built into the client for devices that
//! have no copy of the repository, such as the Android app.
use std::{
    io,
    path::{Path, PathBuf},
};

pub(crate) struct Package {
    /// The package's directory name in the repository.
    pub(crate) name: &'static str,
    pub(crate) content_hash: &'static str,
    /// Paths relative to the package, with `/` separators, and their bytes.
    pub(crate) files: &'static [(&'static str, &'static [u8])],
}

include!(concat!(env!("OUT_DIR"), "/embedded_scenarios.rs"));

/// Writes each embedded package into `root` under a directory named for its
/// content, unless that copy is already there. Copies from earlier versions
/// are kept, because saves made with them still read them.
pub fn install_scenarios(root: &Path) -> io::Result<Vec<PathBuf>> {
    std::fs::create_dir_all(root)?;
    PACKAGES
        .iter()
        .map(|package| install(root, package))
        .collect()
}

fn install(root: &Path, package: &Package) -> io::Result<PathBuf> {
    let hash = package
        .content_hash
        .get(..12)
        .unwrap_or(package.content_hash);
    let target = root.join(format!("{}-{hash}", package.name));
    // The certificate is written last, so its presence marks a whole copy.
    if target.join("validation.json").is_file() {
        return Ok(target);
    }
    let staging = root.join(format!(".installing-{}-{hash}", package.name));
    if staging.exists() {
        std::fs::remove_dir_all(&staging)?;
    }
    let (certificates, contents): (Vec<_>, Vec<_>) = package
        .files
        .iter()
        .partition(|(path, _)| *path == "validation.json");
    for (path, bytes) in contents.into_iter().chain(certificates) {
        let file = staging.join(path);
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(file, bytes)?;
    }
    if target.exists() {
        // An interrupted copy from before certificates were written last.
        std::fs::remove_dir_all(&target)?;
    }
    std::fs::rename(&staging, &target)?;
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_shipped_package_is_embedded_and_installs_as_a_valid_package() {
        let shipped =
            crate::local_host::discover_scenarios(&crate::launch::repo_root().join("scenarios"));
        assert_eq!(
            PACKAGES.len(),
            shipped.len(),
            "every shipped package, and no test fixture, is embedded"
        );
        let root = crate::launch::repo_root()
            .join(".local/3d-embedded-tests")
            .join(uuid::Uuid::new_v4().simple().to_string());
        let installed = install_scenarios(&root).unwrap();
        for directory in &installed {
            let package = tor_server::scenario_package::load(directory, 7, None, false);
            assert!(
                package.is_ok(),
                "{} must load without --allow-unvalidated: {:?}",
                directory.display(),
                package.err()
            );
        }
        let names: Vec<_> = crate::local_host::discover_scenarios(&root)
            .into_iter()
            .map(|choice| choice.id)
            .collect();
        let mut expected: Vec<_> = shipped.into_iter().map(|choice| choice.id).collect();
        expected.sort();
        assert_eq!(names, expected);

        // A second launch keeps the copy, and a broken one is replaced.
        let marker = installed[0].join("catalog-marker");
        std::fs::write(&marker, b"kept").unwrap();
        assert_eq!(install_scenarios(&root).unwrap(), installed);
        assert!(marker.exists(), "an installed copy is not rewritten");
        std::fs::remove_file(installed[0].join("validation.json")).unwrap();
        install_scenarios(&root).unwrap();
        assert!(!marker.exists(), "an incomplete copy is replaced whole");
        assert!(installed[0].join("validation.json").is_file());
    }
}
