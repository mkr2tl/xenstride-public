// Xenstride notice (GPL-2.0-only, section 2a): this file was added relative to ivall/xcl 9aec53d on 2026-11-23; see XENSTRIDE_CHANGES.md at the runtime root.
//! Local metadata-only interface diagnostics. No network client, dump or user payload.
use serde::{Deserialize, Serialize};
#[path = "diagnostics_repeat.rs"]
mod repeated;
pub use repeated::{documented_interface, LoggingMetrics, Repetition};
use std::collections::VecDeque;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Mutex,
};

pub const IUNKNOWN: crate::hle::xgameruntime::api::Guid =
    crate::hle::xgameruntime::api::Guid::from_u128(0x00000000_0000_0000_c000_000000000046);
static SEQUENCE: AtomicU64 = AtomicU64::new(0);
static DROPPED: AtomicU64 = AtomicU64::new(0);
static EVENTS: Mutex<VecDeque<InterfaceEvent>> = Mutex::new(VecDeque::new());
static ACTIVE: Mutex<Vec<ActiveCall>> = Mutex::new(Vec::new());
static CALL_IDS: AtomicU64 = AtomicU64::new(0);
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ActiveCall {
    pub call_id: u64,
    pub thread: u32,
    pub operation: String,
    pub class: Option<String>,
    pub iid: Option<String>,
    pub slot: Option<usize>,
}
#[derive(Clone, Copy)]
pub enum ReturnKind {
    HResult,
    Opaque,
}

