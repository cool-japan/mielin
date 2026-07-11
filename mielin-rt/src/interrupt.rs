//! Interrupt Handler Management for Embedded Runtime
//!
//! Provides portable interrupt handler registration, priority configuration,
//! and interrupt-to-task communication.
//!
//! ## Features
//!
//! - Handler registration with type-safe callbacks
//! - Priority configuration helpers
//! - Interrupt masking utilities
//! - Event queue for interrupt-to-task communication
//! - Nested interrupt support

#![allow(dead_code)]

use core::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};

/// Maximum number of interrupt handlers
const MAX_HANDLERS: usize = 64;

/// Maximum events in the queue
const MAX_EVENTS: usize = 32;

/// Interrupt priority levels (lower = higher priority)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
#[repr(u8)]
pub enum Priority {
    /// Highest priority (cannot be preempted)
    Critical = 0,
    /// High priority
    High = 1,
    /// Medium priority (default)
    #[default]
    Medium = 2,
    /// Low priority
    Low = 3,
    /// Background priority
    Background = 4,
}

impl Priority {
    /// Convert to Cortex-M priority value (0-255)
    pub fn to_cortex_m(self) -> u8 {
        match self {
            Priority::Critical => 0,
            Priority::High => 64,
            Priority::Medium => 128,
            Priority::Low => 192,
            Priority::Background => 255,
        }
    }

    /// Convert from raw priority value
    pub fn from_raw(value: u8) -> Self {
        match value {
            0..=31 => Priority::Critical,
            32..=95 => Priority::High,
            96..=159 => Priority::Medium,
            160..=223 => Priority::Low,
            224..=255 => Priority::Background,
        }
    }
}

/// Interrupt event for communication with tasks
#[derive(Debug, Clone, Copy)]
pub struct InterruptEvent {
    /// Interrupt number that triggered this event
    pub irq: u32,
    /// Timestamp (tick count when event occurred)
    pub timestamp: u64,
    /// Optional data payload
    pub data: u32,
    /// Event priority
    pub priority: Priority,
}

impl InterruptEvent {
    /// Create a new interrupt event
    pub fn new(irq: u32, timestamp: u64) -> Self {
        Self {
            irq,
            timestamp,
            data: 0,
            priority: Priority::Medium,
        }
    }

    /// Create event with data
    pub fn with_data(irq: u32, timestamp: u64, data: u32) -> Self {
        Self {
            irq,
            timestamp,
            data,
            priority: Priority::Medium,
        }
    }

    /// Set event priority
    pub fn with_priority(mut self, priority: Priority) -> Self {
        self.priority = priority;
        self
    }
}

/// Interrupt handler function type
pub type HandlerFn = fn(irq: u32) -> bool;

/// Interrupt handler registration
#[derive(Clone)]
pub struct HandlerEntry {
    /// Interrupt number
    irq: u32,
    /// Handler function
    handler: HandlerFn,
    /// Priority
    priority: Priority,
    /// Enabled flag
    enabled: bool,
    /// Call count
    call_count: u64,
}

impl HandlerEntry {
    /// Create a new handler entry
    pub fn new(irq: u32, handler: HandlerFn) -> Self {
        Self {
            irq,
            handler,
            priority: Priority::default(),
            enabled: true,
            call_count: 0,
        }
    }

    /// Set priority
    pub fn with_priority(mut self, priority: Priority) -> Self {
        self.priority = priority;
        self
    }

    /// Call the handler
    pub fn invoke(&mut self) -> bool {
        if self.enabled {
            self.call_count += 1;
            (self.handler)(self.irq)
        } else {
            false
        }
    }
}

/// Lock-free ring buffer for events
pub struct EventQueue {
    /// Event storage
    events: [Option<InterruptEvent>; MAX_EVENTS],
    /// Head index (next read)
    head: AtomicUsize,
    /// Tail index (next write)
    tail: AtomicUsize,
    /// Overflow count
    overflows: AtomicUsize,
    /// Total events posted
    total_events: AtomicUsize,
}

