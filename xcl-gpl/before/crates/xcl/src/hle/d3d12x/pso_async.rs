// SPDX-License-Identifier: GPL-2.0-only
// Xenstride: local XCL modifications through 2026-11-24; distribution notice added 2026-11-24.
// Exact upstream and local source origins are recorded in xcl-gpl/PROVENANCE.json.
//! Non-blocking pipeline creation. Translating a console shader takes milliseconds; compiling
//! the HLSL with DXC takes 0.2 s to seconds, and a title creates thousands of pipelines while
//! it loads. When a pipeline's DXIL is not in the disk cache yet, the title gets a stand-in
//! pipeline (shaders that draw nothing) at once and the real one is built on a worker; binding
//! (`SetPipelineState`, console list replay, the two-pass lookups) uses the real one as soon
//! as it is there. A graphics pipeline draws nothing until then; a compute pipeline is waited
//! for at its first bind (a skipped dispatch would corrupt what it computes).
//! `XCL_ASYNC_PSO=0` builds every pipeline in the call, as before.
//!
//! GTA6-Lab (on upstream af20b4b's lazy creation): a send the workers cannot take is built
//! inline instead of leaving a stand-in for good (c7fe076), and the counters tell apart what was
//! submitted, deferred by `lazy` and never asked for, sent, started, built, failed and waited.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Condvar, Mutex, OnceLock};

struct Pending {
    /// the build, until a worker – or a bind that cannot wait for the queue – takes it
    job: Mutex<Option<Job>>,
    /// the build went to the workers (a lazy one goes on its first `resolve`)
    sent: std::sync::atomic::AtomicBool,
    /// when it went to the workers (queue time is measured from there)
    queued_at: Mutex<std::time::Instant>,
    /// the real pipeline state (raw pointer), 0 while building or when building failed
    real: AtomicUsize,
    done: Mutex<bool>,
    cv: Condvar,
    compute: bool,
    /// of its shaders (`used`); 0 = none
    key: u64,
    /// it was used (`used::note` is told once)
    used: std::sync::atomic::AtomicBool,
    /// the stand-in was released before the real pipeline was built
    orphan: std::sync::atomic::AtomicBool,
}

impl Pending {
    fn note_use(&self) {
        if self.key != 0 && !self.used.swap(true, Ordering::Relaxed) {
            used::note(self.key);
        }
    }
}

/// stand-in handed to the title → its real pipeline
static PENDING: Mutex<Option<HashMap<usize, Arc<Pending>>>> = Mutex::new(None);

/// The build is done (`real`: its pipeline, 0 = failed).
fn complete(p: &Pending, real: usize) {
    if real == 0 {
        FAILED.fetch_add(1, Ordering::Relaxed);
    } else {
        BUILT.fetch_add(1, Ordering::Relaxed);
    }
    let orphan = {
        let mut done = p.done.lock().unwrap();
        let orphan = p.orphan.load(Ordering::Acquire);
        if !orphan {
            p.real.store(real, Ordering::Release);
        }
        *done = true;
        p.cv.notify_all();
        orphan
    };
    // outside `done`: the Release override takes PENDING (`counts` takes PENDING, then `done`)
    if orphan {
        release(real);
    }
}
static BUILT: AtomicUsize = AtomicUsize::new(0);
static FAILED: AtomicUsize = AtomicUsize::new(0);
static SUBMITTED: AtomicUsize = AtomicUsize::new(0);
static SENT: AtomicUsize = AtomicUsize::new(0);
static RUNNING: AtomicUsize = AtomicUsize::new(0);
static STARTED: AtomicUsize = AtomicUsize::new(0);
static REJECTED: AtomicUsize = AtomicUsize::new(0);
static QUEUE_US: AtomicUsize = AtomicUsize::new(0);
static BUILD_US: AtomicUsize = AtomicUsize::new(0);
static WAIT_US: AtomicUsize = AtomicUsize::new(0);
static STANDIN_BINDS: AtomicUsize = AtomicUsize::new(0);
static FAILED_BINDS: AtomicUsize = AtomicUsize::new(0);
static LONGEST_WAIT_US: AtomicUsize = AtomicUsize::new(0);
static LONG_WAITS: AtomicUsize = AtomicUsize::new(0);

