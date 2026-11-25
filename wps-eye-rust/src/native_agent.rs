//! Pure-Rust native transport to 32-bit WPS Office (Writer / PPT / Spreadsheets) via WpsNativeEyeV3.dll.
//! - Zero external crypto/PE crates: uses Windows CNG BCrypt SHA-256 + lean PE32 export resolver.
//! - Enumerates strictly LIST_MODULES_32BIT and verifies actual loaded module paths against manifest.json.
//! - Protocol v4 atomic timeout safety (never VirtualFreeEx while GUI hook may still touch buffer).

use crate::wps_bridge::{process_name, window_class_name};
use serde::Deserialize;
use std::collections::HashMap;
use std::ffi::c_void;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use windows::core::{BOOL, PWSTR};
use windows::Win32::Foundation::{CloseHandle, FILETIME, HANDLE, HWND, LPARAM, WAIT_OBJECT_0};
use windows::Win32::Security::Cryptography::{
    BCRYPT_ALG_HANDLE, BCRYPT_HASH_HANDLE, BCRYPT_SHA256_ALGORITHM, BCryptCloseAlgorithmProvider,
    BCryptCreateHash, BCryptDestroyHash, BCryptFinishHash, BCryptHashData,
    BCryptOpenAlgorithmProvider,
};
use windows::Win32::System::Diagnostics::Debug::{ReadProcessMemory, WriteProcessMemory};
use windows::Win32::System::Memory::{
    MEM_COMMIT, MEM_RELEASE, MEM_RESERVE, PAGE_READWRITE, VirtualAllocEx, VirtualFreeEx,
};
use windows::Win32::System::ProcessStatus::{
    EnumProcessModulesEx, GetModuleFileNameExW, LIST_MODULES_32BIT,
};
use windows::Win32::System::SystemInformation::GetTickCount64;
use windows::Win32::System::Threading::{
    CreateRemoteThread, GetExitCodeThread, GetProcessTimes, OpenProcess, PROCESS_CREATE_THREAD,
    PROCESS_NAME_FORMAT, PROCESS_QUERY_INFORMATION, PROCESS_QUERY_LIMITED_INFORMATION,
    PROCESS_VM_OPERATION, PROCESS_VM_READ, PROCESS_VM_WRITE, QueryFullProcessImageNameW,
    WaitForSingleObject,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowThreadProcessId, IsWindow, IsWindowVisible,
};

const EMBEDDED_DLL: &[u8] = include_bytes!("../../native/v3/WpsNativeEyeV3.dll");
const EMBEDDED_MANIFEST: &str = include_str!("../../native/v3/manifest.json");

const PROTOCOL_VERSION: u32 = 4;
const MAGIC: u32 = 0x5745_4633;
const HEADER_SIZE: usize = 40;
const RECORD_SIZE: usize = 16 + 96 + 96 + 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AppKind {
    Wps,
    Wpp,
    Et,
}

impl AppKind {
    pub fn as_str(self) -> &'static str {
        match self {
            AppKind::Wps => "wps",
            AppKind::Wpp => "wpp",
            AppKind::Et => "et",
        }
    }
    pub fn exe_name(self) -> &'static str {
        match self {
            AppKind::Wps => "wps.exe",
            AppKind::Wpp => "wpp.exe",
            AppKind::Et => "et.exe",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub pid: u32,
    pub hwnd: isize,
    pub thread: u32,
    pub created_100ns: u64,
    pub app_kind: AppKind,
}

impl Target {
    pub fn identity_key(&self) -> String {
        format!("{}:{}", self.pid, self.created_100ns)
    }
}

#[derive(Debug, Clone)]
pub struct SkinState {
    pub prome: String,
    pub app: String,
    pub prome_dark: i32,
    #[allow(dead_code)]
    pub app_dark: i32,
}

#[derive(Debug, Deserialize)]
struct Manifest {
    protocol: u32,
    agent_sha256: String,
    common_dlls: Vec<String>,
    writer_dlls: Vec<String>,
    wps_files: HashMap<String, String>,
}

fn filetime_to_u64(ft: FILETIME) -> u64 {
    ((ft.dwHighDateTime as u64) << 32) | (ft.dwLowDateTime as u64)
}

pub fn process_creation_time(pid: u32) -> Result<u64, String> {
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid)
            .map_err(|e| format!("OpenProcess({pid}) 失败: {e}"))?;
        let mut c = FILETIME::default();
        let mut e = FILETIME::default();
        let mut k = FILETIME::default();
        let mut u = FILETIME::default();
        let res = GetProcessTimes(h, &mut c, &mut e, &mut k, &mut u);
        let _ = CloseHandle(h);
        res.map_err(|err| format!("GetProcessTimes 失败: {err}"))?;
        Ok(filetime_to_u64(c))
    }
}