impl EventQueue {
    /// Create a new event queue
    pub const fn new() -> Self {
        Self {
            events: [None; MAX_EVENTS],
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
            overflows: AtomicUsize::new(0),
            total_events: AtomicUsize::new(0),
        }
    }

    /// Post an event to the queue
    pub fn post(&mut self, event: InterruptEvent) -> bool {
        let tail = self.tail.load(Ordering::Acquire);
        let next_tail = (tail + 1) % MAX_EVENTS;

        // Check for full queue
        if next_tail == self.head.load(Ordering::Acquire) {
            self.overflows.fetch_add(1, Ordering::Relaxed);
            return false;
        }

        self.events[tail] = Some(event);
        self.tail.store(next_tail, Ordering::Release);
        self.total_events.fetch_add(1, Ordering::Relaxed);
        true
    }

    /// Take an event from the queue
    pub fn take(&mut self) -> Option<InterruptEvent> {
        let head = self.head.load(Ordering::Acquire);
        let tail = self.tail.load(Ordering::Acquire);

        if head == tail {
            return None; // Empty
        }

        let event = self.events[head].take();
        let next_head = (head + 1) % MAX_EVENTS;
        self.head.store(next_head, Ordering::Release);
        event
    }

    /// Peek at the next event without removing
    pub fn peek(&self) -> Option<&InterruptEvent> {
        let head = self.head.load(Ordering::Acquire);
        let tail = self.tail.load(Ordering::Acquire);

        if head == tail {
            return None;
        }

        self.events[head].as_ref()
    }

    /// Check if queue is empty
    pub fn is_empty(&self) -> bool {
        self.head.load(Ordering::Acquire) == self.tail.load(Ordering::Acquire)
    }

    /// Get number of pending events
    pub fn len(&self) -> usize {
        let head = self.head.load(Ordering::Acquire);
        let tail = self.tail.load(Ordering::Acquire);
        if tail >= head {
            tail - head
        } else {
            MAX_EVENTS - head + tail
        }
    }

    /// Get overflow count
    pub fn overflows(&self) -> usize {
        self.overflows.load(Ordering::Relaxed)
    }

    /// Get total events posted
    pub fn total_events(&self) -> usize {
        self.total_events.load(Ordering::Relaxed)
    }

    /// Clear the queue
    pub fn clear(&mut self) {
        self.head.store(0, Ordering::Release);
        self.tail.store(0, Ordering::Release);
        self.events.fill(None);
    }
}

impl Default for EventQueue {
    fn default() -> Self {
        Self::new()
    }
}

/// Interrupt controller abstraction
pub struct InterruptController {
    /// Registered handlers (indexed by IRQ)
    handlers: [Option<HandlerEntry>; MAX_HANDLERS],
    /// Handler count
    handler_count: usize,
    /// Global enable flag
    global_enabled: AtomicBool,
    /// Event queue
    event_queue: EventQueue,
    /// Nesting level (for nested interrupts)
    nesting_level: AtomicU32,
    /// Statistics
    stats: InterruptStats,
}

impl InterruptController {
    /// Create a new interrupt controller
    pub const fn new() -> Self {
        Self {
            handlers: [
                None, None, None, None, None, None, None, None, None, None, None, None, None, None,
                None, None, None, None, None, None, None, None, None, None, None, None, None, None,
                None, None, None, None, None, None, None, None, None, None, None, None, None, None,
                None, None, None, None, None, None, None, None, None, None, None, None, None, None,
                None, None, None, None, None, None, None, None,
            ],
            handler_count: 0,
            global_enabled: AtomicBool::new(true),
            event_queue: EventQueue::new(),
            nesting_level: AtomicU32::new(0),
            stats: InterruptStats::new(),
        }
    }

