# MielinOS Kernel Memory Model and Synchronization Guarantees

This document describes the memory model, synchronization primitives, and safety guarantees of the MielinOS kernel.

## Table of Contents

1. [Memory Layout](#memory-layout)
2. [Memory Allocators](#memory-allocators)
3. [Synchronization Primitives](#synchronization-primitives)
4. [Ordering Guarantees](#ordering-guarantees)
5. [Lock-Free Data Structures](#lock-free-data-structures)
6. [Per-CPU Architecture](#per-cpu-architecture)
7. [Inter-CPU Communication](#inter-cpu-communication)
8. [Safety Invariants](#safety-invariants)

---

## Memory Layout

### Address Space Organization

```
┌─────────────────────────────────┐ 0xFFFF_FFFF_FFFF_FFFF
│      Reserved (Future)          │
├─────────────────────────────────┤
│      Kernel Code & Data         │
├─────────────────────────────────┤
│      Page-Based Heap            │ (Managed by MemoryManager)
├─────────────────────────────────┤
│      Pool Allocator Regions     │ (32B-4KB blocks)
├─────────────────────────────────┤
│      Bump Allocator Heap        │ (Large allocations)
├─────────────────────────────────┤
│      Per-CPU Data Structures    │ (64-byte aligned)
└─────────────────────────────────┘ Base Address
```

### Page Size

- **Standard page size**: 4KB (4096 bytes)
- **Alignment**: All pages are 4KB-aligned
- **Maximum pages**: Configurable at initialization (default: 1024 pages = 4MB)

---

## Memory Allocators

### 1. Page-Based Allocator (`MemoryManager`)

**Purpose**: Manage physical memory pages for kernel data structures and process memory.

**Algorithm**: Free-list based O(1) allocation with configurable strategies:
- **FirstFit**: Allocate first block that fits
- **BestFit**: Minimize fragmentation by finding smallest adequate block
- **WorstFit**: Leave largest remaining block (useful for large allocations)

**Key Features**:
- O(1) allocation and deallocation
- Automatic block coalescing on free
- Fragmentation tracking
- Comprehensive statistics

**Safety Guarantees**:
- All allocations are page-aligned
- Double-free detection
- Out-of-bounds checking
- Address validation

**API**:
```rust
let mut mm = MemoryManager::new(base_addr, total_pages);
let addr = mm.allocate_pages(num_pages)?;  // Returns Err on OOM
mm.free_pages(addr, num_pages)?;            // Returns Err on invalid free
```

### 2. Pool Allocator (`Pool`)

**Purpose**: Fast fixed-size allocations for common object sizes.

**Size Classes**: 32B, 64B, 128B, 256B, 512B, 1KB, 4KB

**Algorithm**: Lock-free free-list using atomic operations

**Safety Guarantees**:
- Size class validation
- Null pointer checks
- Atomic CAS operations prevent ABA problems
- Per-CPU pools eliminate lock contention

**Performance**:
- Allocation: O(1) atomic CAS
- Deallocation: O(1) atomic CAS
- No locks required

**API**:
```rust
let pool = Pool::new(block_size);
let ptr = pool.allocate()?;
pool.deallocate(ptr, block_size);
```

### 3. Heap Allocator (`GlobalAllocator`)

**Purpose**: Implement Rust's `GlobalAlloc` trait for standard library compatibility.

**Strategy**:
- Small allocations (≤4KB): Pool allocator
- Large allocations (>4KB): Bump allocator

**Safety Guarantees**:
- Thread-safe (Mutex-protected)
- Alignment guarantees
- OOM handling with retry mechanism

**API**:
```rust
#[global_allocator]
static ALLOCATOR: GlobalAllocator = GlobalAllocator::new();

// Standard Rust allocation
let vec = Vec::new();  // Uses GlobalAllocator
```

---

## Synchronization Primitives

### 1. Spin Locks (`spin::Mutex`)

**Usage**: Protect shared mutable state in no_std environment.

**Characteristics**:
- Busy-wait spinning (no blocking)
- Not suitable for long-held locks
- Used for: Schedulers, memory managers, task queues

**Ordering**: Acquire on lock, Release on unlock

```rust
let mutex = Mutex::new(data);
let guard = mutex.lock();  // Spin until acquired
// ... critical section ...
drop(guard);  // Automatically releases
```

**Safety Guidelines**:
- Keep critical sections short
- Never hold multiple locks simultaneously (to avoid deadlocks)
- Use atomic operations when possible

### 2. Atomic Operations

**Available Types**:
- `AtomicU8`, `AtomicU16`, `AtomicU32`, `AtomicU64`, `AtomicUsize`
- `AtomicBool`

**Operations**:
- `load()`, `store()` - Simple read/write
- `fetch_add()`, `fetch_sub()` - Atomic arithmetic
- `fetch_or()`, `fetch_and()` - Atomic bitwise
- `compare_exchange()`, `compare_exchange_weak()` - CAS operations
- `swap()` - Atomic exchange

**Example**:
```rust
let counter = AtomicU64::new(0);
counter.fetch_add(1, Ordering::SeqCst);
```

---

## Ordering Guarantees

MielinOS uses Rust's memory ordering model based on C++11 atomics.

### Ordering Types

| Ordering | Guarantees | Use Case |
|----------|------------|----------|
| `Relaxed` | No synchronization | Statistics, counters |
| `Acquire` | Prevents reordering before load | Lock acquisition |
| `Release` | Prevents reordering after store | Lock release |
| `AcqRel` | Both Acquire + Release | Read-modify-write |
| `SeqCst` | Total global order | Critical correctness |

### Usage Patterns

#### 1. Lock Acquisition (Acquire)
```rust
// Load pending IPI bitmap
let pending = self.pending.load(Ordering::Acquire);
```
**Guarantee**: All subsequent loads see effects of prior stores.

#### 2. Lock Release (Release)
```rust
// Mark task as ready
self.status.store(TaskStatus::Ready, Ordering::Release);
```
**Guarantee**: All prior stores visible to threads acquiring this location.

#### 3. Read-Modify-Write (AcqRel)
```rust
// Atomic counter increment
self.counter.fetch_add(1, Ordering::AcqRel);
```
**Guarantee**: Synchronizes with both prior and subsequent operations.

#### 4. Statistics (Relaxed)
```rust
// Increment non-critical counter
self.stats.sent.fetch_add(1, Ordering::Relaxed);
```
**Guarantee**: Atomic but no ordering - sufficient for independent counters.

---

## Lock-Free Data Structures

### 1. Treiber Stack (`Stack`)

**Algorithm**: Lock-free LIFO stack using CAS loops.

**ABA Prevention**: Not required (pointers to static storage).

**Operations**:
- `push()`: O(1) with retry on contention
- `pop()`: O(1) with retry on contention

```rust
let stack = Stack::new();
stack.push(item);
if let Some(item) = stack.pop() { ... }
```

**Safety**: Only use with types that don't require Drop (or handle it externally).

### 2. MPSC Queue (`MPSCQueue`)

**Algorithm**: Lock-free multi-producer single-consumer queue.

**Operations**:
- `enqueue()`: O(1) lock-free
- `dequeue()`: O(1) lock-free

```rust
let queue = MPSCQueue::new();
queue.enqueue(item);  // Multiple producers
if let Some(item) = queue.dequeue() { ... }  // Single consumer
```

**Safety**: Designed for task scheduling - producers add tasks, scheduler dequeues.

### 3. SPSC Ring Buffer (`RingBuffer`)

**Algorithm**: Single-producer single-consumer circular buffer.

**Synchronization**: Atomic indices with Acquire/Release ordering.

**Operations**:
- `write()`: O(1) by producer
- `read()`: O(1) by consumer

```rust
let mut ring = RingBuffer::new();
ring.write(&data);  // Producer
if let Some(data) = ring.read() { ... }  // Consumer
```

**Safety**: Strictly single-producer, single-consumer - not thread-safe otherwise.

### 4. Sequence Lock (`SeqLock`)

**Purpose**: Read-mostly data with infrequent updates.

**Algorithm**: Generation counter with retry on conflict.

**Operations**:
- `read()`: Lock-free, retry on concurrent write
- `write()`: Locked (Mutex)

```rust
let seqlock = SeqLock::new(data);
let value = seqlock.read();  // Always succeeds, may retry
seqlock.write(new_value);     // Exclusive access
```

**Use Case**: Read-heavy workloads like configuration, statistics.

---

## Per-CPU Architecture

### Design Principles

1. **Cache Line Isolation**: Each `PerCpu` struct is 64-byte aligned to prevent false sharing
2. **No Cross-CPU Locks**: CPUs operate on their own data independently
3. **Work Stealing**: Load balancing uses explicit task migration, not shared queues

### Memory Layout

```rust
#[repr(align(64))]
pub struct PerCpu {
    cpu_id: usize,                      // CPU identifier
    scheduler: Mutex<Scheduler>,        // 64 task slots
    pools: [Pool; 7],                   // 7 size classes
    stats: CpuStats,                    // Statistics
    online: AtomicU32,                  // Online/offline state
}
```

### Synchronization

**Within CPU**: Spin locks (low contention, short-held)
**Between CPUs**: IPI + message passing (lock-free)

### Load Balancing

**Work Stealing Algorithm**:
1. Find busiest and idlest CPUs (O(N))
2. Check imbalance threshold (>2 tasks)
3. Extract migratable tasks from busiest CPU
4. Inject tasks into idlest CPU
5. Send IPI to wake up idle CPU

**Safety**:
- Only Ready/Blocked tasks can migrate
- Running tasks protected from migration
- Pinned tasks (CPU affinity) cannot migrate

---

## Inter-CPU Communication

### 1. Inter-Processor Interrupts (IPI)

**Mechanism**: Software-generated interrupts to signal other CPUs.

**IPI Types**:
- `Wakeup` - Wake sleeping CPU
- `Reschedule` - Request task reschedule
- `TlbFlush` - Invalidate TLB
- `CacheInvalidate` - Flush cache
- `FunctionCall` - Execute function on target CPU

**Pending Bitmap**: 32-bit bitmap tracks pending IPIs per CPU (types 0-31).

**Ordering**:
```rust
// Sender
self.pending.fetch_or(bit, Ordering::Release);  // Mark pending

// Receiver
let pending = self.pending.load(Ordering::Acquire);  // Check pending
```

**Platform Support**:
- x86_64: LAPIC ICR writes
- ARM: GICv3 SGI
- RISC-V: SBI IPI calls

### 2. Message Passing

**Structure**: 16x16 matrix of SPSC queues (one per CPU pair).

**Message Format**:
```rust
#[repr(C, align(64))]
pub struct Message {
    msg_type: MessageType,     // Type of message
    from_cpu: u8,              // Source CPU
    to_cpu: u8,                // Destination CPU
    seq: u32,                  // Sequence number
    data: [u64; 7],            // 56 bytes of payload
}
```

**Message Types**:
- `TaskMigration` - Migrate task between CPUs
- `MemoryAlloc` - Request memory allocation
- `MemoryFree` - Request memory deallocation
- `FunctionCall` - Remote procedure call
- `Data` - Generic data transfer
- `Ack` - Acknowledgment
- `Error` - Error response

**Queue Algorithm**: Lock-free SPSC ring buffer (256 messages per queue).

**Synchronization**:
```rust
// Enqueue (sender)
self.write_idx.store(next_idx, Ordering::Release);
self.count.fetch_add(1, Ordering::Release);

// Dequeue (receiver)
let count = self.count.load(Ordering::Acquire);
self.read_idx.store(next_idx, Ordering::Release);
self.count.fetch_sub(1, Ordering::Release);
```

**Notification**: Automatic IPI sent on message enqueue.

### 3. CPU Barriers

**Purpose**: Synchronize all CPUs at a barrier point.

**Algorithm**: Generation-based wait.

```rust
pub struct CpuBarrier {
    count: AtomicUsize,       // Total CPUs
    waiting: AtomicUsize,     // Currently waiting
    generation: AtomicU64,    // Generation counter
}
```

**Operation**:
1. Each CPU increments `waiting`
2. Last CPU to arrive resets `waiting` and increments `generation`
3. Other CPUs spin until `generation` changes

**Use Cases**:
- Kernel initialization
- Global TLB flush
- Coordinated state transitions

---

## Safety Invariants

### Memory Safety

1. **No Double Free**: Tracked via bitmap in `MemoryManager`
   ```rust
   if !self.bitmap.is_allocated(page_idx) {
       return Err(KernelError::DoubleFree { address });
   }
   ```

2. **Address Bounds Checking**: All addresses validated against region
   ```rust
   if address < self.base_address || address >= max_address {
       return Err(KernelError::AddressOutOfBounds { address, max_address });
   }
   ```

3. **Alignment**: All page allocations are 4KB-aligned
   ```rust
   assert_eq!(addr % PAGE_SIZE, 0);
   ```

4. **OOM Handling**: Allocation failures return `Result<T, KernelError>`
   ```rust
   mm.allocate_pages(n)?  // Propagates OutOfMemory error
   ```

### Concurrency Safety

1. **Data Race Freedom**: All shared mutable state protected by locks or atomics
   - `Mutex<T>` for complex state
   - `Atomic*` for simple state

2. **Lock Ordering**: Global lock ordering prevents deadlocks
   - Memory Manager → Pool → Scheduler (never reversed)
   - Per-CPU locks never held across CPUs

3. **ABA Problem**: Prevented by design
   - Pool allocator: Blocks from static array
   - Task IDs: Monotonically increasing
   - Generation counters: 64-bit overflow impossible

4. **Memory Ordering**: Explicit ordering prevents races
   - Acquire before accessing shared state
   - Release after modifying shared state
   - AcqRel for read-modify-write

### Task Scheduler Safety

1. **Task Slot Bounds**: Maximum 64 tasks per CPU
   ```rust
   if self.num_tasks >= MAX_TASKS {
       return Err(KernelError::TaskSpawnFailed);
   }
   ```

2. **State Transitions**: Only valid state transitions allowed
   ```
   Created → Ready → Running → {Ready, Blocked, Terminated}
   Blocked → Ready
   ```

3. **CPU Affinity**: Enforced during migration
   ```rust
   if task.affinity == Some(cpu_id) && cpu_id != target_cpu {
       return false;  // Cannot migrate pinned task
   }
   ```

### Per-CPU Safety

1. **CPU ID Bounds**: Valid CPU IDs (0..MAX_CPUS)
   ```rust
   if cpu_id >= MAX_CPUS {
       return Err(KernelError::HardwareNotSupported);
   }
   ```

2. **Cache Line Alignment**: Prevent false sharing
   ```rust
   #[repr(align(64))]
   pub struct PerCpu { ... }
   ```

3. **Online Status**: Only operate on online CPUs
   ```rust
   if !percpu.is_online() {
       return Err(KernelError::HardwareNotSupported);
   }
   ```

---

## Performance Characteristics

### Memory Allocators

| Operation | Time Complexity | Notes |
|-----------|----------------|-------|
| Page allocation (free-list) | O(1) | Best case |
| Page allocation (no fit) | O(N) | Worst case, rare |
| Page deallocation | O(1) | With coalescing |
| Pool allocation | O(1) | Lock-free CAS |
| Pool deallocation | O(1) | Lock-free CAS |

### Task Scheduler

| Operation | Time Complexity | Notes |
|-----------|----------------|-------|
| Task spawn | O(1) | Find free slot |
| Task schedule | O(N) | Priority scan, N ≤ 64 |
| Task yield | O(1) | State change only |
| Task terminate | O(1) | Mark as terminated |

### IPC

| Operation | Time Complexity | Notes |
|-----------|----------------|-------|
| Send IPI | O(1) | Atomic bit set |
| Handle IPI | O(1) | Atomic bit clear |
| Send message | O(1) | Ring buffer enqueue |
| Receive message | O(N) | Check all sender queues, N ≤ 16 |
| CPU barrier | O(N) | Spin wait, N = CPU count |

---

## Future Enhancements

### Planned Improvements

1. **NUMA Support** (v1.0.0)
   - NUMA-aware memory allocation
   - CPU-to-memory affinity
   - Cross-node bandwidth optimization

2. **Virtual Memory** (v0.2.0)
   - Page table management
   - Memory protection
   - Copy-on-write pages
   - Demand paging

3. **RCU (Read-Copy-Update)** (v1.0.0)
   - Read-side critical sections
   - Grace period detection
   - Deferred reclamation

4. **Hardware Transactional Memory** (v1.0.0)
   - TSX/RTM support (x86)
   - TME support (ARM)
   - Fallback to locks

---

## References

1. Rust Atomics and Locks - Mara Bos (O'Reilly, 2023)
2. The Art of Multiprocessor Programming - Herlihy & Shavit (2020)
3. Linux Kernel Memory Model - Paul E. McKenney
4. C++11 Memory Model - Hans Boehm
5. Lock-Free Programming - Preshing on Programming

---

**Last Updated**: 2026-01-18
**Version**: 0.1.0
**Authors**: MielinOS Contributors
