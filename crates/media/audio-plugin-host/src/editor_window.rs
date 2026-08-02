//! CLAPプラグインGUI埋め込み先ウィンドウを、専用スレッド上の単一winit `EventLoop`で管理する。
//!
//! winitのイベントループは1プロセスに1本が前提のAPIであり、かつSlint本体のイベントループ
//! （メインスレッド）と共存させる統合は未実施（別コミット送り）。そのため本モジュールは
//! 完全に独立したバックグラウンドスレッドでwinit専用イベントループを起動し、ウィンドウの
//! 生成・破棄・クローズ通知をチャネル越しにやり取りする。
//!
//! CLAP GUI拡張の`set_parent`/`show`/`destroy`等プラグイン本体への操作自体は、この
//! スレッドでは行わない（`clap.rs`側がメインスレッドで実行する）。本モジュールが提供する
//! のは「ウィンドウの実体とOSイベントポンプ」のみであり、`RawWindowHandle`を`clap.rs`へ
//! 返却した後は`clap.rs`側がその値を使ってプラグインへ`set_parent`する。

use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Mutex, OnceLock};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy};
use winit::window::{Window, WindowId};

static NEXT_HANDLE: AtomicU64 = AtomicU64::new(1);

/// 本モジュール内でウィンドウ1枚を指し示す不透明ID。`WindowId`（winit内部型）とは別に
/// 発行し、呼び出し側（`clap.rs`）が`winit`型に依存しないようにする。
pub type EditorHandle = u64;

/// `RawWindowHandle`はOS生ポインタを保持するため`Send`未実装。本モジュールは
/// ハンドル値をスレッド間で転送するのみで参照外し・並行アクセスを行わないため、
/// 送出専用ラッパーに限定して`Send`を付与する。
struct SendableRawWindowHandle(RawWindowHandle);

unsafe impl Send for SendableRawWindowHandle {}

enum Command {
    Open {
        title: String,
        width: u32,
        height: u32,
        handle: EditorHandle,
        reply: Sender<Result<SendableRawWindowHandle, String>>,
    },
    Close(EditorHandle),
}

enum Notice {
    ClosedByUser(EditorHandle),
}

struct App {
    windows: HashMap<EditorHandle, Window>,
    id_to_handle: HashMap<WindowId, EditorHandle>,
    commands: Receiver<Command>,
    notices: Sender<Notice>,
}

impl App {
    fn drain_commands(&mut self, event_loop: &ActiveEventLoop) {
        while let Ok(command) = self.commands.try_recv() {
            match command {
                Command::Open {
                    title,
                    width,
                    height,
                    handle,
                    reply,
                } => {
                    let attrs = Window::default_attributes()
                        .with_title(title)
                        .with_inner_size(winit::dpi::LogicalSize::new(width, height));
                    let result = event_loop
                        .create_window(attrs)
                        .map_err(|e| e.to_string())
                        .and_then(|window| {
                            window
                                .window_handle()
                                .map(|h| h.as_raw())
                                .map_err(|e| e.to_string())
                                .map(|raw| (window, raw))
                        });
                    match result {
                        Ok((window, raw)) => {
                            self.id_to_handle.insert(window.id(), handle);
                            self.windows.insert(handle, window);
                            let _ = reply.send(Ok(SendableRawWindowHandle(raw)));
                        }
                        Err(e) => {
                            let _ = reply.send(Err(e));
                        }
                    }
                }
                Command::Close(handle) => {
                    if let Some(window) = self.windows.remove(&handle) {
                        self.id_to_handle.remove(&window.id());
                    }
                }
            }
        }
    }
}

/// ユーザー定義イベント。ウェイクアップ専用で内容を持たない。
struct WakeUp;

impl ApplicationHandler<WakeUp> for App {
    fn resumed(&mut self, _event_loop: &ActiveEventLoop) {}

    fn user_event(&mut self, event_loop: &ActiveEventLoop, _event: WakeUp) {
        self.drain_commands(event_loop);
    }

