// Xenstride notice (GPL-2.0-only, section 2a): this file was added relative to ivall/xcl 9aec53d on 2026-11-23; see XENSTRIDE_CHANGES.md at the runtime root.
//! P0 regression (2026-11-23): the newer console DirectStorage factory and queue.
//!
//! Static evidence (read-only disassembly of a retail Scarlett executable, never run):
//! a function-local singleton calls `DStorageGetFactory(&IID {62959d11-…}, &g)` and executes
//! `int3` when `g == NULL || hr != S_OK`. On success it calls factory slot 8 with 0x2000000
//! and breaks again on a failed HRESULT. Other call sites use slot 4 (OpenFile, the file IID
//! XCL already knows), slot 5 (CreateStatusArray, capacity 1, the status-array IID XCL already
//! knows), slot 10 (CreateQueue with a 0x20-byte desc whose u64 @0x18 is 1, queue IID
//! {3c02cb0c-…}, `int3` on failure) and slot 2 (Release). No other factory slot is referenced.
//!
//! The IIDs are declared locally so the same tests compile against the runtime before the fix
//! and show its failure (`E_NOINTERFACE`, null factory) rather than a compile error.

use super::*;

const X1_FACTORY: GUID = GUID::from_u128(0x62959d11_d45f_4884_a0a3_f7862ea243ae);
const X1_QUEUE: GUID = GUID::from_u128(0x3c02cb0c_c050_4195_99a1_a1ce94ba33c5);
const UNKNOWN: GUID = GUID::from_u128(0x0badc0de_0000_4000_8000_000000000001);

type CreateQueueFn = unsafe extern "system" fn(*mut c_void, *const u8, *const GUID, *mut *mut c_void) -> HRESULT;
type CreateStatusFn = unsafe extern "system" fn(*mut c_void, u32, *const u8, *const GUID, *mut *mut c_void) -> HRESULT;
type SetStagingFn = unsafe extern "system" fn(*mut c_void, u32) -> HRESULT;
type QiFn = unsafe extern "system" fn(*mut c_void, *const GUID, *mut *mut c_void) -> HRESULT;

unsafe fn slot(obj: *mut c_void, i: usize) -> usize {
    *(*(obj as *const *const usize)).add(i)
}

/// The title's 0x20-byte queue desc: source u64 @0, capacity u16 @8, priority i8 @0xA,
/// name @0x10, u64 @0x18 = 1.
fn desc_x1(source: u64, capacity: u16, priority: i8, name: *const u8) -> [u8; 0x20] {
    let mut d = [0u8; 0x20];
    d[0..8].copy_from_slice(&source.to_le_bytes());
    d[8..10].copy_from_slice(&capacity.to_le_bytes());
    d[10] = priority as u8;
    d[0x10..0x18].copy_from_slice(&(name as u64).to_le_bytes());
    d[0x18..0x20].copy_from_slice(&1u64.to_le_bytes());
    d
}

#[test]
fn p0_newer_factory_iid_is_served_with_the_title_call_sequence() {
    unsafe {
        // +0x1d8e4f0: the guarded factory request
        let mut f = std::ptr::null_mut();
        let hr = DStorageGetFactory(&X1_FACTORY, &mut f);
        assert_eq!(hr, S_OK, "DStorageGetFactory(IID_FACTORY_X1) = {:#x}", hr as u32);
        assert!(!f.is_null(), "a null factory reaches the title's int3");
        // the same object answers the older IID and IUnknown, and QI to X1 again
        let qi: QiFn = std::mem::transmute(slot(f, 0));
        let mut again = std::ptr::null_mut();
        assert_eq!(qi(f, &X1_FACTORY, &mut again), S_OK);
        assert_eq!(again, f);
        // +0x1d8e515: slot 8 with 32 MiB, int3 when it fails
        let staging: SetStagingFn = std::mem::transmute(slot(f, 8));
        assert!(staging(f, 0x0200_0000) >= 0);
        // slot 5: CreateStatusArray(1, name, IID_STATUS_ARRAY)
        let create_status: CreateStatusFn = std::mem::transmute(slot(f, 5));
        let mut s = std::ptr::null_mut();
        assert_eq!(create_status(f, 1, b"p0\0".as_ptr(), &IID_STATUS_ARRAY, &mut s), S_OK);
        // +0x28c95c7: slot 10 with the 0x20 desc and the newer queue IID, int3 when it fails
        let create_queue1: CreateQueueFn = std::mem::transmute(slot(f, 10));
        let d = desc_x1(0, 64 + 0x3f, 0, b"p0-file-queue\0".as_ptr());
        let mut q = std::ptr::null_mut();
        let hr = create_queue1(f, d.as_ptr(), &X1_QUEUE, &mut q);
        assert_eq!(hr, S_OK, "factory slot 10 = {:#x}", hr as u32);
        assert!(!q.is_null());
        let queue_qi: QiFn = std::mem::transmute(slot(q, 0));
        let mut older = std::ptr::null_mut();
        assert_eq!(queue_qi(q, &IID_QUEUE, &mut older), S_OK, "one object, both queue IIDs");
        assert_eq!(older, q);
        queue_release(older);
        queue_close(q);
        queue_release(q);
        status_release(s);
    }
}

