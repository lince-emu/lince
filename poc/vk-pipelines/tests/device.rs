// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! End-to-end check on the host's Vulkan device. Skips (passes without checking) on hosts with
//! no Vulkan loader or no Vulkan 1.3 device, so CI without a GPU stays green; on a GPU host it
//! proves every available mechanism accepts the generated SPIR-V.
#![cfg(test)]

use std::collections::BTreeSet;

use poc_vk_pipelines::{Context, HarnessError, Mechanism, Validation, Worker};

#[test]
fn one_sample_creates_every_available_mechanism() {
    let ctx = match Context::new(Validation::Enabled) {
        Ok(ctx) => ctx,
        Err(HarnessError::Loader(_) | HarnessError::NoSuitableDevice) => return,
        Err(e) => panic!("Vulkan setup failed: {e}"),
    };
    let mut worker = Worker::new(&ctx, 0x00C0_FFEE).unwrap();
    let timings = worker.sample(64, 0x00C0_FFEE).unwrap();
    let seen: BTreeSet<Mechanism> = timings.iter().map(|t| t.mechanism).collect();
    let mut expected = BTreeSet::from([
        Mechanism::Monolithic,
        Mechanism::MonolithicNoOpt,
        Mechanism::CacheCold,
        Mechanism::CacheWarm,
    ]);
    if ctx.caps().graphics_pipeline_library {
        expected.extend([
            Mechanism::GplLibraries,
            Mechanism::GplFastLink,
            Mechanism::GplLibrariesRetain,
            Mechanism::GplLtoLink,
        ]);
    }
    if ctx.caps().shader_object {
        expected.insert(Mechanism::ShaderObject);
    }
    assert_eq!(seen, expected);
    assert_eq!(timings.len(), expected.len(), "each mechanism timed once");
    drop(worker);
    assert_eq!(
        ctx.validation_messages(),
        0,
        "validation layer reported problems"
    );
}