pub fn is_process_alive(pid: u32, created_100ns: u64) -> bool {
    matches!(process_creation_time(pid), Ok(actual) if actual == created_100ns)
}

pub fn state_dir() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("WpsNativeEye")
}

pub fn all_wps_targets(preferred_hwnds: &HashMap<u32, isize>) -> Vec<Target> {
    let mut by_pid: HashMap<u32, Vec<(isize, u32, AppKind)>> = HashMap::new();

    unsafe extern "system" fn enum_cb(hwnd: HWND, lp: LPARAM) -> BOOL {
        let map = &mut *(lp.0 as *mut HashMap<u32, Vec<(isize, u32, AppKind)>>);
        if !IsWindowVisible(hwnd).as_bool() {
            return BOOL(1);
        }
        let cls = window_class_name(hwnd);
        let kind_by_cls = match cls.as_str() {
            "OpusApp" => Some(AppKind::Wps),
            "PP12FrameClass" | "PP11FrameClass" => Some(AppKind::Wpp),
            "XLMAIN" => Some(AppKind::Et),
            _ => None,
        };
        let Some(kind) = kind_by_cls else {
            return BOOL(1);
        };
        let mut pid = 0u32;
        let tid = GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if let Ok(exe) = process_name(pid) {
            if exe == kind.exe_name() {
                map.entry(pid).or_default().push((hwnd.0 as isize, tid, kind));
            }
        }
        BOOL(1)
    }

    unsafe {
        let _ = EnumWindows(Some(enum_cb), LPARAM(&mut by_pid as *mut _ as isize));
    }

    let mut out = Vec::new();
    for (pid, entries) in by_pid {
        let wanted = preferred_hwnds.get(&pid).copied();
        let chosen = entries
            .iter()
            .copied()
            .find(|e| Some(e.0) == wanted)
            .unwrap_or(entries[0]);
        if let Ok(created_100ns) = process_creation_time(pid) {
            out.push(Target {
                pid,
                hwnd: chosen.0,
                thread: chosen.1,
                created_100ns,
                app_kind: chosen.2,
            });
        }
    }
    out.sort_by_key(|t| (t.app_kind as u8, t.pid));
    out
}

pub fn sha256_bytes(data: &[u8]) -> Result<String, String> {
    unsafe {
        let mut alg = BCRYPT_ALG_HANDLE::default();
        BCryptOpenAlgorithmProvider(&mut alg, BCRYPT_SHA256_ALGORITHM, None, Default::default())
            .ok()
            .map_err(|e| format!("BCryptOpenAlgorithmProvider 失败: {e}"))?;
        let mut hash = BCRYPT_HASH_HANDLE::default();
        let res = (|| -> Result<String, String> {
            BCryptCreateHash(alg, &mut hash, None, None, 0)
                .ok()
                .map_err(|e| format!("BCryptCreateHash 失败: {e}"))?;
            if !data.is_empty() {
                BCryptHashData(hash, data, 0)
                    .ok()
                    .map_err(|e| format!("BCryptHashData 失败: {e}"))?;
            }
            let mut digest = [0u8; 32];
            BCryptFinishHash(hash, &mut digest, 0)
                .ok()
                .map_err(|e| format!("BCryptFinishHash 失败: {e}"))?;
            let mut hex = String::with_capacity(64);
            for b in digest {
                use std::fmt::Write;
                let _ = write!(&mut hex, "{b:02x}");
            }
            Ok(hex)
        })();
        if !hash.0.is_null() {
            let _ = BCryptDestroyHash(hash);
        }
        let _ = BCryptCloseAlgorithmProvider(alg, 0);
        res
    }
}