fn micros(d: std::time::Duration) -> usize {
    d.as_micros().min(usize::MAX as u128) as usize
}

fn run(p: &Pending, job: Job) {
    QUEUE_US.fetch_add(micros(p.queued_at.lock().unwrap().elapsed()), Ordering::Relaxed);
    STARTED.fetch_add(1, Ordering::Relaxed);
    RUNNING.fetch_add(1, Ordering::Relaxed);
    let started = std::time::Instant::now();
    let real = job();
    BUILD_US.fetch_add(micros(started.elapsed()), Ordering::Relaxed);
    RUNNING.fetch_sub(1, Ordering::Relaxed);
    complete(p, real);
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Timings {
    pub submitted: usize,
    /// lazy (`XCL_LAZY_PSO`) pipelines never asked for: not built, not queued
    #[serde(default)]
    pub deferred_never_requested: usize,
    /// handed to the workers (at once, or on a lazy one's first use)
    #[serde(default)]
    pub sent: usize,
    pub started: usize,
    pub built: usize,
    pub failed: usize,
    pub running: usize,
    /// sent or taken by a bind, not finished (orphans included)
    pub unfinished_including_orphans: usize,
    pub scheduler_rejections: usize,
    /// Cumulative time over jobs/threads; not elapsed process time.
    pub queue_us: usize,
    pub build_work_us: usize,
    /// Includes inline builds taken over at compute bind.
    pub bind_wait_us: usize,
    pub graphics_standin_binds: usize,
    pub failed_graphics_standin_binds: usize,
    /// GTA6-Lab P12: the longest single bind wait (µs) and the waits of a second or more
    #[serde(default)]
    pub longest_bind_wait_us: usize,
    #[serde(default)]
    pub bind_waits_over_1s: usize,
    /// times no frame was presented for `stall_watch::QUIET` while title threads waited here
    #[serde(default)]
    pub stalls_with_pipeline_waits: usize,
}

pub fn timings() -> Timings {
    let (built, failed, _) = counts();
    let submitted = SUBMITTED.load(Ordering::Relaxed);
    let deferred = deferred_never_requested();
    Timings {
        submitted,
        deferred_never_requested: deferred,
        sent: SENT.load(Ordering::Relaxed),
        started: STARTED.load(Ordering::Relaxed),
        built,
        failed,
        running: RUNNING.load(Ordering::Relaxed),
        unfinished_including_orphans: submitted.saturating_sub(built + failed + deferred),
        scheduler_rejections: REJECTED.load(Ordering::Relaxed),
        queue_us: QUEUE_US.load(Ordering::Relaxed),
        build_work_us: BUILD_US.load(Ordering::Relaxed),
        bind_wait_us: WAIT_US.load(Ordering::Relaxed),
        graphics_standin_binds: STANDIN_BINDS.load(Ordering::Relaxed),
        failed_graphics_standin_binds: FAILED_BINDS.load(Ordering::Relaxed),
        longest_bind_wait_us: LONGEST_WAIT_US.load(Ordering::Relaxed),
        bind_waits_over_1s: LONG_WAITS.load(Ordering::Relaxed),
        stalls_with_pipeline_waits: stall_watch::STALLS.load(Ordering::Relaxed),
    }
}

/// Lazy stand-ins whose build was neither sent nor taken by a bind.
fn deferred_never_requested() -> usize {
    PENDING.lock().unwrap().as_ref().map_or(0, |m| {
        m.values()
            .filter(|p| !p.sent.load(Ordering::Acquire) && !*p.done.lock().unwrap() && p.job.lock().unwrap().is_some())
            .count()
    })
}

pub fn enabled() -> bool {
    std::env::var("XCL_ASYNC_PSO").map_or(true, |v| v != "0")
}

/// Builds the real pipeline: its raw pointer, 0 = failed.
pub type Job = Box<dyn FnOnce() -> usize + Send>;

fn queue() -> &'static Mutex<mpsc::Sender<Arc<Pending>>> {
    static Q: OnceLock<Mutex<mpsc::Sender<Arc<Pending>>>> = OnceLock::new();
    Q.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<Arc<Pending>>();
        let rx = Arc::new(Mutex::new(rx));
        // translation recurses deeply on big programs; DXC itself runs on its own pool
        let n = crate::gpu::recomp::dxc::compile_threads();
        for k in 0..n {
            let rx = rx.clone();
            let _ = std::thread::Builder::new().name(format!("xcl-pso-{k}")).stack_size(64 << 20).spawn(move || loop {
                let next = rx.lock().unwrap().recv();
                let Ok(p) = next else { return };
                // (None: a bind took it and builds it itself)
                let job = p.job.lock().unwrap().take();
                if let Some(job) = job {
                    run(&p, job);
                }
            });
        }
        Mutex::new(tx)
    })
}

