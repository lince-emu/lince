// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! LEGAL-30: every Rust, C and C++ source file starts with the project SPDX header.

use std::path::Path;

use crate::error::XtaskError;
use crate::repository;

/// The header may follow a shebang or similar first line, as in the invariant hook.
const HEADER_LINES: usize = 5;
const LICENSE_LINE: &str = "// SPDX-License-Identifier: GPL-3.0-or-later";
const COPYRIGHT_PREFIX: &str = "// SPDX-FileCopyrightText: ";

/// Reports every Rust, C or C++ source in `files` without the SPDX header.
pub(crate) fn check(root: &Path, files: &[String]) -> Result<(), XtaskError> {
    eprintln!("xtask ci: SPDX headers (LEGAL-30)");
    let mut count = 0;
    for file in files.iter().filter(|file| is_checked_source(file)) {
        let Some(source) = repository::read(root, file)? else {
            continue;
        };
        if !has_header(&source) {
            eprintln!("  {file}: missing SPDX header ({COPYRIGHT_PREFIX}…, {LICENSE_LINE})");
            count += 1;
        }
    }
    if count == 0 {
        Ok(())
    } else {
        Err(XtaskError::Violations {
            rule: "LEGAL-30",
            count,
        })
    }
}

/// Source extensions whose files must carry the header; C and C++ since ADR-0030.
const CHECKED_EXTENSIONS: [&str; 5] = ["rs", "cpp", "hpp", "h", "cc"];

fn is_checked_source(file: &str) -> bool {
    Path::new(file)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| CHECKED_EXTENSIONS.contains(&extension))
}

/// Whether the first lines of `source` hold both SPDX header lines.
fn has_header(source: &str) -> bool {
    let head: Vec<&str> = source
        .lines()
        .take(HEADER_LINES)
        .map(str::trim_end)
        .collect();
    head.contains(&LICENSE_LINE) && head.iter().any(|line| line.starts_with(COPYRIGHT_PREFIX))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_the_project_header() {
        let source = "// SPDX-FileCopyrightText: 2026 Lince contributors\n\
                      // SPDX-License-Identifier: GPL-3.0-or-later\n\n//! Docs.\n";
        assert!(has_header(source));
    }

    #[test]
    fn accepts_crlf_line_endings() {
        let source = "// SPDX-FileCopyrightText: 2026 Lince contributors\r\n\
                      // SPDX-License-Identifier: GPL-3.0-or-later\r\n";
        assert!(has_header(source));
    }

    #[test]
    fn checks_rust_c_and_cpp_sources_only() {
        assert!(is_checked_source("crates/lince-shader/cpp/src/ir.cpp"));
        assert!(is_checked_source(
            "crates/lince-shader/cpp/include/lince-shader.h"
        ));
        assert!(is_checked_source("xtask/src/main.rs"));
        assert!(!is_checked_source("docs/backlog.md"));
        assert!(!is_checked_source(".clang-format"));
    }

    #[test]
    fn rejects_a_file_without_header() {
        assert!(!has_header("//! Docs.\nfn main() {}\n"));
    }

    #[test]
    fn rejects_a_different_license() {
        let source = "// SPDX-FileCopyrightText: 2026 Lince contributors\n\
                      // SPDX-License-Identifier: MIT\n";
        assert!(!has_header(source));
    }

    #[test]
    fn rejects_a_missing_copyright_line() {
        assert!(!has_header(
            "// SPDX-License-Identifier: GPL-3.0-or-later\n"
        ));
    }

    #[test]
    fn rejects_a_header_after_the_first_lines() {
        let source = "\n\n\n\n\n// SPDX-FileCopyrightText: 2026 Lince contributors\n\
                      // SPDX-License-Identifier: GPL-3.0-or-later\n";
        assert!(!has_header(source));
    }
}
