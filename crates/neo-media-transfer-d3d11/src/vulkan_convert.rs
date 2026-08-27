use std::ffi::CStr;

use ash::vk;
use bytemuck::{Pod, Zeroable};

pub struct VulkanRawHandles {
    pub entry: ash::Entry,
    pub instance: ash::Instance,
    pub physical_device: vk::PhysicalDevice,
    pub device: ash::Device,
    pub queue: vk::Queue,
    pub queue_family_index: u32,
}

pub unsafe fn extract_vulkan_raw_handles(device: &wgpu::Device) -> Option<VulkanRawHandles> {
    unsafe {
        let hal_device = device.as_hal::<wgpu_hal::api::Vulkan>()?;
        let shared_instance = hal_device.shared_instance();
        let raw_instance = shared_instance.raw_instance().clone();
        let entry = shared_instance.entry().clone();
        Some(VulkanRawHandles {
            entry,
            instance: raw_instance,
            physical_device: hal_device.raw_physical_device(),
            device: hal_device.raw_device().clone(),
            queue: hal_device.raw_queue(),
            queue_family_index: hal_device.queue_family_index(),
        })
    }
}

pub fn dx12_adapter_luid(_device: &wgpu::Device) -> Option<[u8; 8]> {
    unsafe {
        let handles = extract_vulkan_raw_handles(_device)?;
        let mut id_props = vk::PhysicalDeviceIDProperties::default();
        let mut props2 = vk::PhysicalDeviceProperties2::default().push_next(&mut id_props);
        handles
            .instance
            .get_physical_device_properties2(handles.physical_device, &mut props2);
        if id_props.device_luid_valid == vk::TRUE {
            Some(id_props.device_luid)
        } else {
            None
        }
    }
}

