// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `Nanos`: a span or timestamp in nanoseconds (PAT-02, QUAL-20).
//!
//! Frame-ledger events, budgets and guest timers all count nanoseconds; one type keeps them
//! from being mixed with other `u64` counts.

use std::time::Duration;

use crate::error::{CommonError, Result};

/// A number of nanoseconds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct Nanos(u64);

impl Nanos {
    /// Zero nanoseconds.
    pub const ZERO: Self = Self(0);

    /// Wraps a raw count.
    pub const fn new(raw: u64) -> Self {
        Self(raw)
    }

    /// The raw count.
    pub const fn get(self) -> u64 {
        self.0
    }

    /// Saturates at `u64::MAX` nanoseconds.
    #[must_use]
    pub const fn saturating_add(self, other: Self) -> Self {
        Self(self.0.saturating_add(other.0))
    }

    /// Zero when `other` is larger: an elapsed time never goes negative.
    #[must_use]
    pub const fn saturating_sub(self, other: Self) -> Self {
        Self(self.0.saturating_sub(other.0))
    }
}

impl From<Nanos> for Duration {
    fn from(n: Nanos) -> Self {
        Duration::from_nanos(n.0)
    }
}

impl TryFrom<Duration> for Nanos {
    type Error = CommonError;

    /// Fails from about 584 years on, which `u64` nanoseconds cannot hold.
    fn try_from(d: Duration) -> Result<Self> {
        u64::try_from(d.as_nanos())
            .map(Self)
            .map_err(|_| CommonError::DurationTooLong { secs: d.as_secs() })
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::Nanos;
    use crate::error::CommonError;

    #[test]
    fn converts_to_and_from_duration_when_the_value_fits() {
        let d = Duration::new(3, 250);

        let n = Nanos::try_from(d);

        assert_eq!(n, Ok(Nanos::new(3_000_000_250)));
        assert_eq!(Duration::from(Nanos::new(3_000_000_250)), d);
    }

    #[test]
    fn try_from_fails_with_the_seconds_when_the_duration_is_too_long() {
        let d = Duration::from_secs(u64::MAX);

        let n = Nanos::try_from(d);

        assert_eq!(n, Err(CommonError::DurationTooLong { secs: u64::MAX }));
    }

    #[test]
    fn saturating_sub_returns_zero_when_the_subtrahend_is_larger() {
        let early = Nanos::new(10);
        let late = Nanos::new(25);

        let negative = early.saturating_sub(late);
        let positive = late.saturating_sub(early);

        assert_eq!(negative, Nanos::ZERO);
        assert_eq!(positive, Nanos::new(15));
    }

    #[test]
    fn saturating_add_stops_at_the_maximum_when_it_would_overflow() {
        let big = Nanos::new(u64::MAX - 1);

        let sum = big.saturating_add(Nanos::new(5));

        assert_eq!(sum, Nanos::new(u64::MAX));
    }
}
