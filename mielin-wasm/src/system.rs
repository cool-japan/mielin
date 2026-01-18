//! System Host Functions for WASM Agents
//!
//! Provides system-level functionality to WASM agents:
//! - Time and date functions
//! - Random number generation
//! - Environment queries
//! - Process information

use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use wasmtime::{Caller, Linker};

/// System state accessible to WASM agents
#[derive(Clone)]
pub struct SystemState {
    /// Monotonic clock offset from process start
    start_time: std::time::Instant,
    /// Random number generator state (xorshift128+)
    rng_state: Arc<RwLock<XorShift128Plus>>,
    /// Environment variables accessible to the agent
    environment: Arc<RwLock<EnvironmentStore>>,
    /// Process info
    process_info: ProcessInfo,
}

impl SystemState {
    pub fn new() -> Self {
        Self {
            start_time: std::time::Instant::now(),
            rng_state: Arc::new(RwLock::new(XorShift128Plus::new())),
            environment: Arc::new(RwLock::new(EnvironmentStore::new())),
            process_info: ProcessInfo::new(),
        }
    }

    /// Create with a specific seed for deterministic random numbers
    pub fn with_seed(seed: u64) -> Self {
        Self {
            start_time: std::time::Instant::now(),
            rng_state: Arc::new(RwLock::new(XorShift128Plus::with_seed(seed))),
            environment: Arc::new(RwLock::new(EnvironmentStore::new())),
            process_info: ProcessInfo::new(),
        }
    }

    /// Get current timestamp in nanoseconds since UNIX epoch
    pub fn timestamp_nanos(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_nanos() as u64
    }

    /// Get current timestamp in milliseconds since UNIX epoch
    pub fn timestamp_millis(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_millis() as u64
    }

    /// Get current timestamp in seconds since UNIX epoch
    pub fn timestamp_secs(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_secs()
    }

    /// Get monotonic time in nanoseconds since process start
    pub fn monotonic_nanos(&self) -> u64 {
        self.start_time.elapsed().as_nanos() as u64
    }

    /// Get monotonic time in milliseconds since process start
    pub fn monotonic_millis(&self) -> u64 {
        self.start_time.elapsed().as_millis() as u64
    }

    /// Generate a random u64
    pub fn random_u64(&self) -> u64 {
        self.rng_state
            .write()
            .expect("RNG state lock poisoned")
            .next_u64()
    }

    /// Generate a random f64 in range [0.0, 1.0)
    pub fn random_f64(&self) -> f64 {
        let bits = self.random_u64();
        // Use high 53 bits for full f64 precision
        (bits >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
    }

    /// Fill buffer with random bytes
    pub fn random_bytes(&self, buffer: &mut [u8]) {
        let mut rng = self.rng_state.write().expect("RNG state lock poisoned");
        let mut remaining = buffer.len();
        let mut offset = 0;

        while remaining > 0 {
            let value = rng.next_u64();
            let bytes = value.to_le_bytes();
            let copy_len = remaining.min(8);
            buffer[offset..offset + copy_len].copy_from_slice(&bytes[..copy_len]);
            offset += copy_len;
            remaining -= copy_len;
        }
    }

    /// Get environment store
    pub fn environment(&self) -> &Arc<RwLock<EnvironmentStore>> {
        &self.environment
    }

    /// Get process info
    pub fn process_info(&self) -> &ProcessInfo {
        &self.process_info
    }
}

impl Default for SystemState {
    fn default() -> Self {
        Self::new()
    }
}

/// XorShift128+ random number generator
/// Fast, high-quality PRNG suitable for simulation/games
/// Period: 2^128 - 1
pub struct XorShift128Plus {
    state: [u64; 2],
}

impl XorShift128Plus {
    /// Create a new RNG seeded from system time
    pub fn new() -> Self {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_nanos() as u64;
        Self::with_seed(seed)
    }

    /// Create a new RNG with a specific seed
    pub fn with_seed(seed: u64) -> Self {
        // Use splitmix64 to initialize state from a single seed
        let mut state = [0u64; 2];
        let mut x = seed;

        for s in &mut state {
            x = x.wrapping_add(0x9e3779b97f4a7c15);
            let mut z = x;
            z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
            *s = z ^ (z >> 31);
        }

        // Ensure non-zero state
        if state[0] == 0 && state[1] == 0 {
            state[0] = 1;
        }

        Self { state }
    }

