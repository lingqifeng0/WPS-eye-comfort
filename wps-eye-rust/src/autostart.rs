//! Native Windows per-user auto-start manager via HKCU\...\Run (zero admin privileges required).
//! Also respects and clears Task Manager's StartupApproved\Run disable override on re-enable.

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use windows::core::{PCWSTR, w};
use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_BINARY, REG_OPTION_NON_VOLATILE,
    REG_SZ, REG_VALUE_TYPE, RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegOpenKeyExW,
    RegQueryValueExW, RegSetValueExW,
};

const RUN_SUBKEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const APPROVED_SUBKEY: PCWSTR =
    w!("Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\StartupApproved\\Run");
const VALUE_NAME: PCWSTR = w!("WpsEyeComfortRust");

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AutoStartStatus {
    /// True iff HKCU\...\Run points to the current executable AND Task Manager has not disabled it.
    pub enabled: bool,
    /// True iff HKCU\...\Run exists for this app but points to a different file path (e.g. moved EXE).
    pub registered_elsewhere: bool,
    /// True iff Windows Task Manager disabled our Run entry in StartupApproved\Run.
    pub disabled_by_taskmgr: bool,
    /// Raw command stored in HKCU\...\Run if present.
    pub registered_cmd: Option<String>,
}

fn to_wide_z(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
}

