//! Native Win32 system tray icon (Shell_NotifyIconW) with idle/active icon switching and context menu.

use windows::core::PCWSTR;
use windows::Win32::Foundation::{HWND, LPARAM, POINT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Shell::{
    NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW,
    Shell_NotifyIconW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, DestroyIcon, DestroyMenu, GetCursorPos, GetSystemMetrics, HICON,
    IMAGE_ICON, LR_DEFAULTCOLOR, LoadImageW, MF_CHECKED, MF_SEPARATOR, MF_STRING, MF_UNCHECKED,
    PostMessageW, SM_CXICON, SM_CXSMICON, SM_CYICON, SM_CYSMICON, SetForegroundWindow,
    SetMenuDefaultItem, TPM_BOTTOMALIGN, TPM_LEFTALIGN, TPM_RIGHTBUTTON, TrackPopupMenu, WM_APP,
    WM_NULL,
};

pub const WM_TRAY_ICON: u32 = WM_APP + 1;
pub const IDI_APP_IDLE: u16 = 101;
pub const IDI_APP_ACTIVE: u16 = 102;

pub const CMD_SHOW_PANEL: u32 = 4001;
pub const CMD_TOGGLE_SKIN: u32 = 4002;
pub const CMD_TOGGLE_PAGE: u32 = 4003;
pub const CMD_RESTORE_ALL: u32 = 4004;
pub const CMD_RESTORE_AND_EXIT: u32 = 4005;
pub const CMD_TOGGLE_AUTOSTART: u32 = 4006;
pub const CMD_TOGGLE_IMG: u32 = 4007;

pub struct TrayIcon {
    hwnd: HWND,
    icon_idle: HICON,
    icon_active: HICON,
    installed: bool,
    last_active: bool,
    last_tip: String,
}

pub fn load_app_icon(res_id: u16, large: bool) -> HICON {
    unsafe {
        let hinst = GetModuleHandleW(None).unwrap_or_default();
        let cx = GetSystemMetrics(if large { SM_CXICON } else { SM_CXSMICON });
        let cy = GetSystemMetrics(if large { SM_CYICON } else { SM_CYSMICON });
        let h = LoadImageW(
            Some(hinst.into()),
            PCWSTR(res_id as usize as *const u16),
            IMAGE_ICON,
            cx,
            cy,
            LR_DEFAULTCOLOR,
        );
        match h {
            Ok(handle) => HICON(handle.0),
            Err(_) => HICON::default(),
        }
    }
}

fn fill_tip(dst: &mut [u16; 128], text: &str) {
    dst.fill(0);
    for (i, u) in text.encode_utf16().take(127).enumerate() {
        dst[i] = u;
    }
}

impl TrayIcon {
    pub fn new(hwnd: HWND) -> Self {
        let icon_idle = load_app_icon(IDI_APP_IDLE, false);
        let icon_active = load_app_icon(IDI_APP_ACTIVE, false);
        let mut tray = Self {
            hwnd,
            icon_idle,
            icon_active,
            installed: false,
            last_active: false,
            last_tip: String::new(),
        };
        tray.install();
        tray
    }

    fn install(&mut self) {
        let mut nid = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: self.hwnd,
            uID: 1,
            uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
            uCallbackMessage: WM_TRAY_ICON,
            hIcon: self.icon_idle,
            ..Default::default()
        };
        let tip = "WPS 护眼控制器 · 待机中（左键打开，右键菜单）";
        fill_tip(&mut nid.szTip, tip);
        unsafe {
            if Shell_NotifyIconW(NIM_ADD, &nid).as_bool() {
                self.installed = true;
                self.last_tip = tip.to_string();
            }
        }
    }

    pub fn reinstall(&mut self) {
        let prev_active = self.last_active;
        let prev_tip = self.last_tip.clone();
        self.installed = false;
        self.last_active = false;
        self.last_tip.clear();
        self.install();
        if !prev_tip.is_empty() {
            self.update(prev_active, &prev_tip);
        }
    }

    pub fn update(&mut self, active: bool, tip: &str) {
        if !self.installed {
            self.install();
        }
        if !self.installed || (self.last_active == active && self.last_tip == tip) {
            return;
        }
        let mut nid = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: self.hwnd,
            uID: 1,
            uFlags: NIF_ICON | NIF_TIP,
            hIcon: if active && !self.icon_active.is_invalid() {
                self.icon_active
            } else {
                self.icon_idle
            },
            ..Default::default()
        };
        fill_tip(&mut nid.szTip, tip);
        unsafe {
            let _ = Shell_NotifyIconW(NIM_MODIFY, &nid);
        }
        self.last_active = active;
        self.last_tip = tip.to_string();
    }

    pub fn show_context_menu(&self, skin_on: bool, page_on: bool, img_on: bool, autostart_on: bool) {
        unsafe {
            let Ok(menu) = CreatePopupMenu() else {
                return;
            };
            let add = |id: usize, text: &str, flags: windows::Win32::UI::WindowsAndMessaging::MENU_ITEM_FLAGS| {
                let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
                let _ = AppendMenuW(menu, flags, id, PCWSTR(wide.as_ptr()));
            };

            add(CMD_SHOW_PANEL as usize, "打开控制面板", MF_STRING);
            let _ = SetMenuDefaultItem(menu, CMD_SHOW_PANEL, 0);
            let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
            add(
                CMD_TOGGLE_SKIN as usize,
                "原生深色外观（文字 · PPT · 表格）",
                MF_STRING | if skin_on { MF_CHECKED } else { MF_UNCHECKED },
            );
            add(
                CMD_TOGGLE_PAGE as usize,
                "文档页面护眼（文字 · 表格 · PPT）",
                MF_STRING | if page_on { MF_CHECKED } else { MF_UNCHECKED },
            );
            add(
                CMD_TOGGLE_IMG as usize,
                "贴图夜间亮度（独立压暗文档图片）",
                MF_STRING | if img_on { MF_CHECKED } else { MF_UNCHECKED },
            );
            let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
            add(
                CMD_TOGGLE_AUTOSTART as usize,
                "开机自动启动（静默托盘）",
                MF_STRING | if autostart_on { MF_CHECKED } else { MF_UNCHECKED },
            );
            let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
            add(CMD_RESTORE_ALL as usize, "全部一键恢复（保持托盘）", MF_STRING);
            add(CMD_RESTORE_AND_EXIT as usize, "恢复原版并退出", MF_STRING);

            let mut pt = POINT::default();
            let _ = GetCursorPos(&mut pt);
            let _ = SetForegroundWindow(self.hwnd);
            let _ = TrackPopupMenu(
                menu,
                TPM_LEFTALIGN | TPM_BOTTOMALIGN | TPM_RIGHTBUTTON,
                pt.x,
                pt.y,
                None,
                self.hwnd,
                None,
            );
            let _ = PostMessageW(Some(self.hwnd), WM_NULL, WPARAM(0), LPARAM(0));
            let _ = DestroyMenu(menu);
        }
    }

    pub fn remove(&mut self) {
        if self.installed {
            let nid = NOTIFYICONDATAW {
                cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
                hWnd: self.hwnd,
                uID: 1,
                ..Default::default()
            };
            unsafe {
                let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
            }
            self.installed = false;
        }
    }
}

impl Drop for TrayIcon {
    fn drop(&mut self) {
        self.remove();
        unsafe {
            if !self.icon_idle.is_invalid() {
                let _ = DestroyIcon(self.icon_idle);
            }
            if !self.icon_active.is_invalid() {
                let _ = DestroyIcon(self.icon_active);
            }
        }
    }
}
