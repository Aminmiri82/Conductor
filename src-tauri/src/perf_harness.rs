//! The perf harness behind `pnpm perf:run` (scripts/perf-harness.mjs). Only
//! built with the `perf-harness` feature, on macOS, and never shipped.
//!
//! The webview runs a scenario (src/app/perfScenario.ts) and emits a
//! checkpoint event after each step. Each checkpoint records the Rust heap
//! (current, and peak since the previous checkpoint) and, for every process
//! macOS holds this app responsible for (the app plus its WebKit WebContent,
//! Networking, and GPU processes), its memory footprint, CPU time, and
//! wakeups so far. Footprint is what Activity Monitor shows as Memory; RSS
//! would count shared pages and miss compressed ones.
//!
//! The events are plain strings rather than typed specta events so nothing
//! here reaches the generated bindings or a normal build.

use std::alloc::{GlobalAlloc, Layout, System};
use std::ffi::{c_char, c_int, c_void, CStr};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::OnceLock;
use std::time::Instant;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Listener};

/// Counts live heap bytes and the high-water mark. Atomics only, so the
/// overhead is a few instructions per allocation.
pub struct CountingAllocator;

static HEAP_BYTES: AtomicUsize = AtomicUsize::new(0);
static HEAP_PEAK: AtomicUsize = AtomicUsize::new(0);

fn grew(bytes: usize) {
    let now = HEAP_BYTES.fetch_add(bytes, Ordering::Relaxed) + bytes;
    HEAP_PEAK.fetch_max(now, Ordering::Relaxed);
}

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = System.alloc(layout);
        if !pointer.is_null() {
            grew(layout.size());
        }
        pointer
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let pointer = System.alloc_zeroed(layout);
        if !pointer.is_null() {
            grew(layout.size());
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        System.dealloc(pointer, layout);
        HEAP_BYTES.fetch_sub(layout.size(), Ordering::Relaxed);
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let moved = System.realloc(pointer, layout, new_size);
        if !moved.is_null() {
            if new_size >= layout.size() {
                grew(new_size - layout.size());
            } else {
                HEAP_BYTES.fetch_sub(layout.size() - new_size, Ordering::Relaxed);
            }
        }
        moved
    }
}

static STARTED: OnceLock<Instant> = OnceLock::new();

/// Called first thing in `run`, so startup time counts from process start
/// rather than from Tauri's setup.
pub fn mark_start() {
    STARTED.get_or_init(Instant::now);
}

// libproc, the Mach timebase, and the responsibility SPI, all in libSystem.
// Responsibility is how macOS (and Activity Monitor) attributes WebKit's XPC
// processes to the app that launched them; their parent pid is launchd, so it
// is the only reliable link. Fine for a profiling build; never ship it.
extern "C" {
    fn proc_listallpids(buffer: *mut c_void, buffer_size: c_int) -> c_int;
    fn proc_name(pid: c_int, buffer: *mut c_void, buffer_size: u32) -> c_int;
    fn proc_pid_rusage(pid: c_int, flavor: c_int, buffer: *mut c_void) -> c_int;
    fn responsibility_get_pid_responsible_for_pid(pid: c_int) -> c_int;
    fn mach_timebase_info(info: *mut MachTimebase) -> c_int;
}

const RUSAGE_INFO_V4: c_int = 4;

#[repr(C)]
#[derive(Default)]
struct MachTimebase {
    numer: u32,
    denom: u32,
}

/// The head of `rusage_info_v4`, padded past its full size so the kernel's
/// write always fits. Times are in Mach ticks.
#[repr(C)]
struct RusageInfo {
    uuid: [u8; 16],
    user_time: u64,
    system_time: u64,
    pkg_idle_wkups: u64,
    interrupt_wkups: u64,
    pageins: u64,
    wired_size: u64,
    resident_size: u64,
    phys_footprint: u64,
    rest: [u64; 64],
}

/// Totals since the process started; the script turns them into per-step
/// deltas.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProcessUsage {
    pid: i32,
    /// `conductor` or e.g. `com.apple.WebKit.WebContent`.
    name: String,
    footprint: u64,
    cpu_ns: u64,
    /// Idle plus interrupt wakeups: what keeps the CPU from sleeping.
    wakeups: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Checkpoint {
    label: String,
    /// How long the scenario's step took, until its result was on screen.
    step_ms: f64,
    /// Since process start; the gap between two checkpoints is the window
    /// their CPU time and wakeups were counted over.
    at_ms: f64,
    rust_heap: usize,
    /// Highest Rust heap since the previous checkpoint, so it shows what the
    /// step in between needed at its worst.
    rust_heap_peak: usize,
    processes: Vec<ProcessUsage>,
}

fn usage(pid: c_int, timebase: &MachTimebase) -> Option<ProcessUsage> {
    // SAFETY: zeroed is a valid `RusageInfo`, and it is larger than the
    // `rusage_info_v4` the kernel writes into it.
    let mut info: RusageInfo = unsafe { std::mem::zeroed() };
    let status = unsafe { proc_pid_rusage(pid, RUSAGE_INFO_V4, (&raw mut info).cast()) };
    let ticks = info.user_time + info.system_time;
    (status == 0).then(|| ProcessUsage {
        pid,
        name: name(pid),
        footprint: info.phys_footprint,
        cpu_ns: (ticks as u128 * timebase.numer as u128 / timebase.denom as u128) as u64,
        wakeups: info.pkg_idle_wkups + info.interrupt_wkups,
    })
}

