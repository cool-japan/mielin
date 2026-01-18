//! Lock-Free Data Structures Example
//!
//! Demonstrates the kernel's lock-free concurrent data structures:
//! - LockFreeQueue (MPMC)
//! - LockFreeStack (MPMC)
//! - SeqLock (single-writer, multi-reader)
//! - RingBuffer (SPSC)

use mielin_kernel::lockfree::{LockFreeQueue, LockFreeStack, RingBuffer, SeqLock};

fn main() {
    println!("=== MielinOS Lock-Free Data Structures Example ===\n");

    // --- Lock-Free Queue (MPMC) ---
    println!("1. Lock-Free Queue (Multi-Producer Multi-Consumer):");
    demonstrate_queue();
    println!();

    // --- Lock-Free Stack (MPMC) ---
    println!("2. Lock-Free Stack (Multi-Producer Multi-Consumer):");
    demonstrate_stack();
    println!();

    // --- SeqLock (Single-Writer Multi-Reader) ---
    println!("3. SeqLock (Single-Writer Multi-Reader):");
    demonstrate_seqlock();
    println!();

    // --- Ring Buffer (SPSC) ---
    println!("4. Ring Buffer (Single-Producer Single-Consumer):");
    demonstrate_ringbuffer();
    println!();

    println!("=== Example Complete ===");
}

fn demonstrate_queue() {
    let queue = LockFreeQueue::<i32>::new();

    // Enqueue items
    for i in 1..=5 {
        queue.push(i);
        println!("   Enqueued: {}", i);
    }

    println!("   Queue length: {}", queue.len());

    // Dequeue items (FIFO order)
    print!("   Dequeued: ");
    while let Some(item) = queue.pop() {
        print!("{} ", item);
    }
    println!();
    println!("   Queue is empty: {}", queue.is_empty());
}

fn demonstrate_stack() {
    let stack = LockFreeStack::<i32>::new();

    // Push items
    for i in 1..=5 {
        stack.push(i);
        println!("   Pushed: {}", i);
    }

    println!("   Stack length: {}", stack.len());

    // Pop items (LIFO order)
    print!("   Popped: ");
    while let Some(item) = stack.pop() {
        print!("{} ", item);
    }
    println!();
    println!("   Stack is empty: {}", stack.is_empty());
}

fn demonstrate_seqlock() {
    // SeqLock for a simple counter
    #[derive(Clone, Copy, Default)]
    struct Counter {
        value: u64,
    }

    let seqlock = SeqLock::new(Counter { value: 0 });

    // Writer updates
    for i in 1..=3 {
        seqlock.write(Counter { value: i * 100 });
        println!("   Writer set value to: {}", i * 100);
    }

    // Reader reads (may retry if write in progress)
    let value = seqlock.read();
    println!("   Reader got value: {}", value.value);

    // Try read (returns None if write in progress)
    if let Some(value) = seqlock.try_read() {
        println!("   Try read got value: {}", value.value);
    }
}

fn demonstrate_ringbuffer() {
    let buffer: RingBuffer<i32, 8> = RingBuffer::new();

    // Producer pushes items
    for i in 1..=5 {
        match buffer.try_push(i) {
            Ok(()) => println!("   Pushed: {}", i),
            Err(v) => println!("   Buffer full, couldn't push: {}", v),
        }
    }

    println!("   Buffer length: {}", buffer.len());
    println!("   Buffer capacity: {}", buffer.capacity());

    // Consumer pops items
    print!("   Popped: ");
    while let Some(item) = buffer.try_pop() {
        print!("{} ", item);
    }
    println!();
    println!("   Buffer is empty: {}", buffer.is_empty());
}
