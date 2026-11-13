// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The pipeline-creation mechanisms PoC-5 compares, timed one Vulkan call at a time.
//!
//! Every pipeline uses dynamic rendering (one `R8G8B8A8_UNORM` color attachment), dynamic
//! viewport and scissor, no vertex input, and the shared empty layout. Each mechanism in a
//! sample gets its own fragment shader seed, so no mechanism benefits from another's driver
//! cache entry.
//!
//! Threads: one `Worker` per harness thread. Harness workers stand in for Lince's compile
//! workers, the only threads allowed to create pipelines (AGENTS.md invariant 2).
//! INVARIANT-ALLOW: compile-worker
//!
//! Errors abort the run. Objects created earlier in a failed sample are not destroyed, so a
//! failure poisons the [`Context`], which then leaves the device to the OS instead of
//! destroying it with live children (acceptable for a measurement tool).
#![allow(unsafe_code, reason = "Vulkan through ash; see the SAFETY comments")]

use std::time::Instant;

use ash::vk;

use crate::device::Context;
use crate::error::HarnessError;
use crate::spirv;

/// A timed operation. Names are the CSV `mechanism` column.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Mechanism {
    /// `vkCreateGraphicsPipelines`, complete pipeline.
    Monolithic,
    /// Same with `VK_PIPELINE_CREATE_DISABLE_OPTIMIZATION_BIT`.
    MonolithicNoOpt,
    /// Pre-rasterization + fragment-shader libraries (vertex-input and fragment-output
    /// libraries are created once per worker).
    GplLibraries,
    /// Fast link of the four libraries.
    GplFastLink,
    /// Shader libraries with `RETAIN_LINK_TIME_OPTIMIZATION_INFO`.
    GplLibrariesRetain,
    /// Link with `LINK_TIME_OPTIMIZATION`.
    GplLtoLink,
    /// `vkCreateShadersEXT`, linked vertex + fragment.
    ShaderObject,
    /// Monolithic with the worker's own (sharded) `VkPipelineCache`, first creation.
    CacheCold,
    /// Same pipeline again with the now-populated cache.
    CacheWarm,
}

impl Mechanism {
    /// CSV name.
    pub fn name(self) -> &'static str {
        match self {
            Self::Monolithic => "monolithic",
            Self::MonolithicNoOpt => "monolithic_disable_optimization",
            Self::GplLibraries => "gpl_shader_libraries",
            Self::GplFastLink => "gpl_fast_link",
            Self::GplLibrariesRetain => "gpl_shader_libraries_retain",
            Self::GplLtoLink => "gpl_lto_link",
            Self::ShaderObject => "shader_object",
            Self::CacheCold => "pipeline_cache_cold",
            Self::CacheWarm => "pipeline_cache_warm",
        }
    }
}

/// One timed operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Timing {
    /// What was timed.
    pub mechanism: Mechanism,
    /// Wall-clock nanoseconds of the Vulkan call(s).
    pub ns: u64,
}

const COLOR_FORMATS: [vk::Format; 1] = [vk::Format::R8G8B8A8_UNORM];
const DYNAMIC_STATES: [vk::DynamicState; 2] =
    [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];

/// Fragment-shader seeds reserved per sample; each mechanism uses its own slot.
const SHADER_SLOTS_PER_SAMPLE: u64 = 16;

/// The fragment-shader seed of `slot` within the sample derived from `seed`.
fn fragment_seed(seed: u64, slot: u64) -> u64 {
    seed.wrapping_mul(SHADER_SLOTS_PER_SAMPLE)
        .wrapping_add(slot)
}

fn elapsed_ns(start: Instant) -> u64 {
    u64::try_from(start.elapsed().as_nanos()).unwrap_or(u64::MAX)
}

fn blend_attachments() -> [vk::PipelineColorBlendAttachmentState; 1] {
    [vk::PipelineColorBlendAttachmentState::default()
        .color_write_mask(vk::ColorComponentFlags::RGBA)]
}

