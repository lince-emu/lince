// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Conventional Commits (docs/conventions/commits.md §9) for every commit of the branch and
//! for the pull-request title, with the same rules as `.githooks/commit-msg`.
//!
//! The commits checked are `<base>..HEAD`, where `<base>` is `--base` or, without it,
//! `origin/main`. The PR title is checked when `--pr-title` is given (CI passes it from the
//! pull-request event).

use std::path::Path;

use crate::error::XtaskError;
use crate::process;

const DEFAULT_BASE: &str = "origin/main";

/// What `cargo xtask ci` was asked to check.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Range {
    /// `--base REV`: the revision the branch started from.
    pub(crate) base: Option<String>,
    /// `--pr-title TITLE`: the pull-request title, which becomes the squash commit.
    pub(crate) pr_title: Option<String>,
}

const TYPES: [&str; 10] = [
    "feat", "fix", "perf", "refactor", "test", "docs", "build", "ci", "chore", "revert",
];
/// Types whose scope may be omitted for repository-wide changes.
const SCOPE_OPTIONAL: [&str; 3] = ["chore", "build", "revert"];
const MAX_HEADER: usize = 72;
/// GitHub appends ` (#NNNNN)` to the PR title in the squash commit; this leaves room for it.
const MAX_PR_TITLE: usize = 66;
/// Headers the hook lets through: GitHub merges and fixup commits on feature branches.
const EXEMPT_PREFIXES: [&str; 4] = ["Merge ", "fixup! ", "squash! ", "amend! "];
/// Description openings that are not in the imperative mood.
const NOT_IMPERATIVE: [&str; 10] = [
    "added", "adds", "fixed", "fixes", "updated", "updates", "removed", "removes", "changed",
    "changes",
];

/// Checks the branch's commits and, in CI, the pull-request title.
pub(crate) fn check(root: &Path, range: &Range) -> Result<(), XtaskError> {
    eprintln!("xtask ci: commit messages (Conventional Commits)");
    let mut count = 0;
    let base = range.base.as_deref().unwrap_or(DEFAULT_BASE);
    let base_exists = process::succeeds(&mut process::command(
        root,
        "git",
        &["rev-parse", "--verify", "--quiet", base],
    ))?;
    if base_exists {
        let range = format!("{base}..HEAD");
        let listing = process::capture(root, "git", &["rev-list", "--reverse", &range])?;
        for sha in listing.lines() {
            let message = process::capture(root, "git", &["log", "-1", "--format=%B", sha])?;
            if let Err(reason) = validate(&message) {
                let short = sha.get(..10).unwrap_or(sha);
                eprintln!("  commit {short} \"{}\": {reason}", header(&message));
                count += 1;
            }
        }
    } else if range.base.is_some() {
        eprintln!("  --base {base} is not a revision of this clone");
        count += 1;
    } else {
        eprintln!("  skipped: {DEFAULT_BASE} does not exist in this clone");
    }
    if let Some(title) = &range.pr_title
        && let Err(reason) = validate_pr_title(title)
    {
        eprintln!("  pull-request title \"{title}\": {reason}");
        count += 1;
    }
    if count == 0 {
        Ok(())
    } else {
        Err(XtaskError::Violations {
            rule: "commit message",
            count,
        })
    }
}

/// Lines git keeps in a message: comment lines are dropped, as `git commit` does.
fn message_lines(message: &str) -> impl Iterator<Item = &str> {
    message
        .lines()
        .filter(|line| !line.starts_with('#'))
        .map(|line| line.trim_end_matches('\r'))
}

/// The first line with visible text.
fn header(message: &str) -> &str {
    message_lines(message)
        .find(|line| !line.trim().is_empty())
        .unwrap_or("")
}

/// Validates a pull-request title: a header of at most [`MAX_PR_TITLE`] characters, so the
/// squash commit stays within [`MAX_HEADER`] once GitHub appends the PR number.
pub(crate) fn validate_pr_title(title: &str) -> Result<(), String> {
    validate(title)?;
    let length = title.chars().count();
    if length > MAX_PR_TITLE {
        return Err(format!(
            "title is {length} characters; the limit is {MAX_PR_TITLE}, leaving room for \" (#NN)\""
        ));
    }
    Ok(())
}

