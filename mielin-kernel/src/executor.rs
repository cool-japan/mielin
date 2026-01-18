//! Async/Await Executor
//!
//! Provides a `no_std` compatible async runtime for the kernel.
//! Enables cooperative multitasking via Rust's async/await syntax.
//!
//! ## Architecture
//!
//! The executor implements a simple but efficient poll-based async runtime:
//!
//! - **AsyncTask**: Wraps a boxed Future with a unique ID
//! - **AsyncExecutor**: Manages task queue and polling loop
//! - **WakerStore**: Provides wake-up functionality for pending tasks
//!
//! ## Example
//!
//! ```rust,ignore
//! use mielin_kernel::executor::{AsyncExecutor, spawn_async};
//!
//! async fn my_task() -> u32 {
//!     // Async work here
//!     42
//! }
//!
//! let mut executor = AsyncExecutor::new();
//! executor.spawn(my_task());
//! executor.run();
//! ```

use alloc::boxed::Box;
use alloc::collections::VecDeque;
use alloc::sync::Arc;
use alloc::task::Wake;
use core::future::Future;
use core::pin::Pin;
use core::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use core::task::{Context, Poll, Waker};

/// Maximum number of async tasks that can be queued
const MAX_ASYNC_TASKS: usize = 256;

/// Unique identifier for async tasks
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AsyncTaskId(u64);

impl AsyncTaskId {
    /// Create a new unique task ID
    pub fn new() -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(0);
        Self(NEXT_ID.fetch_add(1, Ordering::Relaxed))
    }

    /// Get the raw ID value
    pub fn id(&self) -> u64 {
        self.0
    }
}

impl Default for AsyncTaskId {
    fn default() -> Self {
        Self::new()
    }
}

/// An async task that wraps a Future
pub struct AsyncTask {
    /// Unique task identifier
    id: AsyncTaskId,
    /// The future to poll
    future: Pin<Box<dyn Future<Output = ()> + Send>>,
    /// Whether this task is ready to be polled
    ready: AtomicBool,
}

impl AsyncTask {
    /// Create a new async task from a future
    pub fn new<F>(future: F) -> Self
    where
        F: Future<Output = ()> + Send + 'static,
    {
        Self {
            id: AsyncTaskId::new(),
            future: Box::pin(future),
            ready: AtomicBool::new(true),
        }
    }

    /// Get the task ID
    pub fn id(&self) -> AsyncTaskId {
        self.id
    }

    /// Check if the task is ready to be polled
    pub fn is_ready(&self) -> bool {
        self.ready.load(Ordering::Acquire)
    }

    /// Mark the task as ready to poll
    pub fn mark_ready(&self) {
        self.ready.store(true, Ordering::Release);
    }

    /// Mark the task as pending (waiting)
    pub fn mark_pending(&self) {
        self.ready.store(false, Ordering::Release);
    }
}

/// A simple waker that marks a task as ready
struct TaskWaker {
    task_id: AsyncTaskId,
    wake_queue: Arc<WakeQueue>,
}

impl Wake for TaskWaker {
    fn wake(self: Arc<Self>) {
        self.wake_queue.push(self.task_id);
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.wake_queue.push(self.task_id);
    }
}

/// Queue of task IDs that need to be woken
struct WakeQueue {
    /// Queue of task IDs ready to wake
    queue: spin::Mutex<VecDeque<AsyncTaskId>>,
}

impl WakeQueue {
    /// Create a new wake queue
    const fn new() -> Self {
        Self {
            queue: spin::Mutex::new(VecDeque::new()),
        }
    }

    /// Push a task ID to wake
    fn push(&self, id: AsyncTaskId) {
        let mut queue = self.queue.lock();
        if !queue.contains(&id) {
            queue.push_back(id);
        }
    }

    /// Pop a task ID to process
    fn pop(&self) -> Option<AsyncTaskId> {
        self.queue.lock().pop_front()
    }

    /// Check if there are pending wakes
    fn has_pending(&self) -> bool {
        !self.queue.lock().is_empty()
    }
}

/// Executor statistics
#[derive(Debug, Clone, Default)]
pub struct ExecutorStats {
    /// Number of tasks spawned
    pub tasks_spawned: u64,
    /// Number of tasks completed
    pub tasks_completed: u64,
    /// Number of polls performed
    pub polls: u64,
    /// Number of successful polls (returned Ready)
    pub successful_polls: u64,
}