fn raster_state() -> vk::PipelineRasterizationStateCreateInfo<'static> {
    vk::PipelineRasterizationStateCreateInfo::default()
        .polygon_mode(vk::PolygonMode::FILL)
        .cull_mode(vk::CullModeFlags::NONE)
        .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
        .line_width(1.0)
}

fn multisample_state() -> vk::PipelineMultisampleStateCreateInfo<'static> {
    vk::PipelineMultisampleStateCreateInfo::default()
        .rasterization_samples(vk::SampleCountFlags::TYPE_1)
}

fn input_assembly() -> vk::PipelineInputAssemblyStateCreateInfo<'static> {
    vk::PipelineInputAssemblyStateCreateInfo::default()
        .topology(vk::PrimitiveTopology::TRIANGLE_LIST)
}

fn stage(
    stage: vk::ShaderStageFlags,
    module: vk::ShaderModule,
) -> vk::PipelineShaderStageCreateInfo<'static> {
    vk::PipelineShaderStageCreateInfo::default()
        .stage(stage)
        .module(module)
        .name(c"main")
}

/// Creates one graphics pipeline (complete or library) and times the call.
fn create_timed(
    ctx: &Context,
    cache: vk::PipelineCache,
    info: &vk::GraphicsPipelineCreateInfo<'_>,
) -> Result<(vk::Pipeline, u64), HarnessError> {
    let start = Instant::now();
    // SAFETY: `info` and everything it points to live until the call returns; `cache` is either
    // null or owned by the calling worker, which satisfies its external-synchronization rule.
    let created = unsafe {
        ctx.device
            .create_graphics_pipelines(cache, std::slice::from_ref(info), None)
    };
    let ns = elapsed_ns(start);
    let pipelines = created.map_err(|(_, result)| HarnessError::Vk {
        context: "create graphics pipeline",
        result,
    })?;
    Ok((pipelines[0], ns))
}

/// Complete pipeline from a vertex and a fragment module.
fn monolithic(
    ctx: &Context,
    cache: vk::PipelineCache,
    modules: [vk::ShaderModule; 2],
    flags: vk::PipelineCreateFlags,
) -> Result<(vk::Pipeline, u64), HarnessError> {
    let stages = [
        stage(vk::ShaderStageFlags::VERTEX, modules[0]),
        stage(vk::ShaderStageFlags::FRAGMENT, modules[1]),
    ];
    let vertex_input = vk::PipelineVertexInputStateCreateInfo::default();
    let input_assembly = input_assembly();
    let viewport = vk::PipelineViewportStateCreateInfo::default()
        .viewport_count(1)
        .scissor_count(1);
    let raster = raster_state();
    let multisample = multisample_state();
    let depth = vk::PipelineDepthStencilStateCreateInfo::default();
    let attachments = blend_attachments();
    let blend = vk::PipelineColorBlendStateCreateInfo::default().attachments(&attachments);
    let dynamic = vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&DYNAMIC_STATES);
    let mut rendering =
        vk::PipelineRenderingCreateInfo::default().color_attachment_formats(&COLOR_FORMATS);
    let info = vk::GraphicsPipelineCreateInfo::default()
        .flags(flags)
        .stages(&stages)
        .vertex_input_state(&vertex_input)
        .input_assembly_state(&input_assembly)
        .viewport_state(&viewport)
        .rasterization_state(&raster)
        .multisample_state(&multisample)
        .depth_stencil_state(&depth)
        .color_blend_state(&blend)
        .dynamic_state(&dynamic)
        .layout(ctx.layout)
        .push_next(&mut rendering);
    create_timed(ctx, cache, &info)
}

/// Which graphics-pipeline-library part to create.
#[derive(Clone, Copy)]
enum Part {
    VertexInput,
    PreRaster(vk::ShaderModule),
    Fragment(vk::ShaderModule),
    FragmentOutput,
}

