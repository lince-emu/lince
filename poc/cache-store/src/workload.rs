// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Seeded synthetic workload: keys, blob sizes and payloads that can be verified on read.
//!
//! A key's low 32 bits hold its payload length, and the payload bytes are a function of the
//! key, so a reader can check any returned blob without keeping a copy (a truncated or
//! substituted blob never verifies).

/// `SplitMix64` pseudo-random generator (Steele, Lea and Flood, "Fast splittable
/// pseudorandom number generators", OOPSLA 2014): tiny, seedable, good enough for workloads.
#[derive(Clone, Debug)]
pub struct SplitMix64(u64);

impl SplitMix64 {
    /// Creates a generator from a seed.
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    /// Next 64 random bits.
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform value in `[0, bound)`; `bound` must be non-zero.
    pub fn below(&mut self, bound: u64) -> u64 {
        self.next_u64() % bound
    }
}

/// Blob-size distribution.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SizeDist {
    /// Each power-of-two octave between `min` and `max` is equally likely; uniform inside an
    /// octave. Many small blobs and a long tail of large ones.
    LogUniform {
        /// Smallest blob, bytes.
        min: u32,
        /// Largest blob, bytes.
        max: u32,
    },
    /// Uniform between `min` and `max`.
    Uniform {
        /// Smallest blob, bytes.
        min: u32,
        /// Largest blob, bytes.
        max: u32,
    },
}

impl SizeDist {
    /// Default PoC-6 distribution: 1 KiB to 2 MiB, log-uniform.
    pub const DEFAULT: Self = Self::LogUniform {
        min: 1 << 10,
        max: 2 << 20,
    };

    /// Parses `loguniform:MIN:MAX` or `uniform:MIN:MAX` (bytes).
    pub fn parse(s: &str) -> Option<Self> {
        let mut parts = s.split(':');
        let kind = parts.next()?;
        let min: u32 = parts.next()?.parse().ok()?;
        let max: u32 = parts.next()?.parse().ok()?;
        if min == 0 || min > max || parts.next().is_some() {
            return None;
        }
        match kind {
            "loguniform" => Some(Self::LogUniform { min, max }),
            "uniform" => Some(Self::Uniform { min, max }),
            _ => None,
        }
    }

    fn sample(self, rng: &mut SplitMix64) -> u32 {
        match self {
            Self::Uniform { min, max } => min + rng_u32(rng, max - min + 1),
            Self::LogUniform { min, max } => {
                let lo = min.ilog2();
                let hi = max.ilog2();
                let octave = lo + rng_u32(rng, hi - lo + 1);
                let base = 1u32 << octave;
                (base + rng_u32(rng, base)).clamp(min, max)
            }
        }
    }
}

fn rng_u32(rng: &mut SplitMix64, bound: u32) -> u32 {
    u32::try_from(rng.below(u64::from(bound))).unwrap_or(0)
}

/// Generates the same sequence of `(key, length)` items for the same seed and distribution.
#[derive(Clone, Debug)]
pub struct Workload {
    rng: SplitMix64,
    dist: SizeDist,
}

impl Workload {
    /// Creates a workload generator.
    pub fn new(seed: u64, dist: SizeDist) -> Self {
        Self {
            rng: SplitMix64::new(seed),
            dist,
        }
    }

    /// Next item key; its payload length is [`payload_len`]`(key)`.
    pub fn next_key(&mut self) -> u128 {
        let len = self.dist.sample(&mut self.rng);
        let hi = u128::from(self.rng.next_u64()) << 64;
        let mid = u128::from(self.rng.next_u64() >> 32) << 32;
        hi | mid | u128::from(len)
    }
}

/// Payload length encoded in a workload key.
pub fn payload_len(key: u128) -> usize {
    let [a, b, c, d, ..] = key.to_le_bytes();
    usize::try_from(u32::from_le_bytes([a, b, c, d])).unwrap_or(0)
}

/// The deterministic payload of a workload key.
pub fn payload(key: u128) -> Vec<u8> {
    let len = payload_len(key);
    let folded = key
        .to_le_bytes()
        .as_chunks::<8>()
        .0
        .iter()
        .fold(0u64, |acc, c| acc ^ u64::from_le_bytes(*c));
    let mut rng = SplitMix64::new(folded);
    let mut out = Vec::with_capacity(len + 8);
    while out.len() < len {
        out.extend_from_slice(&rng.next_u64().to_le_bytes());
    }
    out.truncate(len);
    out
}

/// True if `blob` is exactly the payload of `key`.
pub fn verify(key: u128, blob: &[u8]) -> bool {
    blob.len() == payload_len(key) && blob == payload(key).as_slice()
}

#[cfg(test)]
mod tests {
    use super::{SizeDist, Workload, payload, payload_len, verify};

    #[test]
    fn same_seed_same_sequence() {
        let mut a = Workload::new(7, SizeDist::DEFAULT);
        let mut b = Workload::new(7, SizeDist::DEFAULT);
        for _ in 0..100 {
            assert_eq!(a.next_key(), b.next_key());
        }
    }

    #[test]
    fn sizes_stay_in_range() {
        for dist in [SizeDist::DEFAULT, SizeDist::Uniform { min: 100, max: 200 }] {
            let (SizeDist::LogUniform { min, max } | SizeDist::Uniform { min, max }) = dist;
            let mut w = Workload::new(1, dist);
            for _ in 0..2000 {
                let len = payload_len(w.next_key());
                assert!((min as usize..=max as usize).contains(&len));
            }
        }
    }

    #[test]
    fn payload_verifies_only_for_its_key() {
        let mut w = Workload::new(3, SizeDist::Uniform { min: 64, max: 64 });
        let k1 = w.next_key();
        let k2 = w.next_key();
        let p = payload(k1);
        assert!(verify(k1, &p));
        assert!(!verify(k2, &p));
        assert!(!verify(k1, &p[..63]));
    }

    #[test]
    fn parses_distributions() {
        assert_eq!(
            SizeDist::parse("uniform:10:20"),
            Some(SizeDist::Uniform { min: 10, max: 20 })
        );
        assert_eq!(
            SizeDist::parse("loguniform:1024:2097152"),
            Some(SizeDist::DEFAULT)
        );
        assert_eq!(SizeDist::parse("uniform:20:10"), None);
        assert_eq!(SizeDist::parse("zipf:1:2"), None);
    }
}
