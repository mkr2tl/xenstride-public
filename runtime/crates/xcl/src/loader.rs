// Xenstride notice (GPL-2.0-only, section 2a): this file was changed relative to ivall/xcl 9aec53d on 2026-11-22 to 2026-11-23; see XENSTRIDE_CHANGES.md at the runtime root.
//! The XCL PE loader: maps console executables/DLLs into the current process and wires
//! them to HLE modules, game DLLs and real Windows DLLs (design §5.1).

use std::ffi::c_void;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use windows_sys::Win32::System::Memory::{
    VirtualAlloc, VirtualFree, VirtualProtect, MEM_COMMIT, MEM_RELEASE, MEM_RESERVE, PAGE_EXECUTE,
    PAGE_EXECUTE_READ, PAGE_EXECUTE_READWRITE, PAGE_NOACCESS, PAGE_READONLY, PAGE_READWRITE,
};

use crate::modules::{self, MappedModule};
use crate::pe::{self, Layout, OwnedExports, Pe};
use crate::resolver;

static CACHE_DIR: OnceLock<PathBuf> = OnceLock::new();
/// Serialises loads so that two threads calling LoadLibrary on the same game DLL do not
/// map it twice. Re-entrant loads (import resolution mapping dependencies) happen on the
/// same thread, so a re-entrant flag is used instead of holding the mutex across them.
static LOAD_LOCK: Mutex<()> = Mutex::new(());
thread_local! {
    static IN_LOAD: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}
/// Set once the main image is ready; thread notifications are ignored before that.
static READY: AtomicBool = AtomicBool::new(false);

pub fn set_cache_dir(p: PathBuf) {
    let _ = CACHE_DIR.set(p);
}

fn cache_dir() -> PathBuf {
    CACHE_DIR
        .get()
        .cloned()
        .unwrap_or_else(|| std::env::temp_dir().join("xcl_cache"))
}

fn section_protection(ch: u32) -> u32 {
    let x = ch & pe::SCN_MEM_EXECUTE != 0;
    let r = ch & pe::SCN_MEM_READ != 0;
    let w = ch & pe::SCN_MEM_WRITE != 0;
    match (x, r, w) {
        (true, _, true) => PAGE_EXECUTE_READWRITE,
        (true, true, false) => PAGE_EXECUTE_READ,
        (true, false, false) => PAGE_EXECUTE,
        (false, _, true) => PAGE_READWRITE,
        (false, true, false) => PAGE_READONLY,
        (false, false, false) => PAGE_NOACCESS,
    }
}

type TlsCallback = unsafe extern "system" fn(*mut c_void, u32, *mut c_void);
type DllMain = unsafe extern "system" fn(*mut c_void, u32, *mut c_void) -> i32;
type ExeEntry = unsafe extern "system" fn(*mut c_void) -> u32;

/// Upper bound of the TLS callback array walk (launch-prep hardening).
pub(crate) const MAX_TLS_CALLBACKS: usize = 256;

/// launch-prep hardening: byte width patched by a base relocation this loader applies, or an
/// error for any other type (previously only logged, leaving the target unrelocated).
pub(crate) fn reloc_width(kind: u16) -> Result<usize, String> {
    match kind {
        pe::IMAGE_REL_BASED_DIR64 => Ok(8),
        pe::IMAGE_REL_BASED_HIGHLOW => Ok(4),
        k => Err(format!("unsupported relocation type {k}")),
    }
}

/// launch-prep hardening: every relocation must be of a supported type and patch bytes inside
/// the image (the mapping loop writes `base + rva` without any bound).
pub(crate) fn check_relocs(relocs: &[pe::Reloc], image_size: usize) -> Result<(), String> {
    for r in relocs {
        let w = reloc_width(r.kind).map_err(|e| format!("{e} @ {:#x}", r.rva))?;
        if (r.rva as usize).checked_add(w).map_or(true, |end| end > image_size) {
            return Err(format!("relocation @ {:#x} (+{w}) outside the image ({image_size:#x})", r.rva));
        }
    }
    Ok(())
}

/// launch-prep hardening: bounded TLS callback walk. `read_u64(rva)` reads one pointer of the
/// image; every callback must be a VA inside the image whose RVA satisfies `is_exec`.
pub(crate) fn walk_tls_callbacks(
    read_u64: impl Fn(u32) -> Option<u64>,
    image_base: u64,
    image_size: u64,
    array_va: u64,
    is_exec: impl Fn(u64) -> bool,
) -> Result<Vec<u64>, String> {
    let mut out = Vec::new();
    let mut rva = array_va
        .checked_sub(image_base)
        .filter(|r| *r < image_size)
        .ok_or_else(|| format!("TLS callback array {array_va:#x} outside the image"))?;
    loop {
        if rva + 8 > image_size {
            return Err("TLS callback array runs past the image".into());
        }
        let cb = read_u64(rva as u32).ok_or_else(|| format!("TLS callback slot @ {rva:#x} not readable"))?;
        if cb == 0 {
            return Ok(out);
        }
        if out.len() == MAX_TLS_CALLBACKS {
            return Err(format!("more than {MAX_TLS_CALLBACKS} TLS callbacks (unterminated array?)"));
        }
        let cb_rva = cb
            .checked_sub(image_base)
            .filter(|r| *r < image_size)
            .ok_or_else(|| format!("TLS callback {cb:#x} outside the image"))?;
        if !is_exec(cb_rva) {
            return Err(format!("TLS callback {cb:#x} not in an executable section"));
        }
        out.push(cb);
        rva += 8;
    }
}

