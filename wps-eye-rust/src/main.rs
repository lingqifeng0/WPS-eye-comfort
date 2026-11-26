#![windows_subsystem = "windows"]

mod autostart;
mod controller;
mod native_agent;
mod palette;
mod skin_builder;
mod tray;
mod ui_d2d;
mod wps_bridge;

use crate::autostart::AutoStartStatus;
use crate::controller::{Command, ControllerHandle, WM_APP_STATE_UPDATED};
use crate::palette::Adjustments;
use crate::tray::{
    CMD_RESTORE_ALL, CMD_RESTORE_AND_EXIT, CMD_SHOW_PANEL, CMD_TOGGLE_AUTOSTART, CMD_TOGGLE_IMG,
    CMD_TOGGLE_PAGE, CMD_TOGGLE_SKIN, IDI_APP_IDLE, TrayIcon, WM_TRAY_ICON, load_app_icon,
};
use crate::ui_d2d::{
    ControlId, D2dRenderer, IMG_MAX, IMG_MIN, IMG_STEP, LOGICAL_H, LOGICAL_W, Layout, SLIDERS,
    slider_get, slider_set,
};
use std::ffi::c_void;
use windows::core::{BOOL, PCWSTR, w};
use windows::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{
    DWMWA_BORDER_COLOR, DWMWA_CAPTION_COLOR, DWMWA_TEXT_COLOR, DWMWA_USE_IMMERSIVE_DARK_MODE,
    DWMWA_WINDOW_CORNER_PREFERENCE, DwmSetWindowAttribute,
};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, EndPaint, EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, InvalidateRect,
    MONITORINFO, PAINTSTRUCT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::ProcessStatus::K32EmptyWorkingSet;
use windows::Win32::System::Threading::{CreateMutexW, GetCurrentProcess};
use windows::Win32::UI::HiDpi::{
    AdjustWindowRectExForDpi, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForWindow,
    SetProcessDpiAwarenessContext,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    ReleaseCapture, SetCapture, TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent,
};
const WM_MOUSELEAVE: u32 = 0x02A3;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, FindWindowW, GetMessageW,
    GetWindowLongPtrW, GWLP_USERDATA, HMENU, IDC_ARROW, IDC_HAND, ICON_BIG, ICON_SMALL, IDYES,
    KillTimer, LoadCursorW, MB_ICONERROR, MB_ICONWARNING, MB_OK, MB_YESNO, MSG, MessageBoxW,
    PostMessageW, PostQuitMessage, RegisterClassExW, RegisterWindowMessageW, SW_HIDE, SW_RESTORE,
    SW_SHOW, SWP_NOSIZE, SWP_NOZORDER, SWP_SHOWWINDOW, SendMessageW, SetCursor,
    SetForegroundWindow, SetTimer, SetWindowLongPtrW, SetWindowPos, ShowWindow, TranslateMessage,
    WM_APP, WM_CLOSE, WM_COMMAND, WM_DESTROY, WM_DPICHANGED, WM_ENDSESSION, WM_ERASEBKGND,
    WM_LBUTTONDBLCLK, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_PAINT, WM_RBUTTONUP,
    WM_SETCURSOR, WM_SETICON, WM_SIZE, WM_TIMER, WNDCLASSEXW, WS_CAPTION, WS_EX_APPWINDOW,
    WS_MINIMIZEBOX, WS_SYSMENU,
};

const WINDOW_CLASS: PCWSTR = w!("WpsEyeComfortRustPanel");
const WINDOW_TITLE: PCWSTR = w!("WPS 护眼控制器");
const WM_APP_SHOW_PANEL: u32 = WM_APP + 20;

const TIMER_SYNC: usize = 1;
const TIMER_SLIDER_DEBOUNCE: usize = 2;
const TIMER_IMG_DEBOUNCE: usize = 3;

struct AppState {
    hwnd: HWND,
    controller: Option<ControllerHandle>,
    renderer: Option<D2dRenderer>,
    layout: Layout,
    tray: Option<TrayIcon>,
    autostart: AutoStartStatus,
    taskbar_created_msg: u32,
    active_dark_mode: bool,
    live_mode1: Adjustments,
    live_mode2: Adjustments,
    live_img_brightness: i32,
    hovered: Option<ControlId>,
    dragging_slider: Option<usize>,
    dragging_img: bool,
    tracking_mouse: bool,
    visible: bool,
    exiting: bool,
}

impl AppState {
    fn cur_adj(&self) -> Adjustments {
        if self.active_dark_mode {
            self.live_mode1
        } else {
            self.live_mode2
        }
    }