/// Validates one commit message (or a header alone).
pub(crate) fn validate(message: &str) -> Result<(), String> {
    let header = header(message);
    if EXEMPT_PREFIXES.iter().any(|p| header.starts_with(p)) {
        return Ok(());
    }
    let (kind, scope, bang, description) =
        split_header(header).ok_or("header does not match <type>(<scope>): <description>")?;
    let length = header.chars().count();
    if length > MAX_HEADER {
        return Err(format!(
            "header is {length} characters; the limit is {MAX_HEADER}"
        ));
    }
    if scope.is_none() && !SCOPE_OPTIONAL.contains(&kind) {
        return Err(format!("scope is required for '{kind}'"));
    }
    if !description.starts_with(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit()) {
        return Err("description must start with a lowercase letter or digit".to_owned());
    }
    if description.ends_with('.') {
        return Err("description must not end with a period".to_owned());
    }
    if NOT_IMPERATIVE.iter().any(|w| description.starts_with(w)) {
        return Err("use the imperative mood (\"add\", not \"added\"/\"adds\")".to_owned());
    }
    if bang && !message_lines(message).any(|l| l.starts_with("BREAKING CHANGE: ")) {
        return Err("'!' requires a 'BREAKING CHANGE: <impact and migration>' footer".to_owned());
    }
    if message_lines(message)
        .nth(1)
        .is_some_and(|second| !second.is_empty())
    {
        return Err("leave a blank line between the header and the body".to_owned());
    }
    Ok(())
}

/// Splits `type(scope)!: description`; `None` when the header has another shape.
fn split_header(header: &str) -> Option<(&str, Option<&str>, bool, &str)> {
    let kind_end = header.find(|c: char| !c.is_ascii_lowercase())?;
    let kind = TYPES.into_iter().find(|t| *t == &header[..kind_end])?;
    let mut rest = &header[kind_end..];
    let mut scope = None;
    if let Some(after) = rest.strip_prefix('(') {
        let close = after.find(')')?;
        let s = &after[..close];
        let valid = s.starts_with(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit())
            && s.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
        if !valid {
            return None;
        }
        scope = Some(s);
        rest = &after[close + 1..];
    }
    let (bang, rest) = match rest.strip_prefix('!') {
        Some(r) => (true, r),
        None => (false, rest),
    };
    let description = rest.strip_prefix(": ")?;
    (!description.is_empty()).then_some((kind, scope, bang, description))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{validate, validate_pr_title};
    use crate::process;

    /// Messages and whether `.githooks/commit-msg` accepts them (docs/conventions/commits.md).
    const CASES: &[(&str, bool)] = &[
        ("feat(cache): add mmap index\n", true),
        (
            "feat(poc-6): add a harness\n\nWhy it exists.\n\nRefs: T-010\n",
            true,
        ),
        ("chore: tidy the gitignore\n", true),
        (
            "build!: bump msrv\n\nBREAKING CHANGE: needs rust 1.100\n",
            true,
        ),
        ("Merge branch 'main' into feature\n", true),
        ("fixup! feat(cache): add mmap index\n", true),
        ("# comment\nfix(shader): handle predicates\n", true),
        ("feat: add a thing without scope\n", false),
        ("feature(cache): unknown type\n", false),
        ("feat(Cache): uppercase scope\n", false),
        ("feat(cache): Add capital\n", false),
        ("feat(cache): add a period.\n", false),
        ("feat(cache): added past tense\n", false),
        ("feat(cache)!: break things\n\nno footer\n", false),
        ("feat(cache): add body without gap\nbody\n", false),
        ("feat(cache):missing space\n", false),
        (
            "feat(cache): add a description that is far too long for the seventy-two limit\n",
            false,
        ),
    ];

    #[test]
    fn rules_match_the_convention() {
        for &(message, ok) in CASES {
            assert_eq!(validate(message).is_ok(), ok, "{message:?}");
        }
    }

    #[test]
    fn a_pull_request_title_is_checked_as_a_header_of_at_most_66_characters() {
        assert!(validate_pr_title("feat(poc-6): checksum payloads with xxh3-64").is_ok());
        assert!(validate_pr_title("Update stuff").is_err());
        let title_66 = format!("docs(backlog): {}", "a".repeat(51));
        assert_eq!(title_66.chars().count(), 66);
        assert!(validate_pr_title(&title_66).is_ok());
        assert!(validate_pr_title(&format!("{title_66}b")).is_err());
    }

    /// The hook and this module must agree on every case, so CI and local commits never
    /// disagree.
    #[test]
    fn agrees_with_the_commit_msg_hook() {
        let root = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/.."));
        let bash = process::bash(root).expect("bash for the hook");
        let dir = std::env::temp_dir().join(format!("lince-xtask-commits-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        for (i, &(message, _)) in CASES.iter().enumerate() {
            let file = dir.join(format!("msg{i}"));
            std::fs::write(&file, message).expect("write message");
            let hook_ok = process::command(
                root,
                &bash,
                &[".githooks/commit-msg", &file.to_string_lossy()],
            )
            .stderr(std::process::Stdio::null())
            .status()
            .expect("run the hook")
            .success();
            assert_eq!(validate(message).is_ok(), hook_ok, "{message:?}");
        }
        std::fs::remove_dir_all(&dir).expect("remove temp dir");
    }
}
