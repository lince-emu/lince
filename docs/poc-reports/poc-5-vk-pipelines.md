# PoC-5 — Non-blocking pipeline creation

- **Status:** The harness is done. Measured on **one device** (below). Other vendors and
  drivers are follow-ups; the pipeline-binary mechanism is deferred to T-022.
- **Feeds:** ADR-0007 (host graphics API), ADR-0013 (pipeline creation).
- **Code:** [`poc/vk-pipelines/`](../../poc/vk-pipelines/README.md).

## Hardware and drivers

| Item | Value |
|------|-------|
| GPU | AMD Radeon RX 9070 XT |
| Driver | AMD proprietary driver 26.9.2 (LLPC), as the harness reports it; Vulkan 1.4.355 per `vulkaninfo` |
| CPU | AMD Ryzen 5 7600 (6 cores, 12 threads) |
| OS | Microsoft Windows 11 Pro |
| Extensions used | `VK_EXT_graphics_pipeline_library` (fast linking reported), `VK_EXT_shader_object` |
| Validation layer | Not installed, so the run is without validation |
| Implicit layers present | AMD switchable graphics, third-party capture and overlay layers, and Steam Fossilize (see "Threats to validity") |
| Date | 2026-11-13 |

## Method

- **Command:** `harness --ops 16,1024,16384 --samples 10 --workers 1,2,4,8`. This produced
  4,050 rows in `results/pipeline_latency.csv`, which is not committed. Run nonce:
  `1794571502479618200`.
- **Programs:**
  - The vertex shader writes a position whose `x` is a constant derived from the run nonce.
    Each worker shares it across all its samples, as an emulator would share it.
  - The fragment shader starts from `FragCoord.x` and `FragCoord.y` and applies `ops` steps of
    the non-affine recurrence `x[n+1] = x[n] * x[n-1] + k`, so a compiler cannot fold the
    chain into one operation.
  - Every timed creation uses a new fragment program whose constants come from the run nonce,
    so neither the driver's in-memory cache nor its on-disk cache has seen it.
- **Timing:** wall clock around each Vulkan creation call.
  - "GPL shader libraries" is the sum of the pre-rasterization and fragment-shader library
    calls.
  - The vertex-input and fragment-output libraries are created once per worker and not timed.
- **Warm-up:** each worker runs one untimed sample first.
- **Workers:** each worker thread has its own `VkPipelineCache`.
- **Not exercised:** no GPU work is submitted. Rendering identity between mechanisms (a
  roadmap correctness item) is therefore not checked.
- **Sample size:** with 10 samples per worker and size, P99 at 1 worker is the maximum of 10
  values. Treat it as indicative only.

## Results

All times in milliseconds.

### One worker (P50, n = 10)

| Mechanism | 16 ops | 1,024 ops | 16,384 ops |
|-----------|--------|-----------|------------|
| Monolithic | 3.70 | 31.16 | 525.82 |
| Monolithic, `DISABLE_OPTIMIZATION` | 3.71 | 30.27 | 528.97 |
| GPL shader libraries | 3.74 | 30.99 | 557.45 |
| GPL fast link | **0.022** | **0.028** | **0.030** |
| GPL shader libraries (retain) | 3.77 | 31.40 | 549.67 |
| GPL LTO link | 3.76 | 29.99 | 534.02 |
| Shader objects | 3.64 | 34.02 | 615.49 |
| `VkPipelineCache`, cold | 3.39 | 30.05 | 539.34 |
| `VkPipelineCache`, warm | **0.020** | **0.025** | **0.046** |

### Eight workers in parallel (P50 / P99, n = 80)

| Mechanism | 16 ops | 1,024 ops | 16,384 ops |
|-----------|--------|-----------|------------|
| Monolithic | 4.84 / 6.56 | 43.26 / 53.98 | 880.35 / 1,276.33 |
| Monolithic, `DISABLE_OPTIMIZATION` | 4.91 / 8.56 | 44.55 / 53.28 | 867.98 / 1,172.41 |
| GPL shader libraries | 5.07 / 6.58 | 45.88 / 51.49 | 905.79 / 1,271.15 |
| GPL fast link | 0.022 / 0.060 | 0.030 / 0.064 | 0.042 / 0.108 |
| GPL shader libraries (retain) | 5.05 / 10.48 | 45.70 / 51.87 | 905.91 / 1,247.19 |
| GPL LTO link | 4.84 / 7.77 | 44.13 / 55.31 | 857.48 / 1,179.42 |
| Shader objects | 4.99 / 7.09 | 49.27 / 58.24 | 970.97 / 1,562.00 |
| `VkPipelineCache`, cold | 4.78 / 6.29 | 43.82 / 58.47 | 864.83 / 1,325.97 |
| `VkPipelineCache`, warm | 0.025 / 0.085 | 0.027 / 0.089 | 0.069 / 0.552 |