pub fn sha256_file(path: &Path) -> Result<String, String> {
    use std::io::Read;
    let mut file = fs::File::open(path).map_err(|e| format!("打开 {} 失败: {e}", path.display()))?;
    unsafe {
        let mut alg = BCRYPT_ALG_HANDLE::default();
        BCryptOpenAlgorithmProvider(&mut alg, BCRYPT_SHA256_ALGORITHM, None, Default::default())
            .ok()
            .map_err(|e| format!("BCryptOpenAlgorithmProvider 失败: {e}"))?;
        let mut hash = BCRYPT_HASH_HANDLE::default();
        let res = (|| -> Result<String, String> {
            BCryptCreateHash(alg, &mut hash, None, None, 0)
                .ok()
                .map_err(|e| format!("BCryptCreateHash 失败: {e}"))?;
            let mut buf = [0u8; 65_536];
            loop {
                let n = file.read(&mut buf).map_err(|e| format!("读取 {} 失败: {e}", path.display()))?;
                if n == 0 {
                    break;
                }
                BCryptHashData(hash, &buf[..n], 0)
                    .ok()
                    .map_err(|e| format!("BCryptHashData 失败: {e}"))?;
            }
            let mut digest = [0u8; 32];
            BCryptFinishHash(hash, &mut digest, 0)
                .ok()
                .map_err(|e| format!("BCryptFinishHash 失败: {e}"))?;
            let mut hex = String::with_capacity(64);
            for b in digest {
                use std::fmt::Write;
                let _ = write!(&mut hex, "{b:02x}");
            }
            Ok(hex)
        })();
        if !hash.0.is_null() {
            let _ = BCryptDestroyHash(hash);
        }
        let _ = BCryptCloseAlgorithmProvider(alg, 0);
        res
    }
}

// ------------------------------------------------------------------
// Minimal PE32 (x86) export table parser (zero external crate!)
// ------------------------------------------------------------------
fn le_u16(b: &[u8], off: usize) -> Result<u16, String> {
    let s = b.get(off..off + 2).ok_or("PE 文件截断")?;
    Ok(u16::from_le_bytes([s[0], s[1]]))
}
fn le_u32(b: &[u8], off: usize) -> Result<u32, String> {
    let s = b.get(off..off + 4).ok_or("PE 文件截断")?;
    Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}
fn cstr_at(b: &[u8], off: usize) -> Result<&str, String> {
    let tail = b.get(off..).ok_or("PE 字符串越界")?;
    let end = tail.iter().position(|&c| c == 0).ok_or("PE 字符串未终结")?;
    std::str::from_utf8(&tail[..end]).map_err(|_| "PE 导出名非 UTF-8".to_string())
}

#[derive(Debug, Clone)]
enum PeExport {
    Rva(u32),
    Forwarder(String),
}

fn find_pe32_export(pe: &[u8], target_name: &str, exact: bool) -> Result<PeExport, String> {
    if pe.len() < 0x40 || &pe[0..2] != b"MZ" {
        return Err("不是有效的 PE 文件".into());
    }
    let e_lfanew = le_u32(pe, 0x3C)? as usize;
    if pe.get(e_lfanew..e_lfanew + 4) != Some(b"PE\0\0") {
        return Err("无效的 PE 签名".into());
    }
    let coff = e_lfanew + 4;
    let machine = le_u16(pe, coff)?;
    if machine != 0x014C {
        return Err(format!("原生组件位数不是 32 位 x86 (0x{machine:04x})"));
    }
    let num_sections = le_u16(pe, coff + 2)? as usize;
    let opt_size = le_u16(pe, coff + 16)? as usize;
    let opt = coff + 20;
    let opt_magic = le_u16(pe, opt)?;
    if opt_magic != 0x010B {
        return Err("不是 PE32 可选头".into());
    }
    let exp_rva = le_u32(pe, opt + 96)?;
    let exp_size = le_u32(pe, opt + 100)?;
    if exp_rva == 0 {
        return Err("PE 没有导出表".into());
    }

    let sec_start = opt + opt_size;
    let mut sections: Vec<(u32, u32, u32)> = Vec::with_capacity(num_sections);
    for i in 0..num_sections {
        let s = sec_start + i * 40;
        let vsize = le_u32(pe, s + 8)?;
        let vaddr = le_u32(pe, s + 12)?;
        let rsize = le_u32(pe, s + 16)?;
        let rptr = le_u32(pe, s + 20)?;
        let span = vsize.max(rsize);
        sections.push((vaddr, span, rptr));
    }

    let rva_to_off = |rva: u32| -> Result<usize, String> {
        for &(va, span, rp) in &sections {
            if rva >= va && rva < va.saturating_add(span) {
                return Ok((rp + (rva - va)) as usize);
            }
        }
        Err(format!("RVA 0x{rva:x} 无法映射到文件偏移"))
    };

    let exp_off = rva_to_off(exp_rva)?;
    let num_names = le_u32(pe, exp_off + 24)? as usize;
    let funcs_rva = le_u32(pe, exp_off + 28)?;
    let names_rva = le_u32(pe, exp_off + 32)?;
    let ords_rva = le_u32(pe, exp_off + 36)?;

    let funcs_off = rva_to_off(funcs_rva)?;
    let names_off = rva_to_off(names_rva)?;
    let ords_off = rva_to_off(ords_rva)?;

    for i in 0..num_names {
        let name_rva = le_u32(pe, names_off + i * 4)?;
        let name_off = rva_to_off(name_rva)?;
        let nm = cstr_at(pe, name_off)?;
        let matched = if exact { nm == target_name } else { nm.contains(target_name) };
        if matched {
            let ord = le_u16(pe, ords_off + i * 2)? as usize;
            let fn_rva = le_u32(pe, funcs_off + ord * 4)?;
            if fn_rva >= exp_rva && fn_rva < exp_rva.saturating_add(exp_size) {
                let fwd_off = rva_to_off(fn_rva)?;
                return Ok(PeExport::Forwarder(cstr_at(pe, fwd_off)?.to_string()));
            }
            return Ok(PeExport::Rva(fn_rva));
        }
    }
    Err(format!("未找到 PE 导出符号: {target_name}"))
}