    fn cur_adj_mut(&mut self) -> &mut Adjustments {
        if self.active_dark_mode {
            &mut self.live_mode1
        } else {
            &mut self.live_mode2
        }
    }

    fn sync_tray(&mut self) {
        let Some(ref ctrl) = self.controller else {
            return;
        };
        let snap = ctrl.snapshot();
        let active = snap.skin_enabled || snap.page_enabled || snap.img_enabled;
        let tip = if active {
            let mut parts = Vec::new();
            if snap.skin_enabled { parts.push("深色外观"); }
            if snap.page_enabled {
                parts.push(if snap.dark_mode { "深底模式" } else { "夜间柔暗" });
            }
            if snap.img_enabled { parts.push("贴图降亮"); }
            format!("WPS 护眼控制器 · {}运行中", parts.join(" + "))
        } else {
            "WPS 护眼控制器 · 待机中（左键打开，右键菜单）".into()
        };
        if let Some(ref mut tray) = self.tray {
            tray.update(active, &tip);
        }
    }

    fn show_panel(&mut self) {
        self.autostart = autostart::query_status();
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_SHOW);
            let _ = ShowWindow(self.hwnd, SW_RESTORE);
            let _ = SetForegroundWindow(self.hwnd);
            let _ = InvalidateRect(Some(self.hwnd), None, false);
        }
        self.visible = true;
    }

    fn toggle_autostart(&mut self) {
        let cur = autostart::query_status();
        match autostart::set_enabled(!cur.enabled) {
            Ok(next) => {
                self.autostart = next;
                if self.visible {
                    unsafe {
                        let _ = InvalidateRect(Some(self.hwnd), None, false);
                    }
                }
            }
            Err(err) => {
                self.autostart = autostart::query_status();
                show_error_box(self.hwnd, "开机自启设置失败", &err);
            }
        }
    }

    fn hide_to_tray(&mut self) {
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_HIDE);
        }
        self.visible = false;
        self.hovered = None;
        self.dragging_slider = None;
        self.dragging_img = false;
        // Release Direct2D/DirectWrite resources completely & trim working set -> ~3 MB tray RAM!
        if let Some(ref mut r) = self.renderer {
            r.release_render_target();
        }
        self.renderer = None;
        unsafe {
            let _ = K32EmptyWorkingSet(GetCurrentProcess());
        }
    }

    fn restore_and_exit(&mut self) {
        if self.exiting {
            return;
        }
        self.exiting = true;
        unsafe {
            let _ = KillTimer(Some(self.hwnd), TIMER_SYNC);
            let _ = KillTimer(Some(self.hwnd), TIMER_SLIDER_DEBOUNCE);
            let _ = KillTimer(Some(self.hwnd), TIMER_IMG_DEBOUNCE);
        }
        let result = if let Some(ref ctrl) = self.controller {
            ctrl.restore_all_and_wait()
        } else {
            (true, String::new())
        };
        if !result.0 {
            self.show_panel();
            let msg = format!(
                "{}\n\n选择「是」立即强制退出控制器（下次启动仍可自动恢复）；\n选择「否」返回面板稍后重试。",
                result.1
            );
            if !confirm_force_exit(self.hwnd, "部分窗口恢复受阻", &msg) {
                self.exiting = false;
                unsafe {
                    let _ = SetTimer(Some(self.hwnd), TIMER_SYNC, 2000, None);
                }
                return;
            }
        }
        if let Some(ref mut t) = self.tray {
            t.remove();
        }
        if let Some(ctrl) = self.controller.take() {
            ctrl.shutdown();
        }
        unsafe {
            let _ = DestroyWindow(self.hwnd);
        }
    }

    fn dip_pos(&self, lp: LPARAM) -> (f32, f32) {
        let px = (lp.0 & 0xFFFF) as i16 as f32;
        let py = ((lp.0 >> 16) & 0xFFFF) as i16 as f32;
        let dpi = unsafe { GetDpiForWindow(self.hwnd).max(96) as f32 };
        let scale = dpi / 96.0;
        (px / scale, py / scale)
    }

    fn push_adj_debounced(&mut self, immediate: bool) {
        if let Some(ref ctrl) = self.controller {
            ctrl.send(Command::UpdateAdjustments {
                dark_mode: self.active_dark_mode,
                mode1: self.live_mode1,
                mode2: self.live_mode2,
                apply_now: immediate,
            });
        }
        unsafe {
            let _ = KillTimer(Some(self.hwnd), TIMER_SLIDER_DEBOUNCE);
            if !immediate {
                let _ = SetTimer(Some(self.hwnd), TIMER_SLIDER_DEBOUNCE, 180, None);
            }
            let _ = InvalidateRect(Some(self.hwnd), None, false);
        }
    }

    fn push_img_debounced(&mut self, immediate: bool) {
        self.live_img_brightness = self.live_img_brightness.clamp(IMG_MIN, IMG_MAX);
        if let Some(ref ctrl) = self.controller {
            ctrl.send(Command::UpdateImgBrightness(self.live_img_brightness, immediate));
        }
        unsafe {
            let _ = KillTimer(Some(self.hwnd), TIMER_IMG_DEBOUNCE);
            if !immediate {
                let _ = SetTimer(Some(self.hwnd), TIMER_IMG_DEBOUNCE, 140, None);
            }
            let _ = InvalidateRect(Some(self.hwnd), None, false);
        }
    }

    fn on_click(&mut self, id: ControlId, x: f32) {
        let snap = self.controller.as_ref().map(|c| c.snapshot()).unwrap_or_default();
        match id {
            ControlId::SkinToggle => {
                if let Some(ref ctrl) = self.controller {
                    ctrl.send(Command::SetSkinEnabled(!snap.skin_enabled));
                }
            }
            ControlId::PageToggle => {
                if let Some(ref ctrl) = self.controller {
                    ctrl.send(Command::SetPageEnabled(!snap.page_enabled));
                }
            }
            ControlId::ModeDark => {
                self.active_dark_mode = true;
                self.push_adj_debounced(true);
            }
            ControlId::ModeSoftLight => {
                self.active_dark_mode = false;
                self.push_adj_debounced(true);
            }
            ControlId::SliderMinus(i) => {
                let cur = slider_get(&self.cur_adj(), i);
                slider_set(self.cur_adj_mut(), i, cur - SLIDERS[i].step);
                self.push_adj_debounced(true);
            }
            ControlId::SliderPlus(i) => {
                let cur = slider_get(&self.cur_adj(), i);
                slider_set(self.cur_adj_mut(), i, cur + SLIDERS[i].step);
                self.push_adj_debounced(true);
            }
            ControlId::SliderTrack(i) => {
                self.dragging_slider = Some(i);
                unsafe {
                    SetCapture(self.hwnd);
                }
                let nv = self.layout.value_from_x(i, x);
                slider_set(self.cur_adj_mut(), i, nv);
                self.push_adj_debounced(false);
            }
            ControlId::ResetParams => {
                if self.active_dark_mode {
                    self.live_mode1 = Adjustments {
                        dark_mode: true,
                        ..Adjustments::default()
                    };
                } else {
                    self.live_mode2 = Adjustments {
                        dark_mode: false,
                        ..Adjustments::default()
                    };
                }
                self.push_adj_debounced(true);
            }
            ControlId::ImgToggle => {
                if let Some(ref ctrl) = self.controller {
                    ctrl.send(Command::SetImgEnabled(!snap.img_enabled));
                }
            }
            ControlId::ImgMinus => {
                self.live_img_brightness = (self.live_img_brightness - IMG_STEP).clamp(IMG_MIN, IMG_MAX);
                self.push_img_debounced(true);
            }
            ControlId::ImgPlus => {
                self.live_img_brightness = (self.live_img_brightness + IMG_STEP).clamp(IMG_MIN, IMG_MAX);
                self.push_img_debounced(true);
            }
            ControlId::ImgTrack => {
                self.dragging_img = true;
                unsafe {
                    SetCapture(self.hwnd);
                }
                self.live_img_brightness = self.layout.img_value_from_x(x);
                self.push_img_debounced(false);
            }
            ControlId::AutoStartToggle => {
                self.toggle_autostart();
            }
            ControlId::RestoreAll => {
                if let Some(ref ctrl) = self.controller {
                    ctrl.send(Command::RestoreAll);
                }
            }
            ControlId::HideToTray => {
                self.hide_to_tray();
            }
        }
    }
}