The CSV and the harness's stdout also contain the 2- and 4-worker results.

## Comparison with the hypothesis

| Hypothesis | Result on this device |
|------------|-----------------------|
| Fast-link creation ≤ 1 ms | **Met.** 0.016–0.042 ms at P50 at every size and worker count; the highest P99 in the run was 0.33 ms |
| Shader-object creation ≤ 1 ms | **Not met.** 3.64–615.49 ms at 1 worker, about as slow as a monolithic pipeline (0.99–1.17× its P50). The timing suggests the driver compiles on each call (inferred, not observed) |
| LTO and full compiles are orders of magnitude slower | **Met.** An LTO link costs as much as a monolithic compile (3.76–534.02 ms vs 0.02–0.03 ms for a fast link) |
| Pipeline-binary loads are sub-millisecond | **Not measured:** deferred to T-022, because `ash` 0.38 has no `VK_KHR_pipeline_binary`. The warm `VkPipelineCache` row is an in-process hit on the same cache object, never serialized and reloaded, so it is not evidence for this hypothesis |

Other observations, all measured:

- **The compile moves; it does not disappear.** Creating the GPL shader libraries costs about
  as much as a monolithic pipeline. The fast-link path only pays off if the libraries are
  built ahead of time, for example at discovery time (doc 03 §5), or reused.
- **`DISABLE_OPTIMIZATION` made no measurable difference** on this driver: within ±3 % of
  monolithic at 1 worker, at every size.
- **Parallel scaling:** going from 1 to 8 workers raised the per-call P50 of the compiling
  mechanisms by 1.29–1.41× at 16 ops, 1.39–1.48× at 1,024 ops and 1.58–1.67× at 16,384 ops.
  Throughput was not measured: the harness records per-call latency only. P99 tails reached
  1.56 s for the largest programs.

### Note on the program generator

An earlier run of this harness (nonce `1794558088363149700`, same command) used an affine
chain (`x * k + k'`) and measured 54 ms for a monolithic 16,384-op pipeline, about 10× less
than this run (526 ms). One possible cause is that the driver folded the affine chain, which is
why the generator now uses a non-affine recurrence; this is an inference, and the change of
dependency shape and operand count may also explain the gap. The earlier numbers are not
reported here.

## Verdict

**Partial success. The criterion is met on one vendor.** GPL fast linking is a sub-millisecond
path on AMD's proprietary driver. Shader objects are not a fast path on this driver. The
roadmap criterion asks for at least one fast mechanism per tier-1 vendor, so NVIDIA, Intel
and Mesa drivers still need runs.

The roadmap's correctness criteria are **not verified**: no validation layer was installed
(the integration test's zero-message check passes trivially without it), and rendering
identity between mechanisms was not checked because no GPU work is submitted.

## Threats to validity

- Only one GPU, one driver and one run.
- With 10 samples per worker, P99 at 1 worker is the maximum.
- Implicit layers were loaded. In particular, Steam's Fossilize pipeline-caching layer and the
  overlay hooks were installed and may have been active. The run was not repeated with those
  layers disabled.
- No validation layer was installed. The generated SPIR-V was accepted by the driver for
  every mechanism, but it was not validated. The harness's `--validation` flag counts
  validation errors and warnings when the layer is present.
- A Vulkan creation call runs on the calling thread. The 8 workers fit in the CPU's 12
  hardware threads but share its 6 cores, so the 8-worker numbers include that contention.

## ADR changes proposed

None applied. To consider when ADR-0013 is decided:

1. Use GPL libraries with fast link as the primary non-blocking path where fast linking is
   reported. Build the shader libraries at discovery time, off the render path, as already
   planned.
2. Do not treat shader objects as a fast path. On this driver they cost as much as monolithic
   pipelines.
3. Re-run on the other tier-1 vendors, with the validation layer and with implicit layers
   disabled, and check rendering identity. Pipeline binaries are measured in T-022.

These results are specific to the Vulkan backend. ADR-0007 now plans Direct3D 12 and Metal
backends later; their pipeline-creation mechanisms need their own measurements.