// ------------------------------------------------------------------
// NativeAgent
// ------------------------------------------------------------------
pub struct NativeAgent {
    manifest: Manifest,
    dll_path: PathBuf,
    dll_name_lower: String,
    entry_rva: u32,
    hash_cache: HashMap<PathBuf, (u128, u64, String)>,
    export_cache: HashMap<(PathBuf, String), PeExport>,
}

impl NativeAgent {
    pub fn new() -> Result<Self, String> {
        let manifest: Manifest =
            serde_json::from_str(EMBEDDED_MANIFEST).map_err(|e| format!("解析 manifest.json 失败: {e}"))?;
        let digest = sha256_bytes(EMBEDDED_DLL)?;
        if digest != manifest.agent_sha256 || manifest.protocol != PROTOCOL_VERSION {
            return Err("内嵌原生组件校验失败，请重新编译原生 DLL 与清单。".into());
        }
        let entry_rva = match find_pe32_export(EMBEDDED_DLL, "RunFrameRequest", false)? {
            PeExport::Rva(rva) => rva,
            PeExport::Forwarder(_) => return Err("RunFrameRequest 不应为转发符号".into()),
        };

        let dll_name = format!("WpsNativeEyeV3-{}.dll", &digest[..12]);
        let dll_path = state_dir().join("agents").join(&digest).join(&dll_name);
        if let Some(p) = dll_path.parent() {
            fs::create_dir_all(p).map_err(|e| e.to_string())?;
        }
        if !dll_path.is_file() {
            let tmp = dll_path.with_extension("tmp");
            fs::write(&tmp, EMBEDDED_DLL).map_err(|e| format!("写入原生组件缓存失败: {e}"))?;
            fs::rename(&tmp, &dll_path).map_err(|e| format!("提交原生组件缓存失败: {e}"))?;
        } else if sha256_file(&dll_path)? != digest {
            return Err("原生组件缓存校验失败，未连接 WPS。".into());
        }

        Ok(Self {
            manifest,
            dll_path,
            dll_name_lower: dll_name.to_ascii_lowercase(),
            entry_rva,
            hash_cache: HashMap::new(),
            export_cache: HashMap::new(),
        })
    }

    fn file_sha256_cached(&mut self, path: &Path) -> Result<String, String> {
        let canon = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        let md = fs::metadata(&canon).map_err(|e| format!("读取 {} 元数据失败: {e}", canon.display()))?;
        let mtime = md
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let size = md.len();
        if let Some((cm, cs, ch)) = self.hash_cache.get(&canon) {
            if *cm == mtime && *cs == size {
                return Ok(ch.clone());
            }
        }
        let digest = sha256_file(&canon)?;
        self.hash_cache.insert(canon, (mtime, size, digest.clone()));
        Ok(digest)
    }

    fn enum_modules_32(handle: HANDLE) -> Result<HashMap<String, (u32, PathBuf)>, String> {
        unsafe {
            let mut hmods = vec![windows::Win32::Foundation::HMODULE::default(); 512];
            let mut needed = 0u32;
            EnumProcessModulesEx(
                handle,
                hmods.as_mut_ptr(),
                (hmods.len() * std::mem::size_of::<windows::Win32::Foundation::HMODULE>()) as u32,
                &mut needed,
                LIST_MODULES_32BIT,
            )
            .map_err(|e| format!("EnumProcessModulesEx 失败: {e}"))?;
            let count = (needed as usize / std::mem::size_of::<windows::Win32::Foundation::HMODULE>())
                .min(hmods.len());
            let mut out = HashMap::with_capacity(count);
            let mut buf = [0u16; 1024];
            for &hm in &hmods[..count] {
                let n = GetModuleFileNameExW(Some(handle), Some(hm), &mut buf);
                if n > 0 {
                    let p = PathBuf::from(String::from_utf16_lossy(&buf[..n as usize]));
                    if let Some(nm) = p.file_name().and_then(|s| s.to_str()) {
                        out.insert(nm.to_ascii_lowercase(), (hm.0 as usize as u32, p));
                    }
                }
            }
            Ok(out)
        }
    }