fn show_error_box(hwnd: HWND, title: &str, msg: &str) {
    let t: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
    let m: Vec<u16> = msg.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        let _ = MessageBoxW(Some(hwnd), PCWSTR(m.as_ptr()), PCWSTR(t.as_ptr()), MB_OK | MB_ICONERROR);
    }
}

fn confirm_force_exit(hwnd: HWND, title: &str, msg: &str) -> bool {
    let t: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
    let m: Vec<u16> = msg.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        MessageBoxW(
            Some(hwnd),
            PCWSTR(m.as_ptr()),
            PCWSTR(t.as_ptr()),
            MB_YESNO | MB_ICONWARNING,
        ) == IDYES
    }
}

fn apply_dark_dwm(hwnd: HWND) {
    unsafe {
        let dark: i32 = 1;
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE, &dark as *const _ as *const c_void, 4);
        // COLORREF = 0x00BBGGRR matching canvas #13171F -> R=0x13, G=0x17, B=0x1F
        let caption: u32 = 0x001F_1713;
        let text: u32 = 0x00F5_ECE6;
        let border: u32 = 0x0045_342B;
        let corner: u32 = 2; // DWMWCP_ROUND
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_CAPTION_COLOR, &caption as *const _ as *const c_void, 4);
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_TEXT_COLOR, &text as *const _ as *const c_void, 4);
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_BORDER_COLOR, &border as *const _ as *const c_void, 4);
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE, &corner as *const _ as *const c_void, 4);
    }
}