/// To the workers; a send they cannot take is built here (never a stand-in for good).
fn send(p: Arc<Pending>) {
    *p.queued_at.lock().unwrap() = std::time::Instant::now();
    SENT.fetch_add(1, Ordering::Relaxed);
    // Never keep the sender mutex held while a rejected job builds inline.
    let tx = queue().lock().unwrap().clone();
    enqueue(&tx, p);
}

fn enqueue(tx: &mpsc::Sender<Arc<Pending>>, p: Arc<Pending>) {
    if let Err(error) = tx.send(p) {
        REJECTED.fetch_add(1, Ordering::Relaxed);
        crate::xlog_warn!("D3D12.X: background PSO queue disconnected; building the rejected job inline");
        let p = error.0;
        let job = p.job.lock().unwrap().take();
        if let Some(job) = job {
            run(&p, job);
        }
    }
}

/// Release a pipeline state XCL owns (raw pointer, 0 = none).
pub fn release(p: usize) {
    if p != 0 {
        unsafe { drop(<windows::Win32::Graphics::Direct3D12::ID3D12PipelineState as windows::core::Interface>::from_raw(p as *mut std::ffi::c_void)) };
    }
}

/// `standin` went to the title; `job` builds the real pipeline on a worker.
pub fn submit(standin: usize, compute: bool, job: Job) {
    submit_as(standin, compute, job, true, 0);
}

/// Lazy pipeline creation (`XCL_LAZY_PSO=0`: off): a title's pipelines with console shaders get
/// their stand-in when made and are built when first used – the first bind of a graphics one
/// sends it to the workers, a compute one is built at its first bind. A retail game makes
/// ~51 000 pipelines in its first 30 s and uses few; each one built stays in the driver
/// (~400 KB), and the process ran out of commit.
pub fn lazy() -> bool {
    static L: OnceLock<bool> = OnceLock::new();
    *L.get_or_init(|| enabled() && std::env::var("XCL_LAZY_PSO").map_or(true, |v| v != "0"))
}

/// `submit`, sent to the workers only on its first `resolve` (`lazy`) – or now, at a low
/// priority, when an earlier run used a pipeline of these shaders (`key`, 0 = none; `used`).
pub fn submit_lazy(standin: usize, compute: bool, job: Job, key: u64) {
    submit_as(standin, compute, job, false, key);
}