    /// Register an interrupt handler
    pub fn register(
        &mut self,
        irq: u32,
        handler: HandlerFn,
        priority: Priority,
    ) -> Result<(), InterruptError> {
        if irq as usize >= MAX_HANDLERS {
            return Err(InterruptError::InvalidIrq);
        }

        if self.handlers[irq as usize].is_some() {
            return Err(InterruptError::AlreadyRegistered);
        }

        self.handlers[irq as usize] = Some(HandlerEntry::new(irq, handler).with_priority(priority));
        self.handler_count += 1;
        Ok(())
    }

    /// Unregister an interrupt handler
    pub fn unregister(&mut self, irq: u32) -> Result<(), InterruptError> {
        if irq as usize >= MAX_HANDLERS {
            return Err(InterruptError::InvalidIrq);
        }

        if self.handlers[irq as usize].is_none() {
            return Err(InterruptError::NotRegistered);
        }

        self.handlers[irq as usize] = None;
        self.handler_count -= 1;
        Ok(())
    }

    /// Enable a specific interrupt
    pub fn enable(&mut self, irq: u32) -> Result<(), InterruptError> {
        if irq as usize >= MAX_HANDLERS {
            return Err(InterruptError::InvalidIrq);
        }

        if let Some(ref mut entry) = self.handlers[irq as usize] {
            entry.enabled = true;
            Ok(())
        } else {
            Err(InterruptError::NotRegistered)
        }
    }

    /// Disable a specific interrupt
    pub fn disable(&mut self, irq: u32) -> Result<(), InterruptError> {
        if irq as usize >= MAX_HANDLERS {
            return Err(InterruptError::InvalidIrq);
        }

        if let Some(ref mut entry) = self.handlers[irq as usize] {
            entry.enabled = false;
            Ok(())
        } else {
            Err(InterruptError::NotRegistered)
        }
    }

    /// Enable all interrupts globally
    pub fn enable_all(&self) {
        self.global_enabled.store(true, Ordering::Release);
    }

    /// Disable all interrupts globally
    pub fn disable_all(&self) {
        self.global_enabled.store(false, Ordering::Release);
    }

    /// Check if interrupts are globally enabled
    pub fn is_enabled(&self) -> bool {
        self.global_enabled.load(Ordering::Acquire)
    }

    /// Dispatch an interrupt (called from ISR context)
    pub fn dispatch(&mut self, irq: u32, timestamp: u64) -> bool {
        if !self.is_enabled() {
            return false;
        }

        if irq as usize >= MAX_HANDLERS {
            return false;
        }

        // Track nesting
        self.nesting_level.fetch_add(1, Ordering::SeqCst);
        self.stats.total_interrupts.fetch_add(1, Ordering::Relaxed);

        // Update max nesting
        let level = self.nesting_level.load(Ordering::Relaxed);
        let _ = self.stats.max_nesting.fetch_max(level, Ordering::Relaxed);

        let result = if let Some(ref mut entry) = self.handlers[irq as usize] {
            let handled = entry.invoke();

            // Post event to queue if handler returns true
            if handled {
                let event = InterruptEvent::new(irq, timestamp).with_priority(entry.priority);
                self.event_queue.post(event);
            }

            handled
        } else {
            self.stats.unhandled.fetch_add(1, Ordering::Relaxed);
            false
        };

        self.nesting_level.fetch_sub(1, Ordering::SeqCst);
        result
    }

    /// Get pending event from queue
    pub fn next_event(&mut self) -> Option<InterruptEvent> {
        self.event_queue.take()
    }

    /// Check if there are pending events
    pub fn has_pending_events(&self) -> bool {
        !self.event_queue.is_empty()
    }

    /// Get event queue length
    pub fn pending_event_count(&self) -> usize {
        self.event_queue.len()
    }

    /// Get current nesting level
    pub fn nesting_level(&self) -> u32 {
        self.nesting_level.load(Ordering::Relaxed)
    }

    /// Get handler count
    pub fn handler_count(&self) -> usize {
        self.handler_count
    }

    /// Get statistics
    pub fn stats(&self) -> &InterruptStats {
        &self.stats
    }

