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
        apply(&mut core, first, &mut pending_prefetch);
        while let Ok(next) = cmd_rx.try_recv() {
            apply(&mut core, next, &mut pending_prefetch);
        }
        if let Some(target) = pending_prefetch {
            let _ = core.prefetch_at(target);
        }
    }
}

fn apply(core: &mut DecoderCore, cmd: Command, pending_prefetch: &mut Option<i64>) {
    match cmd {
        Command::Prefetch(target) => {
            *pending_prefetch = Some(target);
        }
        Command::FrameGpu(target, device, queue, resp) => {
            let result = core.frame_gpu_at(target, &device, &queue);
            let _ = resp.send(result);
        }
    }
}