fn submit_as(standin: usize, compute: bool, job: Job, to_workers: bool, key: u64) {
    SUBMITTED.fetch_add(1, Ordering::Relaxed);
    let p = Arc::new(Pending {
        job: Mutex::new(Some(job)),
        sent: std::sync::atomic::AtomicBool::new(to_workers),
        queued_at: Mutex::new(std::time::Instant::now()),
        real: AtomicUsize::new(0),
        done: Mutex::new(false),
        cv: Condvar::new(),
        compute,
        key,
        used: Default::default(),
        orphan: Default::default(),
    });
    PENDING.lock().unwrap().get_or_insert_with(HashMap::new).insert(standin, p.clone());
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        crate::xlog_info!("D3D12.X: pipelines whose shaders are not compiled yet get a stand-in and are built in the background");
        stall_watch::start();
    });
    if to_workers {
        send(p);
    } else if used::known(key) {
        if let Some(q) = preload() {
            q.list.lock().unwrap().push_back(p);
            q.cv.notify_one();
        }
    }
}

/// The pipelines a title used, by a hash of their shaders, kept between runs
/// (`used-pipelines.txt` beside the DXIL cache, a key a line).
///
/// A lazy pipeline is built at its first use: until then a graphics one draws nothing, and a
/// compute one is waited for on the title's thread. A retail title's first frames of its world
/// use hundreds of pipelines for the first time at once. The pipelines of an earlier run are
/// built while the title loads instead (`preload`): measured on a build that skips a dispatch
/// instead of waiting for its pipeline, ~2 170 dispatches were skipped in those frames without
/// this and 94 – 150 with it.
mod used {
    use std::collections::HashSet;
    use std::io::Write;
    use std::sync::{Mutex, OnceLock, RwLock};

    struct Used {
        earlier: RwLock<HashSet<u64>>,
        now: Mutex<(HashSet<u64>, Option<std::fs::File>)>,
    }

    /// The keys in a `used-pipelines.txt` (lines that are not a hexadecimal key are passed over).
    pub fn parse(text: &str) -> HashSet<u64> {
        text.lines().filter_map(|l| u64::from_str_radix(l.trim(), 16).ok()).filter(|&k| k != 0).collect()
    }

    fn get() -> &'static Used {
        static U: OnceLock<Used> = OnceLock::new();
        U.get_or_init(|| {
            // (a test neither reads nor writes the user's file)
            if cfg!(test) {
                return Used { earlier: RwLock::new(HashSet::new()), now: Mutex::new((HashSet::new(), None)) };
            }
            let path = crate::gpu::recomp::dxc::cache_dir().join("used-pipelines.txt");
            let earlier = parse(&std::fs::read_to_string(&path).unwrap_or_default());
            if !earlier.is_empty() {
                crate::xlog_info!("D3D12.X: the pipelines of {} shader sets an earlier run used are built ahead of their first use", earlier.len());
            }
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let file = std::fs::OpenOptions::new().create(true).append(true).open(&path).ok();
            Used { now: Mutex::new((earlier.clone(), file)), earlier: RwLock::new(earlier) }
        })
    }

    /// An earlier run used a pipeline of these shaders.
    pub fn known(key: u64) -> bool {
        key != 0 && get().earlier.read().unwrap().contains(&key)
    }

    /// A pipeline of these shaders is used now: kept for the next run.
    pub fn note(key: u64) {
        if key == 0 {
            return;
        }
        let mut g = get().now.lock().unwrap();
        if g.0.insert(key) {
            if let Some(f) = g.1.as_mut() {
                let _ = writeln!(f, "{key:016x}");
            }
        }
    }

    /// (tests) This run used a pipeline of these shaders.
    #[cfg(test)]
    pub fn noted(key: u64) -> bool {
        get().now.lock().unwrap().0.contains(&key)
    }

    #[cfg(test)]
    pub fn earlier_run_used(key: u64) {
        get().earlier.write().unwrap().insert(key);
    }
}

/// The workers that build the pipelines of an earlier run ahead of their first use
/// (`XCL_PSO_PRELOAD=0`: off). Apart from the ones a bind sends to (`queue`) and at the lowest
/// priority: a pipeline that is bound is not held up by the ones made ahead (whoever takes
/// `job` first builds it), and the title's own threads are not either.
struct Preload {
    list: Mutex<std::collections::VecDeque<Arc<Pending>>>,
    cv: Condvar,
}

