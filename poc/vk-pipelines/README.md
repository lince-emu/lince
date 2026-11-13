# PoC-5 — Vulkan pipeline-creation mechanisms

Standalone harness for [PoC-5](../../docs/06-poc-roadmap.md). It provides input for
ADR-0007 and ADR-0013. Report: [`docs/poc-reports/poc-5-vk-pipelines.md`](../../docs/poc-reports/poc-5-vk-pipelines.md).

- **Objective:** measure each pipeline-creation mechanism per vendor.
- **Hypothesis:**
  - Fast-link and shader-object creation take ≤ 1 ms.
  - LTO and full compiles are orders of magnitude slower.
  - Pipeline-binary loads are sub-millisecond. This one is not measured: `ash` 0.38, the
    latest release, predates `VK_KHR_pipeline_binary`.

## Mechanisms

| CSV name | What is timed |
|----------|---------------|
| `monolithic` | `vkCreateGraphicsPipelines` for a complete pipeline |
| `monolithic_disable_optimization` | The same, with `DISABLE_OPTIMIZATION` |
| `gpl_shader_libraries` | Pre-rasterization + fragment-shader libraries |
| `gpl_fast_link` | Linking the four libraries without link-time optimization |
| `gpl_shader_libraries_retain` | Shader libraries with `RETAIN_LINK_TIME_OPTIMIZATION_INFO` |
| `gpl_lto_link` | Linking with `LINK_TIME_OPTIMIZATION` |
| `shader_object` | `vkCreateShadersEXT`, linked vertex + fragment |
| `pipeline_cache_cold` / `pipeline_cache_warm` | Monolithic creation with each worker's own `VkPipelineCache`: the first creation, then the same pipeline again |

For GPL, the vertex-input and fragment-output interface libraries are created once per
worker, as an emulator would reuse them. Mechanisms the device lacks are skipped.

Every sample uses a new fragment program: a SPIR-V generator written from the public
specification, `ops` steps of the non-affine recurrence `x[n+1] = x[n] * x[n-1] + k` (a
driver cannot fold it into one operation), and constants derived from a per-run nonce. The
vertex shader is shared by all samples of a worker, as an emulator would share it, and is
also derived from the nonce. As a result, no driver cache (in memory or on disk) has seen the
vertex shader before the run, or a fragment shader before its sample.

## Running

```bash
cargo test
cargo build --release
./target/release/harness --ops 16,1024,16384 --samples 20 --workers 1,2,4,8
```

- `--validation` enables `VK_LAYER_KHRONOS_validation` if it is installed.
- `--nonce N` repeats a run with the same programs.

Output is appended to `results/pipeline_latency.csv` (gitignored; the `nonce` column tells
runs apart), with columns `nonce,device,driver,mechanism,ops,workers,worker,sample,ns`, plus
a P50/P99 summary on stdout.
Each worker first runs one untimed warm-up sample.

## Safety

`unsafe` is confined to `src/device.rs` and `src/mechanisms.rs`, the two modules that call
`ash`, as allowed for PoC crates. Every block has a `// SAFETY:` comment. The harness only
creates objects: it never records or submits GPU work, so no fences or waits are needed.