fn name(pid: c_int) -> String {
    let mut buffer = [0 as c_char; 256];
    // SAFETY: proc_name writes at most `buffer.len()` bytes and terminates
    // the name, and the buffer starts zeroed.
    unsafe { proc_name(pid, buffer.as_mut_ptr().cast(), buffer.len() as u32) };
    unsafe { CStr::from_ptr(buffer.as_ptr()) }
        .to_string_lossy()
        .into_owned()
}

/// This process and every process macOS holds it responsible for. Only
/// meaningful when LaunchServices started the app (`open`): started from a
/// shell, the terminal app is responsible for the app and its WebKit
/// processes instead, and only this process is found.
fn own_processes() -> Vec<ProcessUsage> {
    let me = std::process::id() as c_int;
    let mut timebase = MachTimebase::default();
    // SAFETY: fills the two fields of the struct it is given.
    unsafe { mach_timebase_info(&mut timebase) };
    // SAFETY: a null buffer asks for the count; the second call writes at
    // most `capacity` pids. Extra room covers processes started in between.
    let count = unsafe { proc_listallpids(std::ptr::null_mut(), 0) }.max(0) as usize;
    let mut pids = vec![0 as c_int; count + 64];
    let bytes = (pids.len() * size_of::<c_int>()) as c_int;
    let listed = unsafe { proc_listallpids(pids.as_mut_ptr().cast(), bytes) }.max(0) as usize;
    pids.truncate(listed);

    let mut processes: Vec<_> = pids
        .into_iter()
        .filter(|&pid| {
            pid == me
                // SAFETY: takes any pid and returns -1 when it has gone.
                || (pid > 0 && unsafe { responsibility_get_pid_responsible_for_pid(pid) } == me)
        })
        .filter_map(|pid| usage(pid, &timebase))
        .collect();
    processes.sort_by_key(|process| process.pid != me);
    processes
}

/// The output is JSON lines: a `pid` line at startup, so the script can stop
/// this process if the scenario hangs, a `startupMs` line, one line per
/// checkpoint, and an `error` line if the scenario fails.
fn append(out: &PathBuf, line: &impl Serialize) {
    let written = OpenOptions::new()
        .create(true)
        .append(true)
        .open(out)
        .and_then(|mut file| writeln!(file, "{}", serde_json::to_string(line)?));
    if let Err(error) = written {
        eprintln!(
            "perf-harness: could not write to {}: {error}",
            out.display()
        );
    }
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct CheckpointEvent {
    label: String,
    step_ms: f64,
}

fn since_start_ms() -> f64 {
    let started = STARTED.get().copied().unwrap_or_else(Instant::now);
    started.elapsed().as_secs_f64() * 1000.0
}

fn checkpoint(event: CheckpointEvent) -> Checkpoint {
    Checkpoint {
        label: event.label,
        step_ms: event.step_ms,
        at_ms: since_start_ms(),
        rust_heap: HEAP_BYTES.load(Ordering::Relaxed),
        rust_heap_peak: HEAP_PEAK.swap(HEAP_BYTES.load(Ordering::Relaxed), Ordering::Relaxed),
        processes: own_processes(),
    }
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ScenarioConfig {
    collection_path: Option<String>,
}

/// Answers the scenario's events. Configured by the environment that
/// scripts/perf-harness.mjs launches the app with.
pub fn listen(app: &AppHandle) {
    let Some(out) = std::env::var_os("CONDUCTOR_PERF_OUT").map(PathBuf::from) else {
        return;
    };
    append(&out, &serde_json::json!({ "pid": std::process::id() }));
    let config = ScenarioConfig {
        collection_path: std::env::var("CONDUCTOR_PERF_COLLECTION").ok(),
    };

    let handle = app.clone();
    app.listen_any("perf:hello", move |_| {
        let _ = handle.emit("perf:config", config.clone());
    });

    // The scenario sends this once the first screen has painted.
    let ready_out = out.clone();
    app.listen_any("perf:ready", move |_| {
        append(
            &ready_out,
            &serde_json::json!({ "startupMs": since_start_ms() }),
        );
    });

    let handle = app.clone();
    let checkpoint_out = out.clone();
    app.listen_any("perf:checkpoint", move |event| {
        match serde_json::from_str(event.payload()) {
            Ok(event) => append(&checkpoint_out, &checkpoint(event)),
            Err(error) => append(
                &checkpoint_out,
                &serde_json::json!({ "error": format!("bad checkpoint: {error}") }),
            ),
        }
        let _ = handle.emit("perf:recorded", ());
    });

    let handle = app.clone();
    app.listen_any("perf:done", move |_| handle.exit(0));

    let handle = app.clone();
    app.listen_any("perf:failed", move |event| {
        let message: String = serde_json::from_str(event.payload()).unwrap_or_default();
        append(&out, &serde_json::json!({ "error": message }));
        handle.exit(1);
    });
}
