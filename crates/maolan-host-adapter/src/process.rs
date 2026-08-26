use crate::types::{HostError, PluginFormat, PluginParamInfo};
use maolan_plugin_host::events::EventPair;
use maolan_plugin_host::protocol::{
    self, MAX_BLOCK_SIZE, MAX_CHANNELS, ParameterEvent, SCRATCH_SIZE, ShmHeader,
};
use maolan_plugin_host::ringbuf::RingBuffer;
use maolan_plugin_host::shm::ShmMapping;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

const READY_TIMEOUT: Duration = Duration::from_secs(10);
const BLOCK_TIMEOUT: Duration = Duration::from_secs(2);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
const REQUEST_CLAP_PARAMETERS: u32 = 9;

pub struct PluginProcess {
    child: Child,
    shm: ShmMapping,
    events: EventPair,
    shm_name: String,
    format: PluginFormat,
    num_inputs: usize,
    num_outputs: usize,
    alive: bool,
}

impl PluginProcess {
    pub fn spawn(
        binary_path: &std::path::Path,
        format: PluginFormat,
        plugin_spec: &str,
        instance_id: &str,
        sample_rate: f64,
        buffer_size: usize,
        num_inputs: usize,
        num_outputs: usize,
    ) -> Result<Self, HostError> {
        let format_tag = format.host_format_tag()?;
        let shm_name = format!(
            "/neoutl-plugin-{}-{}",
            std::process::id(),
            instance_id.replace(['/', '\\'], "_")
        );

        let shm = ShmMapping::create(&shm_name, protocol::SHM_SIZE)
            .map_err(HostError::ShmCreateFailed)?;
        unsafe { protocol::init_shm_layout(shm.as_ptr(), protocol::SHM_SIZE) };

        let events = EventPair::new().map_err(HostError::EventPairFailed)?;

        let mut cmd = Command::new(binary_path);
        cmd.arg(format_tag)
            .arg(plugin_spec)
            .arg(&shm_name)
            .arg(instance_id);

        #[cfg(unix)]
        {
            cmd.arg(events.host_read_fd().to_string())
                .arg(events.host_write_fd().to_string());
        }
        #[cfg(windows)]
        {
            cmd.arg(events.daw_to_host_name())
                .arg(events.host_to_daw_name());
        }

        if matches!(format, PluginFormat::Vst3 | PluginFormat::Lv2) {
            cmd.arg(sample_rate.to_string())
                .arg(buffer_size.to_string())
                .arg(num_inputs.to_string())
                .arg(num_outputs.to_string());
        }

        cmd.stdin(Stdio::null())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());

        let child = cmd.spawn().map_err(HostError::SpawnFailed)?;

        #[cfg(unix)]
        let events = {
            #[cfg(unix)]
            {
                let mut events = events;
                events.close_daw_unused();
                events
            }
            #[cfg(not(unix))]
            {
                events
            }
        };
        #[cfg(not(unix))]
        let events = events;

        let header = unsafe { protocol::header_ref(shm.as_ptr()) };
        if !protocol::wait_for_ready(header, READY_TIMEOUT) {
            return Err(HostError::ReadyTimeout);
        }

        header
            .num_input_channels
            .store(num_inputs.min(MAX_CHANNELS) as u32, Ordering::Release);
        header
            .num_output_channels
            .store(num_outputs.min(MAX_CHANNELS) as u32, Ordering::Release);
        header
            .block_size
            .store(buffer_size.min(MAX_BLOCK_SIZE) as u32, Ordering::Release);

