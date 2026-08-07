use crate::decoder_core::DecoderCore;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread::JoinHandle;

pub enum Command {
    Prefetch(i64),
    FrameGpu(
        i64,
        wgpu::Device,
        wgpu::Queue,
        Sender<Result<wgpu::Texture, String>>,
    ),
    SetOutputSize(u32, u32),
}

pub struct WorkerHandle {
    cmd_tx: Option<Sender<Command>>,
    join: Option<JoinHandle<()>>,
}

impl WorkerHandle {
    pub fn spawn(core: DecoderCore) -> Self {
        let (cmd_tx, cmd_rx) = channel::<Command>();
        let join = std::thread::Builder::new()
            .name("neoutl-ffmpeg-decode".into())
            .spawn(move || run(core, cmd_rx))
            .expect("decode worker thread spawn failed");
        Self {
            cmd_tx: Some(cmd_tx),
            join: Some(join),
        }
    }

    pub fn send(&self, cmd: Command) -> Result<(), String> {
        self.cmd_tx
            .as_ref()
            .expect("cmd_tx present until drop")
            .send(cmd)
            .map_err(|_| "decode worker thread terminated".to_string())
    }
}

impl Drop for WorkerHandle {
    fn drop(&mut self) {
        self.cmd_tx.take();
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

fn run(mut core: DecoderCore, cmd_rx: Receiver<Command>) {
    while let Ok(first) = cmd_rx.recv() {
        let mut pending_prefetch: Option<i64> = None;
        let mut pending_frame_gpu: Option<Command> = None;
        apply_or_defer(
            &mut core,
            first,
            &mut pending_prefetch,
            &mut pending_frame_gpu,
        );
        while let Ok(next) = cmd_rx.try_recv() {
            apply_or_defer(
                &mut core,
                next,
                &mut pending_prefetch,
                &mut pending_frame_gpu,
            );
        }
        if let Some(target) = pending_prefetch {
            let _ = core.prefetch_at(target);
        }
        if let Some(Command::FrameGpu(target, device, queue, resp)) = pending_frame_gpu {
            let result = core.frame_gpu_at(target, &device, &queue);
            let _ = resp.send(result);
        }
    }
}

fn apply_or_defer(
    core: &mut DecoderCore,
    cmd: Command,
    pending_prefetch: &mut Option<i64>,
    pending_frame_gpu: &mut Option<Command>,
) {
    match cmd {
        Command::Prefetch(target) => {
            *pending_prefetch = Some(target);
        }
        Command::FrameGpu(target, device, queue, resp) => {
            if let Some(Command::FrameGpu(_, _, _, stale_resp)) =
                pending_frame_gpu.replace(Command::FrameGpu(target, device, queue, resp))
            {
                let _ = stale_resp.send(Err("superseded".to_owned()));
            }
        }
        Command::SetOutputSize(width, height) => {
            core.set_output_size(width, height);
        }
    }
}