    /// Generate the next random u64
    pub fn next_u64(&mut self) -> u64 {
        let mut s1 = self.state[0];
        let s0 = self.state[1];
        let result = s0.wrapping_add(s1);

        self.state[0] = s0;
        s1 ^= s1 << 23;
        self.state[1] = s1 ^ s0 ^ (s1 >> 17) ^ (s0 >> 26);

        result
    }

    /// Jump forward by 2^64 steps
    /// Useful for creating independent streams
    pub fn jump(&mut self) {
        const JUMP: [u64; 2] = [0xdf900294d8f554a5, 0x170865df4b3201fc];

        let mut s0 = 0u64;
        let mut s1 = 0u64;

        for j in JUMP {
            for b in 0..64 {
                if (j >> b) & 1 != 0 {
                    s0 ^= self.state[0];
                    s1 ^= self.state[1];
                }
                self.next_u64();
            }
        }

        self.state[0] = s0;
        self.state[1] = s1;
    }
}

impl Default for XorShift128Plus {
    fn default() -> Self {
        Self::new()
    }
}

/// Environment variable store for WASM agents
pub struct EnvironmentStore {
    vars: std::collections::HashMap<String, String>,
}

impl EnvironmentStore {
    pub fn new() -> Self {
        Self {
            vars: std::collections::HashMap::new(),
        }
    }

    /// Set an environment variable
    pub fn set(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.vars.insert(key.into(), value.into());
    }

    /// Get an environment variable
    pub fn get(&self, key: &str) -> Option<&String> {
        self.vars.get(key)
    }

    /// Remove an environment variable
    pub fn remove(&mut self, key: &str) -> Option<String> {
        self.vars.remove(key)
    }

    /// Check if a key exists
    pub fn contains(&self, key: &str) -> bool {
        self.vars.contains_key(key)
    }

    /// Get all keys
    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.vars.keys()
    }

    /// Get number of variables
    pub fn len(&self) -> usize {
        self.vars.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.vars.is_empty()
    }

    /// Clear all variables
    pub fn clear(&mut self) {
        self.vars.clear();
    }
}

impl Default for EnvironmentStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Process information available to WASM agents
#[derive(Clone)]
pub struct ProcessInfo {
    /// Unique identifier for this runtime instance
    pub instance_id: u64,
    /// Platform identifier
    pub platform: Platform,
    /// Architecture identifier
    pub arch: Architecture,
    /// Pointer size in bits (32 or 64)
    pub pointer_bits: u32,
    /// Number of logical CPUs available
    pub cpu_count: u32,
    /// Total memory available in bytes (if known)
    pub memory_total: Option<u64>,
}

impl ProcessInfo {
    pub fn new() -> Self {
        // Generate instance ID from time
        let instance_id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_nanos() as u64;

        Self {
            instance_id,
            platform: Platform::current(),
            arch: Architecture::current(),
            pointer_bits: std::mem::size_of::<usize>() as u32 * 8,
            cpu_count: std::thread::available_parallelism()
                .map(|n| n.get() as u32)
                .unwrap_or(1),
            memory_total: None, // Would require platform-specific code
        }
    }
}

impl Default for ProcessInfo {
    fn default() -> Self {
        Self::new()
    }
}

/// Platform identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum Platform {
    Unknown = 0,
    Linux = 1,
    MacOS = 2,
    Windows = 3,
    FreeBSD = 4,
    Android = 5,
    IOS = 6,
    WASM = 7,
}

impl Platform {
    pub fn current() -> Self {
        #[cfg(target_os = "linux")]
        {
            Platform::Linux
        }
        #[cfg(target_os = "macos")]
        {
            Platform::MacOS
        }
        #[cfg(target_os = "windows")]
        {
            Platform::Windows
        }
        #[cfg(target_os = "freebsd")]
        {
            Platform::FreeBSD
        }
        #[cfg(target_os = "android")]
        {
            Platform::Android
        }
        #[cfg(target_os = "ios")]
        {
            Platform::IOS
        }
        #[cfg(all(
            not(target_os = "linux"),
            not(target_os = "macos"),
            not(target_os = "windows"),
            not(target_os = "freebsd"),
            not(target_os = "android"),
            not(target_os = "ios")
        ))]
        {
            Platform::Unknown
        }
    }
}

/// Architecture identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum Architecture {
    Unknown = 0,
    X86 = 1,
    X86_64 = 2,
    ARM = 3,
    AArch64 = 4,
    RISCV32 = 5,
    RISCV64 = 6,
    WASM32 = 7,
    WASM64 = 8,
}