#[test]
fn p0_newer_memory_queue_copies_and_reports_status() {
    unsafe {
        let mut f = std::ptr::null_mut();
        assert_eq!(DStorageGetFactory(&X1_FACTORY, &mut f), S_OK);
        let create_queue1: CreateQueueFn = std::mem::transmute(slot(f, 10));
        let create_status: CreateStatusFn = std::mem::transmute(slot(f, 5));
        let d = desc_x1(1, 8, 2, std::ptr::null());
        let mut q = std::ptr::null_mut();
        assert_eq!(create_queue1(f, d.as_ptr(), &X1_QUEUE, &mut q), S_OK);
        // the older checks still apply to the newer desc: a memory queue is realtime
        let bad = desc_x1(1, 8, 0, std::ptr::null());
        let mut refused = std::ptr::null_mut();
        assert_eq!(create_queue1(f, bad.as_ptr(), &X1_QUEUE, &mut refused), E_QUEUE_PRIORITY);
        assert!(refused.is_null());
        let mut s = std::ptr::null_mut();
        assert_eq!(create_status(f, 1, std::ptr::null(), &IID_STATUS_ARRAY, &mut s), S_OK);

        let src: Vec<u8> = (0..4096u32).map(|i| (i * 7 + 3) as u8).collect();
        let mut dst = vec![0u8; src.len()];
        let mut req = [0u8; REQUEST_BYTES];
        req[0..8].copy_from_slice(&SOURCE_MEMORY_BIT.to_le_bytes());
        req[8..16].copy_from_slice(&(dst.as_mut_ptr() as u64).to_le_bytes());
        req[0x18..0x1C].copy_from_slice(&(dst.len() as u32).to_le_bytes());
        req[0x20..0x28].copy_from_slice(&(src.as_ptr() as u64).to_le_bytes());
        req[0x30..0x34].copy_from_slice(&(src.len() as u32).to_le_bytes());
        let enqueue: unsafe extern "system" fn(*mut c_void, *const u8) = std::mem::transmute(slot(q, 3));
        let status: unsafe extern "system" fn(*mut c_void, *mut c_void, u32) = std::mem::transmute(slot(q, 4));
        let submit: unsafe extern "system" fn(*mut c_void) = std::mem::transmute(slot(q, 6));
        let complete: unsafe extern "system" fn(*mut c_void, u32) -> bool = std::mem::transmute(slot(s, 3));
        let hresult: unsafe extern "system" fn(*mut c_void, u32) -> HRESULT = std::mem::transmute(slot(s, 4));
        enqueue(q, req.as_ptr());
        status(q, s, 0);
        submit(q);
        let t = std::time::Instant::now();
        while !complete(s, 0) && t.elapsed().as_secs() < 5 {
            std::thread::yield_now();
        }
        assert!(complete(s, 0), "the newer queue runs requests");
        assert_eq!(hresult(s, 0), S_OK);
        assert_eq!(dst, src);
        queue_close(q);
        queue_release(q);
        status_release(s);
    }
}

#[test]
fn p0_unknown_iids_still_fail_and_are_recorded() {
    unsafe {
        let mut f = 1usize as *mut c_void;
        assert_eq!(DStorageGetFactory(&UNKNOWN, &mut f), E_NOINTERFACE, "no fabricated success");
        assert!(f.is_null(), "COM: *out is null on failure");
        let found = crate::diagnostics::snapshot().events.iter().any(|e| {
            e.provider == "XCL.xcrdapi"
                && e.hresult == "0x80004002"
                && e.iid.as_deref().is_some_and(|i| i.to_ascii_lowercase().contains("0badc0de-0000-4000-8000-000000000001"))
        });
        assert!(found, "a refused DirectStorage IID must reach interface_diagnostics");
    }
}