fn preload() -> Option<&'static Preload> {
    static Q: OnceLock<Option<Preload>> = OnceLock::new();
    static STARTED: std::sync::Once = std::sync::Once::new();
    let q = Q
        .get_or_init(|| {
            if std::env::var("XCL_PSO_PRELOAD").is_ok_and(|v| v.trim() == "0") {
                return None;
            }
            Some(Preload { list: Mutex::new(Default::default()), cv: Condvar::new() })
        })
        .as_ref()?;
    STARTED.call_once(|| {
        let n = (crate::gpu::recomp::dxc::compile_threads() / 4).clamp(2, 4);
        for k in 0..n {
            let _ = std::thread::Builder::new().name(format!("xcl-pso-pre-{k}")).stack_size(64 << 20).spawn(move || {
                unsafe {
                    use windows_sys::Win32::System::Threading::{GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_LOWEST};
                    SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_LOWEST);
                }
                static DONE: AtomicUsize = AtomicUsize::new(0);
                loop {
                    let p = {
                        let mut g = q.list.lock().unwrap();
                        loop {
                            if let Some(p) = g.pop_front() {
                                break p;
                            }
                            g = q.cv.wait(g).unwrap();
                        }
                    };
                    // (released by the title meanwhile)
                    if p.orphan.load(Ordering::Acquire) {
                        continue;
                    }
                    // (None: a bind took it and builds it itself)
                    let job = p.job.lock().unwrap().take();
                    if let Some(job) = job {
                        let real = job();
                        complete(&p, real);
                        let n = DONE.fetch_add(1, Ordering::Relaxed) + 1;
                        if n.is_power_of_two() {
                            crate::xlog_info!("D3D12.X: {n} pipelines built ahead of their first use");
                        }
                    }
                }
            });
        }
    });
    Some(q)
}

#[cfg(test)]
mod used_tests {
    use super::*;

    #[test]
    fn the_keys_of_a_file_are_read_and_other_lines_passed_over() {
        let keys = used::parse("00000000000000ff\n\nnot a key\n  1a2b \n0\n");
        assert_eq!(keys.len(), 2);
        assert!(keys.contains(&0xff) && keys.contains(&0x1a2b));
    }