/// launch-prep hardening: TLS template and index slot must lie inside the image.
pub(crate) fn check_tls_dir(t: &pe::TlsDir, image_base: u64, image_size: u64) -> Result<(), String> {
    let end = image_base + image_size;
    if t.end_raw < t.start_raw {
        return Err("reversed TLS template".into());
    }
    if (t.start_raw != 0 || t.end_raw != 0) && !(image_base <= t.start_raw && t.end_raw <= end) {
        return Err(format!("TLS template {:#x}..{:#x} outside the image", t.start_raw, t.end_raw));
    }
    // 0: the image has no index variable (upstream c69c4d1 loads it without a write through 0)
    if t.address_of_index != 0 && !(image_base <= t.address_of_index && t.address_of_index + 4 <= end) {
        return Err(format!("TLS index {:#x} outside the image", t.address_of_index));
    }
    Ok(())
}

/// launch-prep hardening: structural checks done on the file BEFORE any memory is reserved, so
/// a rejected image never reaches the module list. Returns the TLS callback VAs (preferred base).
fn validate_image(pe: &Pe) -> Result<Vec<u64>, String> {
    let size = pe.size_of_image as u64;
    let relocs = pe.relocations().map_err(|e| format!("relocations: {e}"))?;
    check_relocs(&relocs, size as usize)?;
    let tls = pe.tls().map_err(|e| format!("TLS directory: {e}"))?;
    let Some(t) = tls else { return Ok(Vec::new()) };
    check_tls_dir(&t, pe.image_base, size)?;
    if t.address_of_callbacks == 0 {
        return Ok(Vec::new());
    }
    let exec = |rva: u64| {
        pe.sections.iter().any(|s| {
            s.characteristics & pe::SCN_MEM_EXECUTE != 0
                && (s.va as u64) <= rva
                && rva < s.va as u64 + s.mapped_size() as u64
        })
    };
    walk_tls_callbacks(|r| pe.u64_at(r), pe.image_base, size, t.address_of_callbacks, exec)
}

/// launch-prep hardening: opt-in policy (XCL_STRICT_IMPORTS=1) that refuses an image whose
/// imports fell back to auto-stubs, instead of only recording them.
fn strict_imports() -> bool {
    std::env::var("XCL_STRICT_IMPORTS").map(|v| v == "1").unwrap_or(false)
}

/// Map an image, apply relocations, resolve imports, set up TLS / exception data and
/// page protections. Does NOT run any code of the image.
pub fn map_image(path: &Path) -> Result<Arc<MappedModule>, String> {
    map_image_ex(path, false)
}