/// One graphics pipeline library.
fn library(ctx: &Context, part: Part, retain: bool) -> Result<(vk::Pipeline, u64), HarnessError> {
    let mut flags = vk::PipelineCreateFlags::LIBRARY_KHR;
    if retain {
        flags |= vk::PipelineCreateFlags::RETAIN_LINK_TIME_OPTIMIZATION_INFO_EXT;
    }
    let vertex_input = vk::PipelineVertexInputStateCreateInfo::default();
    let input_assembly = input_assembly();
    let viewport = vk::PipelineViewportStateCreateInfo::default()
        .viewport_count(1)
        .scissor_count(1);
    let raster = raster_state();
    let multisample = multisample_state();
    let depth = vk::PipelineDepthStencilStateCreateInfo::default();
    let attachments = blend_attachments();
    let blend = vk::PipelineColorBlendStateCreateInfo::default().attachments(&attachments);
    let dynamic = vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&DYNAMIC_STATES);
    let mut rendering =
        vk::PipelineRenderingCreateInfo::default().color_attachment_formats(&COLOR_FORMATS);
    let stages: [vk::PipelineShaderStageCreateInfo<'_>; 1];
    let (kind, info) = match part {
        Part::VertexInput => (
            vk::GraphicsPipelineLibraryFlagsEXT::VERTEX_INPUT_INTERFACE,
            vk::GraphicsPipelineCreateInfo::default()
                .vertex_input_state(&vertex_input)
                .input_assembly_state(&input_assembly),
        ),
        Part::PreRaster(module) => {
            stages = [stage(vk::ShaderStageFlags::VERTEX, module)];
            let info = vk::GraphicsPipelineCreateInfo::default()
                .stages(&stages)
                .viewport_state(&viewport)
                .rasterization_state(&raster)
                .dynamic_state(&dynamic)
                .layout(ctx.layout);
            (
                vk::GraphicsPipelineLibraryFlagsEXT::PRE_RASTERIZATION_SHADERS,
                info,
            )
        }
        Part::Fragment(module) => {
            stages = [stage(vk::ShaderStageFlags::FRAGMENT, module)];
            let info = vk::GraphicsPipelineCreateInfo::default()
                .stages(&stages)
                .multisample_state(&multisample)
                .depth_stencil_state(&depth)
                .layout(ctx.layout);
            (vk::GraphicsPipelineLibraryFlagsEXT::FRAGMENT_SHADER, info)
        }
        Part::FragmentOutput => (
            vk::GraphicsPipelineLibraryFlagsEXT::FRAGMENT_OUTPUT_INTERFACE,
            vk::GraphicsPipelineCreateInfo::default()
                .color_blend_state(&blend)
                .multisample_state(&multisample),
        ),
    };
    let mut library_info = vk::GraphicsPipelineLibraryCreateInfoEXT::default().flags(kind);
    let info = info
        .flags(flags)
        .push_next(&mut rendering)
        .push_next(&mut library_info);
    create_timed(ctx, vk::PipelineCache::null(), &info)
}

/// Links four libraries into a complete pipeline.
fn link(
    ctx: &Context,
    libraries: &[vk::Pipeline; 4],
    optimize: bool,
) -> Result<(vk::Pipeline, u64), HarnessError> {
    let flags = if optimize {
        vk::PipelineCreateFlags::LINK_TIME_OPTIMIZATION_EXT
    } else {
        vk::PipelineCreateFlags::empty()
    };
    let mut library_info = vk::PipelineLibraryCreateInfoKHR::default().libraries(libraries);
    let info = vk::GraphicsPipelineCreateInfo::default()
        .flags(flags)
        .layout(ctx.layout)
        .push_next(&mut library_info);
    create_timed(ctx, vk::PipelineCache::null(), &info)
}

/// The vertex-input and fragment-output libraries, created once per worker.
struct Interface {
    vertex_input: vk::Pipeline,
    fragment_output: vk::Pipeline,
}

/// Per-thread state: a private pipeline cache and the interface libraries.
pub struct Worker<'a> {
    ctx: &'a Context,
    cache: vk::PipelineCache,
    /// `[fast-link set, retain-LTO set]`, when graphics pipeline libraries are available.
    interfaces: Option<[Interface; 2]>,
    vertex: vk::ShaderModule,
    vertex_seed: u64,
}