    /// A lazy pipeline of shaders an earlier run used is built without a bind; another one is
    /// built only at its first use, and that use is kept for the next run.
    #[test]
    fn a_pipeline_an_earlier_run_used_is_built_ahead_of_its_first_use() {
        let (known, unknown) = (0x7E57_7001usize, 0x7E57_7002usize);
        let (known_key, unknown_key) = (0x5EED_0001u64, 0x5EED_0002u64);
        used::earlier_run_used(known_key);
        submit_lazy(known, false, Box::new(|| 0x4321), known_key);
        submit_lazy(unknown, false, Box::new(|| 0x4322), unknown_key);
        let start = std::time::Instant::now();
        while is_pending(known) && start.elapsed() < std::time::Duration::from_secs(10) {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(!is_pending(known), "built by the workers that build ahead, with no bind");
        assert_eq!(resolve(known), 0x4321);
        assert!(is_pending(unknown), "nobody used it and no earlier run did");
        assert!(!used::noted(unknown_key));
        assert_eq!(resolve_wait(unknown), 0x4322);
        assert!(used::noted(unknown_key) && used::noted(known_key), "a first use is kept for the next run");
        released(known);
        released(unknown);
    }
}

fn entry(pso: usize) -> Option<Arc<Pending>> {
    PENDING.lock().unwrap().as_ref()?.get(&pso).cloned()
}

/// The pipeline to use for `pso` now: the real one once built, else `pso` itself.
pub fn resolve(pso: usize) -> usize {
    let Some(p) = entry(pso) else { return pso };
    p.note_use();
    match p.real.load(Ordering::Acquire) {
        0 => {
            // a lazy one is wanted now: to the workers
            if !p.sent.swap(true, Ordering::AcqRel) {
                send(p);
            }
            pso
        }
        r => r,
    }
}

/// `resolve`, waiting for a pipeline still being built – built here if no worker has started
/// it yet (a queue of a title's hundreds of pipelines must not hold up the one it binds now).
pub fn resolve_wait(pso: usize) -> usize {
    let Some(p) = entry(pso) else { return pso };
    p.note_use();
    let started = std::time::Instant::now();
    p.sent.store(true, Ordering::Release);
    let job = p.job.lock().unwrap().take();
    let _wait = stall_watch::Wait::begin(p.compute, job.is_some());
    if let Some(job) = job {
        run(&p, job);
    }
    let mut done = p.done.lock().unwrap();
    while !*done {
        done = p.cv.wait(done).unwrap();
    }
    drop(done);
    let waited = micros(started.elapsed());
    WAIT_US.fetch_add(waited, Ordering::Relaxed);
    LONGEST_WAIT_US.fetch_max(waited, Ordering::Relaxed);
    if waited >= 1_000_000 {
        LONG_WAITS.fetch_add(1, Ordering::Relaxed);
    }
    match p.real.load(Ordering::Acquire) {
        0 => pso,
        r => r,
    }
}

/// For binding: compute pipelines are waited for, graphics ones are not.
pub fn resolve_for_bind(pso: usize) -> usize {
    match entry(pso) {
        Some(p) if p.compute => resolve_wait(pso),
        Some(p) => {
            let result = resolve(pso);
            if result == pso {
                STANDIN_BINDS.fetch_add(1, Ordering::Relaxed);
                if *p.done.lock().unwrap() {
                    FAILED_BINDS.fetch_add(1, Ordering::Relaxed);
                }
            }
            result
        }
        None => pso,
    }
}

/// GTA6-Lab P12: which title threads wait for a pipeline when the frames stop.
///
/// A retail title ends itself (a deliberate null store) when it has not finished a frame for
/// about 30 s. In the P11 run of 2026-11-24 the presents stopped at 149 s while title threads
/// were in `resolve_wait` – one of them building a 300 KB compute program in the driver for
/// 17.5 s – and the title ended itself at 179 s; the log said nothing in between. This watch
/// only reports: when no frame was presented for `QUIET` and title threads wait here, it logs
/// them (again after `QUIET` × 3 and × 5), and counts the episode. It changes no wait.
pub(crate) mod stall_watch {
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    pub const QUIET: Duration = Duration::from_secs(5);
    pub static STALLS: AtomicUsize = AtomicUsize::new(0);

    #[derive(Clone, Copy, Debug)]
    pub struct Waiting {
        pub since: Instant,
        pub compute: bool,
        /// the waiting thread builds the pipeline itself (no worker had taken it)
        pub builds_here: bool,
    }

    static WAITS: Mutex<Option<HashMap<u32, Waiting>>> = Mutex::new(None);

    fn thread_id() -> u32 {
        unsafe { windows_sys::Win32::System::Threading::GetCurrentThreadId() }
    }

    /// A wait of this thread, from `begin` until dropped.
    pub struct Wait(u32);

    impl Wait {
        pub fn begin(compute: bool, builds_here: bool) -> Wait {
            let id = thread_id();
            WAITS.lock().unwrap().get_or_insert_with(HashMap::new).insert(id, Waiting { since: Instant::now(), compute, builds_here });
            Wait(id)
        }
    }

    impl Drop for Wait {
        fn drop(&mut self) {
            if let Some(m) = WAITS.lock().unwrap().as_mut() {
                m.remove(&self.0);
            }
        }
    }