fn map_image_ex(path: &Path, is_main: bool) -> Result<Arc<MappedModule>, String> {
    let file = std::fs::read(path).map_err(|e| format!("reading {}: {e}", path.display()))?;
    let pe = Pe::parse(&file, Layout::File).map_err(|e| format!("{}: {e}", path.display()))?;
    let name = crate::util::dll_key(&path.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default());
    let size = pe.size_of_image as usize;
    // 0. launch-prep hardening: reject malformed relocation / TLS data before mapping
    let tls_callbacks_pref = validate_image(&pe).map_err(|e| format!("{name}: {e}"))?;

    // 1. reserve + commit
    let mut base = unsafe {
        VirtualAlloc(pe.image_base as *const c_void, size, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE)
    } as usize;
    if base == 0 {
        if !pe.has_relocs() {
            return Err(format!(
                "{name}: preferred address {:#x} is taken and the image has no relocations",
                pe.image_base
            ));
        }
        base = unsafe { VirtualAlloc(std::ptr::null(), size, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE) } as usize;
        if base == 0 {
            return Err(format!("{name}: VirtualAlloc({size:#x}) failed"));
        }
    }
    let delta = base as i64 - pe.image_base as i64;
    let fail = |msg: String| {
        unsafe { VirtualFree(base as *mut c_void, 0, MEM_RELEASE) };
        Err(msg)
    };

    // 2. headers + sections
    unsafe {
        let hdr = (pe.size_of_headers as usize).min(file.len()).min(size);
        std::ptr::copy_nonoverlapping(file.as_ptr(), base as *mut u8, hdr);
        for s in &pe.sections {
            let raw = (s.raw_size as usize).min(s.mapped_size() as usize);
            if raw == 0 {
                continue;
            }
            let src = s.raw_ptr as usize;
            if src + raw > file.len() || s.va as usize + raw > size {
                return fail(format!("{name}: section {} outside the file/image", s.name));
            }
            std::ptr::copy_nonoverlapping(file.as_ptr().add(src), (base + s.va as usize) as *mut u8, raw);
        }
    }

    // 3. relocations
    if delta != 0 {
        let relocs = match pe.relocations() {
            Ok(r) => r,
            Err(e) => return fail(format!("{name}: relocations: {e}")),
        };
        for r in relocs {
            let p = base + r.rva as usize;
            unsafe {
                match r.kind {
                    pe::IMAGE_REL_BASED_DIR64 => {
                        let v = (p as *mut u64).read_unaligned();
                        (p as *mut u64).write_unaligned((v as i64 + delta) as u64);
                    }
                    pe::IMAGE_REL_BASED_HIGHLOW => {
                        let v = (p as *mut u32).read_unaligned();
                        (p as *mut u32).write_unaligned((v as i64 + delta) as u32);
                    }
                    // unreachable after validate_image; kept fatal rather than a warning
                    k => return fail(format!("{name}: unsupported relocation type {k} @ {:#x}", r.rva)),
                }
            }
        }
    }

    let exports = OwnedExports::from_pe(&pe).unwrap_or_default();
    let module = Arc::new(MappedModule {
        name: name.clone(),
        path: path.to_path_buf(),
        base,
        size,
        entry_rva: pe.entry_rva,
        is_dll: pe.is_dll(),
        exports,
        tls_callbacks: Mutex::new(Vec::new()),
        no_thread_calls: AtomicBool::new(false),
        initialized: AtomicBool::new(false),
        stack_reserve: pe.stack_reserve,
    });
    // registered before import resolution so cyclic game-DLL imports terminate
    modules::register_mapped(module.clone());
    if is_main {
        // game DLLs initialised during import resolution may already ask for
        // GetModuleHandle(NULL) / GetModuleFileName(NULL)
        modules::set_main(module.clone());
    }
    crate::xlog_info!(
        "loaded {name} @ {base:#x} (size {size:#x}, preferred {:#x}{})",
        pe.image_base,
        if delta != 0 { ", relocated" } else { "" }
    );

    // 4. imports (normal + delay-load, resolved eagerly). A failure from here on takes the
    // module out of the registry again: a later load must not find it (its memory freed, or
    // never initialised) and take it for loaded
    let mut all = match pe.imports() {
        Ok(v) => v,
        Err(e) => {
            // nothing was resolved against it yet: its memory goes too
            modules::unregister_mapped(base);
            return fail(format!("{name}: import table: {e}"));
        }
    };
    match pe.delay_imports() {
        Ok(v) => all.extend(v),
        Err(e) => crate::xlog_warn!("{name}: delay-imports: {e}"),
    }
    for dll in &all {
        for e in &dll.entries {
            let addr = resolver::resolve_import(&dll.dll, &e.what, &name, dll.delay);
            unsafe { ((base + e.iat_rva as usize) as *mut u64).write_unaligned(addr as u64) };
        }
        if dll.delay && dll.module_handle_rva != 0 {
            let h = resolver::load_library(&dll.dll).unwrap_or(1);
            unsafe { ((base + dll.module_handle_rva as usize) as *mut u64).write_unaligned(h as u64) };
        }
    }
    if strict_imports() {
        let stubbed: Vec<String> = resolver::records()
            .into_iter()
            .filter(|r| r.importer == name && r.source == resolver::Source::Stub)
            .map(|r| format!("{}!{}", r.dll, r.function))
            .collect();
        if !stubbed.is_empty() {
            return Err(format!(
                "{name}: XCL_STRICT_IMPORTS=1 and {} import(s) resolved to auto-stubs: {}",
                stubbed.len(),
                stubbed.join(", ")
            ));
        }
    }

    // 5. static TLS (validated in step 0; a parse error is no longer ignored)
    if let Some(t) = pe.tls().map_err(|e| format!("{name}: TLS directory: {e}"))? {
        let adj = |va: u64| (va as i64 + delta) as usize;
        let template = adj(t.start_raw);
        let template_len = t.template_len() as usize;
        let index_ptr = if t.address_of_index == 0 { std::ptr::null_mut() } else { adj(t.address_of_index) as *mut u32 };
        unsafe {
            // game DLLs loaded while its imports were resolved may point into it: its memory stays
            tls_install(&module, template, template_len, t.total_len(), index_ptr).inspect_err(|_| modules::unregister_mapped(base))?;
            if t.address_of_callbacks != 0 {
                // bounded: the validated list (preferred-base VAs) rebased by the load delta,
                // instead of an unbounded `while *cb != 0` walk of mapped memory
                let mut list = module.tls_callbacks.lock().unwrap();
                list.extend(tls_callbacks_pref.iter().map(|&cb| adj(cb)));
            }
        }
    }

    // 6. page protections (launch-prep hardening: results checked)
    unsafe {
        let mut old = 0u32;
        let mut failed = Vec::new();
        if VirtualProtect(base as *const c_void, pe.size_of_headers as usize, PAGE_READONLY, &mut old) == 0 {
            failed.push(format!("headers ({})", windows_sys::Win32::Foundation::GetLastError()));
        }
        for s in &pe.sections {
            let len = ((s.mapped_size() as usize) + 0xFFF) & !0xFFF;
            if len == 0 {
                continue;
            }
            if VirtualProtect((base + s.va as usize) as *const c_void, len, section_protection(s.characteristics), &mut old) == 0 {
                failed.push(format!("{} ({})", s.name, windows_sys::Win32::Foundation::GetLastError()));
            }
        }
        if !failed.is_empty() {
            return Err(format!("{name}: VirtualProtect failed for {}", failed.join(", ")));
        }
        windows_sys::Win32::System::Diagnostics::Debug::FlushInstructionCache(
            windows_sys::Win32::System::Threading::GetCurrentProcess(),
            base as *const c_void,
            size,
        );
    }

    // 7. exception data
    let ex = pe.dir(pe::DIR_EXCEPTION);
    if ex.rva != 0 && ex.size != 0 {
        crate::eh::register_function_table(base, ex.rva, ex.size);
    }
    // 8. stack-cookie failures reported before the process ends
    for s in pe.sections.iter().filter(|s| s.characteristics & 0x2000_0000 != 0) {
        let len = (s.mapped_size() as usize).min(size.saturating_sub(s.va as usize));
        unsafe { crate::eh::hook_gs_failure(base + s.va as usize, len, &name) };
    }
    Ok(module)
}

