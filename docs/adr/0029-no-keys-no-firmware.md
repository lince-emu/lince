# ADR-0029: No console keys and no firmware, including user-provided ones

- **Status:** Accepted
- **Date:** 2026-11-13
- **Deciders:** Mateus Neiva (maintainer)
- **Related:** charter §2.4; doc 09; `docs/conventions/legal-rules.md`; ADR-0024

## Context
The charter already forbids depending on proprietary keys or firmware. Other Switch emulators
ask users to supply their own key files and firmware dumps, which they then read to decrypt
content and to run or extract system data. The previous wording of the Lince-NX documents
forbade shipping, downloading and extracting keys or firmware, but did not explicitly forbid
**reading files the user supplies**.

## Requirements
- No use of console keys or firmware in any form, by any part of the project.
- System functionality provided entirely by original HLE.

## Alternatives
1. Accept user-supplied keys and firmware (common practice elsewhere).
2. Accept user-supplied firmware only for system data (fonts, applets).
3. **Never read, accept, request or process keys or firmware, from any source.**

## Decision
Option 3, decided by the maintainer. Lince-NX:
- has no code path, setting, import dialog, file picker, command-line flag or documentation
  for keys or firmware;
- never searches the user's disk for such files;
- never asks users to provide them, and does not explain how to obtain them;
- provides all system services and system data through original implementations and freely
  licensed substitutes (ADR-0024, doc 09 §3);
- loads only content that is already decrypted (doc 09 §2), and fails with a neutral message
  otherwise.

## Technical justification
Removing keys and firmware from the architecture entirely eliminates the legal exposure
associated with them and forces a clean HLE design with no hidden firmware dependency.

## Advantages
Clear legal position; no firmware-version coupling; simpler setup for users.

## Disadvantages
Some system data (fonts, applets) must be re-created or substituted; games relying on
system applets need original HLE implementations; fewer users can run content out of the box.

## Risks
Pressure from users to add "optional" key or firmware support. This ADR is the answer: such
requests are declined, not deferred.

## Validation plan
- Review rule LEGAL-05/LEGAL-06 on every PR.
- `.githooks/pre-commit` blocks key, firmware and game-content files and known key-handling
  identifiers from being committed.
- `.claude` hooks deny agents access to key files.

## Conditions for revisiting
None. Changing this decision requires a new ADR from the maintainer and legal review.