impl<'a> Worker<'a> {
    /// Creates the worker's cache, interface libraries and the vertex shader it shares across
    /// samples; `vertex_seed` makes that shader distinct per run.
    pub fn new(ctx: &'a Context, vertex_seed: u64) -> Result<Self, HarnessError> {
        Self::create(ctx, vertex_seed).inspect_err(|_| ctx.poison())
    }

    fn create(ctx: &'a Context, vertex_seed: u64) -> Result<Self, HarnessError> {
        // SAFETY: valid device; an empty cache create info.
        let cache = unsafe {
            ctx.device
                .create_pipeline_cache(&vk::PipelineCacheCreateInfo::default(), None)
        }
        .map_err(HarnessError::vk("create pipeline cache"))?;
        let vertex = module(ctx, &spirv::vertex_shader(vertex_seed))?;
        let mut worker = Self {
            ctx,
            cache,
            interfaces: None,
            vertex,
            vertex_seed,
        };
        if ctx.caps().graphics_pipeline_library {
            let make = |retain| -> Result<Interface, HarnessError> {
                Ok(Interface {
                    vertex_input: library(ctx, Part::VertexInput, retain)?.0,
                    fragment_output: library(ctx, Part::FragmentOutput, retain)?.0,
                })
            };
            worker.interfaces = Some([make(false)?, make(true)?]);
        }
        Ok(worker)
    }

    fn destroy_pipeline(&self, pipeline: vk::Pipeline) {
        // SAFETY: the pipeline was created by this worker and never used for recording.
        unsafe { self.ctx.device.destroy_pipeline(pipeline, None) };
    }

    fn destroy_module(&self, module: vk::ShaderModule) {
        // SAFETY: modules are only referenced during creation calls that already returned.
        unsafe { self.ctx.device.destroy_shader_module(module, None) };
    }

    /// Times every available mechanism once, each with a fresh fragment shader of `ops`
    /// recurrence steps derived from `seed`.
    pub fn sample(&mut self, ops: u32, seed: u64) -> Result<Vec<Timing>, HarnessError> {
        self.sample_all(ops, seed)
            .inspect_err(|_| self.ctx.poison())
    }

    fn sample_all(&self, ops: u32, seed: u64) -> Result<Vec<Timing>, HarnessError> {
        let mut out = Vec::new();
        let fresh = |i: u64| {
            module(
                self.ctx,
                &spirv::fragment_shader(ops, fragment_seed(seed, i)),
            )
        };
        for (i, mechanism, flags) in [
            (0, Mechanism::Monolithic, vk::PipelineCreateFlags::empty()),
            (
                1,
                Mechanism::MonolithicNoOpt,
                vk::PipelineCreateFlags::DISABLE_OPTIMIZATION,
            ),
        ] {
            let fragment = fresh(i)?;
            let (pipeline, ns) = monolithic(
                self.ctx,
                vk::PipelineCache::null(),
                [self.vertex, fragment],
                flags,
            )?;
            out.push(Timing { mechanism, ns });
            self.destroy_pipeline(pipeline);
            self.destroy_module(fragment);
        }
        if let Some([fast, retain]) = &self.interfaces {
            out.extend(self.gpl(fast, false, fresh(2)?)?);
            out.extend(self.gpl(retain, true, fresh(3)?)?);
        }
        if let Some(loader) = &self.ctx.shader_object {
            out.push(
                self.shader_object(loader, &spirv::fragment_shader(ops, fragment_seed(seed, 4)))?,
            );
        }
        let fragment = fresh(5)?;
        for mechanism in [Mechanism::CacheCold, Mechanism::CacheWarm] {
            let (pipeline, ns) = monolithic(
                self.ctx,
                self.cache,
                [self.vertex, fragment],
                vk::PipelineCreateFlags::empty(),
            )?;
            out.push(Timing { mechanism, ns });
            self.destroy_pipeline(pipeline);
        }
        self.destroy_module(fragment);
        Ok(out)
    }