unsafe fn tls_install(m: &MappedModule, template: usize, len: usize, total: u64, index_ptr: *mut u32) -> Result<(), String> {
    let k = crate::tls::install(&m.name, template, len, total, index_ptr, &cache_dir())?;
    // the title's executable: slot 0, as a process's executable has it
    if !m.is_dll {
        if let Err(e) = crate::tls::take_slot_zero(&m.name, index_ptr, k) {
            crate::xlog_warn!("TLS: {} keeps slot {k} ({e}) – code built with /GA reads slot 0", m.name);
        }
    }
    Ok(())
}

unsafe fn run_tls_callbacks(m: &MappedModule, reason: u32) {
    let cbs = m.tls_callbacks.lock().map(|v| v.clone()).unwrap_or_default();
    for cb in cbs {
        let f: TlsCallback = std::mem::transmute(cb);
        f(m.base as *mut c_void, reason, std::ptr::null_mut());
    }
}

/// Run TLS callbacks (PROCESS_ATTACH) and DllMain of a freshly mapped DLL.
pub fn init_dll(m: &Arc<MappedModule>) -> Result<(), String> {
    unsafe {
        run_tls_callbacks(m, 1);
        if m.entry_rva != 0 {
            let f: DllMain = std::mem::transmute(m.base + m.entry_rva as usize);
            crate::xlog_info!("DllMain({}, PROCESS_ATTACH)", m.name);
            if f(m.base as *mut c_void, 1, std::ptr::null_mut()) == 0 {
                return Err(format!("DllMain({}) returned FALSE", m.name));
            }
        }
    }
    m.initialized.store(true, Ordering::Release);
    Ok(())
}

struct LoadGuard;
impl LoadGuard {
    fn enter() -> (LoadGuard, Option<std::sync::MutexGuard<'static, ()>>) {
        let outer = IN_LOAD.with(|c| {
            let v = c.get();
            c.set(v + 1);
            v == 0
        });
        let g = if outer { Some(LOAD_LOCK.lock().unwrap_or_else(|e| e.into_inner())) } else { None };
        (LoadGuard, g)
    }
}
impl Drop for LoadGuard {
    fn drop(&mut self) {
        IN_LOAD.with(|c| c.set(c.get().saturating_sub(1)));
    }
}

/// Map + initialise a DLL that ships with the game (idempotent).
pub fn load_game_dll(path: &Path) -> Result<Arc<MappedModule>, String> {
    let key = crate::util::dll_key(&path.to_string_lossy());
    let (_g, _lock) = LoadGuard::enter();
    if let Some(m) = modules::find_mapped_by_name(&key) {
        return Ok(m);
    }
    let m = map_image(path)?;
    if !m.is_dll {
        crate::xlog_warn!("{key} is not a DLL but is being loaded as a library");
    }
    if let Err(e) = init_dll(&m) {
        // as Windows' loader: a DLL whose DllMain fails is not loaded, a later load maps it and
        // runs DllMain again (here its memory stays: what DllMain ran may have left threads or
        // callbacks in it)
        modules::unregister_mapped(m.base);
        return Err(e);
    }
    Ok(m)
}

/// Map the title's main executable.
pub fn load_main(path: &Path) -> Result<Arc<MappedModule>, String> {
    let (_g, _lock) = LoadGuard::enter();
    map_image_ex(path, true)
}

#[inline(always)]
unsafe fn peb() -> *mut u8 {
    let p: *mut u8;
    core::arch::asm!("mov {}, gs:[0x60]", out(reg) p, options(nostack, readonly, preserves_flags));
    p
}

struct EntryArgs {
    entry: usize,
}

unsafe extern "system" fn main_thread(param: *mut c_void) -> u32 {
    let args = Box::from_raw(param as *mut EntryArgs);
    let f: ExeEntry = std::mem::transmute(args.entry);
    f(peb() as *mut c_void)
}

