---
name: open-pr
description: Pushes the current lince/* branch and opens a GitHub pull request whose title is a Conventional Commit header and whose body follows docs/conventions/branches-and-prs.md. Use after a task passes verification.
argument-hint: "[--draft]"
allowed-tools: Bash(git push -u origin lince/*) Bash(git push origin lince/*) Bash(gh pr create *) Bash(gh pr view *) Bash(git log *) Bash(git diff *) Bash(git rev-parse *) Bash(bash .githooks/commit-msg *)
---

# Open a pull request

1. Confirm the branch starts with `lince/`. If not, stop: never push other branches.
2. Confirm there are commits ahead of `origin/main` and the working tree is clean.
3. Check every commit on the branch against `docs/conventions/commits.md`
   (`git log --format=%B origin/main..HEAD`). Fix messages before pushing if needed (ask the
   maintainer before rewriting history that was already pushed).
4. **Title:** one Conventional Commit header summarizing the PR, e.g.
   `feat(cache): add memory-mapped open-addressing index` (≤ 72 characters, no task id).
   Validate it: write it to a temp file and run `bash .githooks/commit-msg <file>`.
5. `git push -u origin <branch>`.
6. `gh pr create --base main --title "<header>" --body-file <tmp>`; add `--draft` if
   `$ARGUMENTS` contains `--draft`, the task is `Needs decision`, or a PoC has no measurements.
7. **Body:** the template in `docs/conventions/branches-and-prs.md` §3, ending with the footers
   the squash commit should carry:

   ```text
   Refs: T-xxx
   ADR: NNNN
   ```

   plus `BREAKING CHANGE: …` when applicable.
8. Print the PR URL. Never merge, close, tag or request reviewers unless asked.
