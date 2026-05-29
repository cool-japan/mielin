//! Async Timer Integration
//!
//! Provides async-await compatible timer primitives backed by the kernel's
//! jiffy-based timer subsystem. Enables non-blocking sleep in async tasks
//! without spinning the executor.
//!
//! # Architecture
//!
//! The bridge between the jiffy counter and Rust's async machinery:
//!
//! ```text
//! timer_interrupt_handler()
//!          ↓
//! tick_async_timers()
//!          ↓
//! AsyncTimerRegistry::tick(current_jiffies)
//!          ↓ (for each expired entry)
//! Waker::wake()  +  AtomicBool::store(true)
//!          ↓
//! AsyncExecutor::poll_once() wakes SleepFuture
//! ```
//!
//! # Usage
//!
//! ```rust,ignore
//! use mielin_kernel::async_timer::sleep_ms_async;
//!
//! async fn delayed_task() {
//!     sleep_ms_async(100).await;  // non-blocking: suspends, not spins
//!     // 100ms have elapsed — continue
//! }
//! ```

use crate::timer::jiffies;
use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::future::Future;
use core::pin::Pin;
use core::sync::atomic::{AtomicBool, Ordering};
use core::task::{Context, Poll, Waker};
use spin::Mutex;

/// Ticks-per-millisecond at the default 1000 Hz tick rate.
///
/// At 1000 Hz there is exactly 1 jiffy per millisecond.  If the kernel is
/// initialised at a different rate, callers that need sub-millisecond precision
/// should use [`sleep_ticks`] or [`sleep_until_jiffy`] directly.
const DEFAULT_TICKS_PER_MS: u64 = 1;

// ---------------------------------------------------------------------------
// AsyncTimerRegistry
// ---------------------------------------------------------------------------

/// Registry mapping `wake_at_jiffies` → list of `(Waker, Arc<AtomicBool>)`.
///
/// Entries are stored in a [`BTreeMap`] keyed by deadline so that
/// [`tick`](Self::tick) can efficiently drain only expired entries via a
/// range query rather than scanning the entire table on every timer interrupt.
pub struct AsyncTimerRegistry {
    /// Sorted map: deadline → Vec<(waker, ready-flag)>
    pending: BTreeMap<u64, Vec<(Waker, Arc<AtomicBool>)>>,
    /// Monotonically increasing count of all registered entries (for stats).
    registered_count: u64,
    /// Monotonically increasing count of all fired (woken) entries (for stats).
    fired_count: u64,
}

impl AsyncTimerRegistry {
    /// Create an empty registry.
    ///
    /// `const fn` so it can initialise the global [`ASYNC_TIMER_REGISTRY`]
    /// static without a runtime call.
    pub const fn new() -> Self {
        Self {
            pending: BTreeMap::new(),
            registered_count: 0,
            fired_count: 0,
        }
    }

    /// Register `waker` to be invoked at or after `wake_at` jiffies.
    ///
    /// The `ready` flag is set to `true` atomically just before the waker is
    /// called, so that [`SleepFuture::poll`] can short-circuit without
    /// re-querying [`jiffies()`].
    pub fn register(&mut self, wake_at: u64, waker: Waker, ready: Arc<AtomicBool>) {
        self.pending
            .entry(wake_at)
            .or_default()
            .push((waker, ready));
        self.registered_count += 1;
    }

    /// Called from the timer interrupt path.
    ///
    /// Drains all entries whose deadline is ≤ `current_jiffies`, sets their
    /// ready flags, and invokes their wakers.  Returns the count of wakers
    /// fired.
    ///
    /// The BTreeMap range split keeps this O(k · log n) where k is the number
    /// of expired entries — efficient even when many futures are pending.
    pub fn tick(&mut self, current_jiffies: u64) -> usize {
        // Collect expired keys first (cannot drain while borrowing the map).
        let expired_keys: Vec<u64> = self
            .pending
            .range(..=current_jiffies)
            .map(|(&k, _)| k)
            .collect();

        let mut fired = 0usize;
        for key in expired_keys {
            if let Some(entries) = self.pending.remove(&key) {
                for (waker, ready) in entries {
                    ready.store(true, Ordering::Release);
                    waker.wake();
                    fired += 1;
                }
            }
        }
        self.fired_count += fired as u64;
        fired
    }