const LIMIT: usize = 256;
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InterfaceEvent {
    pub sequence: u64,
    pub thread: u32,
    pub operation: String,
    pub provider: String,
    pub class: Option<String>,
    pub iid: Option<String>,
    pub hresult: String,
    pub elapsed_us: u64,
    pub stack: Vec<String>,
    #[serde(default)]
    pub call_id: u64,
    #[serde(default)]
    pub slot: Option<usize>,
    #[serde(default)]
    pub raw_return: Option<String>,
    #[serde(default)]
    pub return_kind: String,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Snapshot {
    #[serde(default)]
    pub repetitions: Vec<Repetition>,
    #[serde(default)]
    pub logging: LoggingMetrics,
    pub events: Vec<InterfaceEvent>,
    pub dropped: u64,
    #[serde(default)]
    pub in_flight: Vec<ActiveCall>,
    #[serde(default)]
    pub exceptions: Vec<crate::eh::ExceptionObservation>,
}
pub struct Call {
    operation: &'static str,
    class: Option<String>,
    iid: Option<String>,
    started: std::time::Instant,
    call_id: u64,
    thread: u32,
    slot: Option<usize>,
}
impl Drop for Call {
    fn drop(&mut self) {
        let saved = unsafe { windows_sys::Win32::Foundation::GetLastError() };
        ACTIVE
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|c| c.call_id != self.call_id);
        unsafe {
            windows_sys::Win32::Foundation::SetLastError(saved);
        }
    }
}
impl Call {
    pub unsafe fn new(
        operation: &'static str,
        class: *const std::ffi::c_void,
        iid: *const std::ffi::c_void,
    ) -> Self {
        let format = |p: *const std::ffi::c_void| {
            if p.is_null() {
                None
            } else {
                Some(
                    (p as *const crate::hle::xgameruntime::api::Guid)
                        .read_unaligned()
                        .to_string(),
                )
            }
        };
        let call = Self {
            operation,
            class: format(class),
            iid: format(iid),
            started: std::time::Instant::now(),
            call_id: CALL_IDS.fetch_add(1, Ordering::Relaxed),
            thread: windows_sys::Win32::System::Threading::GetCurrentThreadId(),
            slot: None,
        };
        let mut active = ACTIVE.lock().unwrap_or_else(|e| e.into_inner());
        if active.len() < LIMIT {
            active.push(ActiveCall {
                call_id: call.call_id,
                thread: call.thread,
                operation: operation.into(),
                class: call.class.clone(),
                iid: call.iid.clone(),
                slot: None,
            });
        } else {
            DROPPED.fetch_add(1, Ordering::Relaxed);
        }
        drop(active);
        call
    }
    pub fn with_slot(mut self, slot: usize) -> Self {
        self.slot = Some(slot);
        if let Some(c) = ACTIVE
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter_mut()
            .find(|c| c.call_id == self.call_id)
        {
            c.slot = Some(slot);
        }
        self
    }
    pub fn finish(self, provider: &str, hr: i32) -> i32 {
        self.emit(provider, Some(hr), None);
        hr
    }
    pub fn finish_void(self, provider: &str) {
        self.emit(provider, None, None);
    }
    pub fn finish_raw(self, provider: &str, value: usize, kind: ReturnKind) -> usize {
        let hr = match kind {
            ReturnKind::HResult => Some(value as u32 as i32),
            ReturnKind::Opaque => None,
        };
        self.emit(provider, hr, Some(value));
        value
    }
    fn emit(&self, provider: &str, hr: Option<i32>, raw: Option<usize>) {
        let _work = repeated::Work::start();
        let saved = unsafe { windows_sys::Win32::Foundation::GetLastError() };
        if !hr.is_some_and(|v| v < 0)
            && std::env::var_os("XCL_TRACE_INTERFACES").as_deref()
                != Some(std::ffi::OsStr::new("1"))
        {
            unsafe {
                windows_sys::Win32::Foundation::SetLastError(saved);
            }
            return;
        }
        let group = if self.operation == "QueryInterface" && hr.is_some_and(|v| v < 0) {
            match repeated::observe(self, provider, hr.unwrap()) {
                repeated::Decision::First(index) => Some(index),
                repeated::Decision::Repeat { count, log } => {
                    if log && crate::log::enabled(crate::log::Level::Warn) {
                        crate::xlog_warn!("[interface-repeat] operation={} provider={} iid={} hr={:#010x} count={} last_call={} thread={}",
                            self.operation, provider, self.iid.as_deref().unwrap_or("none"), hr.unwrap() as u32,
                            count, self.call_id, self.thread);
                        repeated::summary_log();
                    }
                    return;
                }
                repeated::Decision::Untracked => None,
            }
        } else { None };
        repeated::stack();
        let mut frames = [std::ptr::null_mut(); 12];
        let n = unsafe {
            windows_sys::Win32::System::Diagnostics::Debug::RtlCaptureStackBackTrace(
                2,
                frames.len() as u32,
                frames.as_mut_ptr(),
                std::ptr::null_mut(),
            )
        };
        let event = InterfaceEvent {
            sequence: SEQUENCE.fetch_add(1, Ordering::Relaxed),
            thread: unsafe { windows_sys::Win32::System::Threading::GetCurrentThreadId() },
            operation: self.operation.into(),
            provider: provider.into(),
            class: self.class.clone(),
            iid: self.iid.clone(),
            hresult: hr
                .map(|v| format!("0x{:08x}", v as u32))
                .unwrap_or_default(),
            call_id: self.call_id,
            slot: self.slot,
            raw_return: raw.map(|v| format!("0x{v:016x}")),
            return_kind: if hr.is_some() {
                "hresult"
            } else if raw.is_some() {
                "opaque"
            } else {
                "void"
            }
            .into(),
            elapsed_us: self.started.elapsed().as_micros().min(u64::MAX as u128) as u64,
            stack: frames[..n as usize]
                .iter()
                .map(|&p| crate::modules::describe_address(p as usize))
                .collect(),
        };
        if let Some(index) = group { repeated::first(index, &event); }
        // Ring collection does not wait for another query. Preserve LastError around logging.
        if let Ok(mut g) = EVENTS.try_lock() {
            if g.len() == LIMIT {
                g.pop_front();
                DROPPED.fetch_add(1, Ordering::Relaxed);
            }
            g.push_back(event.clone());
        } else {
            DROPPED.fetch_add(1, Ordering::Relaxed);
        }
        let level = if hr.is_some_and(|v| v < 0) { crate::log::Level::Warn } else { crate::log::Level::Info };
        if crate::log::enabled(level) { if let Ok(json) = serde_json::to_string(&event) {
            repeated::full_log();
            if hr.is_some_and(|v| v < 0) {
                crate::xlog_warn!("[interface] {json}");
            } else {
                crate::xlog_info!("[interface] {json}");
            }
        }
        }
        unsafe {
            windows_sys::Win32::Foundation::SetLastError(saved);
        }
    }
}
pub fn active_calls() -> Vec<ActiveCall> {
    ACTIVE.try_lock().map(|g| g.clone()).unwrap_or_default()
}
pub fn snapshot() -> Snapshot {
    Snapshot {
        repetitions: repeated::snapshot(),
        logging: repeated::metrics(),
        events: EVENTS
            .try_lock()
            .map(|g| g.iter().cloned().collect())
            .unwrap_or_default(),
        dropped: DROPPED.load(Ordering::Relaxed),
        in_flight: active_calls(),
        exceptions: crate::eh::exception_observations(),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn enabled_trace_records_successes_and_bounds_retained_events() {
        unsafe {
            if std::env::var_os("XCL_DIAGNOSTIC_TEST_CHILD").is_none() {
                let status = std::process::Command::new(std::env::current_exe().unwrap())
                    .args([
                        "--exact",
                        "diagnostics::tests::enabled_trace_records_successes_and_bounds_retained_events",
                        "--test-threads=1",
                    ])
                    .env("XCL_DIAGNOSTIC_TEST_CHILD", "1")
                    .env("XCL_TRACE_INTERFACES", "1")
                    .status()
                    .unwrap();
                assert!(status.success());
                return;
            }
            crate::log::init(None, crate::log::Level::Error, false);
            for _ in 0..300 {
                assert_eq!(
                    Call::new("bounded-success", std::ptr::null(), std::ptr::null())
                        .finish("synthetic", 0),
                    0
                );
            }
            let data = snapshot();
            assert_eq!(data.events.len(), 256);
            assert_eq!(data.dropped, 44);
            assert!(data
                .events
                .iter()
                .all(|e| e.hresult == "0x00000000" && e.provider == "synthetic"));
        }
    }

    #[test]
    fn p0_failed_interface_is_visible_at_warning_log_level() {
        unsafe {
            if std::env::var_os("XCL_P0_WARN_CHILD").is_none() {
                let status = std::process::Command::new(std::env::current_exe().unwrap())
                    .args([
                        "--exact",
                        "diagnostics::tests::p0_failed_interface_is_visible_at_warning_log_level",
                        "--test-threads=1",
                    ])
                    .env("XCL_P0_WARN_CHILD", "1")
                    .status()
                    .unwrap();
                assert!(status.success());
                return;
            }
            let path =
                std::env::temp_dir().join(format!("xcl-p0-warning-{}.log", std::process::id()));
            crate::log::init(Some(&path), crate::log::Level::Warn, false);
            Call::new(
                "synthetic-refusal",
                std::ptr::null(),
                &IUNKNOWN as *const _ as _,
            )
            .finish("synthetic", 0x80004002u32 as i32);
            crate::log::flush();
            let content = std::fs::read_to_string(path).unwrap();
            assert!(content.contains("[interface]") && content.contains("0x80004002"));
        }
    }

    #[test]
    fn p0_opaque_return_keeps_its_full_width_and_has_no_hresult() {
        unsafe {
            if std::env::var_os("XCL_P0_OPAQUE_CHILD").is_none() {
                let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "diagnostics::tests::p0_opaque_return_keeps_its_full_width_and_has_no_hresult", "--test-threads=1"])
                .env("XCL_P0_OPAQUE_CHILD", "1").env("XCL_TRACE_INTERFACES", "1").status().unwrap();
                assert!(status.success());
                return;
            }
            let value = 0xf00d000080004002usize;
            let call = Call::new("opaque-fixture", std::ptr::null(), std::ptr::null());
            assert_eq!(
                call.finish_raw("synthetic", value, ReturnKind::Opaque),
                value
            );
            let record = snapshot()
                .events
                .into_iter()
                .find(|e| e.operation == "opaque-fixture")
                .unwrap();
            assert!(record.hresult.is_empty());
            assert_eq!(record.return_kind, "opaque");
            assert_eq!(record.raw_return.as_deref(), Some("0xf00d000080004002"));
        }
    }

    #[test]
    fn successful_untraced_queries_also_preserve_last_error() {
        unsafe {
            let call = Call::new("synthetic-success", std::ptr::null(), std::ptr::null());
            windows_sys::Win32::Foundation::SetLastError(6789);
            assert_eq!(call.finish("synthetic-provider", 0), 0);
            assert_eq!(windows_sys::Win32::Foundation::GetLastError(), 6789);
        }
    }

    #[test]
    fn failures_are_recorded_locally_without_changing_result_or_last_error() {
        unsafe {
            let id = IUNKNOWN;
            let call = Call::new("synthetic-query", std::ptr::null(), &id as *const _ as _);
            windows_sys::Win32::Foundation::SetLastError(1234);
            assert_eq!(
                call.finish("synthetic-provider", 0x80004002u32 as i32),
                0x80004002u32 as i32
            );
            assert_eq!(windows_sys::Win32::Foundation::GetLastError(), 1234);
            let data = snapshot();
            let event = data
                .events
                .iter()
                .find(|e| e.operation == "synthetic-query")
                .unwrap();
            assert_eq!(event.hresult, "0x80004002");
            assert_eq!(event.provider, "synthetic-provider");
            assert_eq!(
                event.iid.as_deref(),
                Some("00000000-0000-0000-c000-000000000046")
            );
            assert!(!event.stack.is_empty());
        }
    }
    #[test]
    fn p3_query_refusals_keep_first_count_and_latest_without_log_flood() { unsafe {
        if std::env::var_os("P3_DIAGNOSTIC_CHILD").is_none() {
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "diagnostics::tests::p3_query_refusals_keep_first_count_and_latest_without_log_flood", "--test-threads=1"])
                .env("P3_DIAGNOSTIC_CHILD", "1").env_remove("XCL_TRACE_INTERFACES").status().unwrap();
            assert!(status.success()); return;
        }
        let path = std::env::temp_dir().join(format!("xcl-p3-flood-{}.log", std::process::id()));
        crate::log::init(Some(&path), crate::log::Level::Warn, false);
        let iid = crate::hle::xgameruntime::api::Guid::from_u128(0x0742a90b_c387_483f_b946_30a7e4e61458);
        let started = std::time::Instant::now();
        for _ in 0..4096 {
            let call = Call::new("QueryInterface", std::ptr::null(), &iid as *const _ as _);
            windows_sys::Win32::Foundation::SetLastError(31415);
            assert_eq!(call.finish("synthetic.D3D12", 0x80004002u32 as i32), 0x80004002u32 as i32);
            assert_eq!(windows_sys::Win32::Foundation::GetLastError(), 31415);
        }
        let elapsed_us=started.elapsed().as_micros();
        crate::log::flush();
        let log=std::fs::read_to_string(&path).unwrap();
        let data=serde_json::to_value(snapshot()).unwrap();
        if let Some(out)=std::env::var_os("P3_BENCH_OUTPUT") {
            std::fs::write(out,serde_json::to_vec_pretty(&serde_json::json!({"calls":4096,"elapsed_us":elapsed_us,
                "bytes":log.len(),"lines":log.lines().count(),"snapshot":data})).unwrap()).unwrap();
        }
        assert!(log.lines().count() <= 14, "repeated refusals must not flush 4096 full stacks");
        let group=&data["repetitions"][0];
        assert_eq!(group["count"],4096);
        assert_eq!(group["first"]["hresult"],"0x80004002");
        assert_eq!(group["last_examples"].as_array().unwrap().len(),2);
        assert_eq!(group["last_examples"][1]["call_id"],4095);
        assert_eq!(data["dropped"],0);
        for (provider,hr) in [("synthetic.D3D12",0x80004005u32 as i32),("another.D3D12",0x80004002u32 as i32)] {
            assert_eq!(Call::new("QueryInterface",std::ptr::null(),&iid as *const _ as _).finish(provider,hr),hr);
        }
        assert_eq!(snapshot().repetitions.len(),3,"different providers and HRESULTs must not be merged");
    } }

}