    fn handle_image_path(handle: HANDLE) -> Result<PathBuf, String> {
        unsafe {
            let mut buf = vec![0u16; 4096];
            let mut size = buf.len() as u32;
            QueryFullProcessImageNameW(
                handle,
                PROCESS_NAME_FORMAT(0),
                PWSTR(buf.as_mut_ptr()),
                &mut size,
            )
            .map_err(|e| format!("QueryFullProcessImageNameW 失败: {e}"))?;
            Ok(PathBuf::from(String::from_utf16_lossy(&buf[..size as usize])))
        }
    }

    fn verify_loaded(
        &mut self,
        handle: HANDLE,
        target: &Target,
        mods: &HashMap<String, (u32, PathBuf)>,
        require_writer: bool,
    ) -> Result<PathBuf, String> {
        let exe_path = Self::handle_image_path(handle)?;
        let exe_canon = fs::canonicalize(&exe_path).unwrap_or_else(|_| exe_path.clone());
        let exe_lower = exe_canon
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let expected_exe_hash = self
            .manifest
            .wps_files
            .get(&exe_lower)
            .cloned()
            .ok_or_else(|| "仅支持已验证的 WPS Office 12.8.2.19823（32 位）；目标进程不符，未执行。".to_string())?;
        if exe_lower != target.app_kind.exe_name() {
            return Err("目标 WPS 进程名不匹配，未执行。".into());
        }
        let root = exe_canon.parent().ok_or("无法确定 WPS 目录")?.to_path_buf();

        let mut required: Vec<String> = self.manifest.common_dlls.clone();
        if require_writer {
            required.extend(self.manifest.writer_dlls.iter().cloned());
        }

        for name in &required {
            if !mods.contains_key(&name.to_ascii_lowercase()) {
                let label = if require_writer { "WPS 文字" } else { "WPS" };
                return Err(format!("{label}界面尚未加载完毕（缺少 {name}），请等待窗口就绪后重试。"));
            }
        }

        if self.file_sha256_cached(&exe_canon)? != expected_exe_hash {
            return Err("仅支持已验证的 WPS Office 12.8.2.19823（32 位）；主程序版本不符，未执行。".into());
        }

        for name in &required {
            let loaded_path = &mods[&name.to_ascii_lowercase()].1;
            let loaded_canon = fs::canonicalize(loaded_path).unwrap_or_else(|_| loaded_path.clone());
            if loaded_canon.parent() != Some(root.as_path()) {
                return Err(format!("检测到异常加载路径（{name}），已中止连接以保护 WPS。"));
            }
            let expected = self
                .manifest
                .wps_files
                .get(name)
                .cloned()
                .ok_or_else(|| format!("清单缺少组件 {name}"))?;
            if self.file_sha256_cached(&loaded_canon)? != expected {
                return Err("仅支持已验证的 WPS Office 12.8.2.19823（32 位）；组件版本不符，未执行。".into());
            }
        }

        for mod_name in mods.keys() {
            if mod_name.starts_with("wpsframenative")
                || (mod_name.starts_with("wpsnativeeyev3") && *mod_name != self.dll_name_lower)
            {
                return Err("该 WPS 窗口仍加载着旧版本原生组件，请保存文档后退出并重开 WPS 一次。".into());
            }
        }

        Ok(root)
    }

