// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Vulkan instance and device setup with runtime capability detection.
//!
//! Picks the first discrete GPU (else the first device) with Vulkan 1.3 and dynamic rendering,
//! and enables `VK_EXT_graphics_pipeline_library` and `VK_EXT_shader_object` only when the device
//! supports them. Mechanisms that need a missing feature are skipped by the harness.
//!
//! With [`Validation::Enabled`] and `VK_LAYER_KHRONOS_validation` installed, a
//! `VK_EXT_debug_utils` messenger counts validation errors and warnings.
//!
//! Threads: created once on the harness main thread; `&Context` is shared with worker threads.
//! Pipeline creation on one `VkDevice` from several threads is allowed by the Vulkan spec when
//! each thread uses its own pipeline cache.
#![allow(unsafe_code, reason = "Vulkan through ash; see the SAFETY comments")]

use std::ffi::{CStr, c_char, c_void};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use ash::vk;

use crate::error::HarnessError;

const VALIDATION_LAYER: &CStr = c"VK_LAYER_KHRONOS_validation";

/// Debug-utils messenger and the counter its callback increments.
struct Messenger {
    loader: ash::ext::debug_utils::Instance,
    handle: vk::DebugUtilsMessengerEXT,
    /// Validation errors and warnings; boxed so the address given to the layer stays stable.
    count: Box<AtomicU32>,
}

/// Whether to enable the Khronos validation layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Validation {
    /// Enable it if it is installed and count its error and warning messages.
    Enabled,
    /// Do not enable it.
    Disabled,
}

/// What the selected device supports.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Caps {
    /// `VkPhysicalDeviceProperties::deviceName`.
    pub device_name: String,
    /// Driver name and info from `VkPhysicalDeviceDriverProperties`.
    pub driver: String,
    /// `VK_EXT_graphics_pipeline_library` enabled.
    pub graphics_pipeline_library: bool,
    /// The driver reports fast linking for pipeline libraries.
    pub gpl_fast_linking: bool,
    /// `VK_EXT_shader_object` enabled.
    pub shader_object: bool,
}

/// Instance, device and extension loaders. Destroyed in reverse order on drop.
pub struct Context {
    entry: ash::Entry,
    instance: ash::Instance,
    pub(crate) device: ash::Device,
    pub(crate) shader_object: Option<ash::ext::shader_object::Device>,
    /// Empty pipeline layout shared by every pipeline and shader object.
    pub(crate) layout: vk::PipelineLayout,
    caps: Caps,
    messenger: Option<Messenger>,
    /// Set when a creation call failed; objects created before it may still be alive.
    poisoned: AtomicBool,
}

fn c_str_to_string(s: Result<&CStr, std::ffi::FromBytesUntilNulError>) -> String {
    s.map_or_else(|_| "?".to_owned(), |c| c.to_string_lossy().into_owned())
}

struct Selected {
    physical: vk::PhysicalDevice,
    queue_family: u32,
    gpl: bool,
    shader_object: bool,
}

/// Counts validation errors and warnings in the `AtomicU32` passed as user data; the layer
/// prints them as well.
extern "system" fn count_message(
    severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    _types: vk::DebugUtilsMessageTypeFlagsEXT,
    _data: *const vk::DebugUtilsMessengerCallbackDataEXT<'_>,
    user: *mut c_void,
) -> vk::Bool32 {
    let counted = vk::DebugUtilsMessageSeverityFlagsEXT::ERROR
        | vk::DebugUtilsMessageSeverityFlagsEXT::WARNING;
    if severity.intersects(counted) {
        // SAFETY: `user` is the `Messenger::count` box registered with this callback; the box
        // outlives the messenger (it is destroyed first, or leaked together with it).
        let count = unsafe { &*user.cast::<AtomicU32>() };
        // Relaxed: the count is read only after the workers are joined, which orders it.
        count.fetch_add(1, Ordering::Relaxed);
    }
    vk::FALSE
}

fn destroy_messenger(messenger: Option<&Messenger>) {
    if let Some(m) = messenger {
        // SAFETY: the messenger was created from the still-alive instance and nothing uses it.
        unsafe { m.loader.destroy_debug_utils_messenger(m.handle, None) };
    }
}

