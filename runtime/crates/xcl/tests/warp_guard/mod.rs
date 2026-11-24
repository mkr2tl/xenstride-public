// Xenstride notice (GPL-2.0-only, section 2a): this file was added relative to ivall/xcl 9aec53d on 2026-11-24; see XENSTRIDE_CHANGES.md at the runtime root.
//! Xenstride P12.1: every integration test binary runs on WARP.
//!
//! The tests pick their D3D12 adapter from `XCL_ADAPTER` (default: the first high-performance
//! adapter, a physical GPU on this machine), and the titles they start inherit it. Before
//! `main` – so before any test thread – this sets `XCL_ADAPTER=warp`, unless
//! `XCL_TEST_HARDWARE_GPU=1` asks for the physical GPU.

extern "C" fn force_warp() {
    if std::env::var("XCL_TEST_HARDWARE_GPU").as_deref() != Ok("1") {
        std::env::set_var("XCL_ADAPTER", "warp");
    }
    if std::env::var_os("XCL_GPU_AUDIT_DIR").is_some() {
        unsafe { atexit(audit) };
    }
}

extern "C" {
    fn atexit(f: extern "C" fn()) -> i32;
}
#[link(name = "kernel32")]
extern "system" {
    fn GetModuleHandleW(name: *const u16) -> isize;
}

/// `XCL_GPU_AUDIT_DIR` set: at exit, one line per test binary in that folder – which D3D12
/// user-mode drivers the process loaded (d3d10warp.dll = WARP; the others a physical GPU).
extern "C" fn audit() {
    let Some(dir) = std::env::var_os("XCL_GPU_AUDIT_DIR") else { return };
    let loaded = |n: &str| {
        let w: Vec<u16> = n.encode_utf16().chain(Some(0)).collect();
        unsafe { GetModuleHandleW(w.as_ptr()) != 0 }
    };
    let list = ["d3d12core.dll", "d3d10warp.dll", "amdxc64.dll", "amdxx64.dll", "nvwgf2umx.dll", "nvldumdx.dll", "igd12umd64.dll"];
    let found: Vec<&str> = list.iter().copied().filter(|n| loaded(n)).collect();
    let exe = std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_default();
    let line = format!("{}\t{}\t{}\n", std::process::id(), exe, found.join(","));
    let path = std::path::Path::new(&dir).join("integration-tests.tsv");
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = f.write_all(line.as_bytes());
    }
}

#[used]
#[link_section = ".CRT$XCU"]
static FORCE_WARP: extern "C" fn() = force_warp;

/// `XCL_ADAPTER` as the guard left it (a test can assert it).
#[allow(dead_code)]
pub fn adapter() -> String {
    std::env::var("XCL_ADAPTER").unwrap_or_default()
}