impl Architecture {
    pub fn current() -> Self {
        #[cfg(target_arch = "x86")]
        {
            Architecture::X86
        }
        #[cfg(target_arch = "x86_64")]
        {
            Architecture::X86_64
        }
        #[cfg(target_arch = "arm")]
        {
            Architecture::ARM
        }
        #[cfg(target_arch = "aarch64")]
        {
            Architecture::AArch64
        }
        #[cfg(target_arch = "riscv32")]
        {
            Architecture::RISCV32
        }
        #[cfg(target_arch = "riscv64")]
        {
            Architecture::RISCV64
        }
        #[cfg(target_arch = "wasm32")]
        {
            Architecture::WASM32
        }
        #[cfg(target_arch = "wasm64")]
        {
            Architecture::WASM64
        }
        #[cfg(all(
            not(target_arch = "x86"),
            not(target_arch = "x86_64"),
            not(target_arch = "arm"),
            not(target_arch = "aarch64"),
            not(target_arch = "riscv32"),
            not(target_arch = "riscv64"),
            not(target_arch = "wasm32"),
            not(target_arch = "wasm64")
        ))]
        {
            Architecture::Unknown
        }
    }
}

/// System host functions for WASM registration
pub struct SystemHostFunctions;

impl SystemHostFunctions {
    /// Register all system host functions with the Wasmtime linker
    pub fn register<T>(linker: &mut Linker<T>) -> anyhow::Result<()>
    where
        T: HasSystemState + 'static,
    {
        // Time functions
        Self::register_time_functions(linker)?;
        // Random functions
        Self::register_random_functions(linker)?;
        // Environment functions
        Self::register_env_functions(linker)?;
        // Process functions
        Self::register_process_functions(linker)?;

        Ok(())
    }

    fn register_time_functions<T>(linker: &mut Linker<T>) -> anyhow::Result<()>
    where
        T: HasSystemState + 'static,
    {
        // Get current time in nanoseconds since UNIX epoch
        linker.func_wrap("mielin", "time_now_nanos", |caller: Caller<'_, T>| -> i64 {
            caller.data().system_state().timestamp_nanos() as i64
        })?;