pub fn current_exe_clean() -> Result<String, String> {
    let exe = std::env::current_exe().map_err(|e| format!("无法获取当前程序路径: {e}"))?;
    let raw = exe.to_string_lossy().into_owned();
    Ok(raw.strip_prefix(r"\\?\").unwrap_or(&raw).to_string())
}

pub fn expected_command() -> Result<String, String> {
    let exe = current_exe_clean()?;
    Ok(format!("\"{exe}\" --minimized --auto-start"))
}

pub fn extract_exe_from_command(cmd: &str) -> String {
    let trimmed = cmd.trim();
    if let Some(rest) = trimmed.strip_prefix('"') {
        if let Some(end) = rest.find('"') {
            return rest[..end].to_string();
        }
    }
    trimmed.split_whitespace().next().unwrap_or("").to_string()
}

fn read_run_value() -> Option<String> {
    unsafe {
        let mut hkey = HKEY::default();
        if RegOpenKeyExW(HKEY_CURRENT_USER, RUN_SUBKEY, Some(0), KEY_QUERY_VALUE, &mut hkey)
            != ERROR_SUCCESS
        {
            return None;
        }
        let mut reg_type = REG_VALUE_TYPE(0);
        let mut cb_data: u32 = 0;
        let query_len = RegQueryValueExW(
            hkey,
            VALUE_NAME,
            None,
            Some(&mut reg_type),
            None,
            Some(&mut cb_data),
        );
        if query_len != ERROR_SUCCESS || cb_data == 0 || reg_type != REG_SZ {
            let _ = RegCloseKey(hkey);
            return None;
        }
        let mut buf = vec![0u8; cb_data as usize];
        let res = RegQueryValueExW(
            hkey,
            VALUE_NAME,
            None,
            Some(&mut reg_type),
            Some(buf.as_mut_ptr()),
            Some(&mut cb_data),
        );
        let _ = RegCloseKey(hkey);
        if res != ERROR_SUCCESS || reg_type != REG_SZ {
            return None;
        }
        let u16_len = (cb_data as usize) / 2;
        let mut wide = Vec::with_capacity(u16_len);
        for chunk in buf[..u16_len * 2].chunks_exact(2) {
            wide.push(u16::from_le_bytes([chunk[0], chunk[1]]));
        }
        while wide.last() == Some(&0) {
            wide.pop();
        }
        Some(String::from_utf16_lossy(&wide))
    }
}

fn is_disabled_in_task_manager() -> bool {
    unsafe {
        let mut hkey = HKEY::default();
        if RegOpenKeyExW(HKEY_CURRENT_USER, APPROVED_SUBKEY, Some(0), KEY_QUERY_VALUE, &mut hkey)
            != ERROR_SUCCESS
        {
            return false;
        }
        let mut reg_type = REG_VALUE_TYPE(0);
        let mut data = [0u8; 16];
        let mut cb_data: u32 = data.len() as u32;
        let res = RegQueryValueExW(
            hkey,
            VALUE_NAME,
            None,
            Some(&mut reg_type),
            Some(data.as_mut_ptr()),
            Some(&mut cb_data),
        );
        let _ = RegCloseKey(hkey);
        if res != ERROR_SUCCESS || reg_type != REG_BINARY || cb_data < 1 {
            return false;
        }
        // Windows StartupApproved convention: even first byte (e.g. 0x02/0x06) = enabled, odd (0x03) = disabled.
        (data[0] & 1) == 1
    }
}

fn clear_task_manager_override() {
    unsafe {
        let mut hkey = HKEY::default();
        if RegOpenKeyExW(HKEY_CURRENT_USER, APPROVED_SUBKEY, Some(0), KEY_SET_VALUE, &mut hkey)
            == ERROR_SUCCESS
        {
            let _ = RegDeleteValueW(hkey, VALUE_NAME);
            let _ = RegCloseKey(hkey);
        }
    }
}

pub fn query_status() -> AutoStartStatus {
    let Some(cmd) = read_run_value() else {
        return AutoStartStatus::default();
    };
    let cur = current_exe_clean().unwrap_or_default();
    let reg_exe = extract_exe_from_command(&cmd);
    let matches_current = !cur.is_empty() && reg_exe.eq_ignore_ascii_case(&cur);
    let disabled_by_taskmgr = is_disabled_in_task_manager();
    AutoStartStatus {
        enabled: matches_current && !disabled_by_taskmgr,
        registered_elsewhere: !matches_current,
        disabled_by_taskmgr,
        registered_cmd: Some(cmd),
    }
}

pub fn set_enabled(enable: bool) -> Result<AutoStartStatus, String> {
    if enable {
        let cmd = expected_command()?;
        let wide = to_wide_z(&cmd);
        let bytes: &[u8] =
            unsafe { std::slice::from_raw_parts(wide.as_ptr() as *const u8, wide.len() * 2) };
        unsafe {
            let mut hkey = HKEY::default();
            let create_res = RegCreateKeyExW(
                HKEY_CURRENT_USER,
                RUN_SUBKEY,
                Some(0),
                PCWSTR::null(),
                REG_OPTION_NON_VOLATILE,
                KEY_SET_VALUE,
                None,
                &mut hkey,
                None,
            );
            if create_res != ERROR_SUCCESS {
                return Err(format!("无法打开当前用户启动项注册表键（错误码 {}）。", create_res.0));
            }
            let set_res = RegSetValueExW(hkey, VALUE_NAME, Some(0), REG_SZ, Some(bytes));
            let _ = RegCloseKey(hkey);
            if set_res != ERROR_SUCCESS {
                return Err(format!("写入开机自启注册表项失败（错误码 {}）。", set_res.0));
            }
        }
        clear_task_manager_override();
    } else {
        unsafe {
            let mut hkey = HKEY::default();
            if RegOpenKeyExW(HKEY_CURRENT_USER, RUN_SUBKEY, Some(0), KEY_SET_VALUE, &mut hkey)
                == ERROR_SUCCESS
            {
                let _ = RegDeleteValueW(hkey, VALUE_NAME);
                let _ = RegCloseKey(hkey);
            }
        }
        clear_task_manager_override();
    }
    Ok(query_status())
}

#[cfg(test)]
mod tests {
    use super::extract_exe_from_command;

    #[test]
    fn parses_quoted_and_unquoted_run_commands() {
        assert_eq!(
            extract_exe_from_command(
                r#""C:\Program Files\My Tools\WPS-Eye-Comfort-Rust.exe" --minimized --auto-start"#
            ),
            r"C:\Program Files\My Tools\WPS-Eye-Comfort-Rust.exe"
        );
        assert_eq!(
            extract_exe_from_command(r"E:\Tools\wps-eye.exe --minimized"),
            r"E:\Tools\wps-eye.exe"
        );
    }
}