fn lower_monitor_work_rect() -> RECT {
    let mut rects: Vec<RECT> = Vec::new();
    unsafe extern "system" fn cb(hm: HMONITOR, _: HDC, _: *mut RECT, lp: LPARAM) -> BOOL {
        let out = &mut *(lp.0 as *mut Vec<RECT>);
        let mut mi = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if GetMonitorInfoW(hm, &mut mi).as_bool() {
            out.push(mi.rcWork);
        }
        BOOL(1)
    }
    unsafe {
        let _ = EnumDisplayMonitors(None, None, Some(cb), LPARAM(&mut rects as *mut _ as isize));
    }
    rects
        .into_iter()
        .max_by_key(|r| r.top)
        .unwrap_or(RECT { left: 40, top: 40, right: 1200, bottom: 900 })
}

fn window_size_for_dpi(dpi: u32) -> (i32, i32) {
    let scale = dpi.max(96) as f32 / 96.0;
    let mut rc = RECT {
        left: 0,
        top: 0,
        right: (LOGICAL_W * scale).round() as i32,
        bottom: (LOGICAL_H * scale).round() as i32,
    };
    let style = WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX;
    unsafe {
        let _ = AdjustWindowRectExForDpi(&mut rc, style, false, WS_EX_APPWINDOW, dpi.max(96));
    }
    (rc.right - rc.left, rc.bottom - rc.top)
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut AppState;
    if ptr.is_null() {
        return DefWindowProcW(hwnd, msg, wp, lp);
    }
    let state = &mut *ptr;

    if state.taskbar_created_msg != 0 && msg == state.taskbar_created_msg {
        if let Some(ref mut t) = state.tray {
            t.reinstall();
        }
        state.sync_tray();
        return LRESULT(0);
    }

    match msg {
        WM_ERASEBKGND => return LRESULT(1),
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            if state.renderer.is_none() {
                state.renderer = D2dRenderer::new().ok();
            }
            let cur_adj = state.cur_adj();
            if let (Some(ref ctrl), Some(ref mut ren)) = (&state.controller, &mut state.renderer) {
                let snap = ctrl.snapshot();
                ren.paint(
                    hwnd,
                    hdc,
                    &snap,
                    &state.autostart,
                    state.active_dark_mode,
                    cur_adj,
                    state.live_img_brightness,
                    state.hovered,
                    state.dragging_slider,
                    state.dragging_img,
                );
            }
            let _ = EndPaint(hwnd, &ps);
            return LRESULT(0);
        }
        WM_SIZE => {
            let _ = InvalidateRect(Some(hwnd), None, false);
            return LRESULT(0);
        }
        WM_DPICHANGED => {
            let new_dpi = (wp.0 & 0xFFFF) as u32;
            let (w, h) = window_size_for_dpi(new_dpi);
            let suggested = *(lp.0 as *const RECT);
            let _ = SetWindowPos(hwnd, None, suggested.left, suggested.top, w, h, SWP_NOZORDER);
            let _ = InvalidateRect(Some(hwnd), None, false);
            return LRESULT(0);
        }
        WM_MOUSEMOVE => {
            if !state.tracking_mouse {
                let mut tme = TRACKMOUSEEVENT {
                    cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: hwnd,
                    dwHoverTime: 0,
                };
                let _ = TrackMouseEvent(&mut tme);
                state.tracking_mouse = true;
            }
            let (x, y) = state.dip_pos(lp);
            if let Some(idx) = state.dragging_slider {
                let nv = state.layout.value_from_x(idx, x);
                if slider_get(&state.cur_adj(), idx) != nv {
                    slider_set(state.cur_adj_mut(), idx, nv);
                    state.push_adj_debounced(false);
                }
            } else if state.dragging_img {
                let nv = state.layout.img_value_from_x(x);
                if state.live_img_brightness != nv {
                    state.live_img_brightness = nv;
                    state.push_img_debounced(false);
                }
            } else {
                let h = state.layout.hit_test(x, y);
                if h != state.hovered {
                    state.hovered = h;
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
            }
            return LRESULT(0);
        }
        WM_MOUSELEAVE => {
            state.tracking_mouse = false;
            if state.hovered.is_some() {
                state.hovered = None;
                let _ = InvalidateRect(Some(hwnd), None, false);
            }
            return LRESULT(0);
        }
        WM_SETCURSOR => {
            let ht = (lp.0 & 0xFFFF) as u32;
            if ht == 1 {
                let cur_id = if state.hovered.is_some() || state.dragging_slider.is_some() || state.dragging_img {
                    IDC_HAND
                } else {
                    IDC_ARROW
                };
                if let Ok(c) = LoadCursorW(None, cur_id) {
                    SetCursor(Some(c));
                }
                return LRESULT(1);
            }
        }
        WM_LBUTTONDOWN => {
            let (x, y) = state.dip_pos(lp);
            if let Some(id) = state.layout.hit_test(x, y) {
                state.on_click(id, x);
            }
            return LRESULT(0);
        }
        WM_LBUTTONUP => {
            if state.dragging_slider.take().is_some() {
                let _ = ReleaseCapture();
                state.push_adj_debounced(true);
            }
            if state.dragging_img {
                state.dragging_img = false;
                let _ = ReleaseCapture();
                state.push_img_debounced(true);
            }
            return LRESULT(0);
        }
        WM_TIMER => {
            if wp.0 == TIMER_SYNC {
                if let Some(ref ctrl) = state.controller {
                    ctrl.send(Command::SyncTick);
                }
            } else if wp.0 == TIMER_SLIDER_DEBOUNCE {
                let _ = KillTimer(Some(hwnd), TIMER_SLIDER_DEBOUNCE);
                if let Some(ref ctrl) = state.controller {
                    ctrl.send(Command::UpdateAdjustments {
                        dark_mode: state.active_dark_mode,
                        mode1: state.live_mode1,
                        mode2: state.live_mode2,
                        apply_now: true,
                    });
                }
            } else if wp.0 == TIMER_IMG_DEBOUNCE {
                let _ = KillTimer(Some(hwnd), TIMER_IMG_DEBOUNCE);
                if let Some(ref ctrl) = state.controller {
                    ctrl.send(Command::UpdateImgBrightness(state.live_img_brightness, true));
                }
            }
            return LRESULT(0);
        }
        WM_APP_STATE_UPDATED => {
            state.sync_tray();
            if state.visible {
                let _ = InvalidateRect(Some(hwnd), None, false);
            } else if !state.controller.as_ref().map(|c| c.snapshot().busy).unwrap_or(false) {
                let _ = K32EmptyWorkingSet(GetCurrentProcess());
            }
            return LRESULT(0);
        }
        WM_APP_SHOW_PANEL => {
            state.show_panel();
            return LRESULT(0);
        }
        WM_TRAY_ICON => {
            let ev = (lp.0 & 0xFFFF) as u32;
            if ev == WM_LBUTTONUP || ev == WM_LBUTTONDBLCLK {
                if state.visible {
                    state.hide_to_tray();
                } else {
                    state.show_panel();
                }
            } else if ev == WM_RBUTTONUP {
                state.autostart = autostart::query_status();
                let snap = state.controller.as_ref().map(|c| c.snapshot()).unwrap_or_default();
                if let Some(ref t) = state.tray {
                    t.show_context_menu(
                        snap.skin_enabled,
                        snap.page_enabled,
                        snap.img_enabled,
                        state.autostart.enabled,
                    );
                }
            }
            return LRESULT(0);
        }
        WM_COMMAND => {
            let cmd = (wp.0 & 0xFFFF) as u32;
            let snap = state.controller.as_ref().map(|c| c.snapshot()).unwrap_or_default();
            match cmd {
                CMD_SHOW_PANEL => state.show_panel(),
                CMD_TOGGLE_SKIN => {
                    if let Some(ref ctrl) = state.controller {
                        ctrl.send(Command::SetSkinEnabled(!snap.skin_enabled));
                    }
                }
                CMD_TOGGLE_PAGE => {
                    if let Some(ref ctrl) = state.controller {
                        ctrl.send(Command::SetPageEnabled(!snap.page_enabled));
                    }
                }
                CMD_TOGGLE_IMG => {
                    if let Some(ref ctrl) = state.controller {
                        ctrl.send(Command::SetImgEnabled(!snap.img_enabled));
                    }
                }
                CMD_TOGGLE_AUTOSTART => state.toggle_autostart(),
                CMD_RESTORE_ALL => {
                    if let Some(ref ctrl) = state.controller {
                        ctrl.send(Command::RestoreAll);
                    }
                }
                CMD_RESTORE_AND_EXIT => state.restore_and_exit(),
                _ => {}
            }
            return LRESULT(0);
        }
        WM_ENDSESSION => {
            if wp.0 != 0 {
                if let Some(ref mut t) = state.tray {
                    t.remove();
                }
            }
            return LRESULT(0);
        }
        WM_CLOSE => {
            // Close button hides to system tray and KEEPS active dark skin & page palette alive!
            state.hide_to_tray();
            return LRESULT(0);
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            return LRESULT(0);
        }
        _ => {}
    }
    DefWindowProcW(hwnd, msg, wp, lp)
}

