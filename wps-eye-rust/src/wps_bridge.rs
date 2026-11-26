//! Attach to visible WPS Writer windows via AccessibleObjectFromWindow(OBJID_NATIVEOM).
//! Never creates a hidden COM WPS instance.

use std::collections::HashSet;
use std::ffi::c_void;
use windows::core::{BOOL, BSTR, GUID, Interface, PCWSTR, PWSTR};
use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM};
use windows::Win32::System::Com::{
    DISPATCH_METHOD, DISPATCH_PROPERTYGET, DISPATCH_PROPERTYPUT, DISPPARAMS, IDispatch,
};
use windows::Win32::System::Ole::DISPID_PROPERTYPUT;
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_FORMAT, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::UI::Accessibility::AccessibleObjectFromWindow;
use windows::Win32::UI::WindowsAndMessaging::{
    EnumChildWindows, EnumWindows, GetClassNameW, GetWindowThreadProcessId, IsWindowVisible,
};

const OBJID_NATIVEOM: u32 = 0xFFFF_FFF0;

pub fn process_image_path_by_pid(pid: u32) -> Result<std::path::PathBuf, String> {
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid)
            .map_err(|e| format!("OpenProcess({pid}) 失败: {e}"))?;
        let mut buf = vec![0u16; 4096];
        let mut size = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_FORMAT(0),
            PWSTR(buf.as_mut_ptr()),
            &mut size,
        );
        let _ = CloseHandle(handle);
        ok.map_err(|e| format!("QueryFullProcessImageNameW 失败: {e}"))?;
        let path_str = String::from_utf16_lossy(&buf[..size as usize]);
        Ok(std::path::PathBuf::from(path_str))
    }
}

pub fn process_name(pid: u32) -> Result<String, String> {
    let p = process_image_path_by_pid(pid)?;
    p.file_name()
        .map(|s| s.to_string_lossy().to_ascii_lowercase())
        .ok_or_else(|| "无法获取进程文件名".to_string())
}

pub fn window_class_name(hwnd: HWND) -> String {
    unsafe {
        let mut buf = [0u16; 256];
        let n = GetClassNameW(hwnd, &mut buf);
        if n <= 0 {
            String::new()
        } else {
            String::from_utf16_lossy(&buf[..n as usize])
        }
    }
}

#[derive(Clone)]
pub struct ComObj(pub IDispatch);

impl ComObj {
    fn dispid(&self, name: &str) -> Result<i32, String> {
        let mut wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
        let mut name_ptr = PCWSTR(wide.as_mut_ptr());
        let mut id: i32 = 0;
        unsafe {
            self.0
                .GetIDsOfNames(&GUID::zeroed(), &mut name_ptr as *mut PCWSTR, 1, 0x0400, &mut id)
                .map_err(|e| format!("COM GetIDsOfNames({name}) 失败: {e}"))?;
        }
        Ok(id)
    }

    pub fn get_prop(&self, name: &str) -> Result<VARIANT, String> {
        let id = self.dispid(name)?;
        let dp = DISPPARAMS::default();
        let mut result = VARIANT::default();
        unsafe {
            self.0
                .Invoke(
                    id,
                    &GUID::zeroed(),
                    0x0400,
                    DISPATCH_PROPERTYGET,
                    &dp,
                    Some(&mut result),
                    None,
                    None,
                )
                .map_err(|e| format!("COM Get({name}) 失败: {e}"))?;
        }
        Ok(result)
    }

    pub fn get_obj(&self, name: &str) -> Result<ComObj, String> {
        let v = self.get_prop(name)?;
        let disp = IDispatch::try_from(&v).map_err(|e| format!("COM 对象转换({name}) 失败: {e}"))?;
        Ok(ComObj(disp))
    }

    pub fn get_bool(&self, name: &str) -> Result<bool, String> {
        let v = self.get_prop(name)?;
        bool::try_from(&v).map_err(|e| format!("COM bool 转换({name}) 失败: {e}"))
    }

    pub fn get_i32(&self, name: &str) -> Result<i32, String> {
        let v = self.get_prop(name)?;
        i32::try_from(&v).map_err(|e| format!("COM i32 转换({name}) 失败: {e}"))
    }