/// The main async executor
pub struct AsyncExecutor {
    /// Task queue
    tasks: VecDeque<AsyncTask>,
    /// Wake queue for pending tasks
    wake_queue: Arc<WakeQueue>,
    /// Statistics
    stats: ExecutorStats,
    /// Maximum tasks allowed
    max_tasks: usize,
}

impl AsyncExecutor {
    /// Create a new async executor
    pub fn new() -> Self {
        Self {
            tasks: VecDeque::with_capacity(64),
            wake_queue: Arc::new(WakeQueue::new()),
            stats: ExecutorStats::default(),
            max_tasks: MAX_ASYNC_TASKS,
        }
    }

    /// Create an executor with custom max tasks
    pub fn with_capacity(max_tasks: usize) -> Self {
        Self {
            tasks: VecDeque::with_capacity(max_tasks.min(64)),
            wake_queue: Arc::new(WakeQueue::new()),
            stats: ExecutorStats::default(),
            max_tasks,
        }
    }

    /// Spawn a new async task
    pub fn spawn<F>(&mut self, future: F) -> Option<AsyncTaskId>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        if self.tasks.len() >= self.max_tasks {
            return None;
        }

        let task = AsyncTask::new(future);
        let id = task.id();
        self.tasks.push_back(task);
        self.stats.tasks_spawned += 1;
        Some(id)
    }

    /// Spawn a task that returns a value, wrapping it to return ()
    pub fn spawn_with_result<F, T>(&mut self, future: F) -> Option<AsyncTaskId>
    where
        F: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        self.spawn(async move {
            let _ = future.await;
        })
    }

    /// Run the executor until all tasks complete
    pub fn run(&mut self) {
        while !self.tasks.is_empty() {
            self.poll_once();
        }
    }

    /// Run the executor for a limited number of iterations
    pub fn run_iterations(&mut self, max_iterations: usize) -> usize {
        let mut iterations = 0;
        while !self.tasks.is_empty() && iterations < max_iterations {
            self.poll_once();
            iterations += 1;
        }
        iterations
    }

    /// Run the executor until no progress is made
    /// Returns true if all tasks completed
    pub fn run_until_stalled(&mut self) -> bool {
        loop {
            if self.tasks.is_empty() {
                return true;
            }

            let made_progress = self.poll_once();
            if !made_progress && !self.wake_queue.has_pending() {
                return false;
            }
        }
    }

    /// Poll all ready tasks once
    /// Returns true if any task made progress
    pub fn poll_once(&mut self) -> bool {
        // First, wake any tasks that were signaled
        self.process_wakes();

        let mut made_progress = false;
        let mut completed_indices = alloc::vec::Vec::new();

        // Collect task IDs and readiness first to avoid borrow conflicts
        let task_info: alloc::vec::Vec<(usize, AsyncTaskId, bool)> = self
            .tasks
            .iter()
            .enumerate()
            .map(|(idx, task)| (idx, task.id(), task.is_ready()))
            .collect();

        for (idx, task_id, is_ready) in task_info {
            if !is_ready {
                continue;
            }

            // Create waker for this task
            let waker = self.create_waker(task_id);
            let mut context = Context::from_waker(&waker);

            self.stats.polls += 1;

            // Poll the task
            if let Some(task) = self.tasks.get_mut(idx) {
                match task.future.as_mut().poll(&mut context) {
                    Poll::Ready(()) => {
                        self.stats.successful_polls += 1;
                        self.stats.tasks_completed += 1;
                        completed_indices.push(idx);
                        made_progress = true;
                    }
                    Poll::Pending => {
                        task.mark_pending();
                    }
                }
            }
        }

        // Remove completed tasks (in reverse order to preserve indices)
        for idx in completed_indices.into_iter().rev() {
            self.tasks.remove(idx);
        }

        made_progress
    }

    /// Process wake notifications
    fn process_wakes(&mut self) {
        while let Some(task_id) = self.wake_queue.pop() {
            for task in self.tasks.iter() {
                if task.id() == task_id {
                    task.mark_ready();
                    break;
                }
            }
        }
    }

    /// Create a waker for a task
    fn create_waker(&self, task_id: AsyncTaskId) -> Waker {
        let waker = Arc::new(TaskWaker {
            task_id,
            wake_queue: Arc::clone(&self.wake_queue),
        });
        Waker::from(waker)
    }

    /// Get the number of pending tasks
    pub fn pending_tasks(&self) -> usize {
        self.tasks.len()
    }

    /// Check if the executor has any tasks
    pub fn has_tasks(&self) -> bool {
        !self.tasks.is_empty()
    }

    /// Get executor statistics
    pub fn stats(&self) -> &ExecutorStats {
        &self.stats
    }

    /// Reset statistics
    pub fn reset_stats(&mut self) {
        self.stats = ExecutorStats::default();
    }
}

