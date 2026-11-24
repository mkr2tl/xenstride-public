// Xenstride notice (GPL-2.0-only, section 2a): this file was added relative to ivall/xcl 9aec53d on 2026-11-23; see XENSTRIDE_CHANGES.md at the runtime root.
//! Bounded repetition summaries. Only first occurrences capture stacks.
use super::InterfaceEvent;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Mutex,
};

const LIMIT: usize = 128;
static GROUPS: Mutex<Vec<Repetition>> = Mutex::new(Vec::new());
static STACKS: AtomicU64 = AtomicU64::new(0);
static FULL_LOGS: AtomicU64 = AtomicU64::new(0);
static SUMMARY_LOGS: AtomicU64 = AtomicU64::new(0);
static SUPPRESSED: AtomicU64 = AtomicU64::new(0);
static WORK_NS: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Example {
    pub call_id: u64,
    pub thread: u32,
    pub elapsed_us: u64,
    pub uptime_us: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Repetition {
    pub operation: String,
    pub provider: String,
    pub class: Option<String>,
    pub iid: Option<String>,
    pub slot: Option<usize>,
    pub hresult: i32,
    pub documented_interface: Option<String>,
    pub count: u64,
    pub first: Option<InterfaceEvent>,
    /// Most recent calls: metadata only, not a new stack walk per repetition.
    pub last_examples: VecDeque<Example>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct LoggingMetrics {
    pub stack_captures: u64,
    pub full_events_logged: u64,
    pub repeat_summaries_logged: u64,
    pub suppressed_repeats: u64,
    /// Sum of emit work on all threads; not wall time or total API overhead.
    pub work_us: u64,
}
pub struct Work {
    started: std::time::Instant,
    last_error: u32,
}
impl Work {
    pub fn start() -> Self {
        let last_error = unsafe { windows_sys::Win32::Foundation::GetLastError() };
        Self {
            started: std::time::Instant::now(),
            last_error,
        }
    }
}
impl Drop for Work {
    fn drop(&mut self) {
        WORK_NS.fetch_add(
            self.started.elapsed().as_nanos().min(u64::MAX as u128) as u64,
            Ordering::Relaxed,
        );
        unsafe {
            windows_sys::Win32::Foundation::SetLastError(self.last_error);
        }
    }
}
pub enum Decision {
    First(usize),
    Repeat { count: u64, log: bool },
    Untracked,
}

pub fn documented_interface(iid: &str) -> Option<&'static str> {
    match iid {
        "0742a90b-c387-483f-b946-30a7e4e61458" => Some("ID3D12InfoQueue (Windows debug layer)"),
        "0adf7d52-929c-4e61-addb-ffed30de66ef" => {
            Some("ID3D12SharingContract (Windows diagnostics)")
        }
        _ => None,
    }
}
pub fn observe(call: &super::Call, provider: &str, hr: i32) -> Decision {
    let mut groups = GROUPS.lock().unwrap_or_else(|e| e.into_inner());
    let example = Example {
        call_id: call.call_id,
        thread: call.thread,
        elapsed_us: call.started.elapsed().as_micros().min(u64::MAX as u128) as u64,
        uptime_us: (crate::log::uptime() * 1_000_000.0) as u64,
    };
    if let Some(g) = groups.iter_mut().find(|g| {
        g.operation == call.operation
            && g.provider == provider
            && g.iid == call.iid
            && g.class == call.class
            && g.slot == call.slot
            && g.hresult == hr
    }) {
        g.count += 1;
        if g.last_examples.len() == 2 {
            g.last_examples.pop_front();
        }
        g.last_examples.push_back(example);
        let log = g.count.is_power_of_two();
        if !log {
            SUPPRESSED.fetch_add(1, Ordering::Relaxed);
        }
        return Decision::Repeat {
            count: g.count,
            log,
        };
    }
    if groups.len() == LIMIT {
        return Decision::Untracked;
    }
    let index = groups.len();
    groups.push(Repetition {
        operation: call.operation.into(),
        provider: provider.into(),
        class: call.class.clone(),
        iid: call.iid.clone(),
        slot: call.slot,
        hresult: hr,
        documented_interface: call
            .iid
            .as_deref()
            .and_then(documented_interface)
            .map(str::to_string),
        count: 1,
        first: None,
        last_examples: VecDeque::from([example]),
    });
    Decision::First(index)
}
pub fn first(index: usize, event: &InterfaceEvent) {
    if let Some(g) = GROUPS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get_mut(index)
    {
        g.first = Some(event.clone());
    }
}
pub fn stack() {
    STACKS.fetch_add(1, Ordering::Relaxed);
}
pub fn full_log() {
    FULL_LOGS.fetch_add(1, Ordering::Relaxed);
}
pub fn summary_log() {
    SUMMARY_LOGS.fetch_add(1, Ordering::Relaxed);
}
pub fn snapshot() -> Vec<Repetition> {
    GROUPS.try_lock().map(|g| g.clone()).unwrap_or_default()
}
pub fn metrics() -> LoggingMetrics {
    LoggingMetrics {
        stack_captures: STACKS.load(Ordering::Relaxed),
        full_events_logged: FULL_LOGS.load(Ordering::Relaxed),
        repeat_summaries_logged: SUMMARY_LOGS.load(Ordering::Relaxed),
        suppressed_repeats: SUPPRESSED.load(Ordering::Relaxed),
        work_us: WORK_NS.load(Ordering::Relaxed) / 1000,
    }
}