/// Make the game the "main image" of the process and run its entry point on a thread
/// with the stack size its PE header asks for. Usually never returns (the game calls
/// ExitProcess); if the entry returns, its value is returned.
pub fn run_main(m: &Arc<MappedModule>) -> u32 {
    use windows_sys::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
    use windows_sys::Win32::System::Threading::{
        CreateThread, GetExitCodeThread, WaitForSingleObject, INFINITE, STACK_SIZE_PARAM_IS_A_RESERVATION,
    };
    unsafe {
        // PEB.ImageBaseAddress → GetModuleHandle(NULL), resources with NULL module, …
        if crate::util::under_wine() {
            crate::xlog_info!("Wine: PEB.ImageBaseAddress left alone (its loader needs the real main image there)");
        } else {
            *(peb().add(0x10) as *mut usize) = m.base;
        }
        run_tls_callbacks(m, 1);
    }
    READY.store(true, Ordering::Release);
    let stack = (m.stack_reserve as usize).max(1 << 20);
    let args = Box::into_raw(Box::new(EntryArgs { entry: m.base + m.entry_rva as usize }));
    crate::xlog_info!(
        "start {}: entry point {} , stack {:#x}",
        m.name,
        modules::describe_address(m.base + m.entry_rva as usize),
        stack
    );
    crate::log::flush();
    unsafe {
        let h = CreateThread(
            std::ptr::null(),
            stack,
            Some(main_thread),
            args as *const c_void,
            STACK_SIZE_PARAM_IS_A_RESERVATION,
            std::ptr::null_mut(),
        );
        if h.is_null() {
            crate::xlog_error!("CreateThread for the game thread failed");
            return 0xC000_0001;
        }
        let mut code = 0u32;
        if WaitForSingleObject(h, INFINITE) == WAIT_OBJECT_0 {
            GetExitCodeThread(h, &mut code);
        }
        CloseHandle(h);
        code
    }
}