    /// Number of waker slots currently waiting in the registry.
    pub fn pending_count(&self) -> usize {
        self.pending.values().map(|v| v.len()).sum()
    }

    /// Total registrations since the registry was created.
    pub fn registered_count(&self) -> u64 {
        self.registered_count
    }

    /// Total wakers fired since the registry was created.
    pub fn fired_count(&self) -> u64 {
        self.fired_count
    }
}

impl Default for AsyncTimerRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Global async timer registry, protected by a spin-lock.
///
/// The lock is only held for the brief duration of [`register`](AsyncTimerRegistry::register)
/// and [`tick`](AsyncTimerRegistry::tick) calls, so contention is minimal.
pub static ASYNC_TIMER_REGISTRY: Mutex<AsyncTimerRegistry> = Mutex::new(AsyncTimerRegistry::new());

/// Tick the async timer registry with the current jiffy value.
///
/// This function must be called by [`crate::timer`]'s interrupt handler on
/// every timer tick.  Futures whose deadlines have elapsed will have their
/// wakers invoked so that the executor can re-poll them.
pub fn tick_async_timers() {
    let now = jiffies();
    let _fired = ASYNC_TIMER_REGISTRY.lock().tick(now);
}

// ---------------------------------------------------------------------------
// SleepFuture
// ---------------------------------------------------------------------------

/// A [`Future`] that yields [`Poll::Pending`] until the jiffy counter reaches
/// `wake_at`, then returns [`Poll::Ready(())`](Poll::Ready).
///
/// On the **first** poll the future registers itself with
/// [`ASYNC_TIMER_REGISTRY`] so that the timer interrupt path can wake it
/// without polling every pending future on each tick.  Subsequent polls
/// simply check the `ready` flag which is set atomically by
/// [`AsyncTimerRegistry::tick`].
pub struct SleepFuture {
    /// Jiffy value at which this future should become ready.
    wake_at: u64,
    /// Shared flag — set to `true` by [`AsyncTimerRegistry::tick`].
    ready: Arc<AtomicBool>,
    /// `true` once the waker has been registered with the global registry.
    registered: AtomicBool,
}

impl SleepFuture {
    fn new(wake_at: u64) -> Self {
        Self {
            wake_at,
            ready: Arc::new(AtomicBool::new(false)),
            registered: AtomicBool::new(false),
        }
    }
}

impl Future for SleepFuture {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        // Fast path: the timer interrupt already fired for this future.
        if self.ready.load(Ordering::Acquire) {
            return Poll::Ready(());
        }

        // Inline check: the deadline may have passed before we registered
        // (e.g. a zero-millisecond sleep, or extreme scheduler latency).
        if jiffies() >= self.wake_at {
            return Poll::Ready(());
        }

        // Register the waker on the first poll only.  We do not re-register
        // on subsequent polls because the waker provided by the executor is
        // stable across polls of the same task, and re-registering would
        // silently duplicate entries in the registry.
        if !self.registered.load(Ordering::Acquire) {
            self.registered.store(true, Ordering::Release);
            ASYNC_TIMER_REGISTRY.lock().register(
                self.wake_at,
                cx.waker().clone(),
                Arc::clone(&self.ready),
            );
        }

        Poll::Pending
    }
}

// ---------------------------------------------------------------------------
// Constructor helpers
// ---------------------------------------------------------------------------

/// Create a [`SleepFuture`] that completes after approximately `ms`
/// milliseconds (measured in jiffies at the default 1000 Hz tick rate).
///
/// This is the primary convenience API for async timer usage in the kernel.
pub fn sleep_ms_async(ms: u64) -> SleepFuture {
    let ticks = ms.saturating_mul(DEFAULT_TICKS_PER_MS);
    sleep_until_jiffy(jiffies().saturating_add(ticks))
}