        // Get current time in milliseconds since UNIX epoch
        linker.func_wrap(
            "mielin",
            "time_now_millis",
            |caller: Caller<'_, T>| -> i64 {
                caller.data().system_state().timestamp_millis() as i64
            },
        )?;

        // Get current time in seconds since UNIX epoch
        linker.func_wrap("mielin", "time_now_secs", |caller: Caller<'_, T>| -> i64 {
            caller.data().system_state().timestamp_secs() as i64
        })?;

        // Get monotonic time in nanoseconds (for elapsed time measurement)
        linker.func_wrap(
            "mielin",
            "time_monotonic_nanos",
            |caller: Caller<'_, T>| -> i64 {
                caller.data().system_state().monotonic_nanos() as i64
            },
        )?;

        // Get monotonic time in milliseconds
        linker.func_wrap(
            "mielin",
            "time_monotonic_millis",
            |caller: Caller<'_, T>| -> i64 {
                caller.data().system_state().monotonic_millis() as i64
            },
        )?;

        Ok(())
    }

    fn register_random_functions<T>(linker: &mut Linker<T>) -> anyhow::Result<()>
    where
        T: HasSystemState + 'static,
    {
        // Generate random u32
        linker.func_wrap("mielin", "random_u32", |caller: Caller<'_, T>| -> i32 {
            (caller.data().system_state().random_u64() & 0xFFFFFFFF) as i32
        })?;

        // Generate random u64 (returns as two i32s: low, high)
        linker.func_wrap("mielin", "random_u64_low", |caller: Caller<'_, T>| -> i32 {
            // Store the full value and return low bits
            (caller.data().system_state().random_u64() & 0xFFFFFFFF) as i32
        })?;

        linker.func_wrap(
            "mielin",
            "random_u64_high",
            |caller: Caller<'_, T>| -> i32 {
                (caller.data().system_state().random_u64() >> 32) as i32
            },
        )?;

        // Generate random f32 in [0.0, 1.0)
        linker.func_wrap("mielin", "random_f32", |caller: Caller<'_, T>| -> f32 {
            caller.data().system_state().random_f64() as f32
        })?;

        // Generate random f64 in [0.0, 1.0)
        linker.func_wrap("mielin", "random_f64", |caller: Caller<'_, T>| -> f64 {
            caller.data().system_state().random_f64()
        })?;

        // Fill memory with random bytes
        // Returns number of bytes written, or 0 on error
        linker.func_wrap(
            "mielin",
            "random_bytes",
            |mut caller: Caller<'_, T>, ptr: i32, len: i32| -> i32 {
                if len <= 0 {
                    return 0;
                }

                let offset = ptr as usize;
                let length = len as usize;

                // First generate the random bytes
                let random_data: Vec<u8> = {
                    let mut rng_state = caller
                        .data()
                        .system_state()
                        .rng_state
                        .write()
                        .expect("RNG state lock poisoned");

                    let mut buffer = vec![0u8; length];
                    let mut remaining = length;
                    let mut current = 0;

                    while remaining > 0 {
                        let value = rng_state.next_u64();
                        let bytes = value.to_le_bytes();
                        let copy_len = remaining.min(8);
                        buffer[current..current + copy_len].copy_from_slice(&bytes[..copy_len]);
                        current += copy_len;
                        remaining -= copy_len;
                    }
                    buffer
                };

                // Then copy to WASM memory
                let memory = match caller.get_export("memory") {
                    Some(wasmtime::Extern::Memory(mem)) => mem,
                    _ => return 0,
                };

                let data = memory.data_mut(&mut caller);
                if offset + length > data.len() {
                    return 0;
                }

                data[offset..offset + length].copy_from_slice(&random_data);
                len
            },
        )?;

        Ok(())
    }

    fn register_env_functions<T>(linker: &mut Linker<T>) -> anyhow::Result<()>
    where
        T: HasSystemState + 'static,
    {
        // Get environment variable length
        // Returns -1 if not found, otherwise length of value
        linker.func_wrap(
            "mielin",
            "env_get_len",
            |mut caller: Caller<'_, T>, key_ptr: i32, key_len: i32| -> i32 {
                let memory = match caller.get_export("memory") {
                    Some(wasmtime::Extern::Memory(mem)) => mem,
                    _ => return -1,
                };

                let key_offset = key_ptr as usize;
                let key_length = key_len as usize;

                let data = memory.data(&caller);
                if key_offset + key_length > data.len() {
                    return -1;
                }

                let key = match std::str::from_utf8(&data[key_offset..key_offset + key_length]) {
                    Ok(k) => k,
                    Err(_) => return -1,
                };

                let env = caller
                    .data()
                    .system_state()
                    .environment()
                    .read()
                    .expect("Environment lock poisoned");
                match env.get(key) {
                    Some(value) => value.len() as i32,
                    None => -1,
                }
            },
        )?;

        // Get environment variable value
        // Returns bytes written, or -1 on error
        linker.func_wrap(
            "mielin",
            "env_get",
            |mut caller: Caller<'_, T>,
             key_ptr: i32,
             key_len: i32,
             value_ptr: i32,
             value_max_len: i32|
             -> i32 {
                let memory = match caller.get_export("memory") {
                    Some(wasmtime::Extern::Memory(mem)) => mem,
                    _ => return -1,
                };

                let key_offset = key_ptr as usize;
                let key_length = key_len as usize;

                let data = memory.data(&caller);
                if key_offset + key_length > data.len() {
                    return -1;
                }

                let key = match std::str::from_utf8(&data[key_offset..key_offset + key_length]) {
                    Ok(k) => k.to_string(),
                    Err(_) => return -1,
                };

                let value = {
                    let env = caller
                        .data()
                        .system_state()
                        .environment()
                        .read()
                        .expect("Environment lock poisoned");
                    match env.get(&key) {
                        Some(v) => v.clone(),
                        None => return -1,
                    }
                };

                let value_offset = value_ptr as usize;
                let value_max = value_max_len as usize;

                let data = memory.data_mut(&mut caller);
                if value_offset + value_max > data.len() {
                    return -1;
                }

                let copy_len = value.len().min(value_max);
                data[value_offset..value_offset + copy_len]
                    .copy_from_slice(&value.as_bytes()[..copy_len]);

                copy_len as i32
            },
        )?;

        // Check if environment variable exists
        linker.func_wrap(
            "mielin",
            "env_contains",
            |mut caller: Caller<'_, T>, key_ptr: i32, key_len: i32| -> i32 {
                let memory = match caller.get_export("memory") {
                    Some(wasmtime::Extern::Memory(mem)) => mem,
                    _ => return 0,
                };

                let key_offset = key_ptr as usize;
                let key_length = key_len as usize;

                let data = memory.data(&caller);
                if key_offset + key_length > data.len() {
                    return 0;
                }

                let key = match std::str::from_utf8(&data[key_offset..key_offset + key_length]) {
                    Ok(k) => k,
                    Err(_) => return 0,
                };

                let env = caller
                    .data()
                    .system_state()
                    .environment()
                    .read()
                    .expect("Environment lock poisoned");
                env.contains(key) as i32
            },
        )?;

        // Get number of environment variables
        linker.func_wrap("mielin", "env_count", |caller: Caller<'_, T>| -> i32 {
            let env = caller
                .data()
                .system_state()
                .environment()
                .read()
                .expect("Environment lock poisoned");
            env.len() as i32
        })?;

        Ok(())
    }

    fn register_process_functions<T>(linker: &mut Linker<T>) -> anyhow::Result<()>
    where
        T: HasSystemState + 'static,
    {
        // Get runtime instance ID (low 32 bits)
        linker.func_wrap(
            "mielin",
            "process_instance_id_low",
            |caller: Caller<'_, T>| -> i32 {
                (caller.data().system_state().process_info().instance_id & 0xFFFFFFFF) as i32
            },
        )?;

        // Get runtime instance ID (high 32 bits)
        linker.func_wrap(
            "mielin",
            "process_instance_id_high",
            |caller: Caller<'_, T>| -> i32 {
                (caller.data().system_state().process_info().instance_id >> 32) as i32
            },
        )?;

        // Get platform identifier
        linker.func_wrap(
            "mielin",
            "process_platform",
            |caller: Caller<'_, T>| -> i32 {
                caller.data().system_state().process_info().platform as i32
            },
        )?;

        // Get architecture identifier
        linker.func_wrap("mielin", "process_arch", |caller: Caller<'_, T>| -> i32 {
            caller.data().system_state().process_info().arch as i32
        })?;

        // Get pointer size in bits (32 or 64)
        linker.func_wrap(
            "mielin",
            "process_pointer_bits",
            |caller: Caller<'_, T>| -> i32 {
                caller.data().system_state().process_info().pointer_bits as i32
            },
        )?;

        // Get number of logical CPUs
        linker.func_wrap(
            "mielin",
            "process_cpu_count",
            |caller: Caller<'_, T>| -> i32 {
                caller.data().system_state().process_info().cpu_count as i32
            },
        )?;

        // Get total memory in MB (or -1 if unknown)
        linker.func_wrap(
            "mielin",
            "process_memory_total_mb",
            |caller: Caller<'_, T>| -> i32 {
                match caller.data().system_state().process_info().memory_total {
                    Some(bytes) => (bytes / (1024 * 1024)) as i32,
                    None => -1,
                }
            },
        )?;

        Ok(())
    }
}

