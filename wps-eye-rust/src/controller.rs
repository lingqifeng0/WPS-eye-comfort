//! Background worker thread owning COM + NativeAgent + persistent dirty-session ledger.
//! Keeps UI & system tray 100% non-blocking and guarantees faithful restoration of prior WPS state.

use crate::native_agent::{
    AppKind, CanvasEyeConfig, NativeAgent, Target, all_wps_targets, all_wps_targets_for_restore,
    is_process_alive, is_window_visible, state_dir,
};
use crate::palette::{Adjustments, argb};
use crate::skin_builder::{DARK_SKIN_NAME, LIGHT_SKIN_FALLBACK, ensure_dark_skin};
use crate::wps_bridge::{WriterSession, enumerate_writer_sessions};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};
use windows::Win32::UI::WindowsAndMessaging::{IsWindow, PostMessageW, WM_APP};

pub const WM_APP_STATE_UPDATED: u32 = WM_APP + 10;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PageSnapshot {
    orig_protect_eyes: bool,
    orig_blue_screen: bool,
    orig_colors: BTreeMap<String, u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SkinSnapshot {
    orig_prome: String,
    orig_app: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DirtyEntry {
    pid: u32,
    created_100ns: u64,
    app_kind: String,
    page: Option<PageSnapshot>,
    skin: Option<SkinSnapshot>,
    #[serde(default)]
    canvas: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ProfileFile {
    page: Option<PageProfile>,
    #[serde(default)]
    mode1: Option<Adjustments>,
    #[serde(default)]
    mode2: Option<Adjustments>,
    #[serde(default)]
    active_dark_mode: Option<bool>,
    #[serde(default)]
    remember_skin: Option<bool>,
    #[serde(default)]
    remember_page: Option<bool>,
    #[serde(default)]
    remember_img: Option<bool>,
    #[serde(default)]
    img_brightness: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PageProfile {
    adjustments: Adjustments,
}

fn default_mode1() -> Adjustments {
    Adjustments {
        dark_mode: true,
        ..Adjustments::default()
    }
}

fn default_mode2() -> Adjustments {
    Adjustments {
        dark_mode: false,
        ..Adjustments::default()
    }
}

fn resolve_profile_modes(loaded: &ProfileFile) -> (bool, Adjustments, Adjustments) {
    let active_dark = loaded
        .active_dark_mode
        .or_else(|| loaded.page.as_ref().map(|p| p.adjustments.dark_mode))
        .unwrap_or(true);

    let mut m1 = loaded
        .mode1
        .or_else(|| {
            loaded
                .page
                .as_ref()
                .filter(|p| p.adjustments.dark_mode)
                .map(|p| p.adjustments)
        })
        .unwrap_or_else(default_mode1)
        .normalized();
    m1.dark_mode = true;

    let mut m2 = loaded
        .mode2
        .or_else(|| {
            loaded
                .page
                .as_ref()
                .filter(|p| !p.adjustments.dark_mode)
                .map(|p| p.adjustments)
        })
        .unwrap_or_else(default_mode2)
        .normalized();
    m2.dark_mode = false;

    (active_dark, m1, m2)
}

fn compute_canvas_cfg(
    page_enabled: bool,
    dark_mode: bool,
    mode1_adj: Adjustments,
    mode2_adj: Adjustments,
    img_enabled: bool,
    img_brightness: i32,
    app: AppKind,
) -> CanvasEyeConfig {
    let (bg, fg) = mode1_adj.palette();
    let dim_rgb = mode2_adj.soft_dim_rgb();
    let canvas_mode = if !page_enabled {
        0
    } else if dark_mode {
        match app {
            AppKind::Wps => 0, // Word Mode 1 uses native BlueScreen (mode=12)
            AppKind::Et => 1,  // Excel Mode 1 uses native dark sheet remapping
            AppKind::Wpp => 0, // PPT has no Mode 1: keep PPT slide untouched in Mode 1
        }
    } else {
        2 // Mode 2: Word + Excel + PPT all use optical CompositionMode_Multiply soft-dim
    };
    let (dark_bg, dark_fg) = if canvas_mode == 1 {
        (argb(bg), argb(fg))
    } else {
        (0xFF26_2C34, 0xFFE0_E4EA)
    };
    let eff_dim_rgb = if canvas_mode == 2 {
        dim_rgb
    } else {
        (184, 184, 184)
    };
    CanvasEyeConfig {
        canvas_mode,
        dim_rgb: eff_dim_rgb,
        dark_bg,
        dark_fg,
        img_enabled,
        img_brightness: img_brightness.clamp(25, 100),
    }
}

#[derive(Debug, Clone)]
pub struct UiSnapshot {
    pub skin_enabled: bool,
    pub page_enabled: bool,
    pub img_enabled: bool,
    pub img_brightness: i32,
    pub remember_skin: bool,
    pub remember_page: bool,
    pub remember_img: bool,
    pub dark_mode: bool,
    pub mode1_adj: Adjustments,
    pub mode2_adj: Adjustments,
    pub status_text: String,
    pub skin_msg: String,
    pub page_msg: String,
    pub error_text: Option<String>,
    pub wps_count: usize,
    pub wpp_count: usize,
    pub et_count: usize,
    pub dirty_count: usize,
    pub busy: bool,
}

impl Default for UiSnapshot {
    fn default() -> Self {
        Self {
            skin_enabled: false,
            page_enabled: false,
            img_enabled: false,
            img_brightness: 70,
            remember_skin: true,
            remember_page: false,
            remember_img: false,
            dark_mode: true,
            mode1_adj: default_mode1(),
            mode2_adj: default_mode2(),
            status_text: "正在连接 WPS Office…".into(),
            skin_msg: "原生 2025 深色皮肤覆盖文字 · PPT · 表格；保留原图标与顶部菜单。".into(),
            page_msg: "模式 1 仅文字/表格深底；模式 2 含 PPT 柔暗（参数独立保存）。".into(),
            error_text: None,
            wps_count: 0,
            wpp_count: 0,
            et_count: 0,
            dirty_count: 0,
            busy: false,
        }
    }
}

pub enum Command {
    SetSkinEnabled(bool),
    SetPageEnabled(bool),
    SetImgEnabled(bool),
    UpdateAdjustments {
        dark_mode: bool,
        mode1: Adjustments,
        mode2: Adjustments,
        apply_now: bool,
    },
    UpdateImgBrightness(i32, bool), // (brightness 25..=100, apply_now)
    RestoreAll,
    SyncTick,
    Barrier(mpsc::Sender<()>),
    RestoreAllAndExit(mpsc::Sender<(bool, String)>),
    Shutdown,
}

pub struct ControllerHandle {
    pub tx: mpsc::Sender<Command>,
    pub state: Arc<Mutex<UiSnapshot>>,
    notify_hwnd: Arc<Mutex<isize>>,
    worker: Option<thread::JoinHandle<()>>,
}

impl ControllerHandle {
    pub fn start(
        profiles_path: Option<PathBuf>,
        dirty_path: Option<PathBuf>,
        skin_cache_root: Option<PathBuf>,
    ) -> Result<Self, String> {
        let base = state_dir();
        let profiles_path = profiles_path.unwrap_or_else(|| base.join("v4-profiles.json"));
        let dirty_path = dirty_path.unwrap_or_else(|| base.join("v3-dirty-sessions.json"));
        let skin_cache_root = skin_cache_root.unwrap_or_else(|| base.join("skins"));

        // Verify agent eagerly on startup before spawning thread so errors surface cleanly.
        let agent = NativeAgent::new()?;

        let loaded_profile = load_profile(&profiles_path);
        let (init_dark_mode, init_mode1, init_mode2) = resolve_profile_modes(&loaded_profile);
        let init_rem_skin = loaded_profile.remember_skin;
        let init_rem_page = loaded_profile.remember_page;
        let init_rem_img = loaded_profile.remember_img;
        let init_img_brightness = loaded_profile.img_brightness.unwrap_or(70).clamp(25, 100);
        let initial_dirty = load_dirty(&dirty_path);
        let mut init_snap = UiSnapshot::default();
        init_snap.dark_mode = init_dark_mode;
        init_snap.mode1_adj = init_mode1;
        init_snap.mode2_adj = init_mode2;
        init_snap.img_brightness = init_img_brightness;
        init_snap.remember_skin = init_rem_skin.unwrap_or(true);
        init_snap.remember_page = init_rem_page.unwrap_or(false);
        init_snap.remember_img = init_rem_img.unwrap_or(false);
        init_snap.dirty_count = initial_dirty.len();
        if !initial_dirty.is_empty() {
            init_snap.status_text = format!(
                "检测到 {} 个上次未恢复的 WPS 窗口，点击“全部一键恢复”可还原。",
                initial_dirty.len()
            );
        }

        let state = Arc::new(Mutex::new(init_snap));
        let notify_hwnd = Arc::new(Mutex::new(0isize));
        let (tx, rx) = mpsc::channel::<Command>();

        let st_clone = Arc::clone(&state);
        let hwnd_clone = Arc::clone(&notify_hwnd);

        let worker = thread::Builder::new()
            .name("wps-eye-worker".into())
            .spawn(move || {
                unsafe {
                    let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
                }
                let mut core = WorkerCore {
                    agent,
                    profiles_path,
                    dirty_path,
                    skin_cache_root,
                    dirty: initial_dirty,
                    page_applied: HashMap::new(),
                    skin_applied: HashMap::new(),
                    canvas_applied: HashMap::new(),
                    writer_sessions: HashMap::new(),
                    blocked_page: HashSet::new(),
                    blocked_skin: HashSet::new(),
                    blocked_canvas: HashSet::new(),
                    skin_fail_counts: HashMap::new(),
                    page_fail_counts: HashMap::new(),
                    canvas_fail_counts: HashMap::new(),
                    skin_enabled: false,
                    page_enabled: false,
                    img_enabled: false,
                    img_brightness: init_img_brightness,
                    remember_skin: init_rem_skin,
                    remember_page: init_rem_page,
                    remember_img: init_rem_img,
                    dark_mode: init_dark_mode,
                    mode1_adj: init_mode1,
                    mode2_adj: init_mode2,
                    state: st_clone,
                    notify_hwnd: hwnd_clone,
                };
                core.run(rx);
                // Release all cached COM IDispatch pointers strictly BEFORE CoUninitialize()!
                drop(core);
                unsafe {
                    CoUninitialize();
                }
            })
            .map_err(|e| format!("启动后台线程失败: {e}"))?;

        Ok(Self {
            tx,
            state,
            notify_hwnd,
            worker: Some(worker),
        })
    }

    pub fn saved_auto_start_prefs(&self) -> (bool, bool, bool) {
        let s = self.snapshot();
        (s.remember_skin, s.remember_page, s.remember_img)
    }

    pub fn set_notify_hwnd(&self, hwnd: isize) {
        if let Ok(mut g) = self.notify_hwnd.lock() {
            *g = hwnd;
        }
    }

    pub fn snapshot(&self) -> UiSnapshot {
        self.state.lock().map(|g| g.clone()).unwrap_or_default()
    }

    pub fn send(&self, cmd: Command) {
        let _ = self.tx.send(cmd);
    }

    pub fn flush(&self) {
        let (tx, rx) = mpsc::channel();
        if self.tx.send(Command::Barrier(tx)).is_ok() {
            let _ = rx.recv();
        }
    }

    pub fn restore_all_and_wait(&self) -> (bool, String) {
        let (reply_tx, reply_rx) = mpsc::channel();
        if self.tx.send(Command::RestoreAllAndExit(reply_tx)).is_err() {
            return (true, String::new());
        }
        reply_rx.recv().unwrap_or((false, "后台恢复线程异常中断".into()))
    }

    pub fn shutdown(mut self) {
        let _ = self.tx.send(Command::Shutdown);
        if let Some(h) = self.worker.take() {
            let _ = h.join();
        }
    }
}

fn atomic_write_json<T: Serialize>(path: &Path, val: &T) {
    if let Some(p) = path.parent() {
        let _ = fs::create_dir_all(p);
    }
    if let Ok(text) = serde_json::to_string_pretty(val) {
        let tmp = path.with_extension("tmp");
        if fs::write(&tmp, text).is_ok() {
            let _ = fs::rename(&tmp, path);
        }
    }
}

fn load_profile(path: &Path) -> ProfileFile {
    fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str::<ProfileFile>(&s).ok())
        .unwrap_or_default()
}

fn is_hard_error(err: &str) -> bool {
    err.contains("版本")
        || err.contains("SHA")
        || err.contains("签名")
        || err.contains("校验")
        || err.contains("兼容")
        || err.contains("manifest")
        || err.contains("不支持")
}

fn load_dirty(path: &Path) -> BTreeMap<String, DirtyEntry> {
    let raw: BTreeMap<String, DirtyEntry> = fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    if raw.is_empty() {
        return raw;
    }
    let live_window_keys: HashSet<String> = all_wps_targets_for_restore(&HashMap::new())
        .into_iter()
        .map(|t| t.identity_key())
        .collect();
    let mut alive = BTreeMap::new();
    for (k, v) in &raw {
        if is_process_alive(v.pid, v.created_100ns)
            && live_window_keys.contains(k)
            && (v.page.is_some() || v.skin.is_some() || v.canvas)
        {
            alive.insert(k.clone(), v.clone());
        }
    }
    if alive.len() != raw.len() {
        if alive.is_empty() {
            let _ = fs::remove_file(path);
        } else {
            atomic_write_json(path, &alive);
        }
    }
    alive
}

struct WorkerCore {
    agent: NativeAgent,
    profiles_path: PathBuf,
    dirty_path: PathBuf,
    skin_cache_root: PathBuf,
    dirty: BTreeMap<String, DirtyEntry>,
    page_applied: HashMap<String, (u32, u32)>,
    skin_applied: HashMap<String, String>,
    canvas_applied: HashMap<String, CanvasEyeConfig>,
    /// Cached live COM Application sessions per Writer process (`{pid}:{created_100ns}`).
    /// Keeps BlueScreen/ProtectEyes restoration working even when all docx tabs are closed
    /// or when the user switches to a PPT/Excel tab in WPS All-in-One mode!
    writer_sessions: HashMap<String, WriterSession>,
    blocked_page: HashSet<String>,
    blocked_skin: HashSet<String>,
    blocked_canvas: HashSet<String>,
    skin_fail_counts: HashMap<String, u8>,
    page_fail_counts: HashMap<String, u8>,
    canvas_fail_counts: HashMap<String, u8>,
    skin_enabled: bool,
    page_enabled: bool,
    img_enabled: bool,
    img_brightness: i32,
    remember_skin: Option<bool>,
    remember_page: Option<bool>,
    remember_img: Option<bool>,
    dark_mode: bool,
    mode1_adj: Adjustments,
    mode2_adj: Adjustments,
    state: Arc<Mutex<UiSnapshot>>,
    notify_hwnd: Arc<Mutex<isize>>,
}

impl WorkerCore {
    fn notify_ui(&self) {
        if let Ok(g) = self.notify_hwnd.lock() {
            if *g != 0 {
                unsafe {
                    let _ = PostMessageW(Some(HWND(*g as *mut _)), WM_APP_STATE_UPDATED, WPARAM(0), LPARAM(0));
                }
            }
        }
    }

    fn update_ui<F: FnOnce(&mut UiSnapshot)>(&self, f: F) {
        if let Ok(mut g) = self.state.lock() {
            g.skin_enabled = self.skin_enabled;
            g.page_enabled = self.page_enabled;
            g.img_enabled = self.img_enabled;
            g.img_brightness = self.img_brightness;
            g.remember_skin = self.remember_skin.unwrap_or(true);
            g.remember_page = self.remember_page.unwrap_or(false);
            g.remember_img = self.remember_img.unwrap_or(false);
            g.dark_mode = self.dark_mode;
            g.mode1_adj = self.mode1_adj;
            g.mode2_adj = self.mode2_adj;
            g.dirty_count = self.dirty.len();
            f(&mut g);
        }
        self.notify_ui();
    }

    fn save_profile(&self) {
        let mut m1 = self.mode1_adj.normalized();
        m1.dark_mode = true;
        let mut m2 = self.mode2_adj.normalized();
        m2.dark_mode = false;
        let active_adj = if self.dark_mode { m1 } else { m2 };
        atomic_write_json(
            &self.profiles_path,
            &ProfileFile {
                page: Some(PageProfile {
                    adjustments: active_adj,
                }),
                mode1: Some(m1),
                mode2: Some(m2),
                active_dark_mode: Some(self.dark_mode),
                remember_skin: self.remember_skin,
                remember_page: self.remember_page,
                remember_img: self.remember_img,
                img_brightness: Some(self.img_brightness.clamp(25, 100)),
            },
        );
    }

    fn save_dirty(&self) {
        if self.dirty.is_empty() {
            let _ = fs::remove_file(&self.dirty_path);
        } else {
            atomic_write_json(&self.dirty_path, &self.dirty);
        }
    }

    fn prune_dead_dirty(&mut self) {
        if self.dirty.is_empty() {
            return;
        }
        // WPS frequently leaves windowless background stub processes (e.g. et.exe with 0 windows)
        // alive after the user closes the window. Prune both exited PIDs and closed/windowless stubs.
        let live_window_keys: HashSet<String> = all_wps_targets_for_restore(&HashMap::new())
            .into_iter()
            .map(|t| t.identity_key())
            .collect();
        let dead: Vec<String> = self
            .dirty
            .iter()
            .filter(|(k, v)| !is_process_alive(v.pid, v.created_100ns) || !live_window_keys.contains(*k))
            .map(|(k, _)| k.clone())
            .collect();
        if !dead.is_empty() {
            for k in dead {
                self.dirty.remove(&k);
                self.page_applied.remove(&k);
                self.skin_applied.remove(&k);
                self.canvas_applied.remove(&k);
                self.writer_sessions.remove(&k);
            }
            self.save_dirty();
        }
        self.writer_sessions
            .retain(|k, _| live_window_keys.contains(k));
    }

    fn run(&mut self, rx: mpsc::Receiver<Command>) {
        // Initial read-only scan
        self.sync(false, false);

        while let Ok(cmd) = rx.recv() {
            // Coalesce burst slider updates cleanly
            let mut active_cmd = cmd;
            if matches!(
                active_cmd,
                Command::UpdateAdjustments { .. } | Command::UpdateImgBrightness(_, _)
            ) {
                while let Ok(next) = rx.try_recv() {
                    match next {
                        Command::UpdateAdjustments {
                            dark_mode,
                            mode1,
                            mode2,
                            apply_now,
                        } if matches!(active_cmd, Command::UpdateAdjustments { .. }) => {
                            let prev_apply = matches!(
                                active_cmd,
                                Command::UpdateAdjustments {
                                    apply_now: true,
                                    ..
                                }
                            );
                            active_cmd = Command::UpdateAdjustments {
                                dark_mode,
                                mode1,
                                mode2,
                                apply_now: apply_now || prev_apply,
                            };
                        }
                        Command::UpdateImgBrightness(b, apply)
                            if matches!(active_cmd, Command::UpdateImgBrightness(_, _)) =>
                        {
                            let prev_apply = matches!(active_cmd, Command::UpdateImgBrightness(_, true));
                            active_cmd = Command::UpdateImgBrightness(b, apply || prev_apply);
                        }
                        other => {
                            self.handle_cmd(active_cmd);
                            active_cmd = other;
                            break;
                        }
                    }
                }
            }
            if !self.handle_cmd(active_cmd) {
                break;
            }
        }
    }

    fn handle_cmd(&mut self, cmd: Command) -> bool {
        match cmd {
            Command::SetSkinEnabled(true) => {
                self.skin_enabled = true;
                self.remember_skin = Some(true);
                self.blocked_skin.clear();
                self.skin_fail_counts.clear();
                self.save_profile();
                self.update_ui(|s| {
                    s.busy = true;
                    s.error_text = None;
                });
                self.sync(true, false);
                self.update_ui(|s| s.busy = false);
            }
            Command::SetSkinEnabled(false) => {
                self.skin_enabled = false;
                self.remember_skin = Some(false);
                self.blocked_skin.clear();
                self.skin_fail_counts.clear();
                self.save_profile();
                self.update_ui(|s| {
                    s.busy = true;
                    s.error_text = None;
                });
                let (ok, msg) = self.restore_skin();
                self.update_ui(|s| {
                    s.busy = false;
                    s.skin_msg = msg.clone();
                    if !ok {
                        s.error_text = Some(msg.clone());
                        s.status_text = msg;
                    } else {
                        s.error_text = None;
                        if !s.page_enabled {
                            s.status_text = msg;
                        }
                    }
                });
            }
            Command::SetPageEnabled(true) => {
                self.page_enabled = true;
                self.remember_page = Some(true);
                self.blocked_page.clear();
                self.blocked_canvas.clear();
                self.page_fail_counts.clear();
                self.canvas_fail_counts.clear();
                self.save_profile();
                self.update_ui(|s| {
                    s.busy = true;
                    s.error_text = None;
                });
                self.sync(false, true);
                self.update_ui(|s| s.busy = false);
            }
            Command::SetPageEnabled(false) => {
                self.page_enabled = false;
                self.remember_page = Some(false);
                self.blocked_page.clear();
                self.blocked_canvas.clear();
                self.page_fail_counts.clear();
                self.canvas_fail_counts.clear();
                self.save_profile();
                self.update_ui(|s| {
                    s.busy = true;
                    s.error_text = None;
                });
                let (ok, msg) = self.restore_page();
                self.sync(false, true);
                self.update_ui(|s| {
                    s.busy = false;
                    s.page_msg = msg.clone();
                    if !ok {
                        s.error_text = Some(msg.clone());
                        s.status_text = msg;
                    } else if s.error_text.is_none() && !s.skin_enabled && !s.img_enabled {
                        s.status_text = msg;
                    }
                });
            }
            Command::SetImgEnabled(on) => {
                self.img_enabled = on;
                self.remember_img = Some(on);
                self.blocked_canvas.clear();
                self.canvas_fail_counts.clear();
                self.save_profile();
                self.update_ui(|s| {
                    s.busy = true;
                    s.error_text = None;
                });
                self.sync(false, true);
                self.update_ui(|s| s.busy = false);
            }
            Command::UpdateAdjustments {
                dark_mode,
                mode1,
                mode2,
                apply_now,
            } => {
                self.dark_mode = dark_mode;
                let mut m1 = mode1.normalized();
                m1.dark_mode = true;
                let mut m2 = mode2.normalized();
                m2.dark_mode = false;
                self.mode1_adj = m1;
                self.mode2_adj = m2;
                self.save_profile();
                self.update_ui(|_| {});
                if apply_now && self.page_enabled {
                    // Switching to Mode 2 (soft_dim) on Writer: restore BlueScreen first if active!
                    if !self.dark_mode && self.dirty.values().any(|e| e.page.is_some()) {
                        let (ok, msg) = self.restore_page();
                        if !ok {
                            self.update_ui(|s| {
                                s.error_text = Some(msg.clone());
                                s.status_text = msg;
                            });
                        }
                    }
                    self.sync(false, false);
                }
            }
            Command::UpdateImgBrightness(val, apply_now) => {
                self.img_brightness = val.clamp(25, 100);
                self.save_profile();
                self.update_ui(|_| {});
                if apply_now && self.img_enabled {
                    self.sync(false, true);
                }
            }
            Command::RestoreAll => {
                self.skin_enabled = false;
                self.page_enabled = false;
                self.img_enabled = false;
                self.remember_skin = Some(false);
                self.remember_page = Some(false);
                self.remember_img = Some(false);
                self.blocked_page.clear();
                self.blocked_skin.clear();
                self.blocked_canvas.clear();
                self.skin_fail_counts.clear();
                self.page_fail_counts.clear();
                self.canvas_fail_counts.clear();
                self.save_profile();
                self.update_ui(|s| {
                    s.busy = true;
                    s.error_text = None;
                });
                let (ok_c, msg_c) = self.restore_canvas();
                let (ok_p, msg_p) = self.restore_page();
                let (ok_s, msg_s) = self.restore_skin();
                self.update_ui(|s| {
                    s.busy = false;
                    s.page_msg = if ok_p && ok_c { msg_p.clone() } else if !ok_c { msg_c.clone() } else { msg_p.clone() };
                    s.skin_msg = msg_s.clone();
                    if ok_c && ok_p && ok_s {
                        s.error_text = None;
                        s.status_text = "已安全恢复：外观、画布与贴图均已忠实还原。".into();
                    } else {
                        let err = if !ok_c { msg_c } else if !ok_p { msg_p } else { msg_s };
                        s.error_text = Some(err.clone());
                        s.status_text = err;
                    }
                });
            }
            Command::SyncTick => {
                self.sync(false, false);
            }
            Command::Barrier(reply) => {
                let _ = reply.send(());
            }
            Command::RestoreAllAndExit(reply) => {
                self.skin_enabled = false;
                self.page_enabled = false;
                self.img_enabled = false;
                self.blocked_page.clear();
                self.blocked_skin.clear();
                self.blocked_canvas.clear();
                let (ok_c, msg_c) = self.restore_canvas();
                let (ok_p, msg_p) = self.restore_page();
                let (ok_s, msg_s) = self.restore_skin();
                let ok = ok_c && ok_p && ok_s;
                let msg = if ok {
                    "已全部恢复并退出。".into()
                } else if !ok_c {
                    msg_c
                } else if !ok_p {
                    msg_p
                } else {
                    msg_s
                };
                self.update_ui(|s| {
                    if !ok {
                        s.error_text = Some(msg.clone());
                        s.status_text = msg.clone();
                    }
                });
                let _ = reply.send((ok, msg));
                if ok {
                    return false;
                }
            }
            Command::Shutdown => return false,
        }
        true
    }

    fn apply_skin_target(&mut self, target: &Target, force: bool) -> Result<(), String> {
        let key = target.identity_key();
        if !force && self.skin_applied.get(&key).map(|s| s.as_str()) == Some(DARK_SKIN_NAME) {
            return Ok(());
        }
        let office6 = self.agent.verify_target(target, false)?;
        let (skin_name, search_root) = ensure_dark_skin(&office6, &self.skin_cache_root)?;

        let is_first = self.dirty.get(&key).and_then(|e| e.skin.as_ref()).is_none();
        let snap = if is_first {
            let cur = self.agent.query_skin(target)?;
            let orig_prome = if !cur.prome.is_empty() && cur.prome != DARK_SKIN_NAME {
                cur.prome
            } else {
                LIGHT_SKIN_FALLBACK.to_string()
            };
            let orig_app = if !cur.app.is_empty() && cur.app != DARK_SKIN_NAME {
                cur.app
            } else {
                LIGHT_SKIN_FALLBACK.to_string()
            };
            let s = SkinSnapshot { orig_prome, orig_app };
            let entry = self.dirty.entry(key.clone()).or_insert_with(|| DirtyEntry {
                pid: target.pid,
                created_100ns: target.created_100ns,
                app_kind: target.app_kind.as_str().to_string(),
                page: None,
                skin: None,
                canvas: false,
            });
            entry.skin = Some(s.clone());
            self.save_dirty();
            s
        } else {
            self.dirty[&key].skin.clone().unwrap()
        };

        let search_str = search_root.to_string_lossy().into_owned();
        let res = (|| -> Result<(), String> {
            let st = self.agent.switch_skin(target, &skin_name, &search_str)?;
            if st.prome != skin_name || st.app != skin_name || st.prome_dark != 1 {
                return Err(format!(
                    "WPS 原生深色皮肤回读不一致（prome={}, app={}）。",
                    st.prome, st.app
                ));
            }
            self.skin_applied.insert(key.clone(), skin_name);
            Ok(())
        })();

        if res.is_err() && is_first {
            if let Ok(rb) = self.agent.switch_skin(target, &snap.orig_prome, "") {
                if rb.prome == snap.orig_prome {
                    if let Some(e) = self.dirty.get_mut(&key) {
                        e.skin = None;
                        if e.page.is_none() && !e.canvas {
                            self.dirty.remove(&key);
                        }
                    }
                    self.skin_applied.remove(&key);
                    self.save_dirty();
                }
            }
        }
        res
    }

    fn apply_page_target(
        &mut self,
        target: &Target,
        session: &WriterSession,
        desired: (u32, u32),
        force: bool,
    ) -> Result<(), String> {
        let key = target.identity_key();
        // Read-only version check BEFORE touching ProtectEyes or BlueScreen!
        self.agent.verify_target(target, true)?;

        let is_first = self.dirty.get(&key).and_then(|e| e.page.as_ref()).is_none();
        let snap = if is_first {
            let orig_protect = session.get_protect_eyes()?;
            let orig_blue = session.get_blue_screen()?;
            let orig_cols = self.agent.colors(target, None)?;
            let mut map = BTreeMap::new();
            for (k, v) in orig_cols {
                map.insert(format!("{k:08x}"), v);
            }
            let s = PageSnapshot {
                orig_protect_eyes: orig_protect,
                orig_blue_screen: orig_blue,
                orig_colors: map,
            };
            let entry = self.dirty.entry(key.clone()).or_insert_with(|| DirtyEntry {
                pid: target.pid,
                created_100ns: target.created_100ns,
                app_kind: target.app_kind.as_str().to_string(),
                page: None,
                skin: None,
                canvas: false,
            });
            entry.page = Some(s.clone());
            self.save_dirty();
            s
        } else {
            self.dirty[&key].page.clone().unwrap()
        };

        let res = (|| -> Result<(), String> {
            session.set_protect_eyes(false)?;
            if force || self.page_applied.get(&key).copied() != Some(desired) {
                let pairs = [(0xE001_0031u32, desired.0), (0xE001_0032u32, desired.1)];
                let rb = self.agent.colors(target, Some(&pairs))?;
                if rb.get(&0xE001_0031).copied() != Some(desired.0)
                    || rb.get(&0xE001_0032).copied() != Some(desired.1)
                {
                    return Err("页内颜色回读不一致，已中止应用。".into());
                }
            }
            if !session.get_blue_screen()? {
                session.set_blue_screen(true)?;
            }
            self.page_applied.insert(key.clone(), desired);
            Ok(())
        })();

        if res.is_err() && is_first {
            let orig_pairs: Vec<(u32, u32)> = snap
                .orig_colors
                .iter()
                .filter_map(|(k, &v)| u32::from_str_radix(k, 16).ok().map(|id| (id, v)))
                .collect();
            let rb_ok = self.agent.colors(target, Some(&orig_pairs)).is_ok()
                && session.set_blue_screen(snap.orig_blue_screen).is_ok()
                && session.set_protect_eyes(snap.orig_protect_eyes).is_ok();
            if rb_ok {
                if let Some(e) = self.dirty.get_mut(&key) {
                    e.page = None;
                    if e.skin.is_none() {
                        self.dirty.remove(&key);
                    }
                }
                self.page_applied.remove(&key);
                self.save_dirty();
            }
        }
        res
    }

    fn desired_canvas_cfg(&self, app: AppKind) -> CanvasEyeConfig {
        compute_canvas_cfg(
            self.page_enabled,
            self.dark_mode,
            self.mode1_adj,
            self.mode2_adj,
            self.img_enabled,
            self.img_brightness,
            app,
        )
    }

    fn apply_canvas_target(&mut self, target: &Target, cfg: CanvasEyeConfig, force: bool) -> Result<(), String> {
        let key = target.identity_key();
        let is_active = cfg.canvas_mode != 0 || (cfg.img_enabled && cfg.img_brightness < 100);
        let was_dirty = self.dirty.get(&key).map(|e| e.canvas).unwrap_or(false);
        if !is_active && !was_dirty && !self.canvas_applied.contains_key(&key) {
            return Ok(());
        }
        if !force && self.canvas_applied.get(&key).copied() == Some(cfg) {
            return Ok(());
        }
        self.agent.verify_target(target, false)?;
        let _ = self.agent.set_canvas_eye(target, cfg)?;
        if is_active {
            let entry = self.dirty.entry(key.clone()).or_insert_with(|| DirtyEntry {
                pid: target.pid,
                created_100ns: target.created_100ns,
                app_kind: target.app_kind.as_str().to_string(),
                page: None,
                skin: None,
                canvas: false,
            });
            entry.canvas = true;
            self.canvas_applied.insert(key, cfg);
        } else {
            if let Some(e) = self.dirty.get_mut(&key) {
                e.canvas = false;
                if e.page.is_none() && e.skin.is_none() {
                    self.dirty.remove(&key);
                }
            }
            self.canvas_applied.remove(&key);
        }
        self.save_dirty();
        Ok(())
    }

    fn restore_canvas(&mut self) -> (bool, String) {
        self.prune_dead_dirty();
        let keys: Vec<String> = self
            .dirty
            .iter()
            .filter(|(_, e)| e.canvas)
            .map(|(k, _)| k.clone())
            .collect();
        if keys.is_empty() {
            return (true, "画布与贴图渲染未改动，已保持原样。".into());
        }
        let live: HashMap<String, Target> = all_wps_targets_for_restore(&HashMap::new())
            .into_iter()
            .map(|t| (t.identity_key(), t))
            .collect();
        let off_cfg = CanvasEyeConfig {
            canvas_mode: 0,
            img_enabled: false,
            img_brightness: 100,
            ..CanvasEyeConfig::default()
        };
        let mut errors = Vec::new();
        let mut restored = 0usize;
        for key in keys {
            let Some(entry) = self.dirty.get(&key).cloned() else {
                continue;
            };
            let clear_canvas = |this: &mut Self, k: &str| {
                if let Some(e) = this.dirty.get_mut(k) {
                    e.canvas = false;
                    if e.page.is_none() && e.skin.is_none() {
                        this.dirty.remove(k);
                    }
                }
                this.canvas_applied.remove(k);
            };
            if !is_process_alive(entry.pid, entry.created_100ns) {
                clear_canvas(self, &key);
                continue;
            }
            let Some(target) = live.get(&key) else {
                // Window already closed (WPS left a windowless background stub): nothing to restore.
                clear_canvas(self, &key);
                continue;
            };
            match self.agent.set_canvas_eye(target, off_cfg) {
                Ok(_) => {
                    clear_canvas(self, &key);
                    restored += 1;
                }
                Err(e) => {
                    if !is_window_visible(target.hwnd) {
                        clear_canvas(self, &key);
                    } else {
                        errors.push(e);
                    }
                }
            }
        }
        self.save_dirty();
        if let Some(err) = errors.into_iter().next() {
            (false, err)
        } else if restored == 0 {
            (true, "原 WPS 窗口已关闭，画布状态已清理完毕。".into())
        } else {
            (true, format!("已还原 {restored} 个 WPS 窗口的原生画布与贴图渲染。"))
        }
    }

    fn sync(&mut self, force_skin: bool, force_page: bool) {
        self.prune_dead_dirty();
        let sessions = enumerate_writer_sessions();
        let mut sessions_by_pid: HashMap<u32, WriterSession> = HashMap::new();
        let mut pref_hwnds: HashMap<u32, isize> = HashMap::new();
        for s in sessions {
            pref_hwnds.insert(s.pid, s.hwnd.0 as isize);
            sessions_by_pid.insert(s.pid, s);
        }

        let targets = all_wps_targets(&pref_hwnds);
        let all_restore_targets = all_wps_targets_for_restore(&pref_hwnds);
        let all_restore_keys: HashSet<String> = all_restore_targets
            .iter()
            .map(|t| t.identity_key())
            .collect();
        let alive_keys: HashSet<String> = targets.iter().map(|t| t.identity_key()).collect();
        self.page_applied.retain(|k, _| all_restore_keys.contains(k));
        self.skin_applied.retain(|k, _| all_restore_keys.contains(k));
        self.canvas_applied.retain(|k, _| all_restore_keys.contains(k));
        self.writer_sessions.retain(|k, _| all_restore_keys.contains(k));
        self.skin_fail_counts.retain(|k, _| alive_keys.contains(k));
        self.page_fail_counts.retain(|k, _| alive_keys.contains(k));
        self.canvas_fail_counts.retain(|k, _| alive_keys.contains(k));

        // Cache any freshly discovered Writer COM sessions (even on hidden/inactive tabs!)
        for t in &all_restore_targets {
            if t.app_kind == AppKind::Wps {
                if let Some(sess) = sessions_by_pid.get(&t.pid) {
                    self.writer_sessions.insert(t.identity_key(), sess.clone());
                }
            }
        }

        let mut wps_c = 0usize;
        let mut wpp_c = 0usize;
        let mut et_c = 0usize;
        let mut writer_docs = 0usize;
        let mut errors: Vec<String> = Vec::new();

        let (bg, fg) = self.mode1_adj.palette();
        let desired_colors = (argb(bg), argb(fg));

        for t in &targets {
            match t.app_kind {
                AppKind::Wps => wps_c += 1,
                AppKind::Wpp => wpp_c += 1,
                AppKind::Et => et_c += 1,
            }
            let key = t.identity_key();

            if self.skin_enabled && !self.blocked_skin.contains(&key) {
                match self.apply_skin_target(t, force_skin) {
                    Ok(()) => {
                        self.skin_fail_counts.remove(&key);
                    }
                    Err(err) => {
                        if is_hard_error(&err) {
                            if unsafe { IsWindow(Some(HWND(t.hwnd as *mut _))).as_bool() } {
                                self.blocked_skin.insert(key.clone());
                            }
                            errors.push(err);
                        } else {
                            let c = self.skin_fail_counts.entry(key.clone()).or_insert(0);
                            *c = c.saturating_add(1);
                            if force_skin || *c >= 4 {
                                if *c >= 4 && unsafe { IsWindow(Some(HWND(t.hwnd as *mut _))).as_bool() } {
                                    self.blocked_skin.insert(key.clone());
                                }
                                errors.push(err);
                            }
                        }
                    }
                }
            }

            // Word Mode 1 (dark_mode == true): native BlueScreen 2-color display options
            // Word Mode 2 (dark_mode == false): ensure BlueScreen is OFF so optical soft-dim doesn't darken dark bg!
            if t.app_kind == AppKind::Wps {
                let active_sess = sessions_by_pid
                    .get(&t.pid)
                    .cloned()
                    .or_else(|| self.writer_sessions.get(&key).cloned());
                if let Some(sess) = active_sess {
                    let has_docs = sess.doc_count >= 1 || sess.live_doc_count() >= 1;
                    if has_docs {
                        writer_docs += 1;
                    }
                    if self.page_enabled && self.dark_mode && has_docs && !self.blocked_page.contains(&key) {
                        match self.apply_page_target(t, &sess, desired_colors, force_page) {
                            Ok(()) => {
                                self.page_fail_counts.remove(&key);
                            }
                            Err(err) => {
                                if is_hard_error(&err) {
                                    if unsafe { IsWindow(Some(HWND(t.hwnd as *mut _))).as_bool() } {
                                        self.blocked_page.insert(key.clone());
                                    }
                                    errors.push(err);
                                } else {
                                    let c = self.page_fail_counts.entry(key.clone()).or_insert(0);
                                    *c = c.saturating_add(1);
                                    if force_page || *c >= 4 {
                                        if *c >= 4 && unsafe { IsWindow(Some(HWND(t.hwnd as *mut _))).as_bool() } {
                                            self.blocked_page.insert(key.clone());
                                        }
                                        errors.push(err);
                                    }
                                }
                            }
                        }
                    } else if self.page_enabled && !self.dark_mode {
                        if self.dirty.get(&key).and_then(|e| e.page.as_ref()).is_some() {
                            let _ = self.restore_page();
                        } else if let Ok(true) = sess.get_blue_screen() {
                            let _ = sess.set_blue_screen(false);
                        }
                    }
                }
            }

            // Unified native canvas (Mode 1 Excel dark / Mode 2 Word+Excel+PPT multiply) + independent image dimming
            if !self.blocked_canvas.contains(&key) {
                let desired_cfg = self.desired_canvas_cfg(t.app_kind);
                match self.apply_canvas_target(t, desired_cfg, force_page) {
                    Ok(()) => {
                        self.canvas_fail_counts.remove(&key);
                    }
                    Err(err) => {
                        if is_hard_error(&err) {
                            if unsafe { IsWindow(Some(HWND(t.hwnd as *mut _))).as_bool() } {
                                self.blocked_canvas.insert(key.clone());
                            }
                            errors.push(err);
                        } else {
                            let c = self.canvas_fail_counts.entry(key.clone()).or_insert(0);
                            *c = c.saturating_add(1);
                            if force_page || *c >= 4 {
                                if *c >= 4 && unsafe { IsWindow(Some(HWND(t.hwnd as *mut _))).as_bool() } {
                                    self.blocked_canvas.insert(key.clone());
                                }
                                errors.push(err);
                            }
                        }
                    }
                }
            }
        }

        let dirty_n = self.dirty.len();
        self.update_ui(|s| {
            s.wps_count = wps_c;
            s.wpp_count = wpp_c;
            s.et_count = et_c;
            if let Some(first_err) = errors.first() {
                s.error_text = Some(first_err.clone());
                s.status_text = first_err.clone();
                return;
            }
            s.error_text = None;
            if targets.is_empty() {
                s.status_text = if dirty_n > 0 {
                    format!("检测到 {dirty_n} 个待恢复 WPS 进程，暂未找到可见窗口。")
                } else {
                    "等待打开 WPS 文字 / 演示（PPT） / 表格窗口…".into()
                };
                return;
            }
            let mut parts = Vec::new();
            if wps_c > 0 {
                parts.push(format!("文字 {wps_c}"));
            }
            if wpp_c > 0 {
                parts.push(format!("PPT {wpp_c}"));
            }
            if et_c > 0 {
                parts.push(format!("表格 {et_c}"));
            }
            let apps = parts.join(" · ");
            let skin_st = if self.skin_enabled { "深色外观" } else { "原版外观" };
            let page_st = if self.page_enabled {
                if self.dark_mode { "深底模式" } else { "夜间柔暗" }
            } else {
                "原版画布"
            };
            let img_st = if self.img_enabled {
                format!(" · 贴图 {}%", self.img_brightness)
            } else {
                String::new()
            };
            s.status_text = format!("已连接 {apps}  |  {skin_st} · {page_st}{img_st}");
            if self.skin_enabled {
                s.skin_msg = format!("已应用 2025 原生深色皮肤（{apps}），菜单与图标保持原生渲染。");
            }
            if self.page_enabled {
                if self.dark_mode {
                    let w_note = if wps_c > 0 && writer_docs == 0 { "（文字请打开文档标签）" } else { "" };
                    s.page_msg = format!("深底运行中：仅文字/表格生效，PPT 保持原画布{w_note}");
                } else {
                    s.page_msg = format!("柔暗运行中：{apps} 均使用模式 2 参数（不反黑、零闪屏）");
                }
            } else {
                s.page_msg = "模式 1 仅文字/表格深底；模式 2 含 PPT 柔暗（参数独立保存）。".into();
            }
        });
    }

    fn restore_page(&mut self) -> (bool, String) {
        self.prune_dead_dirty();
        let keys: Vec<String> = self
            .dirty
            .iter()
            .filter(|(_, e)| e.page.is_some())
            .map(|(k, _)| k.clone())
            .collect();
        if keys.is_empty() {
            return (true, "本次未修改 WPS 页面设置，已保留您原有的护眼/显示状态。".into());
        }

        let sessions = enumerate_writer_sessions();
        let mut sessions_by_pid: HashMap<u32, WriterSession> = HashMap::new();
        let mut pref_hwnds: HashMap<u32, isize> = HashMap::new();
        for s in sessions {
            pref_hwnds.insert(s.pid, s.hwnd.0 as isize);
            sessions_by_pid.insert(s.pid, s);
        }
        let writer_targets: HashMap<String, Target> = all_wps_targets_for_restore(&pref_hwnds)
            .into_iter()
            .filter(|t| t.app_kind == AppKind::Wps)
            .map(|t| (t.identity_key(), t))
            .collect();

        let mut errors = Vec::new();
        let mut restored = 0usize;

        for key in keys {
            let Some(entry) = self.dirty.get(&key).cloned() else {
                continue;
            };
            let Some(snap) = entry.page else {
                continue;
            };
            let clear_page = |this: &mut Self, k: &str| {
                if let Some(e) = this.dirty.get_mut(k) {
                    e.page = None;
                    if e.skin.is_none() && !e.canvas {
                        this.dirty.remove(k);
                    }
                }
                this.page_applied.remove(k);
            };
            if !is_process_alive(entry.pid, entry.created_100ns)
                || (!writer_targets.contains_key(&key) && !sessions_by_pid.contains_key(&entry.pid))
            {
                clear_page(self, &key);
                continue;
            }

            let orig_pairs: Vec<(u32, u32)> = snap
                .orig_colors
                .iter()
                .filter_map(|(k, &v)| u32::from_str_radix(k, 16).ok().map(|id| (id, v)))
                .collect();

            if let Some(target) = writer_targets.get(&key) {
                match self.agent.colors(target, Some(&orig_pairs)) {
                    Ok(rb) => {
                        let all_match = orig_pairs.iter().all(|&(id, v)| rb.get(&id).copied() == Some(v));
                        if !all_match {
                            if !is_window_visible(target.hwnd) {
                                clear_page(self, &key);
                                continue;
                            }
                            errors.push("页内颜色还原回读不一致。".into());
                            continue;
                        }
                    }
                    Err(e) => {
                        if !is_window_visible(target.hwnd) {
                            clear_page(self, &key);
                            continue;
                        }
                        errors.push(e);
                        continue;
                    }
                }
            }

            let Some(session) = sessions_by_pid
                .get(&entry.pid)
                .cloned()
                .or_else(|| self.writer_sessions.get(&key).cloned())
            else {
                if writer_targets
                    .get(&key)
                    .map(|t| !is_window_visible(t.hwnd))
                    .unwrap_or(true)
                {
                    clear_page(self, &key);
                    continue;
                }
                errors.push(format!(
                    "WPS 文字进程（PID {}）仍在运行但未打开文档标签；请新建空白文档后重试恢复，或关闭该窗口。",
                    entry.pid
                ));
                continue;
            };

            // Independently restore exact pre-apply BlueScreen and ProtectEyes.
            // Note: BlueScreen works even at 0 open documents on a cached COM Application;
            // if ProtectEyes was already false (orig_protect_eyes == false), only flip if currently pressed.
            let res = session
                .set_blue_screen(snap.orig_blue_screen)
                .and_then(|_| session.set_protect_eyes(snap.orig_protect_eyes));
            match res {
                Ok(()) => {
                    clear_page(self, &key);
                    restored += 1;
                }
                Err(e) => {
                    // Evict potentially disconnected cached COM object so next attempt re-acquires cleanly.
                    if !sessions_by_pid.contains_key(&entry.pid) {
                        self.writer_sessions.remove(&key);
                    }
                    errors.push(e);
                }
            }
        }

        self.save_dirty();
        if let Some(err) = errors.into_iter().next() {
            (false, err)
        } else if restored == 0 {
            (true, "原 WPS 文字窗口已关闭，页面状态已清理完毕。".into())
        } else {
            (true, format!("已忠实还原 {restored} 个 WPS 文字窗口启用前的页面与护眼状态。"))
        }
    }

    fn restore_skin(&mut self) -> (bool, String) {
        self.prune_dead_dirty();
        let keys: Vec<String> = self
            .dirty
            .iter()
            .filter(|(_, e)| e.skin.is_some())
            .map(|(k, _)| k.clone())
            .collect();
        if keys.is_empty() {
            return (true, "本次未修改 WPS 外观皮肤，已保持原样。".into());
        }

        let live: HashMap<String, Target> = all_wps_targets_for_restore(&HashMap::new())
            .into_iter()
            .map(|t| (t.identity_key(), t))
            .collect();

        let mut errors = Vec::new();
        let mut restored = 0usize;

        for key in keys {
            let Some(entry) = self.dirty.get(&key).cloned() else {
                continue;
            };
            let Some(snap) = entry.skin else {
                continue;
            };
            let clear_skin = |this: &mut Self, k: &str| {
                if let Some(e) = this.dirty.get_mut(k) {
                    e.skin = None;
                    if e.page.is_none() && !e.canvas {
                        this.dirty.remove(k);
                    }
                }
                this.skin_applied.remove(k);
            };
            if !is_process_alive(entry.pid, entry.created_100ns) {
                clear_skin(self, &key);
                continue;
            }
            let Some(target) = live.get(&key) else {
                // User already closed the WPS window (only windowless background stub remains):
                // clear stale dirty entry cleanly instead of deadlocking restore/exit!
                clear_skin(self, &key);
                continue;
            };
            let orig = if snap.orig_prome.is_empty() {
                LIGHT_SKIN_FALLBACK
            } else {
                snap.orig_prome.as_str()
            };
            match self.agent.switch_skin(target, orig, "") {
                Ok(st) if st.prome == orig && st.app == orig => {
                    clear_skin(self, &key);
                    restored += 1;
                }
                Ok(st) => {
                    if !is_window_visible(target.hwnd) {
                        clear_skin(self, &key);
                    } else {
                        errors.push(format!("WPS 外观恢复回读不一致（{}/{}）。", st.prome, st.app));
                    }
                }
                Err(e) => {
                    if !is_window_visible(target.hwnd) {
                        clear_skin(self, &key);
                    } else {
                        errors.push(e);
                    }
                }
            }
        }

        self.save_dirty();
        if let Some(err) = errors.into_iter().next() {
            (false, err)
        } else if restored == 0 {
            (true, "原 WPS 窗口已关闭，深色外观已清理完毕。".into())
        } else {
            (true, format!("已实时还原 {restored} 个 WPS 窗口原来的原生皮肤。"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_mode1_profile_migrates_without_polluting_mode2() {
        let json = r#"{
            "page": {
                "adjustments": { "brightness": 117, "contrast": 105, "sepia": 15, "grayscale": 0, "dark_mode": true }
            },
            "remember_skin": true
        }"#;
        let loaded: ProfileFile = serde_json::from_str(json).unwrap();
        let (dark, m1, m2) = resolve_profile_modes(&loaded);
        assert!(dark);
        assert_eq!(m1, Adjustments { brightness: 117, contrast: 105, sepia: 15, grayscale: 0, dark_mode: true });
        assert_eq!(m2, default_mode2());
    }

    #[test]
    fn legacy_mode2_profile_migrates_without_polluting_mode1() {
        let json = r#"{
            "page": {
                "adjustments": { "brightness": 70, "contrast": 90, "sepia": 30, "grayscale": 0, "dark_mode": false }
            }
        }"#;
        let loaded: ProfileFile = serde_json::from_str(json).unwrap();
        let (dark, m1, m2) = resolve_profile_modes(&loaded);
        assert!(!dark);
        assert_eq!(m1, default_mode1());
        assert_eq!(m2, Adjustments { brightness: 70, contrast: 90, sepia: 30, grayscale: 0, dark_mode: false });
    }

    #[test]
    fn separate_modes_roundtrip_and_ppt_only_in_mode2() {
        let m1_a = Adjustments { brightness: 120, contrast: 100, sepia: 0, grayscale: 0, dark_mode: true };
        let m1_b = Adjustments { brightness: 80, contrast: 115, sepia: 40, grayscale: 0, dark_mode: true };
        let m2_a = Adjustments { brightness: 65, contrast: 100, sepia: 20, grayscale: 0, dark_mode: false };
        let m2_b = Adjustments { brightness: 110, contrast: 100, sepia: 0, grayscale: 0, dark_mode: false };

        let prof = ProfileFile {
            page: Some(PageProfile { adjustments: m1_a }),
            mode1: Some(m1_a),
            mode2: Some(m2_a),
            active_dark_mode: Some(true),
            ..Default::default()
        };
        let encoded = serde_json::to_string(&prof).unwrap();
        let decoded: ProfileFile = serde_json::from_str(&encoded).unwrap();
        let (dark, m1, m2) = resolve_profile_modes(&decoded);
        assert!(dark);
        assert_eq!(m1, m1_a);
        assert_eq!(m2, m2_a);

        // Mode 1 active: ET uses mode1 palette, PPT is NOT modified (canvas_mode == 0)!
        let wpp_m1_a = compute_canvas_cfg(true, true, m1_a, m2_a, false, 70, AppKind::Wpp);
        let wpp_m1_b = compute_canvas_cfg(true, true, m1_b, m2_a, false, 70, AppKind::Wpp);
        assert_eq!(wpp_m1_a.canvas_mode, 0);
        assert_eq!(wpp_m1_a, wpp_m1_b);

        // Mode 2 active: Word, Excel, and PPT ALL use mode2 soft_dim_rgb!
        let wpp_m2_a = compute_canvas_cfg(true, false, m1_a, m2_a, false, 70, AppKind::Wpp);
        let wpp_m2_b = compute_canvas_cfg(true, false, m1_a, m2_b, false, 70, AppKind::Wpp);
        assert_eq!(wpp_m2_a.canvas_mode, 2);
        assert_eq!(wpp_m2_a.dim_rgb, m2_a.soft_dim_rgb());
        assert_eq!(wpp_m2_b.dim_rgb, m2_b.soft_dim_rgb());
        assert_ne!(wpp_m2_a.dim_rgb, wpp_m2_b.dim_rgb);

        // Changing Mode 2 does NOT alter Mode 1 Excel!
        let et_m1_a = compute_canvas_cfg(true, true, m1_a, m2_a, false, 70, AppKind::Et);
        let et_m1_new_m2 = compute_canvas_cfg(true, true, m1_a, m2_b, false, 70, AppKind::Et);
        assert_eq!(et_m1_a.canvas_mode, 1);
        assert_eq!(et_m1_a.dark_bg, argb(m1_a.palette().0));
        assert_eq!(et_m1_a, et_m1_new_m2);
    }
}