impl Context {
    /// Loads Vulkan and creates the device.
    pub fn new(validation: Validation) -> Result<Self, HarnessError> {
        // SAFETY: loading the system Vulkan loader runs its initialization code; the loader is
        // the platform's own library and the returned entry keeps it loaded.
        let entry = unsafe { ash::Entry::load() }.map_err(HarnessError::Loader)?;
        // SAFETY: the entry is valid; the call only queries layer properties.
        let layers = unsafe { entry.enumerate_instance_layer_properties() }
            .map_err(HarnessError::vk("enumerate layers"))?;
        let validate = validation == Validation::Enabled
            && layers
                .iter()
                .any(|l| l.layer_name_as_c_str().is_ok_and(|n| n == VALIDATION_LAYER));
        let layer_ptrs: Vec<*const c_char> = if validate {
            vec![VALIDATION_LAYER.as_ptr()]
        } else {
            Vec::new()
        };
        let extension_ptrs: Vec<*const c_char> = if validate {
            vec![ash::ext::debug_utils::NAME.as_ptr()]
        } else {
            Vec::new()
        };
        let app = vk::ApplicationInfo::default()
            .application_name(c"lince-poc5")
            .api_version(vk::API_VERSION_1_3);
        let info = vk::InstanceCreateInfo::default()
            .application_info(&app)
            .enabled_layer_names(&layer_ptrs)
            .enabled_extension_names(&extension_ptrs);
        // SAFETY: `info` and everything it points to live until the call returns; the debug
        // utils extension is requested only together with the validation layer that provides it.
        let instance = unsafe { entry.create_instance(&info, None) }
            .map_err(HarnessError::vk("create instance"))?;
        let result = validate
            .then(|| messenger(&entry, &instance))
            .transpose()
            .and_then(|m| Self::with_instance(entry.clone(), instance.clone(), m));
        if result.is_err() {
            // SAFETY: `with_instance` destroys whatever it created before failing, so nothing
            // created from the instance is alive on this path; `entry` is still alive, so the
            // loader is still loaded.
            unsafe { instance.destroy_instance(None) };
        }
        result
    }

    fn with_instance(
        entry: ash::Entry,
        instance: ash::Instance,
        messenger: Option<Messenger>,
    ) -> Result<Self, HarnessError> {
        let selected = select(&instance).and_then(|s| create_device(&instance, &s).map(|d| (s, d)));
        let (selected, device) = match selected {
            Ok(pair) => pair,
            Err(e) => {
                destroy_messenger(messenger.as_ref());
                return Err(e);
            }
        };
        // SAFETY: the device is valid; the layout has no sets or push constants.
        let created = unsafe {
            device.create_pipeline_layout(&vk::PipelineLayoutCreateInfo::default(), None)
        };
        let layout = match created {
            Ok(layout) => layout,
            Err(result) => {
                // SAFETY: nothing else was created from the device yet.
                unsafe { device.destroy_device(None) };
                destroy_messenger(messenger.as_ref());
                return Err(HarnessError::Vk {
                    context: "create pipeline layout",
                    result,
                });
            }
        };
        let caps = describe(&instance, &selected);
        let shader_object = selected
            .shader_object
            .then(|| ash::ext::shader_object::Device::new(&instance, &device));
        Ok(Self {
            entry,
            instance,
            device,
            shader_object,
            layout,
            caps,
            messenger,
            poisoned: AtomicBool::new(false),
        })
    }

    /// What the device supports.
    pub fn caps(&self) -> &Caps {
        &self.caps
    }

    /// The validation layer is enabled.
    pub fn validation_enabled(&self) -> bool {
        self.messenger.is_some()
    }

    /// Validation errors and warnings counted so far (0 when validation is off).
    pub fn validation_messages(&self) -> u32 {
        // Relaxed: callers read it after joining the workers, which orders the increments.
        self.messenger
            .as_ref()
            .map_or(0, |m| m.count.load(Ordering::Relaxed))
    }

    /// Records that a creation call failed, so `drop` leaves the device alive instead of
    /// destroying it while objects of the failed sample may still exist.
    pub(crate) fn poison(&self) {
        // Relaxed: read in `drop`, after the workers are joined, which orders the store.
        self.poisoned.store(true, Ordering::Relaxed);
    }
}