/// Trait for types that contain a SystemState
pub trait HasSystemState {
    fn system_state(&self) -> &SystemState;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_xorshift_deterministic() {
        let mut rng1 = XorShift128Plus::with_seed(12345);
        let mut rng2 = XorShift128Plus::with_seed(12345);

        for _ in 0..100 {
            assert_eq!(rng1.next_u64(), rng2.next_u64());
        }
    }

    #[test]
    fn test_xorshift_different_seeds() {
        let mut rng1 = XorShift128Plus::with_seed(1);
        let mut rng2 = XorShift128Plus::with_seed(2);

        // Should produce different sequences
        let seq1: Vec<u64> = (0..10).map(|_| rng1.next_u64()).collect();
        let seq2: Vec<u64> = (0..10).map(|_| rng2.next_u64()).collect();

        assert_ne!(seq1, seq2);
    }

    #[test]
    fn test_xorshift_non_zero() {
        let mut rng = XorShift128Plus::with_seed(0);

        // Even with seed 0, should produce non-zero values
        let mut has_nonzero = false;
        for _ in 0..100 {
            if rng.next_u64() != 0 {
                has_nonzero = true;
                break;
            }
        }
        assert!(has_nonzero);
    }

    #[test]
    fn test_xorshift_jump() {
        let mut rng1 = XorShift128Plus::with_seed(42);
        let mut rng2 = XorShift128Plus::with_seed(42);

        // Advance rng1 by 2^64 steps
        rng1.jump();

        // rng1 and rng2 should produce different sequences now
        assert_ne!(rng1.next_u64(), rng2.next_u64());
    }