    /// Get statistics for a specific IRQ
    pub fn irq_stats(&self, irq: u32) -> Option<(u64, bool)> {
        if irq as usize >= MAX_HANDLERS {
            return None;
        }

        self.handlers[irq as usize]
            .as_ref()
            .map(|h| (h.call_count, h.enabled))
    }

    /// Set priority for an IRQ
    pub fn set_priority(&mut self, irq: u32, priority: Priority) -> Result<(), InterruptError> {
        if irq as usize >= MAX_HANDLERS {
            return Err(InterruptError::InvalidIrq);
        }

        if let Some(ref mut entry) = self.handlers[irq as usize] {
            entry.priority = priority;
            Ok(())
        } else {
            Err(InterruptError::NotRegistered)
        }
    }

    /// Clear event queue
    pub fn clear_events(&mut self) {
        self.event_queue.clear();
    }
}

impl Default for InterruptController {
    fn default() -> Self {
        Self::new()
    }
}

/// Interrupt statistics
pub struct InterruptStats {
    /// Total interrupts processed
    pub total_interrupts: AtomicUsize,
    /// Unhandled interrupts
    pub unhandled: AtomicUsize,
    /// Maximum nesting level reached
    pub max_nesting: AtomicU32,
}

impl InterruptStats {
    /// Create new statistics
    pub const fn new() -> Self {
        Self {
            total_interrupts: AtomicUsize::new(0),
            unhandled: AtomicUsize::new(0),
            max_nesting: AtomicU32::new(0),
        }
    }

    /// Get total interrupts
    pub fn total(&self) -> usize {
        self.total_interrupts.load(Ordering::Relaxed)
    }

    /// Get unhandled count
    pub fn unhandled(&self) -> usize {
        self.unhandled.load(Ordering::Relaxed)
    }

    /// Get max nesting
    pub fn max_nesting(&self) -> u32 {
        self.max_nesting.load(Ordering::Relaxed)
    }
}

impl Default for InterruptStats {
    fn default() -> Self {
        Self::new()
    }
}

/// Critical section guard for interrupt-safe operations
pub struct CriticalSection<'a> {
    controller: &'a InterruptController,
    was_enabled: bool,
}

impl<'a> CriticalSection<'a> {
    /// Enter a critical section
    pub fn new(controller: &'a InterruptController) -> Self {
        let was_enabled = controller.is_enabled();
        controller.disable_all();
        Self {
            controller,
            was_enabled,
        }
    }
}

impl Drop for CriticalSection<'_> {
    fn drop(&mut self) {
        if self.was_enabled {
            self.controller.enable_all();
        }
    }
}