    fn window_event(&mut self, _event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        if let WindowEvent::CloseRequested = event {
            if let Some(&handle) = self.id_to_handle.get(&id) {
                let _ = self.notices.send(Notice::ClosedByUser(handle));
            }
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.drain_commands(event_loop);
    }
}

struct EditorThread {
    commands: Sender<Command>,
    notices: Mutex<Receiver<Notice>>,
    proxy: EventLoopProxy<WakeUp>,
}

fn thread() -> &'static EditorThread {
    static THREAD: OnceLock<EditorThread> = OnceLock::new();
    THREAD.get_or_init(|| {
        let (command_tx, command_rx) = channel();
        let (notice_tx, notice_rx) = channel();
        let (proxy_tx, proxy_rx) = channel::<EventLoopProxy<WakeUp>>();

        std::thread::Builder::new()
            .name("neoutl-clap-editor-window".into())
            .spawn(move || {
                let event_loop = EventLoop::<WakeUp>::with_user_event()
                    .build()
                    .expect("winit event loop init failed (clap editor thread)");
                let _ = proxy_tx.send(event_loop.create_proxy());
                let mut app = App {
                    windows: HashMap::new(),
                    id_to_handle: HashMap::new(),
                    commands: command_rx,
                    notices: notice_tx,
                };
                let _ = event_loop.run_app(&mut app);
            })
            .expect("failed to spawn clap editor window thread");

        let proxy = proxy_rx
            .recv()
            .expect("clap editor window thread failed to start");

        EditorThread {
            commands: command_tx,
            notices: Mutex::new(notice_rx),
            proxy,
        }
    })
}

/// 新規ウィンドウを生成し、そのネイティブハンドルを返す。呼び出しは同期的にブロックする
/// （ウィンドウ生成完了、またはエラー確定まで）。
pub fn open(
    title: &str,
    width: u32,
    height: u32,
) -> Result<(EditorHandle, RawWindowHandle), PluginErrorLike> {
    let t = thread();
    let handle = NEXT_HANDLE.fetch_add(1, Ordering::Relaxed);
    let (reply_tx, reply_rx) = channel();

    t.commands
        .send(Command::Open {
            title: title.to_owned(),
            width,
            height,
            handle,
            reply: reply_tx,
        })
        .map_err(|_| PluginErrorLike("editor window thread is not running".into()))?;
    let _ = t.proxy.send_event(WakeUp);

    let raw = reply_rx
        .recv()
        .map_err(|_| PluginErrorLike("editor window thread reply lost".into()))?
        .map_err(PluginErrorLike)?;
    Ok((handle, raw.0))
}

/// ウィンドウを破棄する。`handle`が既に無効（ユーザーが閉じた等）でも安全。
pub fn close(handle: EditorHandle) {
    let t = thread();
    let _ = t.commands.send(Command::Close(handle));
    let _ = t.proxy.send_event(WakeUp);
}

/// `handle`のウィンドウがユーザー操作（閉じるボタン等）でクローズ要求を受けたか確認し、
/// 受けていればそのイベントを消費してtrueを返す。呼び出し側（`ClapWrapper::poll_editor`）
/// が毎フレーム呼ぶ想定。
pub fn take_closed_by_user(handle: EditorHandle) -> bool {
    let t = thread();
    let mut found = false;
    if let Ok(notices) = t.notices.lock() {
        while let Ok(notice) = notices.try_recv() {
            let Notice::ClosedByUser(h) = notice;
            if h == handle {
                found = true;
            }
        }
    }
    found
}

/// `crate::error::PluginError`への変換用の軽量ラッパー（本モジュールを`error.rs`非依存に
/// 保つため、文字列のみを運ぶ）。
pub struct PluginErrorLike(pub String);

impl From<PluginErrorLike> for crate::error::PluginError {
    fn from(e: PluginErrorLike) -> Self {
        crate::error::PluginError::Window(e.0)
    }
}