impl Default for AsyncExecutor {
    fn default() -> Self {
        Self::new()
    }
}

/// A simple async yield point
pub struct Yield {
    yielded: bool,
}

impl Yield {
    /// Create a new yield point
    pub fn new() -> Self {
        Self { yielded: false }
    }
}

impl Default for Yield {
    fn default() -> Self {
        Self::new()
    }
}

impl Future for Yield {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.yielded {
            Poll::Ready(())
        } else {
            self.yielded = true;
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

/// Yield execution to allow other tasks to run
pub fn yield_now() -> Yield {
    Yield::new()
}

/// A simple async timer/delay (counts poll iterations)
pub struct Delay {
    remaining: AtomicUsize,
}

impl Delay {
    /// Create a delay for the given number of polls
    pub fn new(polls: usize) -> Self {
        Self {
            remaining: AtomicUsize::new(polls),
        }
    }
}

impl Future for Delay {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let remaining = self.remaining.load(Ordering::Acquire);
        if remaining == 0 {
            Poll::Ready(())
        } else {
            self.remaining.store(remaining - 1, Ordering::Release);
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

/// Create a delay for the given number of polls
pub fn delay(polls: usize) -> Delay {
    Delay::new(polls)
}

/// A one-shot channel for async communication
pub struct Oneshot<T> {
    value: spin::Mutex<Option<T>>,
    completed: AtomicBool,
}

impl<T> Oneshot<T> {
    /// Create a new oneshot channel
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            value: spin::Mutex::new(None),
            completed: AtomicBool::new(false),
        })
    }

    /// Send a value (completes the channel)
    pub fn send(&self, value: T) -> bool {
        if self.completed.load(Ordering::Acquire) {
            return false;
        }

        let mut guard = self.value.lock();
        if guard.is_some() {
            return false;
        }

        *guard = Some(value);
        self.completed.store(true, Ordering::Release);
        true
    }

    /// Try to receive the value (non-blocking)
    pub fn try_recv(&self) -> Option<T> {
        if !self.completed.load(Ordering::Acquire) {
            return None;
        }
        self.value.lock().take()
    }

    /// Check if the channel is complete
    pub fn is_complete(&self) -> bool {
        self.completed.load(Ordering::Acquire)
    }
}

/// Receiver half of a oneshot channel
pub struct OneshotReceiver<T> {
    channel: Arc<Oneshot<T>>,
}

impl<T> OneshotReceiver<T> {
    /// Create a receiver from a oneshot channel
    pub fn new(channel: Arc<Oneshot<T>>) -> Self {
        Self { channel }
    }
}

impl<T> Future for OneshotReceiver<T> {
    type Output = Option<T>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.channel.is_complete() {
            Poll::Ready(self.channel.try_recv())
        } else {
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

/// Create a oneshot channel pair (sender, receiver)
pub fn oneshot<T>() -> (Arc<Oneshot<T>>, OneshotReceiver<T>) {
    let channel = Oneshot::new();
    let receiver = OneshotReceiver::new(Arc::clone(&channel));
    (channel, receiver)
}

/// A joinable async task handle
pub struct JoinHandle<T> {
    receiver: OneshotReceiver<T>,
}

impl<T> JoinHandle<T> {
    /// Create a join handle from a receiver
    pub fn new(receiver: OneshotReceiver<T>) -> Self {
        Self { receiver }
    }
}

impl<T> Future for JoinHandle<T> {
    type Output = Option<T>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.receiver).poll(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_async_task_id_unique() {
        let id1 = AsyncTaskId::new();
        let id2 = AsyncTaskId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_executor_creation() {
        let executor = AsyncExecutor::new();
        assert_eq!(executor.pending_tasks(), 0);
        assert!(!executor.has_tasks());
    }

    #[test]
    fn test_spawn_simple_task() {
        let mut executor = AsyncExecutor::new();

        let id = executor.spawn(async {});
        assert!(id.is_some());
        assert_eq!(executor.pending_tasks(), 1);
    }

    #[test]
    fn test_run_simple_task() {
        let mut executor = AsyncExecutor::new();

        executor.spawn(async {});
        executor.run();

        assert_eq!(executor.pending_tasks(), 0);
        assert_eq!(executor.stats().tasks_completed, 1);
    }

    #[test]
    fn test_run_multiple_tasks() {
        let mut executor = AsyncExecutor::new();

        executor.spawn(async {});
        executor.spawn(async {});
        executor.spawn(async {});

        executor.run();

        assert_eq!(executor.pending_tasks(), 0);
        assert_eq!(executor.stats().tasks_completed, 3);
    }

    #[test]
    fn test_yield_now() {
        let mut executor = AsyncExecutor::new();

        executor.spawn(async {
            yield_now().await;
        });

        executor.run();

        assert_eq!(executor.stats().tasks_completed, 1);
        assert!(executor.stats().polls >= 2); // At least 2 polls due to yield
    }

    #[test]
    fn test_delay() {
        let mut executor = AsyncExecutor::new();

        executor.spawn(async {
            delay(3).await;
        });

        executor.run();

        assert_eq!(executor.stats().tasks_completed, 1);
        assert!(executor.stats().polls >= 4); // Initial + 3 delays
    }

    #[test]
    fn test_executor_stats() {
        let mut executor = AsyncExecutor::new();

        executor.spawn(async {});
        executor.spawn(async {});

        assert_eq!(executor.stats().tasks_spawned, 2);
        assert_eq!(executor.stats().tasks_completed, 0);

        executor.run();

        assert_eq!(executor.stats().tasks_completed, 2);
    }

    #[test]
    fn test_executor_with_capacity() {
        let mut executor = AsyncExecutor::with_capacity(2);

        assert!(executor.spawn(async {}).is_some());
        assert!(executor.spawn(async {}).is_some());
        assert!(executor.spawn(async {}).is_none()); // Should fail - at capacity
    }

    #[test]
    fn test_run_iterations() {
        let mut executor = AsyncExecutor::new();

        executor.spawn(async {
            delay(10).await;
        });

        let iterations = executor.run_iterations(5);
        assert!(iterations <= 5);
        assert!(executor.has_tasks()); // Task should still be pending
    }

    #[test]
    fn test_poll_once() {
        let mut executor = AsyncExecutor::new();

        executor.spawn(async {});

        let made_progress = executor.poll_once();
        assert!(made_progress);
        assert_eq!(executor.pending_tasks(), 0);
    }

    #[test]
    fn test_oneshot_channel() {
        let channel = Oneshot::new();

        assert!(!channel.is_complete());
        assert!(channel.try_recv().is_none());

        assert!(channel.send(42));
        assert!(channel.is_complete());
        assert_eq!(channel.try_recv(), Some(42));
        assert!(channel.try_recv().is_none()); // Already consumed
    }

    #[test]
    fn test_oneshot_double_send() {
        let channel = Oneshot::new();

        assert!(channel.send(1));
        assert!(!channel.send(2)); // Should fail
    }

    #[test]
    fn test_oneshot_receiver() {
        let mut executor = AsyncExecutor::new();
        let (sender, receiver) = oneshot::<i32>();

        executor.spawn(async move {
            let value = receiver.await;
            assert_eq!(value, Some(42));
        });

        // Send value after spawning
        sender.send(42);

        executor.run();
        assert_eq!(executor.stats().tasks_completed, 1);
    }

    #[test]
    fn test_concurrent_tasks_with_yield() {
        let mut executor = AsyncExecutor::new();

        executor.spawn(async {
            yield_now().await;
        });

        executor.spawn(async {
            yield_now().await;
        });

        executor.run();

        assert_eq!(executor.stats().tasks_completed, 2);
    }

    #[test]
    fn test_reset_stats() {
        let mut executor = AsyncExecutor::new();

        executor.spawn(async {});
        executor.run();

        assert!(executor.stats().tasks_completed > 0);

        executor.reset_stats();

        assert_eq!(executor.stats().tasks_completed, 0);
        assert_eq!(executor.stats().polls, 0);
    }

    #[test]
    fn test_spawn_with_result() {
        let mut executor = AsyncExecutor::new();

        let id = executor.spawn_with_result(async { 42 });
        assert!(id.is_some());

        executor.run();
        assert_eq!(executor.stats().tasks_completed, 1);
    }
}