/// Interrupt-related errors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterruptError {
    /// Invalid IRQ number
    InvalidIrq,
    /// Handler already registered
    AlreadyRegistered,
    /// Handler not registered
    NotRegistered,
    /// Queue full
    QueueFull,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_handler(_irq: u32) -> bool {
        true
    }

    fn silent_handler(_irq: u32) -> bool {
        false
    }

    #[test]
    fn test_priority_ordering() {
        assert!(Priority::Critical < Priority::High);
        assert!(Priority::High < Priority::Medium);
        assert!(Priority::Medium < Priority::Low);
        assert!(Priority::Low < Priority::Background);
    }

    #[test]
    fn test_priority_cortex_m() {
        assert_eq!(Priority::Critical.to_cortex_m(), 0);
        assert_eq!(Priority::High.to_cortex_m(), 64);
        assert_eq!(Priority::Medium.to_cortex_m(), 128);
        assert_eq!(Priority::Low.to_cortex_m(), 192);
        assert_eq!(Priority::Background.to_cortex_m(), 255);
    }

    #[test]
    fn test_priority_from_raw() {
        assert_eq!(Priority::from_raw(0), Priority::Critical);
        assert_eq!(Priority::from_raw(50), Priority::High);
        assert_eq!(Priority::from_raw(128), Priority::Medium);
        assert_eq!(Priority::from_raw(200), Priority::Low);
        assert_eq!(Priority::from_raw(255), Priority::Background);
    }

    #[test]
    fn test_interrupt_event() {
        let event = InterruptEvent::new(5, 1000);
        assert_eq!(event.irq, 5);
        assert_eq!(event.timestamp, 1000);
        assert_eq!(event.data, 0);
        assert_eq!(event.priority, Priority::Medium);
    }

    #[test]
    fn test_interrupt_event_with_data() {
        let event = InterruptEvent::with_data(10, 2000, 0x1234);
        assert_eq!(event.irq, 10);
        assert_eq!(event.timestamp, 2000);
        assert_eq!(event.data, 0x1234);
    }

    #[test]
    fn test_interrupt_event_priority() {
        let event = InterruptEvent::new(1, 100).with_priority(Priority::High);
        assert_eq!(event.priority, Priority::High);
    }

    #[test]
    fn test_event_queue_operations() {
        let mut queue = EventQueue::new();

        assert!(queue.is_empty());
        assert_eq!(queue.len(), 0);

        // Post events
        let event1 = InterruptEvent::new(1, 100);
        let event2 = InterruptEvent::new(2, 200);

        assert!(queue.post(event1));
        assert!(!queue.is_empty());
        assert_eq!(queue.len(), 1);

        assert!(queue.post(event2));
        assert_eq!(queue.len(), 2);

        // Peek
        let peek = queue.peek().unwrap();
        assert_eq!(peek.irq, 1);

        // Take
        let taken = queue.take().unwrap();
        assert_eq!(taken.irq, 1);
        assert_eq!(queue.len(), 1);

        let taken = queue.take().unwrap();
        assert_eq!(taken.irq, 2);
        assert!(queue.is_empty());
    }

    #[test]
    fn test_event_queue_overflow() {
        let mut queue = EventQueue::new();

        // Fill queue
        for i in 0..(MAX_EVENTS - 1) {
            assert!(queue.post(InterruptEvent::new(i as u32, i as u64)));
        }

        // Should overflow now
        assert!(!queue.post(InterruptEvent::new(99, 99)));
        assert_eq!(queue.overflows(), 1);
    }

    #[test]
    fn test_event_queue_clear() {
        let mut queue = EventQueue::new();

        queue.post(InterruptEvent::new(1, 100));
        queue.post(InterruptEvent::new(2, 200));
        assert_eq!(queue.len(), 2);

        queue.clear();
        assert!(queue.is_empty());
    }

    #[test]
    fn test_interrupt_controller_register() {
        let mut controller = InterruptController::new();

        assert_eq!(controller.handler_count(), 0);

        controller
            .register(5, dummy_handler, Priority::High)
            .unwrap();
        assert_eq!(controller.handler_count(), 1);

        // Duplicate registration should fail
        let result = controller.register(5, dummy_handler, Priority::High);
        assert_eq!(result, Err(InterruptError::AlreadyRegistered));

        // Invalid IRQ should fail
        let result = controller.register(100, dummy_handler, Priority::High);
        assert_eq!(result, Err(InterruptError::InvalidIrq));
    }

    #[test]
    fn test_interrupt_controller_unregister() {
        let mut controller = InterruptController::new();

        controller
            .register(5, dummy_handler, Priority::High)
            .unwrap();
        assert_eq!(controller.handler_count(), 1);

        controller.unregister(5).unwrap();
        assert_eq!(controller.handler_count(), 0);

        // Unregistering non-existent should fail
        let result = controller.unregister(5);
        assert_eq!(result, Err(InterruptError::NotRegistered));
    }

    #[test]
    fn test_interrupt_enable_disable() {
        let mut controller = InterruptController::new();

        controller
            .register(5, dummy_handler, Priority::High)
            .unwrap();

        // Check initial state
        let (_, enabled) = controller.irq_stats(5).unwrap();
        assert!(enabled);

        // Disable
        controller.disable(5).unwrap();
        let (_, enabled) = controller.irq_stats(5).unwrap();
        assert!(!enabled);

        // Enable
        controller.enable(5).unwrap();
        let (_, enabled) = controller.irq_stats(5).unwrap();
        assert!(enabled);
    }

    #[test]
    fn test_global_enable_disable() {
        let controller = InterruptController::new();

        assert!(controller.is_enabled());

        controller.disable_all();
        assert!(!controller.is_enabled());

        controller.enable_all();
        assert!(controller.is_enabled());
    }

    #[test]
    fn test_dispatch_interrupt() {
        let mut controller = InterruptController::new();

        controller
            .register(5, dummy_handler, Priority::High)
            .unwrap();

        // Dispatch should succeed
        assert!(controller.dispatch(5, 1000));
        assert_eq!(controller.stats().total(), 1);

        // Check call count
        let (count, _) = controller.irq_stats(5).unwrap();
        assert_eq!(count, 1);

        // Event should be queued
        assert!(controller.has_pending_events());
        let event = controller.next_event().unwrap();
        assert_eq!(event.irq, 5);
    }

    #[test]
    fn test_dispatch_disabled() {
        let mut controller = InterruptController::new();

        controller
            .register(5, dummy_handler, Priority::High)
            .unwrap();
        controller.disable_all();

        // Dispatch should not succeed when disabled
        assert!(!controller.dispatch(5, 1000));
        assert_eq!(controller.stats().total(), 0);
    }

    #[test]
    fn test_dispatch_silent_handler() {
        let mut controller = InterruptController::new();

        controller
            .register(5, silent_handler, Priority::High)
            .unwrap();

        // Dispatch succeeds but no event queued
        assert!(!controller.dispatch(5, 1000));
        assert!(!controller.has_pending_events());
    }

    #[test]
    fn test_dispatch_unregistered() {
        let mut controller = InterruptController::new();

        // Dispatching to unregistered IRQ
        assert!(!controller.dispatch(5, 1000));
        assert_eq!(controller.stats().unhandled(), 1);
    }

    #[test]
    fn test_nesting_tracking() {
        let mut controller = InterruptController::new();

        controller
            .register(5, dummy_handler, Priority::High)
            .unwrap();

        assert_eq!(controller.nesting_level(), 0);

        // After dispatch, nesting should be back to 0
        controller.dispatch(5, 1000);
        assert_eq!(controller.nesting_level(), 0);

        // Max nesting should be recorded
        assert_eq!(controller.stats().max_nesting(), 1);
    }

    #[test]
    fn test_set_priority() {
        let mut controller = InterruptController::new();

        controller
            .register(5, dummy_handler, Priority::Medium)
            .unwrap();

        controller.set_priority(5, Priority::Critical).unwrap();

        // Verify by dispatching and checking event priority
        controller.dispatch(5, 1000);
        let event = controller.next_event().unwrap();
        assert_eq!(event.priority, Priority::Critical);
    }

    #[test]
    fn test_critical_section() {
        let controller = InterruptController::new();

        assert!(controller.is_enabled());

        {
            let _cs = CriticalSection::new(&controller);
            assert!(!controller.is_enabled());
        }

        // Should be re-enabled after drop
        assert!(controller.is_enabled());
    }

    #[test]
    fn test_critical_section_when_disabled() {
        let controller = InterruptController::new();

        controller.disable_all();
        assert!(!controller.is_enabled());

        {
            let _cs = CriticalSection::new(&controller);
            assert!(!controller.is_enabled());
        }

        // Should remain disabled since it was disabled before
        assert!(!controller.is_enabled());
    }

    #[test]
    fn test_handler_entry() {
        let mut entry = HandlerEntry::new(5, dummy_handler);
        assert_eq!(entry.irq, 5);
        assert!(entry.enabled);
        assert_eq!(entry.call_count, 0);

        // Invoke
        assert!(entry.invoke());
        assert_eq!(entry.call_count, 1);

        // Disable and invoke
        entry.enabled = false;
        assert!(!entry.invoke());
        assert_eq!(entry.call_count, 1); // Should not increment
    }
}
