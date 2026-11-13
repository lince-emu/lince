// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The crate's error type (PAT-01).

use std::fmt;

use ash::vk;

/// Errors from setting up Vulkan or creating pipeline objects.
#[derive(Debug)]
pub enum HarnessError {
    /// The Vulkan loader could not be loaded.
    Loader(ash::LoadingError),
    /// A Vulkan call failed.
    Vk {
        /// What the harness was doing.
        context: &'static str,
        /// The result code.
        result: vk::Result,
    },
    /// No physical device supports Vulkan 1.3 with dynamic rendering.
    NoSuitableDevice,
}

impl HarnessError {
    /// Wraps a Vulkan result with what the harness was doing.
    pub(crate) fn vk(context: &'static str) -> impl FnOnce(vk::Result) -> Self {
        move |result| Self::Vk { context, result }
    }
}

impl fmt::Display for HarnessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Loader(e) => write!(f, "cannot load the Vulkan loader: {e}"),
            Self::Vk { context, result } => write!(f, "{context}: {result}"),
            Self::NoSuitableDevice => write!(f, "no Vulkan 1.3 device with dynamic rendering"),
        }
    }
}

impl std::error::Error for HarnessError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Loader(e) => Some(e),
            Self::Vk { .. } | Self::NoSuitableDevice => None,
        }
    }
}