    /// (thread, how long, compute, builds here), the longest first
    pub fn waiting(now: Instant) -> Vec<(u32, Duration, bool, bool)> {
        let mut v: Vec<_> = WAITS
            .lock()
            .unwrap()
            .as_ref()
            .map(|m| m.iter().map(|(&t, w)| (t, now.saturating_duration_since(w.since), w.compute, w.builds_here)).collect())
            .unwrap_or_default();
        v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        v
    }

    /// The line for a stall of `quiet` with these waits; None when nothing waits here (the stall
    /// is elsewhere) or when it is not one of the points to report (`QUIET` × 1, 3, 5).
    pub fn report(quiet: Duration, reported: u32, waits: &[(u32, Duration, bool, bool)]) -> Option<String> {
        let next = [1u32, 3, 5].get(reported as usize)?;
        if quiet < QUIET * *next || waits.is_empty() {
            return None;
        }
        let list: Vec<String> = waits
            .iter()
            .take(8)
            .map(|(t, d, c, b)| format!("thread {t}: {} pipeline, {:.1} s{}", if *c { "compute" } else { "graphics" }, d.as_secs_f64(), if *b { ", building it itself" } else { "" }))
            .collect();
        Some(format!(
            "D3D12.X: no frame presented for {:.0} s; {} title thread(s) wait for a pipeline: {}{}",
            quiet.as_secs_f64(),
            waits.len(),
            list.join("; "),
            if waits.len() > 8 { "; …" } else { "" }
        ))
    }

    pub fn start() {
        if cfg!(test) {
            return;
        }
        let _ = std::thread::Builder::new().name("xcl-stall-watch".into()).spawn(|| {
            let (mut last_n, mut last_at, mut reported) = (super::super::present::presents(), Instant::now(), 0u32);
            loop {
                std::thread::sleep(Duration::from_secs(1));
                let now = Instant::now();
                let n = super::super::present::presents();
                // (no frame yet: the title is still starting, its waits are expected)
                if n != last_n || n == 0 {
                    (last_n, last_at, reported) = (n, now, 0);
                    continue;
                }
                if let Some(line) = report(now - last_at, reported, &waiting(now)) {
                    if reported == 0 {
                        STALLS.fetch_add(1, Ordering::Relaxed);
                    }
                    reported += 1;
                    crate::xlog_warn!("{line}");
                }
            }
        });
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn p12_a_stall_is_reported_at_5_15_and_25_s_only_while_threads_wait() {
            let w = [(7, Duration::from_millis(17_500), true, true), (9, Duration::from_secs(3), true, false)];
            assert_eq!(report(Duration::from_secs(4), 0, &w), None, "under QUIET");
            let line = report(Duration::from_secs(5), 0, &w).unwrap();
            assert!(line.contains("no frame presented for 5 s; 2 title thread(s)"), "{line}");
            assert!(line.contains("thread 7: compute pipeline, 17.5 s, building it itself; thread 9: compute pipeline, 3.0 s"), "{line}");
            assert_eq!(report(Duration::from_secs(9), 1, &w), None, "the second report comes at 15 s");
            assert!(report(Duration::from_secs(15), 1, &w).is_some());
            assert!(report(Duration::from_secs(25), 2, &w).is_some());
            assert_eq!(report(Duration::from_secs(60), 3, &w), None, "three reports per stall");
            assert_eq!(report(Duration::from_secs(30), 0, &[]), None, "nothing waits for a pipeline: not this watch's stall");
        }

        #[test]
        fn p12_a_wait_is_listed_while_it_lasts() {
            let w = Wait::begin(true, false);
            let now = waiting(Instant::now());
            assert!(now.iter().any(|&(t, _, c, b)| t == thread_id() && c && !b));
            drop(w);
            assert!(waiting(Instant::now()).iter().all(|&(t, ..)| t != thread_id()));
        }
    }
}

/// The title released stand-in `pso` for good: forget it (its address may come back for
/// another object) and return its real pipeline to release (0: none, or still being built –
/// the worker's result is then released when it arrives).
pub fn released(pso: usize) -> usize {
    let Some(p) = PENDING.lock().unwrap().as_mut().and_then(|m| m.remove(&pso)) else { return 0 };
    let done = p.done.lock().unwrap();
    if *done {
        p.real.swap(0, Ordering::AcqRel)
    } else {
        // the worker stores its result into a Pending nobody will read: mark it orphaned
        p.orphan.store(true, Ordering::Release);
        0
    }
}

/// `pso` is a stand-in whose real pipeline is still being built.
pub fn is_pending(pso: usize) -> bool {
    entry(pso).is_some_and(|p| !*p.done.lock().unwrap())
}

/// (built in the background, failed, still building)
pub fn counts() -> (usize, usize, usize) {
    let pending = PENDING.lock().unwrap().as_ref().map_or(0, |m| m.values().filter(|p| !*p.done.lock().unwrap()).count());
    (BUILT.load(Ordering::Relaxed), FAILED.load(Ordering::Relaxed), pending)
}

#[cfg(test)]
mod p3_tests {
    use super::*;