/// Create a [`SleepFuture`] that completes when `jiffies() >= wake_at`.
///
/// If `wake_at` is already in the past the future resolves on the very next
/// poll without ever touching the registry.
pub fn sleep_until_jiffy(wake_at: u64) -> SleepFuture {
    SleepFuture::new(wake_at)
}

/// Create a [`SleepFuture`] that completes after `ticks` additional jiffy
/// ticks from the moment of creation.
pub fn sleep_ticks(ticks: u64) -> SleepFuture {
    sleep_until_jiffy(jiffies().saturating_add(ticks))
}

// ---------------------------------------------------------------------------
// PeriodicTimer
// ---------------------------------------------------------------------------

/// An async periodic tick source.
///
/// Each call to [`next_tick_future`](Self::next_tick_future) returns a
/// [`SleepFuture`] for the next scheduled deadline and advances the internal
/// clock by one interval.  The caller `.await`s each future in a loop:
///
/// ```rust,ignore
/// let mut timer = PeriodicTimer::new_ms(50); // 50 ms intervals
/// loop {
///     timer.next_tick_future().await;
///     // ... periodic work ...
/// }
/// ```
///
/// Because the deadline is computed eagerly at `next_tick_future()` call time
/// rather than at poll time, the timer is *drift-free*: each deadline is
/// exactly `interval_ticks` after the previous one regardless of scheduling
/// jitter.
pub struct PeriodicTimer {
    interval_ticks: u64,
    next_tick: u64,
    tick_count: u64,
}

impl PeriodicTimer {
    /// Create a periodic timer with the given interval in milliseconds.
    pub fn new_ms(interval_ms: u64) -> Self {
        let interval_ticks = interval_ms.saturating_mul(DEFAULT_TICKS_PER_MS);
        let next_tick = jiffies().saturating_add(interval_ticks);
        Self {
            interval_ticks,
            next_tick,
            tick_count: 0,
        }
    }

    /// Create a periodic timer with the given interval in jiffy ticks.
    pub fn new_ticks(interval_ticks: u64) -> Self {
        let next_tick = jiffies().saturating_add(interval_ticks);
        Self {
            interval_ticks,
            next_tick,
            tick_count: 0,
        }
    }

    /// Return a [`SleepFuture`] for the next scheduled deadline, advancing
    /// the internal deadline by one interval.
    ///
    /// The returned future must be `.await`-ed before calling this method
    /// again to preserve monotonically increasing tick semantics.
    pub fn next_tick_future(&mut self) -> SleepFuture {
        let deadline = self.next_tick;
        self.next_tick = self.next_tick.saturating_add(self.interval_ticks);
        self.tick_count += 1;
        sleep_until_jiffy(deadline)
    }

    /// Number of tick futures that have been *issued* (i.e. the number of
    /// times [`next_tick_future`](Self::next_tick_future) has been called),
    /// whether or not all of them have been awaited to completion.
    pub fn tick_count(&self) -> u64 {
        self.tick_count
    }

    /// Configured interval between ticks in jiffy ticks.
    pub fn interval_ticks(&self) -> u64 {
        self.interval_ticks
    }

    /// The jiffy deadline for the *next* tick that will be returned by
    /// [`next_tick_future`](Self::next_tick_future).
    pub fn next_deadline(&self) -> u64 {
        self.next_tick
    }
}

// ---------------------------------------------------------------------------
// Stats
// ---------------------------------------------------------------------------

/// Point-in-time snapshot of async timer registry statistics.
#[derive(Debug, Clone, Copy)]
pub struct AsyncTimerStats {
    /// Total number of `(Waker, ready-flag)` pairs ever registered.
    pub registered_count: u64,
    /// Total number of wakers that have been fired.
    pub fired_count: u64,
    /// Number of waker slots currently in the registry awaiting a deadline.
    pub pending_count: usize,
}