pub fn query_vram_budget_bytes(device: &wgpu::Device) -> Option<u64> {
    unsafe {
        let handles = extract_vulkan_raw_handles(device)?;
        let mut budget_props = vk::PhysicalDeviceMemoryBudgetPropertiesEXT::default();
        let mut mem_props2 =
            vk::PhysicalDeviceMemoryProperties2::default().push_next(&mut budget_props);
        handles
            .instance
            .get_physical_device_memory_properties2(handles.physical_device, &mut mem_props2);
        budget_props.heap_budget.iter().copied().max()
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct ColorTags {
    pub matrix_coefficients: u32,
    pub transfer_characteristics: u32,
    pub color_primaries: u32,
    pub full_range: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct PushConstants {
    tags: ColorTags,
    width: u32,
    height: u32,
    _pad0: u32,
    _pad1: u32,
}

const SEMI_PLANAR_TO_RGBA_SPV: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/semi_planar_to_rgba.spv"));

pub struct SemiPlanarConvertEngine {
    device: ash::Device,
    pipeline: vk::Pipeline,
    pipeline_layout: vk::PipelineLayout,
    descriptor_set_layout: vk::DescriptorSetLayout,
    descriptor_pool: vk::DescriptorPool,
    shader_module: vk::ShaderModule,
    command_pool: vk::CommandPool,
    sampler: vk::Sampler,
}

impl SemiPlanarConvertEngine {
    pub fn new(device: ash::Device, queue_family_index: u32) -> Result<Self, String> {
        unsafe {
            let spirv_words: Vec<u32> = SEMI_PLANAR_TO_RGBA_SPV
                .chunks_exact(4)
                .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                .collect();
            let shader_info = vk::ShaderModuleCreateInfo::default().code(&spirv_words);
            let shader_module = device
                .create_shader_module(&shader_info, None)
                .map_err(|e| format!("vkCreateShaderModule失敗: {e}"))?;

            let bindings = [
                vk::DescriptorSetLayoutBinding::default()
                    .binding(0)
                    .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::COMPUTE),
                vk::DescriptorSetLayoutBinding::default()
                    .binding(1)
                    .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::COMPUTE),
                vk::DescriptorSetLayoutBinding::default()
                    .binding(2)
                    .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::COMPUTE),
                vk::DescriptorSetLayoutBinding::default()
                    .binding(3)
                    .descriptor_type(vk::DescriptorType::SAMPLER)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::COMPUTE),
            ];
            let layout_info = vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings);
            let descriptor_set_layout = device
                .create_descriptor_set_layout(&layout_info, None)
                .map_err(|e| format!("vkCreateDescriptorSetLayout失敗: {e}"))?;

            let push_constant_range = vk::PushConstantRange::default()
                .stage_flags(vk::ShaderStageFlags::COMPUTE)
                .offset(0)
                .size(std::mem::size_of::<PushConstants>() as u32);
            let set_layouts = [descriptor_set_layout];
            let push_constant_ranges = [push_constant_range];
            let pipeline_layout_info = vk::PipelineLayoutCreateInfo::default()
                .set_layouts(&set_layouts)
                .push_constant_ranges(&push_constant_ranges);
            let pipeline_layout = device
                .create_pipeline_layout(&pipeline_layout_info, None)
                .map_err(|e| format!("vkCreatePipelineLayout失敗: {e}"))?;

            let entry_point: &CStr = c"main";
            let stage_info = vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::COMPUTE)
                .module(shader_module)
                .name(entry_point);
            let pipeline_info = vk::ComputePipelineCreateInfo::default()
                .stage(stage_info)
                .layout(pipeline_layout);
            let pipeline = device
                .create_compute_pipelines(vk::PipelineCache::null(), &[pipeline_info], None)
                .map_err(|(_, e)| format!("vkCreateComputePipelines失敗: {e}"))?[0];

            let pool_sizes = [
                vk::DescriptorPoolSize {
                    ty: vk::DescriptorType::SAMPLED_IMAGE,
                    descriptor_count: 2 * RING_MULTIPLIER,
                },
                vk::DescriptorPoolSize {
                    ty: vk::DescriptorType::STORAGE_IMAGE,
                    descriptor_count: 1 * RING_MULTIPLIER,
                },
                vk::DescriptorPoolSize {
                    ty: vk::DescriptorType::SAMPLER,
                    descriptor_count: 1 * RING_MULTIPLIER,
                },
            ];
            let pool_info = vk::DescriptorPoolCreateInfo::default()
                .max_sets(RING_MULTIPLIER)
                .pool_sizes(&pool_sizes)
                .flags(vk::DescriptorPoolCreateFlags::FREE_DESCRIPTOR_SET);
            let descriptor_pool = device
                .create_descriptor_pool(&pool_info, None)
                .map_err(|e| format!("vkCreateDescriptorPool失敗: {e}"))?;

            let cmd_pool_info = vk::CommandPoolCreateInfo::default()
                .queue_family_index(queue_family_index)
                .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER);
            let command_pool = device
                .create_command_pool(&cmd_pool_info, None)
                .map_err(|e| format!("vkCreateCommandPool失敗: {e}"))?;

            let sampler_info = vk::SamplerCreateInfo::default()
                .mag_filter(vk::Filter::NEAREST)
                .min_filter(vk::Filter::NEAREST)
                .mipmap_mode(vk::SamplerMipmapMode::NEAREST)
                .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .unnormalized_coordinates(false);
            let sampler = device
                .create_sampler(&sampler_info, None)
                .map_err(|e| format!("vkCreateSampler失敗: {e}"))?;

            Ok(Self {
                device,
                pipeline,
                pipeline_layout,
                descriptor_set_layout,
                descriptor_pool,
                shader_module,
                command_pool,
                sampler,
            })
        }
    }

    pub fn convert(&self, req: ConvertRequest) -> Result<(), String> {
        unsafe {
            let plane0_view =
                self.create_plane_view(req.src_image, req.y_format, vk::ImageAspectFlags::PLANE_0)?;
            let plane1_view = self.create_plane_view(
                req.src_image,
                req.uv_format,
                vk::ImageAspectFlags::PLANE_1,
            )?;
            let output_view = self.create_color_view(req.dst_image, req.dst_format)?;

            let alloc_info = vk::CommandBufferAllocateInfo::default()
                .command_pool(self.command_pool)
                .level(vk::CommandBufferLevel::PRIMARY)
                .command_buffer_count(1);
            let cmd = self
                .device
                .allocate_command_buffers(&alloc_info)
                .map_err(|e| format!("vkAllocateCommandBuffers失敗: {e}"))?[0];

            let set_layouts = [self.descriptor_set_layout];
            let ds_alloc_info = vk::DescriptorSetAllocateInfo::default()
                .descriptor_pool(self.descriptor_pool)
                .set_layouts(&set_layouts);
            let descriptor_set = self
                .device
                .allocate_descriptor_sets(&ds_alloc_info)
                .map_err(|e| format!("vkAllocateDescriptorSets失敗: {e}"))?[0];

            let plane0_info = vk::DescriptorImageInfo::default()
                .image_view(plane0_view)
                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL);
            let plane1_info = vk::DescriptorImageInfo::default()
                .image_view(plane1_view)
                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL);
            let output_info = vk::DescriptorImageInfo::default()
                .image_view(output_view)
                .image_layout(vk::ImageLayout::GENERAL);
            let sampler_info = vk::DescriptorImageInfo::default().sampler(self.sampler);

            let writes = [
                vk::WriteDescriptorSet::default()
                    .dst_set(descriptor_set)
                    .dst_binding(0)
                    .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                    .image_info(std::slice::from_ref(&plane0_info)),
                vk::WriteDescriptorSet::default()
                    .dst_set(descriptor_set)
                    .dst_binding(1)
                    .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                    .image_info(std::slice::from_ref(&plane1_info)),
                vk::WriteDescriptorSet::default()
                    .dst_set(descriptor_set)
                    .dst_binding(2)
                    .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                    .image_info(std::slice::from_ref(&output_info)),
                vk::WriteDescriptorSet::default()
                    .dst_set(descriptor_set)
                    .dst_binding(3)
                    .descriptor_type(vk::DescriptorType::SAMPLER)
                    .image_info(std::slice::from_ref(&sampler_info)),
            ];
            self.device.update_descriptor_sets(&writes, &[]);

            let begin_info = vk::CommandBufferBeginInfo::default()
                .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
            self.device
                .begin_command_buffer(cmd, &begin_info)
                .map_err(|e| format!("vkBeginCommandBuffer失敗: {e}"))?;

            let plane0_barrier = vk::ImageMemoryBarrier::default()
                .old_layout(vk::ImageLayout::UNDEFINED)
                .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                .src_access_mask(vk::AccessFlags::empty())
                .dst_access_mask(vk::AccessFlags::SHADER_READ)
                .image(req.src_image)
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::PLANE_0,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                });
            let plane1_barrier = vk::ImageMemoryBarrier {
                subresource_range: vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::PLANE_1,
                    ..plane0_barrier.subresource_range
                },
                ..plane0_barrier
            };
            let dst_barrier = vk::ImageMemoryBarrier::default()
                .old_layout(vk::ImageLayout::UNDEFINED)
                .new_layout(vk::ImageLayout::GENERAL)
                .src_access_mask(vk::AccessFlags::empty())
                .dst_access_mask(vk::AccessFlags::SHADER_WRITE)
                .image(req.dst_image)
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                });
            self.device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::TOP_OF_PIPE,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[plane0_barrier, plane1_barrier, dst_barrier],
            );

            self.device
                .cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, self.pipeline);
            self.device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::COMPUTE,
                self.pipeline_layout,
                0,
                &[descriptor_set],
                &[],
            );
            let push = PushConstants {
                tags: req.tags,
                width: req.width,
                height: req.height,
                _pad0: 0,
                _pad1: 0,
            };
            self.device.cmd_push_constants(
                cmd,
                self.pipeline_layout,
                vk::ShaderStageFlags::COMPUTE,
                0,
                bytemuck::bytes_of(&push),
            );
            let group_x = req.width.div_ceil(8);
            let group_y = req.height.div_ceil(8);
            self.device.cmd_dispatch(cmd, group_x, group_y, 1);

            self.device
                .end_command_buffer(cmd)
                .map_err(|e| format!("vkEndCommandBuffer失敗: {e}"))?;

            let submit_wait_semaphores = [req.fence_semaphore];
            let submit_signal_semaphores = [req.fence_semaphore];
            let cmd_buffers = [cmd];
            let wait_stage_mask = [vk::PipelineStageFlags::COMPUTE_SHADER];
            let wait_values_submit = [req.wait_value];
            let signal_values_submit = [req.signal_value];
            let mut timeline_submit = vk::TimelineSemaphoreSubmitInfo::default()
                .wait_semaphore_values(&wait_values_submit)
                .signal_semaphore_values(&signal_values_submit);
            let submit_info = vk::SubmitInfo::default()
                .wait_semaphores(&submit_wait_semaphores)
                .wait_dst_stage_mask(&wait_stage_mask)
                .command_buffers(&cmd_buffers)
                .signal_semaphores(&submit_signal_semaphores)
                .push_next(&mut timeline_submit);
            self.device
                .queue_submit(req.queue, &[submit_info], vk::Fence::null())
                .map_err(|e| format!("vkQueueSubmit失敗: {e}"))?;

            let wait_semaphores = [req.fence_semaphore];
            let wait_values = [req.signal_value];
            let mut wait_info = vk::SemaphoreWaitInfo::default()
                .semaphores(&wait_semaphores)
                .values(&wait_values);
            self.device
                .wait_semaphores(&mut wait_info, u64::MAX)
                .map_err(|e| format!("vkWaitSemaphores失敗: {e}"))?;

            self.device.destroy_image_view(plane0_view, None);
            self.device.destroy_image_view(plane1_view, None);
            self.device.destroy_image_view(output_view, None);
            self.device
                .free_descriptor_sets(self.descriptor_pool, &[descriptor_set])
                .map_err(|e| format!("vkFreeDescriptorSets失敗: {e}"))?;
            self.device.free_command_buffers(self.command_pool, &[cmd]);

            Ok(())
        }
    }

    unsafe fn create_plane_view(
        &self,
        image: vk::Image,
        format: vk::Format,
        aspect: vk::ImageAspectFlags,
    ) -> Result<vk::ImageView, String> {
        unsafe {
            let info = vk::ImageViewCreateInfo::default()
                .image(image)
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(format)
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: aspect,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                });
            self.device
                .create_image_view(&info, None)
                .map_err(|e| format!("vkCreateImageView(plane)失敗: {e}"))
        }
    }

    unsafe fn create_color_view(
        &self,
        image: vk::Image,
        format: vk::Format,
    ) -> Result<vk::ImageView, String> {
        unsafe {
            let info = vk::ImageViewCreateInfo::default()
                .image(image)
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(format)
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                });
            self.device
                .create_image_view(&info, None)
                .map_err(|e| format!("vkCreateImageView(color)失敗: {e}"))
        }
    }
}

pub struct ConvertRequest {
    pub src_image: vk::Image,
    pub y_format: vk::Format,
    pub uv_format: vk::Format,
    pub dst_image: vk::Image,
    pub dst_format: vk::Format,
    pub width: u32,
    pub height: u32,
    pub queue: vk::Queue,
    pub fence_semaphore: vk::Semaphore,
    pub wait_value: u64,
    pub signal_value: u64,
    pub tags: ColorTags,
}

const RING_MULTIPLIER: u32 = 4;

impl Drop for SemiPlanarConvertEngine {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_sampler(self.sampler, None);
            self.device.destroy_command_pool(self.command_pool, None);
            self.device
                .destroy_descriptor_pool(self.descriptor_pool, None);
            self.device.destroy_pipeline(self.pipeline, None);
            self.device
                .destroy_pipeline_layout(self.pipeline_layout, None);
            self.device
                .destroy_descriptor_set_layout(self.descriptor_set_layout, None);
            self.device.destroy_shader_module(self.shader_module, None);
        }
    }
}
