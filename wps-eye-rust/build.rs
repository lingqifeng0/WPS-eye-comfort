use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

fn find_rc() -> Option<PathBuf> {
    let kits = Path::new(r"C:\Program Files (x86)\Windows Kits\10\bin");
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(kits) {
        for e in entries.flatten() {
            let p = e.path().join("x64").join("rc.exe");
            if p.is_file() {
                candidates.push(p);
            }
        }
    }
    candidates.sort();
    candidates.pop()
}

fn find_cvtres() -> Option<PathBuf> {
    let vs_roots = [
        r"C:\Program Files (x86)\Microsoft Visual Studio\18\BuildTools\VC\Tools\MSVC",
        r"C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Tools\MSVC",
        r"C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Tools\MSVC",
    ];
    let mut candidates: Vec<PathBuf> = Vec::new();
    for root in vs_roots {
        if let Ok(entries) = std::fs::read_dir(root) {
            for e in entries.flatten() {
                let p = e.path().join("bin").join("Hostx64").join("x64").join("cvtres.exe");
                if p.is_file() {
                    candidates.push(p);
                }
            }
        }
    }
    candidates.sort();
    candidates.pop()
}

fn main() {
    println!("cargo:rerun-if-changed=assets/app.rc");
    println!("cargo:rerun-if-changed=assets/app.manifest");
    println!("cargo:rerun-if-changed=assets/wps-eye.ico");
    println!("cargo:rerun-if-changed=assets/wps-eye-active.ico");
    println!("cargo:rerun-if-changed=../native/v3/WpsNativeEyeV3.dll");
    println!("cargo:rerun-if-changed=../native/v3/manifest.json");

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let rc_file = manifest_dir.join("assets").join("app.rc");
    let res_file = out_dir.join("app.res");
    let obj_file = out_dir.join("app_res.o");

    if let (Some(rc), Some(cvtres)) = (find_rc(), find_cvtres()) {
        let s1 = Command::new(&rc)
            .arg("/nologo")
            .arg("/fo")
            .arg(&res_file)
            .arg(&rc_file)
            .current_dir(manifest_dir.join("assets"))
            .status();
        if matches!(s1, Ok(s) if s.success()) && res_file.is_file() {
            let s2 = Command::new(&cvtres)
                .arg("/nologo")
                .arg("/MACHINE:X64")
                .arg(format!("/OUT:{}", obj_file.display()))
                .arg(&res_file)
                .status();
            if matches!(s2, Ok(s) if s.success()) && obj_file.is_file() {
                println!("cargo:rustc-link-arg-bins={}", obj_file.display());
                return;
            }
        }
    }
    println!("cargo:warning=rc.exe or cvtres.exe failed; EXE resource icon/manifest skipped");
}