/// Return a point-in-time snapshot of async timer registry statistics.
pub fn async_timer_stats() -> AsyncTimerStats {
    let reg = ASYNC_TIMER_REGISTRY.lock();
    AsyncTimerStats {
        registered_count: reg.registered_count(),
        fired_count: reg.fired_count(),
        pending_count: reg.pending_count(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::collections::BTreeSet;
    use alloc::sync::Arc;
    use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use core::task::{Context, RawWaker, RawWakerVTable, Waker};

    // -----------------------------------------------------------------------
    // Test-only PRNG (no external deps)
    // -----------------------------------------------------------------------

    struct Xorshift64(u64);

    impl Xorshift64 {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
    }

    // -----------------------------------------------------------------------
    // Waker helpers
    // -----------------------------------------------------------------------

    /// A minimal no-op waker that does nothing when woken.
    fn noop_waker() -> Waker {
        static VTABLE: RawWakerVTable =
            RawWakerVTable::new(|p| RawWaker::new(p, &VTABLE), |_| {}, |_| {}, |_| {});
        unsafe { Waker::from_raw(RawWaker::new(core::ptr::null(), &VTABLE)) }
    }

    /// A counting waker that increments `counter` each time it is woken.
    ///
    /// The `Arc<AtomicU64>` is transferred into the raw waker and properly
    /// reference-counted via the vtable clone/drop functions.
    fn counting_waker(counter: Arc<AtomicU64>) -> Waker {
        let ptr = Arc::into_raw(counter) as *const ();

        unsafe fn clone_fn(ptr: *const ()) -> RawWaker {
            let arc = Arc::from_raw(ptr as *const AtomicU64);
            let cloned = Arc::clone(&arc);
            core::mem::forget(arc);
            RawWaker::new(Arc::into_raw(cloned) as *const (), &VTABLE)
        }
        unsafe fn wake_fn(ptr: *const ()) {
            let arc = Arc::from_raw(ptr as *const AtomicU64);
            arc.fetch_add(1, Ordering::Relaxed);
            // arc drops here, decrementing the refcount
        }
        unsafe fn wake_by_ref_fn(ptr: *const ()) {
            let arc = Arc::from_raw(ptr as *const AtomicU64);
            arc.fetch_add(1, Ordering::Relaxed);
            core::mem::forget(arc);
        }
        unsafe fn drop_fn(ptr: *const ()) {
            drop(Arc::from_raw(ptr as *const AtomicU64));
        }

        static VTABLE: RawWakerVTable =
            RawWakerVTable::new(clone_fn, wake_fn, wake_by_ref_fn, drop_fn);

        unsafe { Waker::from_raw(RawWaker::new(ptr, &VTABLE)) }
    }

    /// Poll a pinned boxed future with a no-op waker and return its result.
    fn poll_once<F: Future>(
        future: &mut core::pin::Pin<alloc::boxed::Box<F>>,
    ) -> core::task::Poll<F::Output> {
        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);
        future.as_mut().poll(&mut cx)
    }

    // -----------------------------------------------------------------------
    // AsyncTimerRegistry — unit tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_async_timer_registry_tick_fires_wakers() {
        let mut reg = AsyncTimerRegistry::new();
        let counter = Arc::new(AtomicU64::new(0));
        let ready = Arc::new(AtomicBool::new(false));
        reg.register(10, counting_waker(Arc::clone(&counter)), Arc::clone(&ready));
        let fired = reg.tick(10);
        assert_eq!(fired, 1);
        assert!(ready.load(Ordering::Acquire));
        assert_eq!(counter.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn test_async_timer_registry_tick_only_fires_ready_entries() {
        let mut reg = AsyncTimerRegistry::new();
        let c1 = Arc::new(AtomicU64::new(0));
        let c2 = Arc::new(AtomicU64::new(0));
        let r1 = Arc::new(AtomicBool::new(false));
        let r2 = Arc::new(AtomicBool::new(false));
        reg.register(5, counting_waker(Arc::clone(&c1)), Arc::clone(&r1));
        reg.register(20, counting_waker(Arc::clone(&c2)), Arc::clone(&r2));
        // Tick at jiffy 10: only entry at deadline 5 should fire.
        let fired = reg.tick(10);
        assert_eq!(fired, 1);
        assert_eq!(c1.load(Ordering::Relaxed), 1);
        assert_eq!(c2.load(Ordering::Relaxed), 0);
        // Tick at jiffy 25: the remaining entry fires.
        let fired2 = reg.tick(25);
        assert_eq!(fired2, 1);
        assert_eq!(c2.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn test_async_timer_registry_pending_count() {
        let mut reg = AsyncTimerRegistry::new();
        assert_eq!(reg.pending_count(), 0);
        let r = Arc::new(AtomicBool::new(false));
        reg.register(100, noop_waker(), Arc::clone(&r));
        reg.register(100, noop_waker(), Arc::clone(&r));
        reg.register(200, noop_waker(), Arc::clone(&r));
        assert_eq!(reg.pending_count(), 3);
        reg.tick(150);
        assert_eq!(reg.pending_count(), 1);
    }

    #[test]
    fn test_async_timer_registry_multiple_wakers_same_tick() {
        let mut reg = AsyncTimerRegistry::new();
        let total = Arc::new(AtomicU64::new(0));
        for _ in 0..5 {
            let r = Arc::new(AtomicBool::new(false));
            reg.register(42, counting_waker(Arc::clone(&total)), r);
        }
        let fired = reg.tick(42);
        assert_eq!(fired, 5);
        assert_eq!(total.load(Ordering::Relaxed), 5);
    }

    #[test]
    fn test_async_timer_registry_empty_tick_noop() {
        let mut reg = AsyncTimerRegistry::new();
        let fired = reg.tick(9999);
        assert_eq!(fired, 0);
        assert_eq!(reg.pending_count(), 0);
    }

    #[test]
    fn test_async_timer_registry_sorted_firing_order() {
        let mut reg = AsyncTimerRegistry::new();
        // Register five entries with deadlines spread across the range [10, 50].
        for deadline in [30u64, 10, 50, 20, 40] {
            let r = Arc::new(AtomicBool::new(false));
            reg.register(deadline, noop_waker(), r);
        }
        // Tick at 30: entries at 10, 20, 30 (three) should fire.
        let fired = reg.tick(30);
        assert_eq!(fired, 3);
        // Entries at 40, 50 remain.
        assert_eq!(reg.pending_count(), 2);
    }

    #[test]
    fn test_async_timer_stats() {
        let mut reg = AsyncTimerRegistry::new();
        assert_eq!(reg.registered_count(), 0);
        assert_eq!(reg.fired_count(), 0);
        let r = Arc::new(AtomicBool::new(false));
        reg.register(1, noop_waker(), Arc::clone(&r));
        reg.register(2, noop_waker(), Arc::clone(&r));
        assert_eq!(reg.registered_count(), 2);
        assert_eq!(reg.fired_count(), 0);
        reg.tick(1);
        assert_eq!(reg.fired_count(), 1);
        reg.tick(2);
        assert_eq!(reg.fired_count(), 2);
    }

    // -----------------------------------------------------------------------
    // SleepFuture — unit tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_sleep_future_ready_immediately_when_past_deadline() {
        // Deadline 0 is always in the past (jiffies() >= 0).
        let mut f = alloc::boxed::Box::pin(sleep_until_jiffy(0));
        assert_eq!(poll_once(&mut f), core::task::Poll::Ready(()));
    }

    #[test]
    fn test_sleep_until_jiffy_past_is_ready() {
        let current = jiffies();
        let mut f = alloc::boxed::Box::pin(sleep_until_jiffy(current.saturating_sub(100)));
        assert_eq!(poll_once(&mut f), core::task::Poll::Ready(()));
    }

    #[test]
    fn test_sleep_future_pending_when_deadline_in_future() {
        let current = jiffies();
        let mut f = alloc::boxed::Box::pin(sleep_until_jiffy(current + 1_000_000));
        assert_eq!(poll_once(&mut f), core::task::Poll::Pending);
    }

    #[test]
    fn test_sleep_ms_async_returns_future() {
        // sleep_ms_async(0) with 0 extra ticks should resolve immediately.
        let mut f = alloc::boxed::Box::pin(sleep_ms_async(0));
        // It may be Ready or Pending depending on jiffies() race; just ensure
        // it does not panic or hang.
        let _result = poll_once(&mut f);
    }

    #[test]
    fn test_sleep_future_waker_registered_on_first_poll() {
        // Verify that polling a pending future registers into ASYNC_TIMER_REGISTRY.
        let before = async_timer_stats().registered_count;
        let current = jiffies();
        let mut f = alloc::boxed::Box::pin(sleep_until_jiffy(current + 999_999));
        let _ = poll_once(&mut f);
        let after = async_timer_stats().registered_count;
        assert!(
            after > before,
            "expected at least one new registration in the global registry"
        );
    }

    #[test]
    fn test_sleep_future_ready_flag_set_by_tick() {
        let mut reg = AsyncTimerRegistry::new();
        let ready = Arc::new(AtomicBool::new(false));
        reg.register(1000, noop_waker(), Arc::clone(&ready));
        assert!(!ready.load(Ordering::Acquire));
        reg.tick(1000);
        assert!(ready.load(Ordering::Acquire));
    }

    #[test]
    fn test_sleep_future_poll_pending_then_ready() {
        // Create a future with a far-future deadline, poll it (→ Pending),
        // then synthetically set the ready flag and poll again (→ Ready).
        let deadline = jiffies() + 5_000_000;
        let mut f = SleepFuture::new(deadline);
        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);

        // First poll: returns Pending and registers the waker.
        let first = core::pin::Pin::new(&mut f).poll(&mut cx);
        assert_eq!(first, core::task::Poll::Pending);

        // Simulate the timer interrupt setting the ready flag.
        f.ready.store(true, Ordering::Release);

        // Second poll: the ready flag is set → Ready.
        let second = core::pin::Pin::new(&mut f).poll(&mut cx);
        assert_eq!(second, core::task::Poll::Ready(()));
    }

    #[test]
    fn test_multiple_sleep_futures_wake_in_order() {
        let mut reg = AsyncTimerRegistry::new();
        let c1 = Arc::new(AtomicU64::new(0));
        let c2 = Arc::new(AtomicU64::new(0));
        let c3 = Arc::new(AtomicU64::new(0));
        let r = Arc::new(AtomicBool::new(false));
        reg.register(10, counting_waker(Arc::clone(&c1)), Arc::clone(&r));
        reg.register(20, counting_waker(Arc::clone(&c2)), Arc::clone(&r));
        reg.register(30, counting_waker(Arc::clone(&c3)), Arc::clone(&r));

        reg.tick(10);
        assert_eq!(c1.load(Ordering::Relaxed), 1);
        assert_eq!(c2.load(Ordering::Relaxed), 0);
        assert_eq!(c3.load(Ordering::Relaxed), 0);

        reg.tick(20);
        assert_eq!(c2.load(Ordering::Relaxed), 1);
        assert_eq!(c3.load(Ordering::Relaxed), 0);

        reg.tick(30);
        assert_eq!(c3.load(Ordering::Relaxed), 1);
    }

    // -----------------------------------------------------------------------
    // PeriodicTimer — unit tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_periodic_timer_advances_next_tick() {
        let mut pt = PeriodicTimer::new_ticks(10);
        let d0 = pt.next_deadline();
        let _f0 = pt.next_tick_future();
        let d1 = pt.next_deadline();
        assert_eq!(d1, d0 + 10, "deadline must advance by interval_ticks");
    }

    #[test]
    fn test_periodic_timer_tick_count() {
        let mut pt = PeriodicTimer::new_ticks(5);
        assert_eq!(pt.tick_count(), 0);
        let _f1 = pt.next_tick_future();
        assert_eq!(pt.tick_count(), 1);
        let _f2 = pt.next_tick_future();
        assert_eq!(pt.tick_count(), 2);
        let _f3 = pt.next_tick_future();
        assert_eq!(pt.tick_count(), 3);
    }

    #[test]
    fn test_periodic_timer_interval() {
        let pt = PeriodicTimer::new_ticks(77);
        assert_eq!(pt.interval_ticks(), 77);
    }

    #[test]
    fn test_periodic_timer_new_ms_uses_default_ticks_per_ms() {
        let pt = PeriodicTimer::new_ms(50);
        assert_eq!(
            pt.interval_ticks(),
            50 * DEFAULT_TICKS_PER_MS,
            "interval_ticks must equal ms * DEFAULT_TICKS_PER_MS"
        );
    }

    #[test]
    fn test_periodic_timer_deadlines_are_monotone() {
        let mut pt = PeriodicTimer::new_ticks(7);
        let mut last = pt.next_deadline();
        for _ in 0..10 {
            let _f = pt.next_tick_future();
            let next = pt.next_deadline();
            assert!(next > last, "deadlines must be strictly increasing");
            last = next;
        }
    }

    // -----------------------------------------------------------------------
    // Integration — tick_async_timers
    // -----------------------------------------------------------------------

    #[test]
    fn test_tick_async_timers_integrates_with_registry() {
        // Register directly into the global registry with a past deadline,
        // then call tick_async_timers() which reads jiffies() and fires it.
        let current = jiffies();
        let counter = Arc::new(AtomicU64::new(0));
        let ready = Arc::new(AtomicBool::new(false));
        {
            let mut reg = ASYNC_TIMER_REGISTRY.lock();
            reg.register(
                current.saturating_sub(1),
                counting_waker(Arc::clone(&counter)),
                Arc::clone(&ready),
            );
        }
        tick_async_timers();
        assert!(
            counter.load(Ordering::Relaxed) >= 1,
            "tick_async_timers must invoke the waker for past-deadline entries"
        );
    }

    // -----------------------------------------------------------------------
    // Xorshift64 — PRNG correctness
    // -----------------------------------------------------------------------

    #[test]
    fn test_xorshift64_produces_nonzero_sequence() {
        let mut rng = Xorshift64(0xDEAD_BEEF_CAFE_1337);
        let mut seen = BTreeSet::new();
        for _ in 0..100 {
            seen.insert(rng.next());
        }
        // A correctly seeded xorshift64 must produce 100 distinct values.
        assert_eq!(seen.len(), 100);
    }

    #[test]
    fn test_xorshift64_never_produces_zero_from_nonzero_seed() {
        // The xorshift64 period is 2^64 - 1; it can never output 0 if seeded
        // with a non-zero value.
        let mut rng = Xorshift64(1);
        for _ in 0..1000 {
            assert_ne!(rng.next(), 0);
        }
    }

    // -----------------------------------------------------------------------
    // Additional edge-case tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_sleep_ticks_zero_resolves_immediately() {
        let mut f = alloc::boxed::Box::pin(sleep_ticks(0));
        // jiffies() >= jiffies() + 0 is trivially true.
        assert_eq!(poll_once(&mut f), core::task::Poll::Ready(()));
    }

    #[test]
    fn test_async_timer_registry_fired_count_accumulates() {
        let mut reg = AsyncTimerRegistry::new();
        let r = Arc::new(AtomicBool::new(false));
        for i in 1u64..=5 {
            reg.register(i, noop_waker(), Arc::clone(&r));
        }
        assert_eq!(reg.fired_count(), 0);
        for i in 1u64..=5 {
            reg.tick(i);
            assert_eq!(reg.fired_count(), i);
        }
    }

    #[test]
    fn test_sleep_future_does_not_double_register() {
        // Poll a pending future twice; the registered_count should increase
        // by exactly 1 (first poll), not 2.
        let before = async_timer_stats().registered_count;
        let current = jiffies();
        let mut f = SleepFuture::new(current + 888_888);
        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);
        let _ = core::pin::Pin::new(&mut f).poll(&mut cx);
        let _ = core::pin::Pin::new(&mut f).poll(&mut cx);
        let after = async_timer_stats().registered_count;
        assert_eq!(
            after - before,
            1,
            "future should register exactly once across multiple polls"
        );
    }
}
