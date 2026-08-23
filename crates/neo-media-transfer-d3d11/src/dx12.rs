use windows::Win32::Graphics::Direct3D12::*;
use windows::Win32::Graphics::Dxgi::Common::*;
use windows::core::Interface;

pub struct Dx12RawHandles {
    pub device: ID3D12Device,
    pub queue: ID3D12CommandQueue,
}

pub unsafe fn extract_dx12_raw_handles(device: &wgpu::Device) -> Option<Dx12RawHandles> {
    unsafe {
        let hal_device = device.as_hal::<wgpu_hal::api::Dx12>()?;
        let raw_device: ID3D12Device = hal_device.raw_device().clone();
        let raw_queue: ID3D12CommandQueue = hal_device.raw_queue().clone();
        Some(Dx12RawHandles {
            device: raw_device,
            queue: raw_queue,
        })
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ColorTags {
    pub matrix_coefficients: u32,
    pub transfer_characteristics: u32,
    pub color_primaries: u32,
    pub full_range: u32,
}

pub struct SemiPlanarConvertEngine {
    device: ID3D12Device,
    queue: ID3D12CommandQueue,
    root_signature: ID3D12RootSignature,
    pso: ID3D12PipelineState,
    cmd_allocator: ID3D12CommandAllocator,
    cmd_list: ID3D12GraphicsCommandList,
    srv_uav_heap: ID3D12DescriptorHeap,
    heap_increment: u32,
    fence: ID3D12Fence,
    fence_value: std::sync::atomic::AtomicU64,
}

fn descriptor_range(
    kind: D3D12_DESCRIPTOR_RANGE_TYPE,
    base: u32,
    count: u32,
) -> D3D12_DESCRIPTOR_RANGE {
    D3D12_DESCRIPTOR_RANGE {
        RangeType: kind,
        NumDescriptors: count,
        BaseShaderRegister: base,
        RegisterSpace: 0,
        OffsetInDescriptorsFromTableStart: D3D12_DESCRIPTOR_RANGE_OFFSET_APPEND,
    }
}

unsafe fn build_root_signature(device: &ID3D12Device) -> Result<ID3D12RootSignature, String> {
    unsafe {
        let srv_range = [descriptor_range(D3D12_DESCRIPTOR_RANGE_TYPE_SRV, 0, 2)];
        let uav_range = [descriptor_range(D3D12_DESCRIPTOR_RANGE_TYPE_UAV, 0, 1)];

        let params = [
            D3D12_ROOT_PARAMETER {
                ParameterType: D3D12_ROOT_PARAMETER_TYPE_DESCRIPTOR_TABLE,
                Anonymous: D3D12_ROOT_PARAMETER_0 {
                    DescriptorTable: D3D12_ROOT_DESCRIPTOR_TABLE {
                        NumDescriptorRanges: srv_range.len() as u32,
                        pDescriptorRanges: srv_range.as_ptr(),
                    },
                },
                ShaderVisibility: D3D12_SHADER_VISIBILITY_ALL,
            },
            D3D12_ROOT_PARAMETER {
                ParameterType: D3D12_ROOT_PARAMETER_TYPE_DESCRIPTOR_TABLE,
                Anonymous: D3D12_ROOT_PARAMETER_0 {
                    DescriptorTable: D3D12_ROOT_DESCRIPTOR_TABLE {
                        NumDescriptorRanges: uav_range.len() as u32,
                        pDescriptorRanges: uav_range.as_ptr(),
                    },
                },
                ShaderVisibility: D3D12_SHADER_VISIBILITY_ALL,
            },
            D3D12_ROOT_PARAMETER {
                ParameterType: D3D12_ROOT_PARAMETER_TYPE_32BIT_CONSTANTS,
                Anonymous: D3D12_ROOT_PARAMETER_0 {
                    Constants: D3D12_ROOT_CONSTANTS {
                        ShaderRegister: 0,
                        RegisterSpace: 0,
                        Num32BitValues: 4,
                    },
                },
                ShaderVisibility: D3D12_SHADER_VISIBILITY_ALL,
            },
        ];

        let sampler = D3D12_STATIC_SAMPLER_DESC {
            Filter: D3D12_FILTER_MIN_MAG_MIP_LINEAR,
            AddressU: D3D12_TEXTURE_ADDRESS_MODE_CLAMP,
            AddressV: D3D12_TEXTURE_ADDRESS_MODE_CLAMP,
            AddressW: D3D12_TEXTURE_ADDRESS_MODE_CLAMP,
            ShaderRegister: 0,
            RegisterSpace: 0,
            ShaderVisibility: D3D12_SHADER_VISIBILITY_ALL,
            ..Default::default()
        };

        let desc = D3D12_ROOT_SIGNATURE_DESC {
            NumParameters: params.len() as u32,
            pParameters: params.as_ptr(),
            NumStaticSamplers: 1,
            pStaticSamplers: &sampler,
            Flags: D3D12_ROOT_SIGNATURE_FLAG_NONE,
        };

        let mut blob: Option<windows::Win32::Graphics::Direct3D::ID3DBlob> = None;
        let mut error_blob: Option<windows::Win32::Graphics::Direct3D::ID3DBlob> = None;
        D3D12SerializeRootSignature(
            &desc,
            D3D_ROOT_SIGNATURE_VERSION_1,
            &mut blob,
            Some(&mut error_blob),
        )
        .map_err(|e| format!("D3D12SerializeRootSignature失敗: {e}"))?;
        let blob = blob.ok_or_else(|| "ルートシグネチャblob未取得".to_owned())?;
        let slice =
            std::slice::from_raw_parts(blob.GetBufferPointer() as *const u8, blob.GetBufferSize());
        device
            .CreateRootSignature(0, slice)
            .map_err(|e| format!("CreateRootSignature失敗: {e}"))
    }
}

impl SemiPlanarConvertEngine {
    pub unsafe fn new(handles: &Dx12RawHandles, dxil_bytes: &'static [u8]) -> Result<Self, String> {
        unsafe {
            let device = handles.device.clone();
            let queue = handles.queue.clone();

            let root_signature = build_root_signature(&device)?;

            let pso_desc = D3D12_COMPUTE_PIPELINE_STATE_DESC {
                pRootSignature: windows::core::ManuallyDrop::new(&root_signature),
                CS: D3D12_SHADER_BYTECODE {
                    pShaderBytecode: dxil_bytes.as_ptr() as *const _,
                    BytecodeLength: dxil_bytes.len(),
                },
                ..Default::default()
            };
            let pso: ID3D12PipelineState = device
                .CreateComputePipelineState(&pso_desc)
                .map_err(|e| format!("CreateComputePipelineState失敗: {e}"))?;

            let cmd_allocator: ID3D12CommandAllocator = device
                .CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT)
                .map_err(|e| format!("CreateCommandAllocator失敗: {e}"))?;
            let cmd_list: ID3D12GraphicsCommandList = device
                .CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, &cmd_allocator, None)
                .map_err(|e| format!("CreateCommandList失敗: {e}"))?;
            cmd_list.Close().map_err(|e| format!("{e}"))?;

            let heap_desc = D3D12_DESCRIPTOR_HEAP_DESC {
                Type: D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV,
                NumDescriptors: 3,
                Flags: D3D12_DESCRIPTOR_HEAP_FLAG_SHADER_VISIBLE,
                NodeMask: 0,
            };
            let srv_uav_heap: ID3D12DescriptorHeap = device
                .CreateDescriptorHeap(&heap_desc)
                .map_err(|e| format!("CreateDescriptorHeap失敗: {e}"))?;
            let heap_increment =
                device.GetDescriptorHandleIncrementSize(D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV);

            let fence: ID3D12Fence = device
                .CreateFence(0, D3D12_FENCE_FLAG_NONE)
                .map_err(|e| format!("CreateFence失敗: {e}"))?;

            Ok(Self {
                device,
                queue,
                root_signature,
                pso,
                cmd_allocator,
                cmd_list,
                srv_uav_heap,
                heap_increment,
                fence,
                fence_value: std::sync::atomic::AtomicU64::new(0),
            })
        }
    }

    unsafe fn cpu_handle(&self, slot: u32) -> D3D12_CPU_DESCRIPTOR_HANDLE {
        unsafe {
            let mut h = self.srv_uav_heap.GetCPUDescriptorHandleForHeapStart();
            h.ptr += (slot * self.heap_increment) as usize;
            h
        }
    }

    unsafe fn gpu_handle(&self, slot: u32) -> D3D12_GPU_DESCRIPTOR_HANDLE {
        unsafe {
            let mut h = self.srv_uav_heap.GetGPUDescriptorHandleForHeapStart();
            h.ptr += (slot * self.heap_increment) as u64;
            h
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub unsafe fn convert(
        &self,
        src_resource: &ID3D12Resource,
        y_format: DXGI_FORMAT,
        uv_format: DXGI_FORMAT,
        dst_resource: &ID3D12Resource,
        dst_format: DXGI_FORMAT,
        width: u32,
        height: u32,
        tags: ColorTags,
    ) -> Result<(), String> {
        unsafe {
            self.cmd_allocator.Reset().map_err(|e| format!("{e}"))?;
            self.cmd_list
                .Reset(&self.cmd_allocator, None)
                .map_err(|e| format!("{e}"))?;

            let y_srv = D3D12_SHADER_RESOURCE_VIEW_DESC {
                Format: y_format,
                ViewDimension: D3D12_SRV_DIMENSION_TEXTURE2D,
                Shader4ComponentMapping: D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING,
                Anonymous: D3D12_SHADER_RESOURCE_VIEW_DESC_0 {
                    Texture2D: D3D12_TEX2D_SRV {
                        MostDetailedMip: 0,
                        MipLevels: 1,
                        PlaneSlice: 0,
                        ResourceMinLODClamp: 0.0,
                    },
                },
            };
            self.device
                .CreateShaderResourceView(src_resource, Some(&y_srv), self.cpu_handle(0));

            let uv_srv = D3D12_SHADER_RESOURCE_VIEW_DESC {
                Format: uv_format,
                ViewDimension: D3D12_SRV_DIMENSION_TEXTURE2D,
                Shader4ComponentMapping: D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING,
                Anonymous: D3D12_SHADER_RESOURCE_VIEW_DESC_0 {
                    Texture2D: D3D12_TEX2D_SRV {
                        MostDetailedMip: 0,
                        MipLevels: 1,
                        PlaneSlice: 1,
                        ResourceMinLODClamp: 0.0,
                    },
                },
            };
            self.device
                .CreateShaderResourceView(src_resource, Some(&uv_srv), self.cpu_handle(1));

            let uav = D3D12_UNORDERED_ACCESS_VIEW_DESC {
                Format: dst_format,
                ViewDimension: D3D12_UAV_DIMENSION_TEXTURE2D,
                Anonymous: D3D12_UNORDERED_ACCESS_VIEW_DESC_0 {
                    Texture2D: D3D12_TEX2D_UAV {
                        MipSlice: 0,
                        PlaneSlice: 0,
                    },
                },
            };
            self.device.CreateUnorderedAccessView(
                dst_resource,
                None,
                Some(&uav),
                self.cpu_handle(2),
            );

            self.cmd_list.SetComputeRootSignature(&self.root_signature);
            let heaps = [Some(self.srv_uav_heap.clone())];
            self.cmd_list.SetDescriptorHeaps(&heaps);
            self.cmd_list
                .SetComputeRootDescriptorTable(0, self.gpu_handle(0));
            self.cmd_list
                .SetComputeRootDescriptorTable(1, self.gpu_handle(2));
            let constants: [u32; 4] = [
                tags.matrix_coefficients,
                tags.transfer_characteristics,
                tags.color_primaries,
                tags.full_range,
            ];
            self.cmd_list
                .SetComputeRoot32BitConstants(2, 4, constants.as_ptr() as *const _, 0);
            self.cmd_list.SetPipelineState(&self.pso);

            let groups_x = width.div_ceil(8);
            let groups_y = height.div_ceil(8);
            self.cmd_list.Dispatch(groups_x, groups_y, 1);

            self.cmd_list.Close().map_err(|e| format!("{e}"))?;
            let lists = [Some(
                self.cmd_list
                    .cast::<ID3D12CommandList>()
                    .map_err(|e| format!("{e}"))?,
            )];
            self.queue.ExecuteCommandLists(&lists);

            let value = self
                .fence_value
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
                + 1;
            self.queue
                .Signal(&self.fence, value)
                .map_err(|e| format!("{e}"))?;
            while self.fence.GetCompletedValue() < value {
                std::thread::yield_now();
            }
            Ok(())
        }
    }
}