fn messenger(entry: &ash::Entry, instance: &ash::Instance) -> Result<Messenger, HarnessError> {
    let loader = ash::ext::debug_utils::Instance::new(entry, instance);
    let count = Box::new(AtomicU32::new(0));
    let info = vk::DebugUtilsMessengerCreateInfoEXT::default()
        .message_severity(
            vk::DebugUtilsMessageSeverityFlagsEXT::ERROR
                | vk::DebugUtilsMessageSeverityFlagsEXT::WARNING,
        )
        .message_type(
            vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
                | vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
                | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE,
        )
        .pfn_user_callback(Some(count_message))
        .user_data(std::ptr::from_ref::<AtomicU32>(&count).cast_mut().cast());
    // SAFETY: the instance enabled `VK_EXT_debug_utils`; the callback only increments the
    // boxed counter, whose address stays valid for as long as the messenger exists.
    let handle = unsafe { loader.create_debug_utils_messenger(&info, None) }
        .map_err(HarnessError::vk("create debug messenger"))?;
    Ok(Messenger {
        loader,
        handle,
        count,
    })
}

fn has_extension(props: &[vk::ExtensionProperties], name: &CStr) -> bool {
    props
        .iter()
        .any(|p| p.extension_name_as_c_str().is_ok_and(|n| n == name))
}

/// Optional extension features of one device.
struct Supported {
    dynamic_rendering: bool,
    gpl: bool,
    shader_object: bool,
}

/// Queries features. Extension structures are chained only when the device advertises the
/// extension, as the structure-chain valid-usage rules require.
fn features(
    instance: &ash::Instance,
    physical: vk::PhysicalDevice,
    extensions: &[vk::ExtensionProperties],
) -> Supported {
    let gpl_ext = has_extension(extensions, ash::ext::graphics_pipeline_library::NAME)
        && has_extension(extensions, ash::khr::pipeline_library::NAME);
    let so_ext = has_extension(extensions, ash::ext::shader_object::NAME);
    let mut v13 = vk::PhysicalDeviceVulkan13Features::default();
    let mut gpl = vk::PhysicalDeviceGraphicsPipelineLibraryFeaturesEXT::default();
    let mut so = vk::PhysicalDeviceShaderObjectFeaturesEXT::default();
    let mut query = vk::PhysicalDeviceFeatures2::default().push_next(&mut v13);
    if gpl_ext {
        query = query.push_next(&mut gpl);
    }
    if so_ext {
        query = query.push_next(&mut so);
    }
    // SAFETY: the chain holds the core 1.3 structure plus structures only for extensions the
    // device advertises; the device supports Vulkan 1.3 (checked by the caller).
    unsafe { instance.get_physical_device_features2(physical, &mut query) };
    Supported {
        dynamic_rendering: v13.dynamic_rendering == vk::TRUE,
        gpl: gpl_ext && gpl.graphics_pipeline_library == vk::TRUE,
        shader_object: so_ext && so.shader_object == vk::TRUE,
    }
}

/// Picks a device and checks which optional features it has.
fn select(instance: &ash::Instance) -> Result<Selected, HarnessError> {
    // SAFETY: the instance is valid.
    let mut devices = unsafe { instance.enumerate_physical_devices() }
        .map_err(HarnessError::vk("enumerate devices"))?;
    devices.sort_by_key(|&d| {
        // SAFETY: each handle comes from the enumeration above.
        unsafe { instance.get_physical_device_properties(d) }.device_type
            != vk::PhysicalDeviceType::DISCRETE_GPU
    });
    for physical in devices {
        // SAFETY: valid physical device handle.
        let props = unsafe { instance.get_physical_device_properties(physical) };
        if props.api_version < vk::API_VERSION_1_3 {
            continue;
        }
        // SAFETY: valid physical device handle.
        let extensions = unsafe { instance.enumerate_device_extension_properties(physical) }
            .map_err(HarnessError::vk("enumerate device extensions"))?;
        let supported = features(instance, physical, &extensions);
        if !supported.dynamic_rendering {
            continue;
        }
        // SAFETY: valid physical device handle.
        let families = unsafe { instance.get_physical_device_queue_family_properties(physical) };
        let Some(queue_family) = (0u32..)
            .zip(&families)
            .find(|(_, f)| f.queue_flags.contains(vk::QueueFlags::GRAPHICS))
            .map(|(i, _)| i)
        else {
            continue;
        };
        return Ok(Selected {
            physical,
            queue_family,
            gpl: supported.gpl,
            shader_object: supported.shader_object,
        });
    }
    Err(HarnessError::NoSuitableDevice)
}

