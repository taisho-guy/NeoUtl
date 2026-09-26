//! wgpu::Buffer(MAP_WRITE)をクラスサイズ別リングプールとして再利用し、
//! CPUデコード結果をVRAMへCPUコピー1回で転送する。
//!
//! 経路: デコーダ出力→(map_async)mappedバッファ直書き→unmap→
//! copy_buffer_to_texture。write_texture経由の内部二重コピーを排除する。
//! mlock/mmap等OS固有APIに依存せず、wgpu自体がDX12/Vulkan/Metal各バックエンドで
//! ホスト可視メモリ確保を抽象化するため、cfg分岐は不要。

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

const RING_DEPTH: usize = 4;
const CLASS_ALIGN: u64 = 4096;

fn class_size(byte_len: u64) -> u64 {
    byte_len.max(1).div_ceil(CLASS_ALIGN) * CLASS_ALIGN
}

/// 実データ幅(パディング前bytes_per_row)から、
/// `COPY_BYTES_PER_ROW_ALIGNMENT`境界へパディングしたレイアウトを求める。
fn padded_bytes_per_row(unpadded_bytes_per_row: u32) -> u32 {
    let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    unpadded_bytes_per_row.div_ceil(align) * align
}

struct PoolState {
    free: HashMap<u64, VecDeque<wgpu::Buffer>>,
    allocated: HashMap<u64, usize>,
}

/// デバイス単位で共有するステージングバッファプール。
pub(crate) struct StagingPool {
    device: Arc<wgpu::Device>,
    state: Mutex<PoolState>,
}

impl StagingPool {
    pub(crate) fn new(device: Arc<wgpu::Device>) -> Arc<Self> {
        Arc::new(Self {
            device,
            state: Mutex::new(PoolState {
                free: HashMap::new(),
                allocated: HashMap::new(),
            }),
        })
    }

    /// クラスサイズのバッファを取得する。空きが無くRING_DEPTH上限に達している場合、
    /// 既存送信の完了(checkinによる返却)を待って再試行する。
    fn acquire(&self, class_size: u64) -> wgpu::Buffer {
        loop {
            {
                let mut state = self.state.lock().expect("staging pool mutex poisoned");
                if let Some(buffer) = state
                    .free
                    .get_mut(&class_size)
                    .and_then(VecDeque::pop_front)
                {
                    return buffer;
                }
                let count = state.allocated.entry(class_size).or_insert(0);
                if *count < RING_DEPTH {
                    *count += 1;
                    return self.device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some("neoutl_staging_pool_buffer"),
                        size: class_size,
                        usage: wgpu::BufferUsages::MAP_WRITE | wgpu::BufferUsages::COPY_SRC,
                        mapped_at_creation: false,
                    });
                }
            }
            self.device
                .poll(wgpu::PollType::wait_indefinitely())
                .expect("device poll失敗");
        }
    }

    fn checkin(&self, class_size: u64, buffer: wgpu::Buffer) {
        self.state
            .lock()
            .expect("staging pool mutex poisoned")
            .free
            .entry(class_size)
            .or_default()
            .push_back(buffer);
    }

    /// CPU平面データをmappedステージングバッファへ行単位コピーし、
    /// `copy_buffer_to_texture`でVRAMへ転送する。
    ///
    /// `src`は`src_stride`バイト/行、`unpadded_bytes_per_row`は実データ幅
    /// (`width * ピクセルあたりバイト数`)を表す。両者は一致しない場合がある
    /// (デコーダ側stride > 実データ幅)。
    ///
    /// 戻り値の送信完了を呼び出し側が待つ必要は無い。バッファは
    /// `queue.on_submitted_work_done`経由で完了後にプールへ自動返却される。
    pub(crate) fn upload_plane(
        self: &Arc<Self>,
        queue: &wgpu::Queue,
        texture: &wgpu::Texture,
        width: u32,
        height: u32,
        unpadded_bytes_per_row: u32,
        src: &[u8],
        src_stride: u32,
    ) -> wgpu::SubmissionIndex {
        let padded_row = padded_bytes_per_row(unpadded_bytes_per_row);
        let total_bytes = padded_row as u64 * height as u64;
        let class_size = class_size(total_bytes);

        let buffer = self.acquire(class_size);

        {
            let slice = buffer.slice(0..total_bytes);
            let (tx, rx) = std::sync::mpsc::channel();
            slice.map_async(wgpu::MapMode::Write, move |result| {
                let _ = tx.send(result);
            });
            self.device
                .poll(wgpu::PollType::wait_indefinitely())
                .expect("device poll失敗");
            rx.recv()
                .expect("map_asyncコールバック未起動")
                .expect("staging bufferのmapに失敗");

            let mut view = slice
                .get_mapped_range_mut()
                .expect("map_async成功直後のget_mapped_range_mut失敗");
            let row_bytes = unpadded_bytes_per_row as usize;
            for row in 0..height as usize {
                let src_off = row * src_stride as usize;
                let dst_off = row * padded_row as usize;
                view.slice(dst_off..dst_off + row_bytes)
                    .copy_from_slice(&src[src_off..src_off + row_bytes]);
            }
        }
        buffer.unmap();

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("neoutl_staging_pool_upload"),
            });
        encoder.copy_buffer_to_texture(
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        let submission_index = queue.submit(Some(encoder.finish()));

        let pool = self.clone();
        queue.on_submitted_work_done(move || pool.checkin(class_size, buffer));

        submission_index
    }
}