    pub fn put_bool(&self, name: &str, val: bool) -> Result<(), String> {
        let id = self.dispid(name)?;
        let mut arg = VARIANT::from(val);
        let mut named = DISPID_PROPERTYPUT;
        let dp = DISPPARAMS {
            rgvarg: &mut arg,
            rgdispidNamedArgs: &mut named,
            cArgs: 1,
            cNamedArgs: 1,
        };
        unsafe {
            self.0
                .Invoke(
                    id,
                    &GUID::zeroed(),
                    0x0400,
                    DISPATCH_PROPERTYPUT,
                    &dp,
                    None,
                    None,
                    None,
                )
                .map_err(|e| format!("COM Put({name}) 失败: {e}"))?;
        }
        Ok(())
    }

    pub fn call_str_arg(&self, name: &str, arg_str: &str) -> Result<VARIANT, String> {
        let id = self.dispid(name)?;
        let mut arg = VARIANT::from(BSTR::from(arg_str));
        let dp = DISPPARAMS {
            rgvarg: &mut arg,
            rgdispidNamedArgs: std::ptr::null_mut(),
            cArgs: 1,
            cNamedArgs: 0,
        };
        let mut result = VARIANT::default();
        unsafe {
            self.0
                .Invoke(
                    id,
                    &GUID::zeroed(),
                    0x0400,
                    DISPATCH_METHOD | DISPATCH_PROPERTYGET,
                    &dp,
                    Some(&mut result),
                    None,
                    None,
                )
                .map_err(|e| format!("COM Call({name}, {arg_str}) 失败: {e}"))?;
        }
        Ok(result)
    }
}

fn native_dispatch(hwnd: HWND) -> Result<ComObj, String> {
    let mut ptr: *mut c_void = std::ptr::null_mut();
    unsafe {
        let hr = AccessibleObjectFromWindow(hwnd, OBJID_NATIVEOM, &IDispatch::IID, &mut ptr);
        if let Err(e) = hr {
            return Err(format!("WPS 窗口未提供原生接口 ({e})"));
        }
        if ptr.is_null() {
            return Err("WPS 窗口未提供原生接口（空指针）".into());
        }
        // Take ownership of the COM reference returned by AccessibleObjectFromWindow.
        let disp = IDispatch::from_raw(ptr);
        Ok(ComObj(disp))
    }
}

#[derive(Clone)]
pub struct WriterSession {
    pub app: ComObj,
    pub pid: u32,
    pub hwnd: HWND,
    pub doc_count: i32,
}

impl WriterSession {
    pub fn live_doc_count(&self) -> i32 {
        self.app
            .get_obj("Documents")
            .and_then(|d| d.get_i32("Count"))
            .unwrap_or(0)
    }

    pub fn get_blue_screen(&self) -> Result<bool, String> {
        self.app.get_obj("Options")?.get_bool("BlueScreen")
    }

    pub fn set_blue_screen(&self, val: bool) -> Result<(), String> {
        let opts = self.app.get_obj("Options")?;
        opts.put_bool("BlueScreen", val)?;
        if opts.get_bool("BlueScreen")? != val {
            return Err("WPS 未确认深色页内显示已切换。".into());
        }
        Ok(())
    }

    pub fn get_protect_eyes(&self) -> Result<bool, String> {
        let bars = self.app.get_obj("CommandBars")?;
        let v = bars.call_str_arg("GetPressedMso", "ProtectEyes")?;
        bool::try_from(&v).map_err(|e| format!("读取 ProtectEyes 状态失败: {e}"))
    }

    pub fn set_protect_eyes(&self, desired: bool) -> Result<(), String> {
        let bars = self.app.get_obj("CommandBars")?;
        let cur = bool::try_from(&bars.call_str_arg("GetPressedMso", "ProtectEyes")?)
            .map_err(|e| format!("读取 ProtectEyes 状态失败: {e}"))?;
        if cur != desired {
            let _ = bars.call_str_arg("ExecuteMso", "ProtectEyes")?;
        }
        let after = bool::try_from(&bars.call_str_arg("GetPressedMso", "ProtectEyes")?)
            .map_err(|e| format!("回读 ProtectEyes 状态失败: {e}"))?;
        if after != desired {
            let word = if desired { "恢复开启" } else { "关闭" };
            return Err(format!("WPS 自带绿色护眼未能{word}，未确认操作成功。"));
        }
        Ok(())
    }
}