    /// Pure read-only pre-flight check (zero DLL injection, zero writes).
    /// Returns verified WPS `office6` directory path.
    pub fn verify_target(&mut self, target: &Target, require_writer: bool) -> Result<PathBuf, String> {
        unsafe {
            let hwnd = HWND(target.hwnd as *mut c_void);
            if !IsWindow(Some(hwnd)).as_bool() {
                return Err("WPS 窗口已关闭或改变，请重试。".into());
            }
            let mut pid = 0u32;
            let tid = GetWindowThreadProcessId(hwnd, Some(&mut pid));
            if pid != target.pid || tid != target.thread {
                return Err("WPS 窗口句柄已失效，请重试。".into());
            }
            let handle = OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, false, target.pid)
                .map_err(|_| "无法连接 WPS，请让控制器与 WPS 使用相同的运行权限。".to_string())?;
            let res = (|| -> Result<PathBuf, String> {
                if process_creation_time(target.pid)? != target.created_100ns {
                    return Err("WPS 进程已经重新启动，请刷新后重试。".into());
                }
                let mods = Self::enum_modules_32(handle)?;
                self.verify_loaded(handle, target, &mods, require_writer)
            })();
            let _ = CloseHandle(handle);
            res
        }
    }

    fn resolve_remote_export(
        &mut self,
        mods: &HashMap<String, (u32, PathBuf)>,
        module_lower: &str,
        symbol: &str,
    ) -> Result<u32, String> {
        let (base, mut path) = mods
            .get(module_lower)
            .cloned()
            .ok_or_else(|| format!("目标进程未加载模块 {module_lower}"))?;
        let key = (path.clone(), symbol.to_string());
        let exp = if let Some(cached) = self.export_cache.get(&key) {
            cached.clone()
        } else {
            let mut bytes = fs::read(&path).map_err(|e| format!("读取 {} 失败: {e}", path.display()))?;
            // Handle 64-bit System32 redirection for 32-bit WOW64 targets -> SysWOW64
            let parsed = match find_pe32_export(&bytes, symbol, true) {
                Ok(e) => e,
                Err(_) => {
                    let is_sys32 = path
                        .parent()
                        .and_then(|p| p.file_name())
                        .map(|n| n.to_string_lossy().eq_ignore_ascii_case("system32"))
                        .unwrap_or(false);
                    if is_sys32 {
                        let wow = path
                            .parent()
                            .unwrap()
                            .parent()
                            .unwrap()
                            .join("SysWOW64")
                            .join(path.file_name().unwrap());
                        path = wow;
                        bytes = fs::read(&path).map_err(|e| format!("读取 SysWOW64 DLL 失败: {e}"))?;
                        find_pe32_export(&bytes, symbol, true)?
                    } else {
                        return Err(format!("无法解析 32 位导出 {module_lower}!{symbol}"));
                    }
                }
            };
            self.export_cache.insert(key, parsed.clone());
            parsed
        };
        match exp {
            PeExport::Rva(rva) => Ok(base.wrapping_add(rva)),
            PeExport::Forwarder(fwd) => {
                let (lib, fn_name) = fwd.rsplit_once('.').ok_or("无效转发导出")?;
                let next_mod = format!("{}.dll", lib.to_ascii_lowercase());
                self.resolve_remote_export(mods, &next_mod, fn_name)
            }
        }
    }

    pub fn run(
        &mut self,
        target: &Target,
        mode: u32,
        records: &[(&str, &str, &str, &str)],
        require_writer: bool,
    ) -> Result<Vec<(String, String, String, String)>, String> {
        if !matches!(mode, 0 | 9 | 10 | 12 | 13 | 14) || records.len() > 20 {
            return Err("不支持的原生请求参数".into());
        }
        unsafe {
            let hwnd = HWND(target.hwnd as *mut c_void);
            if !IsWindow(Some(hwnd)).as_bool() {
                return Err("WPS 窗口已关闭或改变，请重试。".into());
            }
            let mut pid = 0u32;
            let tid = GetWindowThreadProcessId(hwnd, Some(&mut pid));
            if pid != target.pid || tid != target.thread {
                return Err("WPS 窗口已关闭或改变，请重试。".into());
            }
            let access = PROCESS_CREATE_THREAD
                | PROCESS_QUERY_INFORMATION
                | PROCESS_VM_OPERATION
                | PROCESS_VM_WRITE
                | PROCESS_VM_READ;
            let handle = OpenProcess(access, false, target.pid)
                .map_err(|_| "无法连接 WPS，请让控制器与 WPS 使用相同的运行权限。".to_string())?;

            let res = self.run_with_handle(handle, target, mode, records, require_writer);
            let _ = CloseHandle(handle);
            res
        }
    }

    unsafe fn alloc_remote(handle: HANDLE, data: &[u8]) -> Result<*mut c_void, String> {
        let addr = VirtualAllocEx(handle, None, data.len(), MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE);
        if addr.is_null() {
            return Err("VirtualAllocEx 失败".into());
        }
        if let Err(e) = WriteProcessMemory(handle, addr, data.as_ptr() as *const c_void, data.len(), None) {
            let _ = VirtualFreeEx(handle, addr, 0, MEM_RELEASE);
            return Err(format!("WriteProcessMemory 失败: {e}"));
        }
        Ok(addr)
    }

    unsafe fn run_remote_thread(handle: HANDLE, entry: u32, arg: *mut c_void, wait_ms: u32) -> Result<u32, String> {
        let start: Option<unsafe extern "system" fn(*mut c_void) -> u32> =
            Some(std::mem::transmute(entry as usize));
        let th = CreateRemoteThread(handle, None, 0, start, Some(arg), 0, None)
            .map_err(|e| format!("CreateRemoteThread 失败: {e}"))?;
        let wait = WaitForSingleObject(th, wait_ms);
        if wait != WAIT_OBJECT_0 {
            let _ = CloseHandle(th);
            return Err("WPS 未及时响应；本次状态未知，控制器已停止自动重试。".into());
        }
        let mut code = 0u32;
        let ok = GetExitCodeThread(th, &mut code);
        let _ = CloseHandle(th);
        ok.map_err(|e| format!("GetExitCodeThread 失败: {e}"))?;
        Ok(code)
    }

    unsafe fn run_with_handle(
        &mut self,
        handle: HANDLE,
        target: &Target,
        mode: u32,
        records: &[(&str, &str, &str, &str)],
        require_writer: bool,
    ) -> Result<Vec<(String, String, String, String)>, String> {
        if process_creation_time(target.pid)? != target.created_100ns {
            return Err("WPS 进程已经重新启动，请刷新后重试。".into());
        }
        let mut mods = Self::enum_modules_32(handle)?;
        if matches!(mode, 10 | 13) && !mods.contains_key(&self.dll_name_lower) {
            return Ok(Vec::new());
        }
        self.verify_loaded(handle, target, &mods, require_writer)?;

        if !mods.contains_key(&self.dll_name_lower) {
            let wide: Vec<u16> = self
                .dll_path
                .to_string_lossy()
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();
            let raw_bytes = std::slice::from_raw_parts(wide.as_ptr() as *const u8, wide.len() * 2);
            let addr = Self::alloc_remote(handle, raw_bytes)?;
            let load_lib = self.resolve_remote_export(&mods, "kernel32.dll", "LoadLibraryW")?;
            let loaded = Self::run_remote_thread(handle, load_lib, addr, 6000)?;
            let _ = VirtualFreeEx(handle, addr, 0, MEM_RELEASE);
            if loaded == 0 {
                return Err("WPS 未能加载本机原生组件。".into());
            }
            mods = Self::enum_modules_32(handle)?;
        }

        let dll_base = mods
            .get(&self.dll_name_lower)
            .ok_or("未找到已加载的原生组件基址")?
            .0;

        let (expire_ms, remote_wait_ms, poll_secs) = if mode == 14 {
            (11_500u64, 12_000u32, 12.0f64)
        } else {
            (5_500u64, 6_000u32, 6.0f64)
        };
        let expire_tick = GetTickCount64().saturating_add(expire_ms);

        let total_len = HEADER_SIZE + records.len() * RECORD_SIZE;
        let mut payload = vec![0u8; total_len];
        payload[0..4].copy_from_slice(&MAGIC.to_le_bytes());
        payload[4..8].copy_from_slice(&PROTOCOL_VERSION.to_le_bytes());
        payload[8..12].copy_from_slice(&mode.to_le_bytes());
        payload[12..16].copy_from_slice(&(records.len() as u32).to_le_bytes());
        // done=0, status=0 at 16..24
        payload[24..28].copy_from_slice(&target.thread.to_le_bytes());
        payload[28..32].copy_from_slice(&(target.hwnd as u32).to_le_bytes());
        payload[32..40].copy_from_slice(&expire_tick.to_le_bytes());

        let write_field = |dst: &mut [u8], val: &str| -> Result<(), String> {
            let b = val.as_bytes();
            if b.len() >= dst.len() {
                return Err("原生请求字段超长".into());
            }
            dst[..b.len()].copy_from_slice(b);
            Ok(())
        };

        for (i, &(k, st, nm, val)) in records.iter().enumerate() {
            let base = HEADER_SIZE + i * RECORD_SIZE;
            write_field(&mut payload[base..base + 16], k)?;
            write_field(&mut payload[base + 16..base + 112], st)?;
            write_field(&mut payload[base + 112..base + 208], nm)?;
            write_field(&mut payload[base + 208..base + 720], val)?;
        }

        let addr = Self::alloc_remote(handle, &payload)?;
        let code = Self::run_remote_thread(handle, dll_base.wrapping_add(self.entry_rva), addr, remote_wait_ms)?;

        // DLL guarantees GUI hook did NOT take ownership of `addr` for these codes.
        if matches!(code, 10 | 11 | 12 | 13 | 14 | 15 | 17) {
            let _ = VirtualFreeEx(handle, addr, 0, MEM_RELEASE);
            return Err(match code {
                14 => "WPS 界面暂时忙或有弹窗阻塞，本次请求已安全取消，请关闭 WPS 弹窗后重试。".into(),
                15 => "WPS 正在处理上一个请求，请稍后重试。".into(),
                _ => format!("原生组件拒绝连接（{code}），未确认成功。"),
            });
        }

        // Code 0 or 16: GUI hook claimed ownership; poll until done == 1 before freeing!
        let deadline = Instant::now() + Duration::from_secs_f64(poll_secs);
        let mut buf = vec![0u8; total_len];
        while Instant::now() < deadline {
            ReadProcessMemory(handle, addr, buf.as_mut_ptr() as *mut c_void, total_len, None)
                .map_err(|e| format!("ReadProcessMemory 失败: {e}"))?;
            let magic = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
            let version = u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]);
            let count = u32::from_le_bytes([buf[12], buf[13], buf[14], buf[15]]) as usize;
            let done = i32::from_le_bytes([buf[16], buf[17], buf[18], buf[19]]);
            let status = i32::from_le_bytes([buf[20], buf[21], buf[22], buf[23]]);
            let gui = u32::from_le_bytes([buf[24], buf[25], buf[26], buf[27]]);
            let hwnd = u32::from_le_bytes([buf[28], buf[29], buf[30], buf[31]]);

            if done != 0 {
                let _ = VirtualFreeEx(handle, addr, 0, MEM_RELEASE);
                if magic != MAGIC
                    || version != PROTOCOL_VERSION
                    || gui != target.thread
                    || hwnd != (target.hwnd as u32)
                    || count != records.len()
                {
                    return Err("原生回读格式校验失败。".into());
                }
                if status == -5 {
                    return Err("WPS 响应过慢，迟到请求已自动作废以避免覆盖新状态。".into());
                }
                if status != 1 {
                    return Err(format!("WPS 未完成设置（{status}）。"));
                }
                let read_field = |slice: &[u8]| -> String {
                    let end = slice.iter().position(|&c| c == 0).unwrap_or(slice.len());
                    String::from_utf8_lossy(&slice[..end]).into_owned()
                };
                let mut out = Vec::with_capacity(count);
                for i in 0..count {
                    let b = HEADER_SIZE + i * RECORD_SIZE;
                    out.push((
                        read_field(&buf[b..b + 16]),
                        read_field(&buf[b + 16..b + 112]),
                        read_field(&buf[b + 112..b + 208]),
                        read_field(&buf[b + 208..b + 720]),
                    ));
                }
                return Ok(out);
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        // Never VirtualFreeEx on timeout while GUI thread may still access `addr`!
        Err("WPS 界面回调超时；已保留请求内存以防崩溃，请关闭 WPS 弹窗后重试。".into())
    }

    pub fn colors(
        &mut self,
        target: &Target,
        values: Option<&[(u32, u32)]>,
    ) -> Result<HashMap<u32, u32>, String> {
        let owned: Vec<(String, String, String, String)> = match values {
            Some(vals) => vals
                .iter()
                .map(|&(k, v)| ("setting".into(), String::new(), format!("{k:08x}"), format!("{v:08x}")))
                .collect(),
            None => [0xE001_0031u32, 0xE001_0032u32]
                .iter()
                .map(|&k| ("query".into(), String::new(), format!("{k:08x}"), String::new()))
                .collect(),
        };
        let refs: Vec<(&str, &str, &str, &str)> = owned
            .iter()
            .map(|(a, b, c, d)| (a.as_str(), b.as_str(), c.as_str(), d.as_str()))
            .collect();
        let res = self.run(target, 12, &refs, true)?;
        let mut map = HashMap::new();
        for (_, _, k, v) in res {
            let key = u32::from_str_radix(&k, 16).map_err(|_| "回读颜色键解析失败")?;
            let val = u32::from_str_radix(&v, 16).map_err(|_| "回读颜色值解析失败")?;
            map.insert(key, val);
        }
        Ok(map)
    }

    pub fn query_skin(&mut self, target: &Target) -> Result<SkinState, String> {
        let res = self.run(target, 14, &[("query", "state", "", "")], false)?;
        Self::parse_skin_state(&res)
    }

    pub fn switch_skin(&mut self, target: &Target, skin_name: &str, search_dir: &str) -> Result<SkinState, String> {
        let res = self.run(target, 14, &[("switch", "state", skin_name, search_dir)], false)?;
        Self::parse_skin_state(&res)
    }

    fn parse_skin_state(res: &[(String, String, String, String)]) -> Result<SkinState, String> {
        let first = res.first().ok_or("未读取到 WPS 皮肤状态。")?;
        let mut parts = first.3.split('|');
        let app = parts.next().unwrap_or("").to_string();
        let prome_dark = parts.next().and_then(|s| s.parse::<i32>().ok()).unwrap_or(-1);
        let app_dark = parts.next().and_then(|s| s.parse::<i32>().ok()).unwrap_or(-1);
        Ok(SkinState {
            prome: first.2.clone(),
            app,
            prome_dark,
            app_dark,
        })
    }
}