    fn pending(job: Job, sent: bool) -> Arc<Pending> {
        Arc::new(Pending {
            job: Mutex::new(Some(job)),
            sent: std::sync::atomic::AtomicBool::new(sent),
            queued_at: Mutex::new(std::time::Instant::now()),
            real: AtomicUsize::new(0),
            done: Mutex::new(false),
            cv: Condvar::new(),
            compute: false,
            orphan: Default::default(),
            key: 0,
            used: Default::default(),
        })
    }

    /// P12: a compute bind that waits for a slow build is listed while it waits and counted after.
    #[test]
    fn p12_a_slow_compute_bind_is_listed_while_it_waits_and_counted() {
        let key = Box::into_raw(Box::new(0u8)) as usize;
        let before = timings();
        submit(key, true, Box::new(|| {
            std::thread::sleep(std::time::Duration::from_millis(1100));
            0x5151
        }));
        let waiter = std::thread::spawn(move || resolve_for_bind(key));
        let start = std::time::Instant::now();
        let mut seen = false;
        while !seen && start.elapsed() < std::time::Duration::from_secs(5) {
            seen = stall_watch::waiting(std::time::Instant::now()).iter().any(|w| w.2);
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(seen, "the waiting bind is listed");
        assert_eq!(waiter.join().unwrap(), 0x5151);
        let t = timings();
        assert!(t.bind_waits_over_1s > before.bind_waits_over_1s);
        assert!(t.longest_bind_wait_us >= 1_000_000);
        assert_eq!(released(key), 0x5151);
        drop(unsafe { Box::from_raw(key as *mut u8) });
    }

    #[test]
    fn p3_disconnected_scheduler_does_not_leave_a_pipeline_pending_forever() {
        let (tx, rx) = mpsc::channel();
        drop(rx);
        let p = pending(Box::new(|| 0), true);
        enqueue(&tx, p.clone());
        assert!(*p.done.lock().unwrap(), "rejected background submission must finish explicitly, not remain a permanent stand-in");
        assert!(p.job.lock().unwrap().is_none());
    }

    /// Upstream's lazy pipelines: one never asked for is neither built nor counted as queued;
    /// a compute bind builds it in the call.
    #[test]
    fn p6_lazy_pipelines_are_counted_apart_until_their_first_use() {
        let key = Box::into_raw(Box::new(0u8)) as usize; // a unique stand-in address
        let before = timings();
        submit_lazy(key, true, Box::new(|| 0), 0);
        let t = timings();
        assert_eq!(t.submitted, before.submitted + 1);
        assert!(t.deferred_never_requested >= 1);
        assert!(is_pending(key));
        // compute: built at its first bind, in the call (a failed build: the stand-in stays)
        assert_eq!(resolve_for_bind(key), key);
        assert!(!is_pending(key));
        assert!(timings().failed > before.failed);
        assert_eq!(released(key), 0);
        drop(unsafe { Box::from_raw(key as *mut u8) });
    }
}