pub fn enumerate_writer_sessions() -> Vec<WriterSession> {
    use std::collections::HashMap;

    struct TopHost {
        hwnd: HWND,
        pid: u32,
        is_opus: bool,
        visible: bool,
    }

    let mut hosts: Vec<TopHost> = Vec::new();
    unsafe extern "system" fn enum_top(hwnd: HWND, lp: LPARAM) -> BOOL {
        let out = &mut *(lp.0 as *mut Vec<TopHost>);
        let cls = window_class_name(hwnd);
        let is_opus = cls == "OpusApp";
        let is_wps_host = is_opus
            || cls == "XLMAIN"
            || cls == "PP12FrameClass"
            || cls == "PP11FrameClass"
            || cls == "KPromeMainWindow";
        if !is_wps_host {
            return BOOL(1);
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if let Ok(name) = process_name(pid) {
            if matches!(name.as_str(), "wps.exe" | "et.exe" | "wpp.exe") {
                let visible = IsWindowVisible(hwnd).as_bool();
                out.push(TopHost {
                    hwnd,
                    pid,
                    is_opus,
                    visible,
                });
            }
        }
        BOOL(1)
    }

    unsafe {
        let _ = EnumWindows(Some(enum_top), LPARAM(&mut hosts as *mut _ as isize));
    }

    // Prefer visible OpusApp first, then hidden OpusApp, then other WPS multi-tab host windows.
    hosts.sort_by_key(|h| (!h.is_opus, !h.visible));

    let mut opus_by_pid: HashMap<u32, HWND> = HashMap::new();
    for h in &hosts {
        if h.is_opus {
            opus_by_pid.entry(h.pid).or_insert(h.hwnd);
        }
    }

    // Collect _WwG child windows across ALL WPS hosts (handles WPS All-in-One multi-tab mode
    // where a Writer _WwG tab may be hosted under an active PPT/Excel/Prometheus frame!).
    let mut wwg_by_pid: HashMap<u32, Vec<(HWND, bool)>> = HashMap::new();
    for h in &hosts {
        let mut children: Vec<HWND> = Vec::new();
        unsafe extern "system" fn enum_child(child: HWND, lp: LPARAM) -> BOOL {
            let out = &mut *(lp.0 as *mut Vec<HWND>);
            if window_class_name(child) == "_WwG" {
                out.push(child);
            }
            BOOL(1)
        }
        unsafe {
            let _ = EnumChildWindows(
                Some(h.hwnd),
                Some(enum_child),
                LPARAM(&mut children as *mut _ as isize),
            );
        }
        for child in children {
            let mut cpid = 0u32;
            unsafe {
                GetWindowThreadProcessId(child, Some(&mut cpid));
            }
            if let Ok(name) = process_name(cpid) {
                if name == "wps.exe" {
                    let cvis = unsafe { IsWindowVisible(child).as_bool() };
                    wwg_by_pid.entry(cpid).or_default().push((child, cvis));
                }
            }
        }
    }

    let mut sessions = Vec::new();
    let mut seen_pids: HashSet<u32> = HashSet::new();

    for (pid, mut candidates) in wwg_by_pid {
        if seen_pids.contains(&pid) {
            continue;
        }
        // Visible _WwG windows first
        candidates.sort_by_key(|&(_, vis)| !vis);
        let root_hwnd = opus_by_pid
            .get(&pid)
            .copied()
            .unwrap_or(candidates[0].0);

        for (child, _) in candidates {
            let Ok(obj) = native_dispatch(child) else {
                continue;
            };
            let Ok(app) = obj.get_obj("Application") else {
                continue;
            };
            let doc_count = app
                .get_obj("Documents")
                .and_then(|d| d.get_i32("Count"))
                .unwrap_or(0);
            sessions.push(WriterSession {
                app,
                pid,
                hwnd: root_hwnd,
                doc_count,
            });
            seen_pids.insert(pid);
            break;
        }
    }
    sessions
}