    /// Shader libraries then link; `lto` selects the retain + link-time-optimization path.
    fn gpl(
        &self,
        set: &Interface,
        lto: bool,
        fragment: vk::ShaderModule,
    ) -> Result<[Timing; 2], HarnessError> {
        let (pre, pre_ns) = library(self.ctx, Part::PreRaster(self.vertex), lto)?;
        let (frag, frag_ns) = library(self.ctx, Part::Fragment(fragment), lto)?;
        let (linked, link_ns) = link(
            self.ctx,
            &[set.vertex_input, pre, frag, set.fragment_output],
            lto,
        )?;
        for p in [linked, pre, frag] {
            self.destroy_pipeline(p);
        }
        self.destroy_module(fragment);
        let (libs, linking) = if lto {
            (Mechanism::GplLibrariesRetain, Mechanism::GplLtoLink)
        } else {
            (Mechanism::GplLibraries, Mechanism::GplFastLink)
        };
        Ok([
            Timing {
                mechanism: libs,
                ns: pre_ns + frag_ns,
            },
            Timing {
                mechanism: linking,
                ns: link_ns,
            },
        ])
    }

    fn shader_object(
        &self,
        loader: &ash::ext::shader_object::Device,
        fragment: &[u32],
    ) -> Result<Timing, HarnessError> {
        let (vs, fs) = (
            to_bytes(&spirv::vertex_shader(self.vertex_seed)),
            to_bytes(fragment),
        );
        let infos = [
            vk::ShaderCreateInfoEXT::default()
                .flags(vk::ShaderCreateFlagsEXT::LINK_STAGE)
                .stage(vk::ShaderStageFlags::VERTEX)
                .next_stage(vk::ShaderStageFlags::FRAGMENT)
                .code_type(vk::ShaderCodeTypeEXT::SPIRV)
                .code(&vs)
                .name(c"main"),
            vk::ShaderCreateInfoEXT::default()
                .flags(vk::ShaderCreateFlagsEXT::LINK_STAGE)
                .stage(vk::ShaderStageFlags::FRAGMENT)
                .code_type(vk::ShaderCodeTypeEXT::SPIRV)
                .code(&fs)
                .name(c"main"),
        ];
        let start = Instant::now();
        // SAFETY: the shader-object feature is enabled; `infos` and their code live until the
        // call returns.
        let created = unsafe { loader.create_shaders(&infos, None) };
        let ns = elapsed_ns(start);
        let shaders = created.map_err(|(_, result)| HarnessError::Vk {
            context: "create shader objects",
            result,
        })?;
        for shader in shaders {
            // SAFETY: created above and never bound.
            unsafe { loader.destroy_shader(shader, None) };
        }
        Ok(Timing {
            mechanism: Mechanism::ShaderObject,
            ns,
        })
    }
}

/// SPIR-V words as bytes, for `VkShaderCreateInfoEXT::pCode`.
fn to_bytes(words: &[u32]) -> Vec<u8> {
    words.iter().flat_map(|w| w.to_le_bytes()).collect()
}

fn module(ctx: &Context, words: &[u32]) -> Result<vk::ShaderModule, HarnessError> {
    let info = vk::ShaderModuleCreateInfo::default().code(words);
    // SAFETY: `words` is a complete SPIR-V module from `spirv` and outlives the call.
    unsafe { ctx.device.create_shader_module(&info, None) }
        .map_err(HarnessError::vk("create shader module"))
}

impl Drop for Worker<'_> {
    fn drop(&mut self) {
        if let Some(sets) = self.interfaces.take() {
            for set in sets {
                self.destroy_pipeline(set.vertex_input);
                self.destroy_pipeline(set.fragment_output);
            }
        }
        self.destroy_module(self.vertex);
        // SAFETY: the cache is owned by this worker and no creation call is in progress.
        unsafe { self.ctx.device.destroy_pipeline_cache(self.cache, None) };
    }
}
