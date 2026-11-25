//! Background worker thread owning COM + NativeAgent + persistent dirty-session ledger.
//! Keeps UI & system tray 100% non-blocking and guarantees faithful restoration of prior WPS state.

use crate::native_agent::{AppKind, NativeAgent, Target, all_wps_targets, is_process_alive, state_dir};
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
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ProfileFile {
    page: Option<PageProfile>,
    #[serde(default)]
    remember_skin: Option<bool>,
    #[serde(default)]
    remember_page: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PageProfile {
    adjustments: Adjustments,
}

#[derive(Debug, Clone)]
pub struct UiSnapshot {
    pub skin_enabled: bool,
    pub page_enabled: bool,
    pub remember_skin: bool,
    pub remember_page: bool,
    pub adjustments: Adjustments,
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
            remember_skin: true,
            remember_page: false,
            adjustments: Adjustments::default(),
            status_text: "正在连接 WPS Office…".into(),
            skin_msg: "原生 2025 深色皮肤覆盖文字 · PPT · 表格；保留原图标与顶部菜单。".into(),
            page_msg: "实时调整文字纸张与自动文字；恢复时忠实还原原自带护眼与深色设置。".into(),
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
    UpdateAdjustments(Adjustments, bool), // (adj, apply_now)
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
        let initial_adj = loaded_profile
            .page
            .as_ref()
            .map(|p| p.adjustments.normalized())
            .unwrap_or_default();
        let init_rem_skin = loaded_profile.remember_skin;
        let init_rem_page = loaded_profile.remember_page;
        let initial_dirty = load_dirty(&dirty_path);
        let mut init_snap = UiSnapshot::default();
        init_snap.adjustments = initial_adj;
        init_snap.remember_skin = init_rem_skin.unwrap_or(true);
        init_snap.remember_page = init_rem_page.unwrap_or(false);
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
                    blocked_page: HashSet::new(),
                    blocked_skin: HashSet::new(),
                    skin_fail_counts: HashMap::new(),
                    page_fail_counts: HashMap::new(),
                    skin_enabled: false,
                    page_enabled: false,
                    remember_skin: init_rem_skin,
                    remember_page: init_rem_page,
                    adjustments: initial_adj,
                    state: st_clone,
                    notify_hwnd: hwnd_clone,
                };
                core.run(rx);
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

    pub fn saved_auto_start_prefs(&self) -> (bool, bool) {
        let s = self.snapshot();
        (s.remember_skin, s.remember_page)
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
    let mut alive = BTreeMap::new();
    for (k, v) in raw {
        if is_process_alive(v.pid, v.created_100ns) && (v.page.is_some() || v.skin.is_some()) {
            alive.insert(k, v);
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
    blocked_page: HashSet<String>,
    blocked_skin: HashSet<String>,
    skin_fail_counts: HashMap<String, u8>,
    page_fail_counts: HashMap<String, u8>,
    skin_enabled: bool,
    page_enabled: bool,
    remember_skin: Option<bool>,
    remember_page: Option<bool>,
    adjustments: Adjustments,
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
            g.remember_skin = self.remember_skin.unwrap_or(true);
            g.remember_page = self.remember_page.unwrap_or(false);
            g.adjustments = self.adjustments;
            g.dirty_count = self.dirty.len();
            f(&mut g);
        }
        self.notify_ui();
    }

    fn save_profile(&self) {
        atomic_write_json(
            &self.profiles_path,
            &ProfileFile {
                page: Some(PageProfile {
                    adjustments: self.adjustments.normalized(),
                }),
                remember_skin: self.remember_skin,
                remember_page: self.remember_page,
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
        let dead: Vec<String> = self
            .dirty
            .iter()
            .filter(|(_, v)| !is_process_alive(v.pid, v.created_100ns))
            .map(|(k, _)| k.clone())
            .collect();
        if !dead.is_empty() {
            for k in dead {
                self.dirty.remove(&k);
                self.page_applied.remove(&k);
                self.skin_applied.remove(&k);
            }
            self.save_dirty();
        }
    }

    fn run(&mut self, rx: mpsc::Receiver<Command>) {
        // Initial read-only scan
        self.sync(false, false);

        while let Ok(cmd) = rx.recv() {
            // Coalesce burst slider updates cleanly
            let mut active_cmd = cmd;
            if matches!(active_cmd, Command::UpdateAdjustments(_, _)) {
                while let Ok(next) = rx.try_recv() {
                    match next {
                        Command::UpdateAdjustments(a, apply) => {
                            active_cmd = Command::UpdateAdjustments(a, apply);
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
                self.page_fail_counts.clear();
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
                self.page_fail_counts.clear();
                self.save_profile();
                self.update_ui(|s| {
                    s.busy = true;
                    s.error_text = None;
                });
                let (ok, msg) = self.restore_page();
                self.update_ui(|s| {
                    s.busy = false;
                    s.page_msg = msg.clone();
                    if !ok {
                        s.error_text = Some(msg.clone());
                        s.status_text = msg;
                    } else {
                        s.error_text = None;
                        if !s.skin_enabled {
                            s.status_text = msg;
                        }
                    }
                });
            }
            Command::UpdateAdjustments(adj, apply_now) => {
                self.adjustments = adj.normalized();
                self.save_profile();
                self.update_ui(|_| {});
                if apply_now && self.page_enabled {
                    self.sync(false, true);
                }
            }
            Command::RestoreAll => {
                self.skin_enabled = false;
                self.page_enabled = false;
                self.remember_skin = Some(false);
                self.remember_page = Some(false);
                self.blocked_page.clear();
                self.blocked_skin.clear();
                self.skin_fail_counts.clear();
                self.page_fail_counts.clear();
                self.save_profile();
                self.update_ui(|s| {
                    s.busy = true;
                    s.error_text = None;
                });
                let (ok_p, msg_p) = self.restore_page();
                let (ok_s, msg_s) = self.restore_skin();
                self.update_ui(|s| {
                    s.busy = false;
                    s.page_msg = msg_p.clone();
                    s.skin_msg = msg_s.clone();
                    if ok_p && ok_s {
                        s.error_text = None;
                        s.status_text = "已安全恢复：未改动项保持原样，已改动项已忠实还原。".into();
                    } else {
                        let err = if !ok_p { msg_p } else { msg_s };
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
                self.blocked_page.clear();
                self.blocked_skin.clear();
                let (ok_p, msg_p) = self.restore_page();
                let (ok_s, msg_s) = self.restore_skin();
                let ok = ok_p && ok_s;
                let msg = if ok {
                    "已全部恢复并退出。".into()
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
                        if e.page.is_none() {
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
        let alive_keys: HashSet<String> = targets.iter().map(|t| t.identity_key()).collect();
        self.page_applied.retain(|k, _| alive_keys.contains(k));
        self.skin_applied.retain(|k, _| alive_keys.contains(k));
        self.skin_fail_counts.retain(|k, _| alive_keys.contains(k));
        self.page_fail_counts.retain(|k, _| alive_keys.contains(k));

        let mut wps_c = 0usize;
        let mut wpp_c = 0usize;
        let mut et_c = 0usize;
        let mut writer_docs = 0usize;
        let mut errors: Vec<String> = Vec::new();

        let (bg, fg) = self.adjustments.palette();
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

            if t.app_kind == AppKind::Wps {
                if let Some(sess) = sessions_by_pid.get(&t.pid) {
                    writer_docs += 1;
                    if self.page_enabled && !self.blocked_page.contains(&key) {
                        match self.apply_page_target(t, sess, desired_colors, force_page) {
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
            let skin_st = if self.skin_enabled { "深色外观已开启" } else { "原版外观" };
            let page_st = if self.page_enabled {
                if writer_docs > 0 { "文字页内已调色" } else { "等待文字文档" }
            } else {
                "原版页内"
            };
            s.status_text = format!("已连接 {apps}  |  {skin_st} · {page_st}");
            if self.skin_enabled {
                s.skin_msg = format!("已实时应用 2025 原生深色皮肤（{apps}），顶部菜单与图标保持原生渲染。");
            }
            if self.page_enabled && writer_docs > 0 {
                s.page_msg = "已实时应用并回读确认；恢复时忠实还原启用前的自带护眼与深色状态。".into();
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
        let writer_targets: HashMap<String, Target> = all_wps_targets(&pref_hwnds)
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
            if !is_process_alive(entry.pid, entry.created_100ns) {
                if let Some(e) = self.dirty.get_mut(&key) {
                    e.page = None;
                    if e.skin.is_none() {
                        self.dirty.remove(&key);
                    }
                }
                self.page_applied.remove(&key);
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
                            errors.push("页内颜色还原回读不一致。".into());
                            continue;
                        }
                    }
                    Err(e) => {
                        errors.push(e);
                        continue;
                    }
                }
            }

            let Some(session) = sessions_by_pid.get(&entry.pid) else {
                errors.push(format!(
                    "WPS 文字进程（PID {}）仍在运行但未打开文档标签；请新建空白文档后重试恢复，或关闭该窗口。",
                    entry.pid
                ));
                continue;
            };

            // Independently restore exact pre-apply BlueScreen and ProtectEyes
            let res = session
                .set_blue_screen(snap.orig_blue_screen)
                .and_then(|_| session.set_protect_eyes(snap.orig_protect_eyes));
            match res {
                Ok(()) => {
                    if let Some(e) = self.dirty.get_mut(&key) {
                        e.page = None;
                        if e.skin.is_none() {
                            self.dirty.remove(&key);
                        }
                    }
                    self.page_applied.remove(&key);
                    restored += 1;
                }
                Err(e) => errors.push(e),
            }
        }

        self.save_dirty();
        if let Some(err) = errors.into_iter().next() {
            (false, err)
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

        let live: HashMap<String, Target> = all_wps_targets(&HashMap::new())
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
            if !is_process_alive(entry.pid, entry.created_100ns) {
                if let Some(e) = self.dirty.get_mut(&key) {
                    e.skin = None;
                    if e.page.is_none() {
                        self.dirty.remove(&key);
                    }
                }
                self.skin_applied.remove(&key);
                continue;
            }
            let Some(target) = live.get(&key) else {
                errors.push(format!(
                    "WPS 进程（PID {}）仍在运行但窗口未就绪，请显示该窗口后重试恢复外观。",
                    entry.pid
                ));
                continue;
            };
            let orig = if snap.orig_prome.is_empty() {
                LIGHT_SKIN_FALLBACK
            } else {
                snap.orig_prome.as_str()
            };
            match self.agent.switch_skin(target, orig, "") {
                Ok(st) if st.prome == orig && st.app == orig => {
                    if let Some(e) = self.dirty.get_mut(&key) {
                        e.skin = None;
                        if e.page.is_none() {
                            self.dirty.remove(&key);
                        }
                    }
                    self.skin_applied.remove(&key);
                    restored += 1;
                }
                Ok(st) => errors.push(format!("WPS 外观恢复回读不一致（{}/{}）。", st.prome, st.app)),
                Err(e) => errors.push(e),
            }
        }

        self.save_dirty();
        if let Some(err) = errors.into_iter().next() {
            (false, err)
        } else {
            (true, format!("已实时还原 {restored} 个 WPS 窗口原来的原生皮肤。"))
        }
    }
}