    #[test]
    fn test_system_state_timestamp() {
        let state = SystemState::new();

        let ts1 = state.timestamp_millis();
        std::thread::sleep(std::time::Duration::from_millis(10));
        let ts2 = state.timestamp_millis();

        assert!(ts2 > ts1);
    }

    #[test]
    fn test_system_state_monotonic() {
        let state = SystemState::new();

        let m1 = state.monotonic_nanos();
        std::thread::sleep(std::time::Duration::from_millis(1));
        let m2 = state.monotonic_nanos();

        assert!(m2 > m1);
    }

    #[test]
    fn test_system_state_random_f64_range() {
        let state = SystemState::with_seed(999);

        for _ in 0..1000 {
            let val = state.random_f64();
            assert!(val >= 0.0, "Value {} should be >= 0.0", val);
            assert!(val < 1.0, "Value {} should be < 1.0", val);
        }
    }

    #[test]
    fn test_system_state_random_bytes() {
        let state = SystemState::with_seed(777);

        let mut buffer = [0u8; 32];
        state.random_bytes(&mut buffer);

        // Should not be all zeros
        assert!(buffer.iter().any(|&b| b != 0));
    }

    #[test]
    fn test_environment_store() {
        let mut env = EnvironmentStore::new();

        assert!(env.is_empty());

        env.set("KEY1", "value1");
        env.set("KEY2", "value2");

        assert_eq!(env.len(), 2);
        assert!(env.contains("KEY1"));
        assert_eq!(env.get("KEY1"), Some(&"value1".to_string()));

        let removed = env.remove("KEY1");
        assert_eq!(removed, Some("value1".to_string()));
        assert!(!env.contains("KEY1"));
    }

    #[test]
    fn test_platform_detection() {
        let platform = Platform::current();

        // Should be one of the known platforms
        match platform {
            Platform::Linux
            | Platform::MacOS
            | Platform::Windows
            | Platform::FreeBSD
            | Platform::Android
            | Platform::IOS
            | Platform::WASM
            | Platform::Unknown => {}
        }
    }

    #[test]
    fn test_architecture_detection() {
        let arch = Architecture::current();

        // Should be one of the known architectures
        match arch {
            Architecture::X86
            | Architecture::X86_64
            | Architecture::ARM
            | Architecture::AArch64
            | Architecture::RISCV32
            | Architecture::RISCV64
            | Architecture::WASM32
            | Architecture::WASM64
            | Architecture::Unknown => {}
        }
    }

    #[test]
    fn test_process_info() {
        let info = ProcessInfo::new();

        assert!(info.cpu_count >= 1);
        assert!(info.pointer_bits == 32 || info.pointer_bits == 64);
    }

    #[test]
    fn test_system_state_with_seed() {
        let state1 = SystemState::with_seed(12345);
        let state2 = SystemState::with_seed(12345);

        // Same seed should produce same random sequence
        assert_eq!(state1.random_u64(), state2.random_u64());
    }

    #[test]
    fn test_random_u64_distribution() {
        let state = SystemState::with_seed(54321);

        let mut low_count = 0;
        let mut high_count = 0;
        let midpoint = u64::MAX / 2;

        for _ in 0..1000 {
            let val = state.random_u64();
            if val < midpoint {
                low_count += 1;
            } else {
                high_count += 1;
            }
        }

        // Should be roughly evenly distributed (within 30%)
        let ratio = low_count as f64 / high_count as f64;
        assert!(
            ratio > 0.7 && ratio < 1.3,
            "Distribution ratio {} seems skewed",
            ratio
        );
    }

    #[test]
    fn test_environment_clear() {
        let mut env = EnvironmentStore::new();
        env.set("A", "1");
        env.set("B", "2");

        assert_eq!(env.len(), 2);
        env.clear();
        assert!(env.is_empty());
    }

    #[test]
    fn test_environment_keys() {
        let mut env = EnvironmentStore::new();
        env.set("KEY_A", "val_a");
        env.set("KEY_B", "val_b");

        let keys: Vec<&String> = env.keys().collect();
        assert_eq!(keys.len(), 2);
        assert!(keys.iter().any(|k| *k == "KEY_A"));
        assert!(keys.iter().any(|k| *k == "KEY_B"));
    }
}
