// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! LEGAL-31: every crate manifest declares `license = "GPL-3.0-or-later"` in `[package]`.
//!
//! The declaration must be literal; `license.workspace = true` does not count, so each
//! manifest states its license on its own (LEGAL-31 wording).

use std::path::Path;

use crate::error::XtaskError;
use crate::repository;

const LICENSE_VALUE: &str = "\"GPL-3.0-or-later\"";

/// What a `Cargo.toml` says about the project license.
#[derive(Debug, PartialEq, Eq)]
enum ManifestLicense {
    /// No `[package]` section (for example the workspace root).
    NotAPackage,
    /// `[package]` declares the project license.
    Declared,
    /// `[package]` exists but does not declare the project license.
    Missing,
}

/// Reports every crate manifest in `files` that does not declare the project license.
pub(crate) fn check(root: &Path, files: &[String]) -> Result<(), XtaskError> {
    eprintln!("xtask ci: crate license fields (LEGAL-31)");
    let mut count = 0;
    for file in files.iter().filter(|file| is_manifest(file)) {
        let Some(manifest) = repository::read(root, file)? else {
            continue;
        };
        if inspect(&manifest) == ManifestLicense::Missing {
            eprintln!("  {file}: [package] must declare license = {LICENSE_VALUE}");
            count += 1;
        }
    }
    if count == 0 {
        Ok(())
    } else {
        Err(XtaskError::Violations {
            rule: "LEGAL-31",
            count,
        })
    }
}

fn is_manifest(file: &str) -> bool {
    file == "Cargo.toml" || file.ends_with("/Cargo.toml")
}

fn inspect(manifest: &str) -> ManifestLicense {
    let mut in_package = false;
    let mut has_package = false;
    for line in manifest.lines().map(str::trim) {
        if line.starts_with('[') {
            in_package = line == "[package]";
            has_package |= in_package;
        } else if in_package && is_license_line(line) {
            return ManifestLicense::Declared;
        }
    }
    if has_package {
        ManifestLicense::Missing
    } else {
        ManifestLicense::NotAPackage
    }
}

fn is_license_line(line: &str) -> bool {
    line.split_once('=')
        .is_some_and(|(key, value)| key.trim() == "license" && value.trim() == LICENSE_VALUE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_a_literal_license_in_package() {
        let manifest = "[package]\nname = \"a\"\nlicense = \"GPL-3.0-or-later\"\n";
        assert_eq!(inspect(manifest), ManifestLicense::Declared);
    }

    #[test]
    fn rejects_a_package_without_license() {
        assert_eq!(
            inspect("[package]\nname = \"a\"\n"),
            ManifestLicense::Missing
        );
    }

    #[test]
    fn rejects_another_license() {
        let manifest = "[package]\nlicense = \"MIT\"\n";
        assert_eq!(inspect(manifest), ManifestLicense::Missing);
    }

    #[test]
    fn rejects_an_inherited_license() {
        let manifest = "[package]\nlicense.workspace = true\n";
        assert_eq!(inspect(manifest), ManifestLicense::Missing);
    }

    #[test]
    fn ignores_license_keys_outside_package() {
        let manifest =
            "[package]\nname = \"a\"\n\n[package.metadata.x]\nlicense = \"GPL-3.0-or-later\"\n";
        assert_eq!(inspect(manifest), ManifestLicense::Missing);
    }

    #[test]
    fn skips_a_workspace_manifest() {
        let manifest = "[workspace]\nmembers = [\"crates/*\"]\n";
        assert_eq!(inspect(manifest), ManifestLicense::NotAPackage);
    }

    #[test]
    fn recognizes_nested_manifests_only_by_file_name() {
        assert!(is_manifest("Cargo.toml"));
        assert!(is_manifest("crates/lince-common/Cargo.toml"));
        assert!(!is_manifest("crates/NotCargo.toml"));
    }
}