fn create_device(instance: &ash::Instance, s: &Selected) -> Result<ash::Device, HarnessError> {
    let mut extensions: Vec<*const c_char> = Vec::new();
    if s.gpl {
        extensions.push(ash::khr::pipeline_library::NAME.as_ptr());
        extensions.push(ash::ext::graphics_pipeline_library::NAME.as_ptr());
    }
    if s.shader_object {
        extensions.push(ash::ext::shader_object::NAME.as_ptr());
    }
    let priorities = [1.0f32];
    let queues = [vk::DeviceQueueCreateInfo::default()
        .queue_family_index(s.queue_family)
        .queue_priorities(&priorities)];
    let mut v13 = vk::PhysicalDeviceVulkan13Features::default().dynamic_rendering(true);
    let mut gpl = vk::PhysicalDeviceGraphicsPipelineLibraryFeaturesEXT::default()
        .graphics_pipeline_library(true);
    let mut so = vk::PhysicalDeviceShaderObjectFeaturesEXT::default().shader_object(true);
    let mut info = vk::DeviceCreateInfo::default()
        .queue_create_infos(&queues)
        .enabled_extension_names(&extensions)
        .push_next(&mut v13);
    if s.gpl {
        info = info.push_next(&mut gpl);
    }
    if s.shader_object {
        info = info.push_next(&mut so);
    }
    // SAFETY: every enabled extension and feature was reported as supported by `select`; the
    // create info and its chain live until the call returns.
    unsafe { instance.create_device(s.physical, &info, None) }
        .map_err(HarnessError::vk("create device"))
}

fn describe(instance: &ash::Instance, s: &Selected) -> Caps {
    let mut driver = vk::PhysicalDeviceDriverProperties::default();
    let mut gpl_props = vk::PhysicalDeviceGraphicsPipelineLibraryPropertiesEXT::default();
    let mut props = vk::PhysicalDeviceProperties2::default().push_next(&mut driver);
    if s.gpl {
        props = props.push_next(&mut gpl_props);
    }
    // SAFETY: valid physical device; the chain contains core 1.2 and enabled-extension structs.
    unsafe { instance.get_physical_device_properties2(s.physical, &mut props) };
    let device_name = c_str_to_string(props.properties.device_name_as_c_str());
    Caps {
        device_name,
        driver: format!(
            "{} {}",
            c_str_to_string(driver.driver_name_as_c_str()),
            c_str_to_string(driver.driver_info_as_c_str())
        ),
        graphics_pipeline_library: s.gpl,
        gpl_fast_linking: s.gpl && gpl_props.graphics_pipeline_library_fast_linking == vk::TRUE,
        shader_object: s.shader_object,
    }
}

impl Drop for Context {
    fn drop(&mut self) {
        // Relaxed: the workers that could poison the context were joined before this drop.
        if self.poisoned.load(Ordering::Relaxed) {
            // A failed sample may have left pipelines or modules alive; destroying the device
            // would violate VUID-vkDestroyDevice-device-05137. The process is about to exit, so
            // the device, instance and loader are left to the OS. The messenger stays alive,
            // so its counter is leaked with it; a leaked entry reference keeps the loader loaded.
            std::mem::forget(self.messenger.take());
            std::mem::forget(self.entry.clone());
            return;
        }
        // SAFETY: not poisoned, so every worker finished its samples and destroyed all its
        // pipelines, libraries, modules, shader objects and caches (workers borrow the context
        // and are dropped first). No GPU work was ever submitted.
        unsafe {
            self.device.destroy_pipeline_layout(self.layout, None);
            self.device.destroy_device(None);
        }
        destroy_messenger(self.messenger.as_ref());
        // SAFETY: the device and messenger created from the instance are destroyed above.
        unsafe { self.instance.destroy_instance(None) };
    }
}
