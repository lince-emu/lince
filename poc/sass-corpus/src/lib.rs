// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! T-014: a team-written Ampere SASS corpus (decisions D-005, D-010, D-011).
//!
//! - [`parse`] reads the text form of a program in `programs/`.
//! - [`isa`] encodes each instruction into its 128-bit SM 8.x form, from the bit positions in
//!   `docs/research/ampere-sass-encoding.md`.
//! - [`exec`] runs the program on the CPU to compute its expected output.
//!
//! No NVIDIA SDK tool is involved at any step (LEGAL-15).

pub mod corpus;
pub mod exec;
pub mod isa;
pub mod parse;