fn run_self_test(out_json: &str) -> Result<(), String> {
    use crate::native_agent::{AppKind, all_wps_targets};
    use crate::palette::contrast_ratio;
    use crate::wps_bridge::enumerate_writer_sessions;
    use std::collections::HashMap;

    unsafe {
        let _ = windows::Win32::System::Com::CoInitializeEx(
            None,
            windows::Win32::System::Com::COINIT_APARTMENTTHREADED,
        );
    }

    let tmp_dir = std::env::temp_dir().join("wps-eye-rust-selftest");
    let _ = std::fs::create_dir_all(&tmp_dir);
    let prof = tmp_dir.join("profiles.json");
    let dirty = tmp_dir.join("dirty.json");
    let _ = std::fs::remove_file(&prof);
    let _ = std::fs::remove_file(&dirty);

    let ctrl = ControllerHandle::start(Some(prof), Some(dirty.clone()), None)?;
    let wait_done = |c: &ControllerHandle| {
        c.flush();
    };

    let targets = all_wps_targets(&HashMap::new());
    let mut report = serde_json::Map::new();
    report.insert(
        "targets".into(),
        serde_json::json!(targets
            .iter()
            .map(|t| format!("{}:pid={}", t.app_kind.as_str(), t.pid))
            .collect::<Vec<_>>()),
    );

    let mut checks = Vec::<String>::new();

    // Test 1: Skin toggle across ALL currently open WPS apps (Writer / PPT / ET)
    ctrl.send(Command::SetSkinEnabled(true));
    wait_done(&ctrl);
    let st_dark = ctrl.snapshot();
    report.insert(
        "st_dark".into(),
        serde_json::json!({
            "skin_enabled": st_dark.skin_enabled,
            "status_text": st_dark.status_text,
            "skin_msg": st_dark.skin_msg,
            "error_text": st_dark.error_text,
            "wps": st_dark.wps_count,
            "wpp": st_dark.wpp_count,
            "et": st_dark.et_count,
            "dirty": st_dark.dirty_count,
        }),
    );
    if let Some(err) = &st_dark.error_text {
        return Err(format!("EnableSkin failed: {err}"));
    }
    if !targets.is_empty() {
        let mut probe_agent = crate::native_agent::NativeAgent::new()?;
        let mut states = Vec::new();
        for t in &targets {
            let sk = probe_agent.query_skin(t)?;
            states.push(format!("{:?}(pid={}): prome={} app={} dark={}", t.app_kind, t.pid, sk.prome, sk.app, sk.prome_dark));
        }
        report.insert("queried_after_enable".into(), serde_json::json!(states));
        for t in &targets {
            let sk = probe_agent.query_skin(t)?;
            if sk.prome != skin_builder::DARK_SKIN_NAME || sk.prome_dark != 1 {
                return Err(format!(
                    "Skin readback failed for {:?} (pid={}, hwnd=0x{:x}): {:?} | all={:?} | st_dark={:?}",
                    t.app_kind, t.pid, t.hwnd, sk, report["queried_after_enable"], report["st_dark"]
                ));
            }
        }
        checks.push("rust_native_dark_skin_verified".into());
    }

    // Test 2: Writer page palette + ProtectEyes faithful restoration if Writer doc is open
    let writer_sessions = enumerate_writer_sessions();
    if let Some(ws) = writer_sessions.first() {
        let init_pe = ws.get_protect_eyes()?;
        let init_bs = ws.get_blue_screen()?;

        // Set ProtectEyes=True, BlueScreen=False
        ws.set_blue_screen(false)?;
        ws.set_protect_eyes(true)?;
        // Unmodified page restore MUST preserve ProtectEyes=True
        ctrl.send(Command::SetPageEnabled(false));
        wait_done(&ctrl);
        if !ws.get_protect_eyes()? {
            return Err("Unmodified restore clobbered ProtectEyes!".into());
        }
        checks.push("unmodified_restore_preserves_protect_eyes".into());

        // Apply page -> ProtectEyes off, BlueScreen on -> restore page -> ProtectEyes MUST return True!
        let m2_default = Adjustments { dark_mode: false, ..Adjustments::default() };
        ctrl.send(Command::UpdateAdjustments {
            dark_mode: true,
            mode1: Adjustments { brightness: 100, contrast: 100, sepia: 100, grayscale: 0, dark_mode: true },
            mode2: m2_default,
            apply_now: false,
        });
        ctrl.send(Command::SetPageEnabled(true));
        wait_done(&ctrl);
        if let Some(err) = &ctrl.snapshot().error_text {
            return Err(format!("EnablePage failed: {err}"));
        }
        if !ws.get_blue_screen()? || ws.get_protect_eyes()? {
            return Err("EnablePage state mismatch".into());
        }

        // Switch to grayscale=100 while sepia=100 -> must shift to neutral gray!
        let gray_adj = Adjustments { brightness: 100, contrast: 100, sepia: 100, grayscale: 100, dark_mode: true };
        let (gbg, gfg) = gray_adj.palette();
        assert!(contrast_ratio(gbg, gfg) >= 4.5 && gbg.0 == gbg.1 && gbg.1 == gbg.2);
        ctrl.send(Command::UpdateAdjustments {
            dark_mode: true,
            mode1: gray_adj,
            mode2: m2_default,
            apply_now: true,
        });
        wait_done(&ctrl);
        let mut probe_agent = crate::native_agent::NativeAgent::new()?;
        let wt = targets.iter().find(|t| t.app_kind == AppKind::Wps).unwrap();
        let rb = probe_agent.colors(wt, None)?;
        if rb.get(&0xE001_0031).copied() != Some(palette::argb(gbg)) {
            return Err("Grayscale live color readback mismatch".into());
        }
        checks.push("sepia_100_grayscale_100_live_verified".into());

        // Restore page -> must restore ProtectEyes=True & BlueScreen=False!
        ctrl.send(Command::SetPageEnabled(false));
        wait_done(&ctrl);
        if ws.get_blue_screen()? || !ws.get_protect_eyes()? {
            return Err("Page restore failed to faithfully restore ProtectEyes=True!".into());
        }
        checks.push("page_restore_faithfully_restored_protect_eyes".into());

        // Restore initial machine state
        ws.set_blue_screen(init_bs)?;
        ws.set_protect_eyes(init_pe)?;
    }

    // Restore skin & exit cleanly
    let (ok, msg) = ctrl.restore_all_and_wait();
    if !ok {
        return Err(format!("RestoreAllAndExit failed: {msg}"));
    }
    if !targets.is_empty() {
        let mut probe_agent = crate::native_agent::NativeAgent::new()?;
        for t in &targets {
            let sk = probe_agent.query_skin(t)?;
            if sk.prome != "2018white" || sk.prome_dark != 0 {
                return Err(format!("Skin restore readback failed for {:?}: {:?}", t.app_kind, sk));
            }
        }
        checks.push("rust_native_skin_restored_to_2018white".into());
    }
    ctrl.shutdown();

    report.insert("status".into(), serde_json::json!("passed"));
    report.insert("checks".into(), serde_json::json!(checks));
    std::fs::write(out_json, serde_json::to_string_pretty(&report).unwrap()).map_err(|e| e.to_string())?;
    Ok(())
}

