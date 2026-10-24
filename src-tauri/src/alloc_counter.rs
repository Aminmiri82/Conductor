//! Counts Rust heap bytes. Only built for tests and for perf harness builds;
//! the shipped app uses the system allocator directly.
//!
//! - Perf harness builds read the process-wide totals at each checkpoint
//!   (`live`, `take_peak`).
//! - Tests measure one closure on the current thread (`peak_while`), so other
//!   tests running in parallel do not count. SQLite's own allocations do not
//!   go through Rust and are never counted.

use std::alloc::{GlobalAlloc, Layout, System};

pub struct CountingAllocator;

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
        shrank(layout.size());
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let moved = System.realloc(pointer, layout, new_size);
        if !moved.is_null() {
            if new_size >= layout.size() {
                grew(new_size - layout.size());
            } else {
                shrank(layout.size() - new_size);
            }
        }
        moved
    }
}

fn grew(bytes: usize) {
    #[cfg(all(feature = "perf-harness", target_os = "macos"))]
    process::grew(bytes);
    #[cfg(test)]
    per_thread::grew(bytes);
}

fn shrank(bytes: usize) {
    #[cfg(all(feature = "perf-harness", target_os = "macos"))]
    process::shrank(bytes);
    #[cfg(test)]
    per_thread::shrank(bytes);
}

/// Atomics only, so the overhead is a few instructions per allocation.
#[cfg(all(feature = "perf-harness", target_os = "macos"))]
mod process {
    use std::sync::atomic::{AtomicUsize, Ordering};

    static LIVE: AtomicUsize = AtomicUsize::new(0);
    static PEAK: AtomicUsize = AtomicUsize::new(0);

    pub fn grew(bytes: usize) {
        let now = LIVE.fetch_add(bytes, Ordering::Relaxed) + bytes;
        PEAK.fetch_max(now, Ordering::Relaxed);
    }

    pub fn shrank(bytes: usize) {
        LIVE.fetch_sub(bytes, Ordering::Relaxed);
    }

    pub fn live() -> usize {
        LIVE.load(Ordering::Relaxed)
    }

    /// The highest live heap since the previous call.
    pub fn take_peak() -> usize {
        PEAK.swap(live(), Ordering::Relaxed)
    }
}

#[cfg(all(feature = "perf-harness", target_os = "macos"))]
pub use process::{live, take_peak};

#[cfg(test)]
pub use per_thread::peak_while;

/// Per thread and off unless `peak_while` is running, so tests that do not
/// measure pay one thread-local read per allocation. Const-initialized cells
/// with no destructor, so reading them never allocates and never panics, even
/// while the thread is exiting.
#[cfg(test)]
mod per_thread {
    use std::cell::Cell;

    thread_local! {
        static MEASURING: Cell<bool> = const { Cell::new(false) };
        // Signed: memory allocated before the measurement may be freed
        // during it.
        static LIVE: Cell<isize> = const { Cell::new(0) };
        static PEAK: Cell<isize> = const { Cell::new(0) };
    }

    pub fn grew(bytes: usize) {
        if MEASURING.get() {
            let now = LIVE.get() + bytes as isize;
            LIVE.set(now);
            PEAK.set(PEAK.get().max(now));
        }
    }

    pub fn shrank(bytes: usize) {
        if MEASURING.get() {
            LIVE.set(LIVE.get() - bytes as isize);
        }
    }

    /// Runs `work` and returns the most heap it held at once on this thread,
    /// above what was live when it started. Allocations on other threads (a
    /// runtime's workers) are not counted, so measure synchronous work.
    pub fn peak_while<T>(work: impl FnOnce() -> T) -> (T, usize) {
        LIVE.set(0);
        PEAK.set(0);
        MEASURING.set(true);
        let result = work();
        MEASURING.set(false);
        (result, PEAK.get().max(0) as usize)
    }
}
