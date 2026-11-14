// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Fuzzes the NRO header parser: any input must give a header or an error, never a panic.

#![no_main]

use libfuzzer_sys::fuzz_target;
use lince_loader::NroHeader;

fuzz_target!(|data: &[u8]| {
    if let Ok(header) = NroHeader::parse(data) {
        assert!(u64::from(header.size) <= data.len() as u64);
    }
});