fn main() {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }

    let args: Vec<String> = std::env::args().collect();
    if let Some(pos) = args.iter().position(|a| a == "--self-test") {
        let out_path = args.get(pos + 1).map(|s| s.as_str()).unwrap_or("rust-selftest.json");
        if let Err(err) = run_self_test(out_path) {
            let _ = std::fs::write(
                out_path,
                serde_json::json!({ "status": "failed", "error": err }).to_string(),
            );
            std::process::exit(1);
        }
        return;
    }

    let start_minimized = args.iter().any(|a| a == "--minimized");
    let auto_start_boot = args.iter().any(|a| a == "--auto-start");
    let auto_skin = args.iter().any(|a| a == "--enable-skin");
    let auto_page = args.iter().any(|a| a == "--enable-page");
    let auto_img = args.iter().any(|a| a == "--enable-img");

    // Single-instance check: if already running, bring the existing panel window forward!
    unsafe {
        let mutex = CreateMutexW(None, false, w!("Local\\WpsEyeComfortRustV4"));
        if mutex.is_err() || GetLastError() == ERROR_ALREADY_EXISTS {
            if let Ok(existing) = FindWindowW(WINDOW_CLASS, PCWSTR::null()) {
                if !existing.0.is_null() {
                    let _ = PostMessageW(Some(existing), WM_APP_SHOW_PANEL, WPARAM(0), LPARAM(0));
                }
            }
            return;
        }
    }

    let controller = match ControllerHandle::start(None, None, None) {
        Ok(c) => c,
        Err(err) => {
            show_error_box(HWND::default(), "WPS 护眼控制器启动失败", &err);
            return;
        }
    };
    let init_snap = controller.snapshot();
    let init_dark_mode = init_snap.dark_mode;
    let init_mode1 = init_snap.mode1_adj;
    let init_mode2 = init_snap.mode2_adj;
    let init_img_brightness = init_snap.img_brightness;

    // Lazy-initialize Direct2D only when the panel is actually shown (keeps --minimized boot < 4 MB RAM!)
    let renderer = if start_minimized {
        None
    } else {
        match D2dRenderer::new() {
            Ok(r) => Some(r),
            Err(err) => {
                show_error_box(HWND::default(), "图形界面初始化失败", &err);
                controller.shutdown();
                return;
            }
        }
    };

    unsafe {
        let hinst = GetModuleHandleW(None).unwrap_or_default();
        let icon_lg = load_app_icon(IDI_APP_IDLE, true);
        let icon_sm = load_app_icon(IDI_APP_IDLE, false);

        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wnd_proc),
            hInstance: hinst.into(),
            hIcon: icon_lg,
            hIconSm: icon_sm,
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            lpszClassName: WINDOW_CLASS,
            ..Default::default()
        };
        let _ = RegisterClassExW(&wc);

        let work = lower_monitor_work_rect();
        let (win_w, win_h) = window_size_for_dpi(96);
        let style = WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX;

        let hwnd = match CreateWindowExW(
            WS_EX_APPWINDOW,
            WINDOW_CLASS,
            WINDOW_TITLE,
            style,
            work.left + 48,
            work.top + 48,
            win_w,
            win_h,
            None,
            Some(HMENU::default()),
            Some(hinst.into()),
            None,
        ) {
            Ok(h) => h,
            Err(e) => {
                show_error_box(HWND::default(), "创建窗口失败", &e.to_string());
                controller.shutdown();
                return;
            }
        };

        // Recompute exact DPI-scaled size for the lower monitor and clamp vertically inside work area
        let actual_dpi = GetDpiForWindow(hwnd).max(96);
        let (sw, sh) = window_size_for_dpi(actual_dpi);
        let win_x = (work.left + 48).min(work.right - sw - 8).max(work.left);
        let win_y = (work.top + 32).min(work.bottom - sh - 8).max(work.top);
        let _ = SetWindowPos(hwnd, None, win_x, win_y, sw, sh, SWP_NOZORDER);

        apply_dark_dwm(hwnd);
        if !icon_lg.is_invalid() {
            SendMessageW(hwnd, WM_SETICON, Some(WPARAM(ICON_BIG as usize)), Some(LPARAM(icon_lg.0 as isize)));
        }
        if !icon_sm.is_invalid() {
            SendMessageW(hwnd, WM_SETICON, Some(WPARAM(ICON_SMALL as usize)), Some(LPARAM(icon_sm.0 as isize)));
        }

        controller.set_notify_hwnd(hwnd.0 as isize);
        let (pref_skin, pref_page, pref_img) = controller.saved_auto_start_prefs();
        if auto_skin || (auto_start_boot && pref_skin) {
            controller.send(Command::SetSkinEnabled(true));
        }
        if auto_page || (auto_start_boot && pref_page) {
            controller.send(Command::SetPageEnabled(true));
        }
        if auto_img || (auto_start_boot && pref_img) {
            controller.send(Command::SetImgEnabled(true));
        }

        let tray = TrayIcon::new(hwnd);
        let taskbar_created_msg = RegisterWindowMessageW(w!("TaskbarCreated"));
        let mut app = Box::new(AppState {
            hwnd,
            controller: Some(controller),
            renderer,
            layout: Layout::compute(),
            tray: Some(tray),
            autostart: autostart::query_status(),
            taskbar_created_msg,
            active_dark_mode: init_dark_mode,
            live_mode1: init_mode1,
            live_mode2: init_mode2,
            live_img_brightness: init_img_brightness,
            hovered: None,
            dragging_slider: None,
            dragging_img: false,
            tracking_mouse: false,
            visible: !start_minimized,
            exiting: false,
        });

        let raw_ptr = &mut *app as *mut AppState as isize;
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, raw_ptr);
        let _ = SetTimer(Some(hwnd), TIMER_SYNC, 2000, None);

        if start_minimized {
            app.hide_to_tray();
            app.sync_tray();
        } else {
            let _ = SetWindowPos(hwnd, None, win_x, win_y, sw, sh, SWP_NOZORDER | SWP_SHOWWINDOW | SWP_NOSIZE);
            let _ = ShowWindow(hwnd, SW_SHOW);
        }

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
        if let Some(ctrl) = app.controller.take() {
            ctrl.shutdown();
        }
    }
}
