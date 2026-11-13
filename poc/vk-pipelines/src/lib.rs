// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! PoC-5: latency of Vulkan pipeline-creation mechanisms (docs/06-poc-roadmap.md, ADR-0007,
//! ADR-0013).
//!
//! - `spirv` generates distinct vertex/fragment programs of a chosen size.
//! - [`Context`] sets up a Vulkan 1.3 device and detects optional capabilities.
//! - [`Worker`] times each available [`Mechanism`] for one fresh program.
//!
//! `VK_KHR_pipeline_binary` is not measured: `ash` 0.38 (the latest release) predates it.

mod device;
mod error;
mod mechanisms;
mod spirv;

pub use device::{Caps, Context, Validation};
pub use error::HarnessError;
pub use mechanisms::{Mechanism, Timing, Worker};