        Ok(Self {
            child,
            shm,
            events,
            shm_name,
            format,
            num_inputs,
            num_outputs,
            alive: true,
        })
    }

    fn header(&self) -> &ShmHeader {
        unsafe { protocol::header_ref(self.shm.as_ptr()) }
    }

    pub fn is_alive(&self) -> bool {
        self.alive
    }

    pub fn process_stereo(
        &mut self,
        in_l: &[f32],
        in_r: &[f32],
        out_l: &mut [f32],
        out_r: &mut [f32],
        frames: usize,
    ) -> Result<(), HostError> {
        if !self.alive {
            return Err(HostError::ProcessDead);
        }
        if frames > MAX_BLOCK_SIZE {
            return Err(HostError::BlockTooLarge {
                frames,
                max: MAX_BLOCK_SIZE,
            });
        }
        let ptr = self.shm.as_ptr();
        let header = self.header();
        header.block_size.store(frames as u32, Ordering::Release);

        unsafe {
            if self.num_inputs > 0 {
                std::ptr::copy_nonoverlapping(
                    in_l.as_ptr(),
                    protocol::audio_channel_ptr(ptr, 0, 0),
                    frames,
                );
            }
            if self.num_inputs > 1 {
                std::ptr::copy_nonoverlapping(
                    in_r.as_ptr(),
                    protocol::audio_channel_ptr(ptr, 1, 0),
                    frames,
                );
            }
        }

        if self.events.signal_host().is_err() {
            self.alive = false;
            return Err(HostError::ProcessDead);
        }
        if self.events.wait_host(BLOCK_TIMEOUT).is_err() {
            self.alive = false;
            return Err(HostError::ProcessDead);
        }

        unsafe {
            if self.num_outputs > 0 {
                std::ptr::copy_nonoverlapping(
                    protocol::audio_channel_ptr(ptr, 0, 1),
                    out_l.as_mut_ptr(),
                    frames,
                );
            }
            if self.num_outputs > 1 {
                std::ptr::copy_nonoverlapping(
                    protocol::audio_channel_ptr(ptr, 1, 1),
                    out_r.as_mut_ptr(),
                    frames,
                );
            } else if self.num_outputs == 1 {
                out_r[..frames].copy_from_slice(&out_l[..frames]);
            }
        }
        Ok(())
    }

    pub fn set_parameter_value(&self, param_id: u32, value: f32) {
        let ptr = self.shm.as_ptr();
        let ring = unsafe {
            let buf = protocol::param_ring_ptr(ptr);
            let (w, r) = protocol::param_indices(ptr);
            RingBuffer::new(buf, w, r, protocol::RING_CAPACITY)
        };
        ring.push(ParameterEvent {
            param_index: param_id,
            value,
            sample_offset: 0,
            event_kind: protocol::PARAM_EVENT_VALUE,
        });
    }

    pub fn full_param_info_list(&self) -> Result<Vec<PluginParamInfo>, HostError> {
        if self.format != PluginFormat::Clap {
            return Ok(Vec::new());
        }
        if !self.alive {
            return Err(HostError::ProcessDead);
        }
        let header = self.header();
        header.request_status.store(0, Ordering::Release);
        header
            .request_type
            .store(REQUEST_CLAP_PARAMETERS, Ordering::Release);

        if self.events.signal_host().is_err() {
            return Err(HostError::ProcessDead);
        }

        let start = Instant::now();
        loop {
            let status = header.request_status.load(Ordering::Acquire);
            if status == 1 {
                break;
            }
            if status == 2 {
                header.request_type.store(0, Ordering::Release);
                return Err(HostError::RequestFailed(
                    "plugin-host reported CLAP parameter enumeration error".to_string(),
                ));
            }
            if start.elapsed() >= REQUEST_TIMEOUT {
                header.request_type.store(0, Ordering::Release);
                return Err(HostError::RequestTimeout);
            }
            std::thread::yield_now();
        }

        let size = header.scratch_size.load(Ordering::Acquire) as usize;
        header.request_type.store(0, Ordering::Release);
        let result =
            unsafe { decode_clap_parameters(protocol::scratch_ptr(self.shm.as_ptr()), size) };
        result.ok_or(HostError::ScratchDecodeFailed)
    }

    pub fn shm_name(&self) -> &str {
        &self.shm_name
    }
}

impl Drop for PluginProcess {
    fn drop(&mut self) {
        if self.alive {
            self.header().shutdown_request.store(1, Ordering::Release);
            let _ = self.events.signal_host();
        }
        let deadline = Instant::now() + Duration::from_millis(500);
        loop {
            match self.child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(10));
                }
                _ => {
                    let _ = self.child.kill();
                    let _ = self.child.wait();
                    break;
                }
            }
        }
        let _ = ShmMapping::unlink(&self.shm_name);
    }
}

unsafe fn decode_clap_parameters(scratch: *mut u8, size: usize) -> Option<Vec<PluginParamInfo>> {
    if size < 4 || size > SCRATCH_SIZE {
        return None;
    }
    let mut offset = 0usize;
    let read_u32 = |off: usize| -> Option<u32> {
        if off + 4 > size {
            return None;
        }
        Some(unsafe { std::ptr::read_unaligned(scratch.add(off) as *const u32) })
    };
    let read_f64_bits = |off: usize| -> Option<f64> {
        if off + 8 > size {
            return None;
        }
        let bits = unsafe { std::ptr::read_unaligned(scratch.add(off) as *const u64) };
        Some(f64::from_bits(bits))
    };
    let read_string = |off: usize, len: usize| -> Option<String> {
        if off + len > size {
            return None;
        }
        let bytes = unsafe { std::slice::from_raw_parts(scratch.add(off), len) };
        String::from_utf8(bytes.to_vec()).ok()
    };

    let count = read_u32(offset)? as usize;
    offset += 4;
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        let id = read_u32(offset)?;
        offset += 4;
        let name_len = read_u32(offset)? as usize;
        offset += 4;
        let name = read_string(offset, name_len)?;
        offset += name_len;
        let module_len = read_u32(offset)? as usize;
        offset += 4;
        let module = read_string(offset, module_len)?;
        offset += module_len;
        let min = read_f64_bits(offset)?;
        offset += 8;
        let max = read_f64_bits(offset)?;
        offset += 8;
        let default = read_f64_bits(offset)?;
        offset += 8;
        out.push(PluginParamInfo {
            id,
            name,
            symbol: String::new(),
            unit: String::new(),
            comment: module,
            min,
            max,
            default,
        });
    }
    Some(out)
}
