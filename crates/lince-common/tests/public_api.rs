// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The crate's re-exports work together through the public API only (QUAL-73).

use std::time::Duration;

use lince_common::{CommonError, GpuVa, GuestVa, Nanos};

#[test]
fn a_mapped_range_end_is_computed_through_named_methods_when_it_fits() {
    let base = GuestVa::new(0x8000_0000);

    let end = base.checked_add(0x4000);

    assert_eq!(end.map(GuestVa::get), Some(0x8000_4000));
    assert_eq!(end.and_then(|e| e.checked_offset_from(base)), Some(0x4000));
}

#[test]
fn addresses_keep_their_raw_value_when_round_tripped() {
    let raw = 0x0000_0042_0000_1000;

    let gpu = GpuVa::new(raw);

    assert_eq!(gpu.get(), raw);
}

#[test]
fn errors_print_the_failing_value_when_displayed() {
    let too_long = Nanos::try_from(Duration::from_secs(u64::MAX));

    let message = too_long.map_err(|e: CommonError| e.to_string());

    assert_eq!(
        message,
        Err(format!(
            "duration of {} s does not fit in 64-bit nanoseconds",
            u64::MAX
        ))
    );
}