/// Called from xcl.exe's own TLS callback for every thread event in the process.
pub fn on_tls_event(reason: u32) {
    match reason {
        2 => unsafe {
            // DLL_THREAD_ATTACH: game TLS blocks first, then game callbacks/DllMains.
            crate::tls::on_thread_attach();
            if !READY.load(Ordering::Acquire) {
                return;
            }
            for m in modules::mapped_modules() {
                run_tls_callbacks(&m, 2);
                if m.is_dll && m.initialized.load(Ordering::Acquire) && !m.no_thread_calls.load(Ordering::Relaxed) && m.entry_rva != 0 {
                    let f: DllMain = std::mem::transmute(m.base + m.entry_rva as usize);
                    f(m.base as *mut c_void, 2, std::ptr::null_mut());
                }
            }
        },
        3 => unsafe {
            crate::tls::on_thread_detach();
            if !READY.load(Ordering::Acquire) {
                return;
            }
            for m in modules::mapped_modules().into_iter().rev() {
                if m.is_dll && m.initialized.load(Ordering::Acquire) && !m.no_thread_calls.load(Ordering::Relaxed) && m.entry_rva != 0 {
                    let f: DllMain = std::mem::transmute(m.base + m.entry_rva as usize);
                    f(m.base as *mut c_void, 3, std::ptr::null_mut());
                }
                run_tls_callbacks(&m, 3);
            }
        },
        0 => {
            // DLL_PROCESS_DETACH (ExitProcess): write the report while everything is alive.
            crate::report::write_final(None);
            crate::log::flush();
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A PE32+ DLL of one section (.text at RVA 0x1000) holding `code`, its entry point at the
    /// start of it, preferred at `image_base` and relocatable (one empty block at RVA 0x1100);
    /// `import_dir`: (RVA, size) of the import directory, if any.
    fn tiny_dll(image_base: u64, code: &[u8], import_dir: Option<(u32, u32)>) -> Vec<u8> {
        let mut f = vec![0u8; 0x400];
        let put16 = |f: &mut Vec<u8>, at: usize, v: u16| f[at..at + 2].copy_from_slice(&v.to_le_bytes());
        let put32 = |f: &mut Vec<u8>, at: usize, v: u32| f[at..at + 4].copy_from_slice(&v.to_le_bytes());
        f[0..2].copy_from_slice(b"MZ");
        put32(&mut f, 0x3C, 0x40);
        f[0x40..0x44].copy_from_slice(b"PE\0\0");
        // COFF: x64, 1 section, optional header 0xF0, executable | large address aware | DLL
        put16(&mut f, 0x44, 0x8664);
        put16(&mut f, 0x46, 1);
        put16(&mut f, 0x54, 0xF0);
        put16(&mut f, 0x56, 0x2022);
        let o = 0x58;
        put16(&mut f, o, 0x20B);
        put32(&mut f, o + 0x10, 0x1000); // entry point
        put32(&mut f, o + 0x14, 0x1000); // base of code
        f[o + 0x18..o + 0x20].copy_from_slice(&image_base.to_le_bytes());
        put32(&mut f, o + 0x20, 0x1000); // section alignment
        put32(&mut f, o + 0x24, 0x200); // file alignment
        put16(&mut f, o + 0x28, 6);
        put16(&mut f, o + 0x30, 6);
        put32(&mut f, o + 0x38, 0x2000); // size of image
        put32(&mut f, o + 0x3C, 0x200); // size of headers
        put16(&mut f, o + 0x44, 3); // console subsystem
        f[o + 0x48..o + 0x50].copy_from_slice(&0x10_0000u64.to_le_bytes());
        f[o + 0x50..o + 0x58].copy_from_slice(&0x1000u64.to_le_bytes());
        f[o + 0x58..o + 0x60].copy_from_slice(&0x10_0000u64.to_le_bytes());
        f[o + 0x60..o + 0x68].copy_from_slice(&0x1000u64.to_le_bytes());
        put32(&mut f, o + 0x6C, 16); // data directories
        if let Some((rva, size)) = import_dir {
            put32(&mut f, o + 0x70 + 8, rva);
            put32(&mut f, o + 0x70 + 12, size);
        }
        // base relocations: one block (page 0x1000, 8 bytes: no entries) at RVA 0x1100
        put32(&mut f, o + 0x70 + 5 * 8, 0x1100);
        put32(&mut f, o + 0x70 + 5 * 8 + 4, 8);
        put32(&mut f, 0x300, 0x1000);
        put32(&mut f, 0x304, 8);
        // section .text: RVA 0x1000, 0x200 bytes at file offset 0x200, code | execute | read
        let s = o + 0xF0;
        f[s..s + 5].copy_from_slice(b".text");
        put32(&mut f, s + 8, 0x200);
        put32(&mut f, s + 12, 0x1000);
        put32(&mut f, s + 16, 0x200);
        put32(&mut f, s + 20, 0x200);
        put32(&mut f, s + 36, 0x6000_0020);
        f[0x200..0x200 + code.len()].copy_from_slice(code);
        f
    }

    fn write(name: &str, bytes: &[u8]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("xcl-loader-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join(name);
        std::fs::write(&p, bytes).unwrap();
        p
    }

    /// DllMain returning FALSE: the load fails, the DLL is not left registered as loaded, and a
    /// second load runs DllMain again (before: the second load returned the uninitialised module).
    #[test]
    fn a_dll_whose_dllmain_fails_is_not_loaded() {
        // xor eax, eax; ret
        let p = write("xcl_dllmain_false.dll", &tiny_dll(0x7E51_0000_0000, &[0x31, 0xC0, 0xC3], None));
        let key = crate::util::dll_key("xcl_dllmain_false.dll");
        for attempt in 0..2 {
            let r = load_game_dll(&p);
            assert!(r.as_ref().is_err_and(|e| e.contains("returned FALSE")), "attempt {attempt}: {:?}", r.map(|m| m.base));
            assert!(modules::find_mapped_by_name(&key).is_none(), "attempt {attempt}: still registered");
        }
        // the same code returning TRUE loads, once
        let p = write("xcl_dllmain_true.dll", &tiny_dll(0x7E52_0000_0000, &[0xB8, 1, 0, 0, 0, 0xC3], None));
        let m = load_game_dll(&p).unwrap();
        assert!(m.initialized.load(Ordering::Acquire));
        assert_eq!(load_game_dll(&p).unwrap().base, m.base);
    }

    /// An import table that does not parse: the load fails and the module – whose memory is
    /// freed – is not found afterwards (before: still registered over freed memory, and a second
    /// load returned it).
    #[test]
    fn a_dll_with_a_broken_import_table_is_not_left_registered() {
        // a descriptor at RVA 0x1010 whose name RVA lies outside the image
        let mut code = vec![0xB8, 1, 0, 0, 0, 0xC3];
        code.resize(0x10, 0xCC);
        for v in [0x1000u32, 0, 0, 0x7FFF_0000, 0x1000] {
            code.extend_from_slice(&v.to_le_bytes());
        }
        let p = write("xcl_bad_imports.dll", &tiny_dll(0x7E53_0000_0000, &code, Some((0x1010, 40))));
        let key = crate::util::dll_key("xcl_bad_imports.dll");
        for attempt in 0..2 {
            let r = load_game_dll(&p);
            assert!(r.as_ref().is_err_and(|e| e.contains("import table")), "attempt {attempt}: {:?}", r.map(|m| m.base));
            assert!(modules::find_mapped_by_name(&key).is_none(), "attempt {attempt}: still registered");
        }
    }
}

/// launch-prep hardening regression tests (synthetic values only; nothing is mapped or run).
#[cfg(test)]
mod launch_prep_tests {
    use super::*;
    use crate::pe::{Reloc, TlsDir, IMAGE_REL_BASED_DIR64, IMAGE_REL_BASED_HIGHLOW};

    #[test]
    fn unsupported_relocation_type_is_an_error_not_a_warning() {
        let ok = [Reloc { rva: 0x1000, kind: IMAGE_REL_BASED_DIR64 }, Reloc { rva: 0x1010, kind: IMAGE_REL_BASED_HIGHLOW }];
        assert!(check_relocs(&ok, 0x2000).is_ok());
        for kind in [1u16, 2, 4, 5, 7, 9, 11] {
            let e = check_relocs(&[Reloc { rva: 0x1000, kind }], 0x2000).unwrap_err();
            assert!(e.contains(&format!("unsupported relocation type {kind}")), "{e}");
        }
    }

    #[test]
    fn relocation_target_must_fit_inside_the_image() {
        assert!(check_relocs(&[Reloc { rva: 0x1ff8, kind: IMAGE_REL_BASED_DIR64 }], 0x2000).is_ok());
        assert!(check_relocs(&[Reloc { rva: 0x1ff9, kind: IMAGE_REL_BASED_DIR64 }], 0x2000).is_err());
        assert!(check_relocs(&[Reloc { rva: 0x1ffd, kind: IMAGE_REL_BASED_HIGHLOW }], 0x2000).is_err());
        assert!(check_relocs(&[Reloc { rva: u32::MAX, kind: IMAGE_REL_BASED_DIR64 }], 0x2000).is_err());
    }

    const BASE: u64 = 0x1_4000_0000;
    fn image(slots: &[u64]) -> Vec<u8> {
        let mut v = vec![0u8; 0x3000];
        for (i, s) in slots.iter().enumerate() {
            v[0x2000 + i * 8..0x2008 + i * 8].copy_from_slice(&s.to_le_bytes());
        }
        v
    }
    fn walk(img: &[u8], array_va: u64) -> Result<Vec<u64>, String> {
        let rd = |r: u32| img.get(r as usize..r as usize + 8).map(|b| u64::from_le_bytes(b.try_into().unwrap()));
        // .text = [0x1000, 0x2000)
        walk_tls_callbacks(rd, BASE, img.len() as u64, array_va, |r| (0x1000..0x2000).contains(&r))
    }

    #[test]
    fn tls_callbacks_are_bounded_and_validated() {
        let img = image(&[BASE + 0x1000, BASE + 0x1100, 0]);
        assert_eq!(walk(&img, BASE + 0x2000).unwrap(), vec![BASE + 0x1000, BASE + 0x1100]);
        // callback outside the image / outside executable code
        assert!(walk(&image(&[BASE + 0x9000, 0]), BASE + 0x2000).unwrap_err().contains("outside the image"));
        assert!(walk(&image(&[BASE + 0x2800, 0]), BASE + 0x2000).unwrap_err().contains("not in an executable"));
        // array outside the image, or running past its end without a terminator
        assert!(walk(&img, BASE - 8).is_err());
        let mut tail = image(&[]);
        tail[0x2ff8..0x3000].copy_from_slice(&(BASE + 0x1000).to_le_bytes());
        assert!(walk(&tail, BASE + 0x2ff8).unwrap_err().contains("runs past"));
        // more than MAX_TLS_CALLBACKS non-null entries is refused
        let many: Vec<u64> = std::iter::repeat(BASE + 0x1000).take(MAX_TLS_CALLBACKS + 1).collect();
        let mut big = vec![0u8; 0x2000 + (MAX_TLS_CALLBACKS + 2) * 8];
        for (i, s) in many.iter().enumerate() {
            big[0x2000 + i * 8..0x2008 + i * 8].copy_from_slice(&s.to_le_bytes());
        }
        assert!(walk(&big, BASE + 0x2000).unwrap_err().contains("more than"));
    }

    #[test]
    fn tls_directory_addresses_must_be_inside_the_image() {
        let good = TlsDir { start_raw: BASE + 0x2000, end_raw: BASE + 0x2010, address_of_index: BASE + 0x2100, ..Default::default() };
        assert!(check_tls_dir(&good, BASE, 0x3000).is_ok());
        assert!(check_tls_dir(&TlsDir { end_raw: BASE + 0x1000, ..good }, BASE, 0x3000).is_err());
        assert!(check_tls_dir(&TlsDir { end_raw: BASE + 0x4000, ..good }, BASE, 0x3000).is_err());
        assert!(check_tls_dir(&TlsDir { address_of_index: BASE + 0x2ffe, ..good }, BASE, 0x3000).is_err());
        // no index variable: accepted (upstream c69c4d1); outside the image: refused
        assert!(check_tls_dir(&TlsDir { address_of_index: 0, ..good }, BASE, 0x3000).is_ok());
        assert!(check_tls_dir(&TlsDir { address_of_index: BASE - 8, ..good }, BASE, 0x3000).is_err());
    }
}

/// launch-prep hardening, end to end through the real `map_image` (no code of the image runs:
/// map_image never calls the entry point, DllMain or TLS callbacks).
#[cfg(test)]
mod launch_prep_e2e {
    use super::*;

    const TEXT_RVA: u32 = 0x1000;
    const DATA_RVA: u32 = 0x2000;
    const CB_RVA: u32 = 0x1010;

    /// Minimal PE32+ DLL (a DLL, not an EXE: for a non-DLL image `tls_install` moves the image to TLS
    /// slot 0, which would steal the test harness's own slot 0 and abort the next thread spawn):
    /// .text (RX) with two `ret`s, .data (RW) with a TLS directory, a 16-byte
    /// template, the index slot, a one-entry callback array and base relocations for every VA
    /// in the TLS directory and in the callback array (what a linker emits).
    fn synthetic(image_base: u64, cb_rva: u32, extra_reloc_kind: Option<u16>) -> Vec<u8> {
        let mut b = vec![0u8; 0xA00];
        b[0..2].copy_from_slice(b"MZ");
        b[0x3C..0x40].copy_from_slice(&0x80u32.to_le_bytes());
        b[0x80..0x84].copy_from_slice(b"PE\0\0");
        let put16 = |b: &mut Vec<u8>, o: usize, v: u16| b[o..o + 2].copy_from_slice(&v.to_le_bytes());
        let put32 = |b: &mut Vec<u8>, o: usize, v: u32| b[o..o + 4].copy_from_slice(&v.to_le_bytes());
        let put64 = |b: &mut Vec<u8>, o: usize, v: u64| b[o..o + 8].copy_from_slice(&v.to_le_bytes());
        put16(&mut b, 0x84, 0x8664);
        put16(&mut b, 0x86, 2);
        put16(&mut b, 0x94, 240);
        put16(&mut b, 0x96, 0x2022); // EXECUTABLE_IMAGE | LARGE_ADDRESS_AWARE | DLL
        let o = 0x98;
        put16(&mut b, o, 0x20B);
        put32(&mut b, o + 16, TEXT_RVA);
        put64(&mut b, o + 24, image_base);
        put32(&mut b, o + 32, 0x1000);
        put32(&mut b, o + 36, 0x200);
        put16(&mut b, o + 48, 6);
        put32(&mut b, o + 56, 0x3000);
        put32(&mut b, o + 60, 0x400);
        put16(&mut b, o + 68, 3);
        put16(&mut b, o + 70, 0x8160);
        put64(&mut b, o + 72, 0x100000);
        put64(&mut b, o + 80, 0x1000);
        put64(&mut b, o + 88, 0x100000);
        put64(&mut b, o + 96, 0x1000);
        put32(&mut b, o + 108, 16);
        let dir = |i: usize| o + 112 + 8 * i;
        put32(&mut b, dir(5), DATA_RVA + 0x200); // base relocations
        put32(&mut b, dir(5) + 4, 8 + 2 * 6);
        put32(&mut b, dir(9), DATA_RVA); // TLS
        put32(&mut b, dir(9) + 4, 40);
        let sec = |b: &mut Vec<u8>, i: usize, name: &[u8], va: u32, vs: u32, raw: u32, rs: u32, ch: u32| {
            let s = o + 240 + 40 * i;
            b[s..s + name.len()].copy_from_slice(name);
            b[s + 8..s + 12].copy_from_slice(&vs.to_le_bytes());
            b[s + 12..s + 16].copy_from_slice(&va.to_le_bytes());
            b[s + 16..s + 20].copy_from_slice(&rs.to_le_bytes());
            b[s + 20..s + 24].copy_from_slice(&raw.to_le_bytes());
            b[s + 36..s + 40].copy_from_slice(&ch.to_le_bytes());
        };
        sec(&mut b, 0, b".text", TEXT_RVA, 0x1000, 0x400, 0x200, 0x6000_0020);
        sec(&mut b, 1, b".data", DATA_RVA, 0x1000, 0x600, 0x400, 0xC000_0040);
        b[0x400] = 0xC3;
        b[0x410] = 0xC3;
        let d = 0x600usize; // file offset of DATA_RVA
        put64(&mut b, d, image_base + 0x2100); // StartAddressOfRawData
        put64(&mut b, d + 8, image_base + 0x2110); // EndAddressOfRawData
        put64(&mut b, d + 16, image_base + 0x2120); // AddressOfIndex
        put64(&mut b, d + 24, image_base + 0x2130); // AddressOfCallBacks
        b[d + 0x100..d + 0x110].copy_from_slice(b"TLS-TEMPLATE-16B");
        put64(&mut b, d + 0x130, image_base + cb_rva as u64); // callback[0]; callback[1] = 0
        let r = d + 0x200; // relocation block for page DATA_RVA
        put32(&mut b, r, DATA_RVA);
        put32(&mut b, r + 4, 8 + 2 * 6);
        let last = extra_reloc_kind.map(|k| (k << 12) | 0x140).unwrap_or(0);
        for (i, e) in [0xA000u16, 0xA008, 0xA010, 0xA018, 0xA130, last].iter().enumerate() {
            put16(&mut b, r + 8 + 2 * i, *e);
        }
        b
    }

    fn write(name: &str, bytes: &[u8]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("xcl-launch-prep-e2e-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join(name);
        std::fs::write(&p, bytes).unwrap();
        p
    }

    /// Preferred base = this test process's own image base: always taken, so the loader must relocate.
    fn taken_base() -> u64 {
        unsafe { windows_sys::Win32::System::LibraryLoader::GetModuleHandleW(std::ptr::null()) as u64 }
    }

    #[test]
    fn relocated_image_gets_rebased_tls_callback_and_relocated_tls_data() {
        let pref = taken_base();
        let m = map_image(&write("lp_e2e_ok.dll", &synthetic(pref, CB_RVA, None))).expect("valid synthetic image must map");
        assert_ne!(m.base as u64, pref, "preferred base was taken: relocation path expected");
        let cbs = m.tls_callbacks.lock().unwrap().clone();
        assert_eq!(cbs, vec![m.base + CB_RVA as usize], "callback rebased by the load delta");
        // the relocation pass patched the in-image VAs consistently with the rebased list
        let slot = unsafe { ((m.base + 0x2130) as *const u64).read_unaligned() };
        assert_eq!(slot as usize, cbs[0]);
        let cb_ptr = unsafe { ((m.base + 0x2018) as *const u64).read_unaligned() };
        assert_eq!(cb_ptr as usize, m.base + 0x2130);
    }

    #[test]
    fn unsupported_relocation_type_refuses_the_image_before_mapping() {
        let r = map_image(&write("lp_e2e_badreloc.dll", &synthetic(taken_base(), CB_RVA, Some(5))));
        let e = r.err().expect("type-5 relocation must be refused");
        assert!(e.contains("unsupported relocation type 5"), "{e}");
        assert!(modules::find_mapped_by_name("lp_e2e_badreloc.dll").is_none(), "rejected image must not be registered");
    }

    #[test]
    fn tls_callback_outside_executable_code_refuses_the_image() {
        let r = map_image(&write("lp_e2e_badcb.dll", &synthetic(taken_base(), 0x2100, None)));
        let e = r.err().expect("callback into .data must be refused");
        assert!(e.contains("not in an executable section"), "{e}");
        assert!(modules::find_mapped_by_name("lp_e2e_badcb.dll").is_none());
    }
}
