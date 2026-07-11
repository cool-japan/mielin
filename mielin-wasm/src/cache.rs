//! WASM Module Caching
//!
//! Provides caching of compiled WASM modules to avoid repeated compilation costs.
//! Supports LRU eviction, TTL expiration, and persistent cache storage.

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

/// Cache key for WASM modules
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CacheKey {
    /// Hash of the WASM bytecode
    bytecode_hash: u64,
    /// Configuration hash (affects compilation)
    config_hash: u64,
}

impl CacheKey {
    /// Create a cache key from WASM bytecode
    pub fn from_bytecode(bytecode: &[u8]) -> Self {
        Self {
            bytecode_hash: Self::compute_hash(bytecode),
            config_hash: 0,
        }
    }

    /// Create a cache key with configuration
    pub fn from_bytecode_and_config(bytecode: &[u8], config_hash: u64) -> Self {
        Self {
            bytecode_hash: Self::compute_hash(bytecode),
            config_hash,
        }
    }

    /// Compute FNV-1a hash of data
    fn compute_hash(data: &[u8]) -> u64 {
        const FNV_OFFSET: u64 = 0xcbf29ce484222325;
        const FNV_PRIME: u64 = 0x100000001b3;

        let mut hash = FNV_OFFSET;
        for &byte in data {
            hash ^= byte as u64;
            hash = hash.wrapping_mul(FNV_PRIME);
        }
        hash
    }

    /// Get the bytecode hash
    pub fn bytecode_hash(&self) -> u64 {
        self.bytecode_hash
    }

    /// Get the config hash
    pub fn config_hash(&self) -> u64 {
        self.config_hash
    }
}

/// Cache entry containing compiled module data
#[derive(Debug, Clone)]
pub struct CacheEntry<T> {
    /// The cached module
    module: T,
    /// When this entry was created
    created_at: Instant,
    /// When this entry was last accessed
    last_accessed: Instant,
    /// Number of times this entry was accessed
    access_count: u64,
    /// Size of the original bytecode
    bytecode_size: usize,
    /// Monotonic access-sequence stamp used for deterministic LRU ordering.
    ///
    /// Unlike `last_accessed` (an `Instant`), this is guaranteed to be
    /// strictly comparable across entries even when several accesses land
    /// within the same clock tick, avoiding nondeterministic tie-breaking
    /// via `HashMap` iteration order.
    last_access_seq: u64,
}

impl<T: Clone> CacheEntry<T> {
    /// Create a new cache entry
    pub fn new(module: T, bytecode_size: usize) -> Self {
        let now = Instant::now();
        Self {
            module,
            created_at: now,
            last_accessed: now,
            access_count: 1,
            bytecode_size,
            last_access_seq: 0,
        }
    }

    /// Get the cached module
    pub fn module(&self) -> &T {
        &self.module
    }

    /// Get a clone of the cached module
    pub fn module_cloned(&self) -> T {
        self.module.clone()
    }

    /// Check if this entry has expired
    pub fn is_expired(&self, ttl: Duration) -> bool {
        self.created_at.elapsed() > ttl
    }

    /// Get the age of this entry
    pub fn age(&self) -> Duration {
        self.created_at.elapsed()
    }

    /// Get time since last access
    pub fn idle_time(&self) -> Duration {
        self.last_accessed.elapsed()
    }

    /// Get access count
    pub fn access_count(&self) -> u64 {
        self.access_count
    }

    /// Get bytecode size
    pub fn bytecode_size(&self) -> usize {
        self.bytecode_size
    }
}

/// Cache statistics
#[derive(Debug, Default, Clone)]
pub struct CacheStats {
    /// Number of cache hits
    pub hits: u64,
    /// Number of cache misses
    pub misses: u64,
    /// Number of entries evicted
    pub evictions: u64,
    /// Number of entries expired
    pub expirations: u64,
    /// Current number of entries
    pub entries: usize,
    /// Total bytecode bytes cached
    pub total_bytes: usize,
}

impl CacheStats {
    /// Calculate hit rate
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }

    /// Calculate miss rate
    pub fn miss_rate(&self) -> f64 {
        1.0 - self.hit_rate()
    }
}

/// Cache configuration
#[derive(Debug, Clone)]
pub struct CacheConfig {
    /// Maximum number of entries
    pub max_entries: usize,
    /// Maximum total memory (in bytes of original bytecode)
    pub max_bytes: usize,
    /// Time-to-live for entries
    pub ttl: Option<Duration>,
    /// Eviction policy
    pub eviction_policy: EvictionPolicy,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            max_entries: 100,
            max_bytes: 64 * 1024 * 1024,          // 64 MB
            ttl: Some(Duration::from_secs(3600)), // 1 hour
            eviction_policy: EvictionPolicy::Lru,
        }
    }
}

impl CacheConfig {
    /// Create config for small/embedded systems
    pub fn small() -> Self {
        Self {
            max_entries: 10,
            max_bytes: 4 * 1024 * 1024,          // 4 MB
            ttl: Some(Duration::from_secs(600)), // 10 minutes
            eviction_policy: EvictionPolicy::Lru,
        }
    }

    /// Create config for large systems
    pub fn large() -> Self {
        Self {
            max_entries: 1000,
            max_bytes: 512 * 1024 * 1024,          // 512 MB
            ttl: Some(Duration::from_secs(86400)), // 24 hours
            eviction_policy: EvictionPolicy::Lfu,
        }
    }

    /// Create config with no limits
    pub fn unlimited() -> Self {
        Self {
            max_entries: usize::MAX,
            max_bytes: usize::MAX,
            ttl: None,
            eviction_policy: EvictionPolicy::None,
        }
    }
}

/// Eviction policy
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvictionPolicy {
    /// Least Recently Used
    Lru,
    /// Least Frequently Used
    Lfu,
    /// First In First Out
    Fifo,
    /// No eviction (will reject new entries when full)
    None,
}

/// Module cache implementation
pub struct ModuleCache<T: Clone> {
    entries: RwLock<HashMap<CacheKey, CacheEntry<T>>>,
    config: CacheConfig,
    stats: Mutex<CacheStats>,
    insertion_order: Mutex<Vec<CacheKey>>,
    /// Monotonic counter handed out to entries on insert/access, used to
    /// order LRU eviction deterministically instead of relying on
    /// `Instant`-based tie-breaks (see `CacheEntry::last_access_seq`).
    access_seq: std::sync::atomic::AtomicU64,
}

impl<T: Clone> ModuleCache<T> {
    /// Create a new module cache with default config
    pub fn new() -> Self {
        Self::with_config(CacheConfig::default())
    }

    /// Create a new module cache with custom config
    pub fn with_config(config: CacheConfig) -> Self {
        Self {
            entries: RwLock::new(HashMap::new()),
            config,
            stats: Mutex::new(CacheStats::default()),
            insertion_order: Mutex::new(Vec::new()),
            access_seq: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// Get the next monotonic access-sequence stamp for LRU ordering.
    fn next_access_seq(&self) -> u64 {
        self.access_seq
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    }

    /// Get a cached module
    pub fn get(&self, key: &CacheKey) -> Option<T> {
        // Use write lock to update last_accessed for LRU
        let mut entries = self.entries.write().expect("Cache entries lock poisoned");

        if let Some(entry) = entries.get_mut(key) {
            // Check expiration
            if let Some(ttl) = self.config.ttl {
                if entry.is_expired(ttl) {
                    entries.remove(key);
                    drop(entries);

                    let mut order = self
                        .insertion_order
                        .lock()
                        .expect("Cache insertion order lock poisoned");
                    order.retain(|k| k != key);

                    let mut stats = self.stats.lock().expect("Cache stats lock poisoned");
                    stats.misses += 1;
                    stats.expirations += 1;
                    return None;
                }
            }

            // Update access tracking for LRU
            let seq = self.next_access_seq();
            entry.last_accessed = Instant::now();
            entry.access_count += 1;
            entry.last_access_seq = seq;

            // Record hit
            let mut stats = self.stats.lock().expect("Cache stats lock poisoned");
            stats.hits += 1;

            return Some(entry.module.clone());
        }

        drop(entries);

        // Cache miss
        let mut stats = self.stats.lock().expect("Cache stats lock poisoned");
        stats.misses += 1;
        None
    }

    /// Insert a module into the cache
    pub fn insert(&self, key: CacheKey, module: T, bytecode_size: usize) -> bool {
        // Check if we need to evict
        self.maybe_evict(bytecode_size);

        let mut entries = self.entries.write().expect("Cache entries lock poisoned");

        // Check limits after eviction
        if entries.len() >= self.config.max_entries
            && !entries.contains_key(&key)
            && self.config.eviction_policy == EvictionPolicy::None
        {
            return false;
        }

        let total_bytes: usize = entries.values().map(|e| e.bytecode_size()).sum();
        if total_bytes + bytecode_size > self.config.max_bytes
            && !entries.contains_key(&key)
            && self.config.eviction_policy == EvictionPolicy::None
        {
            return false;
        }

        let is_new = !entries.contains_key(&key);
        let mut entry = CacheEntry::new(module, bytecode_size);
        entry.last_access_seq = self.next_access_seq();
        entries.insert(key.clone(), entry);

        if is_new {
            let mut order = self
                .insertion_order
                .lock()
                .expect("Cache insertion order lock poisoned");
            order.push(key);
        }

        // Update stats
        let mut stats = self.stats.lock().expect("Cache stats lock poisoned");
        stats.entries = entries.len();
        stats.total_bytes = entries.values().map(|e| e.bytecode_size()).sum();

        true
    }

    /// Remove an entry
    pub fn remove(&self, key: &CacheKey) -> Option<T> {
        let mut entries = self.entries.write().expect("Cache entries lock poisoned");
        let result = entries.remove(key).map(|e| e.module);

        // Update insertion order
        let mut order = self
            .insertion_order
            .lock()
            .expect("Cache insertion order lock poisoned");
        order.retain(|k| k != key);

        // Update stats
        let mut stats = self.stats.lock().expect("Cache stats lock poisoned");
        stats.entries = entries.len();
        stats.total_bytes = entries.values().map(|e| e.bytecode_size()).sum();

        result
    }

    /// Clear the cache
    pub fn clear(&self) {
        let mut entries = self.entries.write().expect("Cache entries lock poisoned");
        entries.clear();

        let mut order = self
            .insertion_order
            .lock()
            .expect("Cache insertion order lock poisoned");
        order.clear();

        let mut stats = self.stats.lock().expect("Cache stats lock poisoned");
        stats.entries = 0;
        stats.total_bytes = 0;
    }

    /// Get cache statistics
    pub fn stats(&self) -> CacheStats {
        self.stats
            .lock()
            .expect("Cache stats lock poisoned")
            .clone()
    }

    /// Get cache configuration
    pub fn config(&self) -> &CacheConfig {
        &self.config
    }

    /// Get number of entries
    pub fn len(&self) -> usize {
        self.entries
            .read()
            .expect("Cache entries lock poisoned")
            .len()
    }

    /// Check if cache is empty
    pub fn is_empty(&self) -> bool {
        self.entries
            .read()
            .expect("Cache entries lock poisoned")
            .is_empty()
    }

    /// Remove expired entries
    pub fn cleanup_expired(&self) -> usize {
        let ttl = match self.config.ttl {
            Some(ttl) => ttl,
            None => return 0,
        };

        let mut entries = self.entries.write().expect("Cache entries lock poisoned");
        let initial_len = entries.len();

        entries.retain(|_, entry| !entry.is_expired(ttl));

        let removed = initial_len - entries.len();

        if removed > 0 {
            let mut order = self
                .insertion_order
                .lock()
                .expect("Cache insertion order lock poisoned");
            order.retain(|key| entries.contains_key(key));

            let mut stats = self.stats.lock().expect("Cache stats lock poisoned");
            stats.expirations += removed as u64;
            stats.entries = entries.len();
            stats.total_bytes = entries.values().map(|e| e.bytecode_size()).sum();
        }

        removed
    }

    /// Evict entries if necessary
    fn maybe_evict(&self, new_size: usize) {
        if self.config.eviction_policy == EvictionPolicy::None {
            return;
        }

        // First cleanup expired entries
        self.cleanup_expired();

        let mut entries = self.entries.write().expect("Cache entries lock poisoned");

        // Check if we need to evict for entry count
        while entries.len() >= self.config.max_entries {
            if let Some(key) = self.select_eviction_target(&entries) {
                entries.remove(&key);
                let mut order = self
                    .insertion_order
                    .lock()
                    .expect("Cache insertion order lock poisoned");
                order.retain(|k| k != &key);
                let mut stats = self.stats.lock().expect("Cache stats lock poisoned");
                stats.evictions += 1;
            } else {
                break;
            }
        }

        // Check if we need to evict for size
        let mut total_bytes: usize = entries.values().map(|e| e.bytecode_size()).sum();
        while total_bytes + new_size > self.config.max_bytes && !entries.is_empty() {
            if let Some(key) = self.select_eviction_target(&entries) {
                if let Some(entry) = entries.remove(&key) {
                    total_bytes -= entry.bytecode_size();
                }
                let mut order = self
                    .insertion_order
                    .lock()
                    .expect("Cache insertion order lock poisoned");
                order.retain(|k| k != &key);
                let mut stats = self.stats.lock().expect("Cache stats lock poisoned");
                stats.evictions += 1;
            } else {
                break;
            }
        }
    }

    /// Select an entry to evict based on policy
    fn select_eviction_target(
        &self,
        entries: &HashMap<CacheKey, CacheEntry<T>>,
    ) -> Option<CacheKey> {
        if entries.is_empty() {
            return None;
        }

        match self.config.eviction_policy {
            EvictionPolicy::Lru => {
                // Find least recently used: the entry with the smallest
                // monotonic access-sequence stamp. This is deterministic
                // even when multiple accesses land within the same
                // `Instant` tick, unlike ordering by `idle_time()` (which
                // ties and falls back to nondeterministic HashMap
                // iteration order).
                entries
                    .iter()
                    .min_by_key(|(_, entry)| entry.last_access_seq)
                    .map(|(key, _)| key.clone())
            }
            EvictionPolicy::Lfu => {
                // Find least frequently used
                entries
                    .iter()
                    .min_by_key(|(_, entry)| entry.access_count())
                    .map(|(key, _)| key.clone())
            }
            EvictionPolicy::Fifo => {
                // First in first out
                let order = self
                    .insertion_order
                    .lock()
                    .expect("Cache insertion order lock poisoned");
                order.first().cloned()
            }
            EvictionPolicy::None => None,
        }
    }
}

impl<T: Clone> Default for ModuleCache<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Thread-safe wrapper around ModuleCache
pub type SharedModuleCache<T> = Arc<ModuleCache<T>>;

/// Create a new shared module cache
pub fn shared_cache<T: Clone>() -> SharedModuleCache<T> {
    Arc::new(ModuleCache::new())
}

/// Create a new shared module cache with config
pub fn shared_cache_with_config<T: Clone>(config: CacheConfig) -> SharedModuleCache<T> {
    Arc::new(ModuleCache::with_config(config))
}

/// Serializable module cache entry for persistence
#[derive(Debug, Clone)]
pub struct SerializedCacheEntry {
    /// Cache key
    pub key: CacheKey,
    /// Serialized module bytes
    pub module_bytes: Vec<u8>,
    /// Original bytecode hash for verification
    pub bytecode_hash: u64,
    /// Original bytecode size
    pub bytecode_size: usize,
}

/// Cache serialization trait
pub trait CacheSerializer<T> {
    /// Serialize a module to bytes
    fn serialize(&self, module: &T) -> Option<Vec<u8>>;
    /// Deserialize a module from bytes
    fn deserialize(&self, bytes: &[u8]) -> Option<T>;
}

/// Persistent cache storage trait
pub trait CacheStorage {
    /// Store a serialized entry
    fn store(&self, entry: &SerializedCacheEntry) -> Result<(), std::io::Error>;
    /// Load a serialized entry
    fn load(&self, key: &CacheKey) -> Result<Option<SerializedCacheEntry>, std::io::Error>;
    /// Remove a serialized entry
    fn remove(&self, key: &CacheKey) -> Result<(), std::io::Error>;
    /// List all keys
    fn list_keys(&self) -> Result<Vec<CacheKey>, std::io::Error>;
    /// Clear all entries
    fn clear(&self) -> Result<(), std::io::Error>;
}

/// File-based persistent cache storage
///
/// Stores compiled modules in a directory structure for persistence across restarts.
#[cfg(feature = "persistent-cache")]
pub mod storage {
    use super::*;
    use std::fs::{self, File};
    use std::io::{Read as IoRead, Write};
    use std::path::{Path, PathBuf};

    /// File system based cache storage
    pub struct FileSystemStorage {
        base_path: PathBuf,
    }

    impl FileSystemStorage {
        /// Create new file system storage
        pub fn new<P: AsRef<Path>>(base_path: P) -> std::io::Result<Self> {
            let path = base_path.as_ref().to_path_buf();
            fs::create_dir_all(&path)?;
            Ok(Self { base_path: path })
        }

        /// Get the file path for a cache key
        fn key_to_path(&self, key: &CacheKey) -> PathBuf {
            let filename = format!(
                "{:016x}_{:016x}.cache",
                key.bytecode_hash(),
                key.config_hash()
            );
            self.base_path.join(filename)
        }

        /// Get the metadata path for a cache key
        fn key_to_meta_path(&self, key: &CacheKey) -> PathBuf {
            let filename = format!(
                "{:016x}_{:016x}.meta",
                key.bytecode_hash(),
                key.config_hash()
            );
            self.base_path.join(filename)
        }
    }

    impl CacheStorage for FileSystemStorage {
        fn store(&self, entry: &SerializedCacheEntry) -> Result<(), std::io::Error> {
            let path = self.key_to_path(&entry.key);
            let meta_path = self.key_to_meta_path(&entry.key);

            // Write module bytes
            let mut file = File::create(&path)?;
            file.write_all(&entry.module_bytes)?;
            file.sync_all()?;

            // Write metadata
            let meta = format!(
                "{},{},{}",
                entry.key.bytecode_hash(),
                entry.key.config_hash(),
                entry.bytecode_size
            );
            let mut meta_file = File::create(&meta_path)?;
            meta_file.write_all(meta.as_bytes())?;
            meta_file.sync_all()?;

            Ok(())
        }

        fn load(&self, key: &CacheKey) -> Result<Option<SerializedCacheEntry>, std::io::Error> {
            let path = self.key_to_path(key);
            let meta_path = self.key_to_meta_path(key);

            if !path.exists() || !meta_path.exists() {
                return Ok(None);
            }

            // Read metadata
            let mut meta_content = String::new();
            File::open(&meta_path)?.read_to_string(&mut meta_content)?;
            let parts: Vec<&str> = meta_content.split(',').collect();
            if parts.len() != 3 {
                return Ok(None);
            }

            let bytecode_hash: u64 = parts[0].parse().map_err(|_| {
                std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid metadata")
            })?;
            let bytecode_size: usize = parts[2].parse().map_err(|_| {
                std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid metadata")
            })?;

            // Read module bytes
            let mut module_bytes = Vec::new();
            File::open(&path)?.read_to_end(&mut module_bytes)?;

            Ok(Some(SerializedCacheEntry {
                key: key.clone(),
                module_bytes,
                bytecode_hash,
                bytecode_size,
            }))
        }

        fn remove(&self, key: &CacheKey) -> Result<(), std::io::Error> {
            let path = self.key_to_path(key);
            let meta_path = self.key_to_meta_path(key);

            if path.exists() {
                fs::remove_file(&path)?;
            }
            if meta_path.exists() {
                fs::remove_file(&meta_path)?;
            }
            Ok(())
        }

        fn list_keys(&self) -> Result<Vec<CacheKey>, std::io::Error> {
            let mut keys = Vec::new();

            for entry in fs::read_dir(&self.base_path)? {
                let entry = entry?;
                let path = entry.path();

                if let Some(ext) = path.extension() {
                    if ext == "meta" {
                        if let Some(stem) = path.file_stem() {
                            let stem_str = stem.to_string_lossy();
                            let parts: Vec<&str> = stem_str.split('_').collect();
                            if parts.len() == 2 {
                                if let (Ok(bytecode_hash), Ok(config_hash)) = (
                                    u64::from_str_radix(parts[0], 16),
                                    u64::from_str_radix(parts[1], 16),
                                ) {
                                    keys.push(CacheKey {
                                        bytecode_hash,
                                        config_hash,
                                    });
                                }
                            }
                        }
                    }
                }
            }

            Ok(keys)
        }

        fn clear(&self) -> Result<(), std::io::Error> {
            for entry in fs::read_dir(&self.base_path)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_file() {
                    fs::remove_file(path)?;
                }
            }
            Ok(())
        }
    }
}

/// Wasmtime module serializer
///
/// Serializes compiled Wasmtime modules for persistent caching.
pub struct WasmtimeModuleSerializer;

impl WasmtimeModuleSerializer {
    /// Serialize a Wasmtime module to bytes
    ///
    /// Note: This requires the module to be compiled with serialization support.
    pub fn serialize(module: &wasmtime::Module) -> Option<Vec<u8>> {
        module.serialize().ok()
    }

    /// Deserialize a Wasmtime module from bytes
    ///
    /// # Safety
    /// The caller must ensure the serialized bytes were produced by the same
    /// engine configuration that created this engine.
    pub unsafe fn deserialize(engine: &wasmtime::Engine, bytes: &[u8]) -> Option<wasmtime::Module> {
        wasmtime::Module::deserialize(engine, bytes).ok()
    }
}

/// Cached WASM executor
///
/// Wraps a WasmExecutor with a module cache for faster repeated compilations.
pub struct CachedExecutor {
    executor: crate::executor::WasmExecutor,
    cache: SharedModuleCache<Vec<u8>>,
    config_hash: u64,
}

impl CachedExecutor {
    /// Create a new cached executor with default cache config
    pub fn new() -> Result<Self, crate::WasmError> {
        Self::with_cache_config(CacheConfig::default())
    }

    /// Create a cached executor with custom cache config
    pub fn with_cache_config(cache_config: CacheConfig) -> Result<Self, crate::WasmError> {
        use crate::executor::WasmExecutor;

        let executor = WasmExecutor::new()?;
        let cache = shared_cache_with_config(cache_config);

        Ok(Self {
            executor,
            cache,
            config_hash: 0, // Default config
        })
    }

    /// Create a cached executor with hardware capabilities
    pub fn with_capabilities(
        capabilities: mielin_hal::capabilities::HardwareCapabilities,
    ) -> Result<Self, crate::WasmError> {
        use crate::executor::WasmExecutor;

        let executor = WasmExecutor::with_capabilities(capabilities)?;
        let cache = shared_cache();

        Ok(Self {
            executor,
            cache,
            config_hash: capabilities.bits(),
        })
    }

    /// Get the underlying executor
    pub fn executor(&self) -> &crate::executor::WasmExecutor {
        &self.executor
    }

    /// Get cache statistics
    pub fn cache_stats(&self) -> CacheStats {
        self.cache.stats()
    }

    /// Clear the cache
    pub fn clear_cache(&self) {
        self.cache.clear();
    }

    /// Compile a module with caching
    ///
    /// Returns a compiled module, using cached version if available.
    pub fn compile_module(&self, wasm_bytes: &[u8]) -> Result<wasmtime::Module, crate::WasmError> {
        let key = CacheKey::from_bytecode_and_config(wasm_bytes, self.config_hash);

        // Check cache first
        if let Some(serialized) = self.cache.get(&key) {
            // Safety: We're deserializing with the same engine that serialized
            if let Some(module) = unsafe {
                WasmtimeModuleSerializer::deserialize(self.executor.engine(), &serialized)
            } {
                return Ok(module);
            }
            // Cache hit but deserialization failed - remove stale entry
            self.cache.remove(&key);
        }

        // Compile fresh
        let module = self.executor.compile_module(wasm_bytes)?;

        // Try to cache the compiled module
        if let Some(serialized) = WasmtimeModuleSerializer::serialize(&module) {
            self.cache.insert(key, serialized, wasm_bytes.len());
        }

        Ok(module)
    }

    /// Instantiate a compiled module
    pub fn instantiate(
        &self,
        module: &wasmtime::Module,
        capabilities: mielin_hal::capabilities::HardwareCapabilities,
    ) -> Result<(wasmtime::Instance, wasmtime::Store<crate::host::HostState>), crate::WasmError>
    {
        self.executor.instantiate(module, capabilities)
    }

    /// Execute an agent with caching
    pub fn execute(
        &self,
        agent: &mielin_cells::Agent,
    ) -> Result<crate::executor::ExecutionResult, crate::WasmError> {
        self.execute_with_capabilities(agent, mielin_hal::capabilities::HardwareCapabilities::NONE)
    }

    /// Execute an agent with capabilities and caching
    pub fn execute_with_capabilities(
        &self,
        agent: &mielin_cells::Agent,
        capabilities: mielin_hal::capabilities::HardwareCapabilities,
    ) -> Result<crate::executor::ExecutionResult, crate::WasmError> {
        let wasm_bytes = agent.dna().binary();

        if wasm_bytes.len() < 4 || &wasm_bytes[0..4] != b"\0asm" {
            return Err(crate::WasmError::InvalidModule(
                "Invalid WASM magic number".to_string(),
            ));
        }

        let module = self.compile_module(wasm_bytes)?;
        let (instance, mut store) = self.instantiate(&module, capabilities)?;

        let start_func = instance
            .get_func(&mut store, "_start")
            .or_else(|| instance.get_func(&mut store, "main"));

        if let Some(func) = start_func {
            func.call(&mut store, &[], &mut [])
                .map_err(|e| crate::WasmError::ExecutionFailed(e.to_string()))?;
        }

        Ok(crate::executor::ExecutionResult {
            exit_code: 0,
            memory_snapshot: vec![],
        })
    }

    /// Validate WASM bytes
    pub fn validate(&self, wasm_bytes: &[u8]) -> Result<(), crate::WasmError> {
        self.executor.validate(wasm_bytes)
    }

    /// Precompile and cache a module
    ///
    /// Useful for warming up the cache during initialization.
    pub fn precompile(&self, wasm_bytes: &[u8]) -> Result<(), crate::WasmError> {
        let _ = self.compile_module(wasm_bytes)?;
        Ok(())
    }

    /// Precompile multiple modules in parallel
    ///
    /// Returns the number of successfully precompiled modules.
    pub fn precompile_batch(&self, modules: &[&[u8]]) -> usize {
        modules
            .iter()
            .filter(|bytes| self.precompile(bytes).is_ok())
            .count()
    }
}

impl Default for CachedExecutor {
    fn default() -> Self {
        Self::new().expect("Failed to create default CachedExecutor")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_key_from_bytecode() {
        let bytecode = b"\x00asm\x01\x00\x00\x00";
        let key1 = CacheKey::from_bytecode(bytecode);
        let key2 = CacheKey::from_bytecode(bytecode);
        assert_eq!(key1, key2);

        let different = b"\x00asm\x01\x00\x00\x01";
        let key3 = CacheKey::from_bytecode(different);
        assert_ne!(key1, key3);
    }

    #[test]
    fn test_cache_key_with_config() {
        let bytecode = b"\x00asm\x01\x00\x00\x00";
        let key1 = CacheKey::from_bytecode_and_config(bytecode, 0);
        let key2 = CacheKey::from_bytecode_and_config(bytecode, 1);
        assert_ne!(key1, key2);
    }

    #[test]
    fn test_cache_entry_expiration() {
        let entry = CacheEntry::new("test".to_string(), 100);
        assert!(!entry.is_expired(Duration::from_secs(10)));

        // Can't reliably test actual expiration without sleeping
    }

    #[test]
    fn test_cache_stats_hit_rate() {
        let mut stats = CacheStats::default();
        assert_eq!(stats.hit_rate(), 0.0);

        stats.hits = 3;
        stats.misses = 1;
        assert_eq!(stats.hit_rate(), 0.75);
    }

    #[test]
    fn test_module_cache_basic() {
        let cache: ModuleCache<String> = ModuleCache::new();

        let key = CacheKey::from_bytecode(b"test");
        assert!(cache.get(&key).is_none());

        cache.insert(key.clone(), "module1".to_string(), 100);
        assert_eq!(cache.get(&key), Some("module1".to_string()));
    }

    #[test]
    fn test_module_cache_remove() {
        let cache: ModuleCache<String> = ModuleCache::new();

        let key = CacheKey::from_bytecode(b"test");
        cache.insert(key.clone(), "module".to_string(), 100);

        let removed = cache.remove(&key);
        assert_eq!(removed, Some("module".to_string()));
        assert!(cache.get(&key).is_none());
    }

    #[test]
    fn test_module_cache_clear() {
        let cache: ModuleCache<String> = ModuleCache::new();

        for i in 0..5 {
            let key = CacheKey::from_bytecode(format!("test{}", i).as_bytes());
            cache.insert(key, format!("module{}", i), 100);
        }

        assert_eq!(cache.len(), 5);
        cache.clear();
        assert!(cache.is_empty());
    }

    #[test]
    fn test_module_cache_stats() {
        let cache: ModuleCache<String> = ModuleCache::new();

        let key = CacheKey::from_bytecode(b"test");

        // Miss
        cache.get(&key);
        let stats = cache.stats();
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.hits, 0);

        // Insert
        cache.insert(key.clone(), "module".to_string(), 100);
        let stats = cache.stats();
        assert_eq!(stats.entries, 1);
        assert_eq!(stats.total_bytes, 100);

        // Hit
        cache.get(&key);
        let stats = cache.stats();
        assert_eq!(stats.hits, 1);
    }

    #[test]
    fn test_cache_config_presets() {
        let small = CacheConfig::small();
        assert_eq!(small.max_entries, 10);

        let large = CacheConfig::large();
        assert_eq!(large.max_entries, 1000);

        let unlimited = CacheConfig::unlimited();
        assert_eq!(unlimited.eviction_policy, EvictionPolicy::None);
    }

    #[test]
    fn test_module_cache_lru_eviction() {
        let config = CacheConfig {
            max_entries: 3,
            max_bytes: usize::MAX,
            ttl: None,
            eviction_policy: EvictionPolicy::Lru,
        };
        let cache: ModuleCache<String> = ModuleCache::with_config(config);

        // Insert 3 entries
        for i in 0..3 {
            let key = CacheKey::from_bytecode(format!("test{}", i).as_bytes());
            cache.insert(key, format!("module{}", i), 100);
        }
        assert_eq!(cache.len(), 3);

        // Access entry 0 to make it recently used
        let key0 = CacheKey::from_bytecode(b"test0");
        cache.get(&key0);

        // Insert 4th entry - should evict entry 1 (LRU)
        let key3 = CacheKey::from_bytecode(b"test3");
        cache.insert(key3.clone(), "module3".to_string(), 100);

        // Entry 0 should still exist (was accessed)
        assert!(cache.get(&key0).is_some());
        // Entry 3 should exist (just added)
        assert!(cache.get(&key3).is_some());
    }

    #[test]
    fn test_module_cache_size_limit() {
        let config = CacheConfig {
            max_entries: usize::MAX,
            max_bytes: 250, // Allow ~2 entries of 100 bytes
            ttl: None,
            eviction_policy: EvictionPolicy::Fifo,
        };
        let cache: ModuleCache<String> = ModuleCache::with_config(config);

        // Insert 3 entries of 100 bytes each
        for i in 0..3 {
            let key = CacheKey::from_bytecode(format!("test{}", i).as_bytes());
            cache.insert(key, format!("module{}", i), 100);
        }

        // Should have evicted oldest entries to stay under 250 bytes
        let stats = cache.stats();
        assert!(stats.total_bytes <= 250);
    }

    #[test]
    fn test_module_cache_no_eviction_policy() {
        let config = CacheConfig {
            max_entries: 2,
            max_bytes: usize::MAX,
            ttl: None,
            eviction_policy: EvictionPolicy::None,
        };
        let cache: ModuleCache<String> = ModuleCache::with_config(config);

        // Insert 2 entries
        for i in 0..2 {
            let key = CacheKey::from_bytecode(format!("test{}", i).as_bytes());
            assert!(cache.insert(key, format!("module{}", i), 100));
        }

        // 3rd insert should fail
        let key2 = CacheKey::from_bytecode(b"test2");
        assert!(!cache.insert(key2, "module2".to_string(), 100));
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn test_shared_cache() {
        let cache: SharedModuleCache<String> = shared_cache();
        let key = CacheKey::from_bytecode(b"test");

        // Use from multiple references
        let cache2 = Arc::clone(&cache);
        cache.insert(key.clone(), "module".to_string(), 100);
        assert_eq!(cache2.get(&key), Some("module".to_string()));
    }

    #[test]
    fn test_cache_entry_accessors() {
        let entry = CacheEntry::new("test".to_string(), 256);
        assert_eq!(entry.module(), &"test".to_string());
        assert_eq!(entry.bytecode_size(), 256);
        assert_eq!(entry.access_count(), 1);
        assert!(entry.age() < Duration::from_secs(1));
        assert!(entry.idle_time() < Duration::from_secs(1));
    }

    #[test]
    fn test_cache_key_hash() {
        let key1 = CacheKey::from_bytecode(b"hello");
        let key2 = CacheKey::from_bytecode(b"hello");

        // Keys should be usable in HashMap
        let mut map = HashMap::new();
        map.insert(key1.clone(), "value1");
        assert_eq!(map.get(&key2), Some(&"value1"));
    }

    #[test]
    fn test_serialized_cache_entry() {
        let key = CacheKey::from_bytecode(b"test");
        let entry = SerializedCacheEntry {
            key: key.clone(),
            module_bytes: vec![1, 2, 3, 4],
            bytecode_hash: key.bytecode_hash(),
            bytecode_size: 100,
        };

        assert_eq!(entry.bytecode_hash, key.bytecode_hash());
        assert_eq!(entry.bytecode_size, 100);
    }

    #[test]
    fn test_cached_executor_creation() {
        let executor = CachedExecutor::new();
        assert!(executor.is_ok());

        if let Ok(exec) = executor {
            let stats = exec.cache_stats();
            assert_eq!(stats.hits, 0);
            assert_eq!(stats.misses, 0);
        }
    }

    #[test]
    fn test_cached_executor_compile_and_cache() {
        let executor = CachedExecutor::new().expect("Failed to create executor");

        let wasm = wat::parse_str("(module)").expect("Failed to parse WAT");

        // First compile - cache miss
        let module1 = executor.compile_module(&wasm);
        assert!(module1.is_ok());

        let stats = executor.cache_stats();
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.entries, 1);

        // Second compile - should be cache hit
        let module2 = executor.compile_module(&wasm);
        assert!(module2.is_ok());

        let stats = executor.cache_stats();
        assert_eq!(stats.hits, 1);
    }

    #[test]
    fn test_cached_executor_cache_clear() {
        let executor = CachedExecutor::new().expect("Failed to create executor");

        let wasm = wat::parse_str("(module)").expect("Failed to parse WAT");
        let _ = executor.compile_module(&wasm);

        let stats = executor.cache_stats();
        assert_eq!(stats.entries, 1);

        executor.clear_cache();

        let stats = executor.cache_stats();
        assert_eq!(stats.entries, 0);
    }

    #[cfg_attr(miri, ignore)]
    #[test]
    fn test_cached_executor_precompile() {
        let executor = CachedExecutor::new().expect("Failed to create executor");

        let wasm1 = wat::parse_str("(module)").expect("Failed to parse WAT");
        let wasm2 = wat::parse_str("(module (func))").expect("Failed to parse WAT");

        // Precompile single
        assert!(executor.precompile(&wasm1).is_ok());
        assert_eq!(executor.cache_stats().entries, 1);

        // Precompile batch
        let modules: Vec<&[u8]> = vec![&wasm1, &wasm2];
        let count = executor.precompile_batch(&modules);
        assert_eq!(count, 2);

        // wasm1 was already cached, so still 2 entries
        assert_eq!(executor.cache_stats().entries, 2);
    }

    #[test]
    fn test_cached_executor_validate() {
        let executor = CachedExecutor::new().expect("Failed to create executor");

        let valid_wasm = wat::parse_str("(module)").expect("Failed to parse WAT");
        assert!(executor.validate(&valid_wasm).is_ok());

        let invalid_wasm = b"not wasm";
        assert!(executor.validate(invalid_wasm).is_err());
    }

    #[test]
    fn test_cached_executor_with_custom_config() {
        let config = CacheConfig {
            max_entries: 5,
            max_bytes: 1024 * 1024,
            ttl: None,
            eviction_policy: EvictionPolicy::Lfu,
        };

        let executor = CachedExecutor::with_cache_config(config);
        assert!(executor.is_ok());
    }

    #[test]
    fn test_wasmtime_module_serializer() {
        use crate::executor::WasmExecutor;

        let executor = WasmExecutor::new().expect("Failed to create executor");
        let wasm = wat::parse_str("(module)").expect("Failed to parse WAT");
        let module = executor
            .compile_module(&wasm)
            .expect("Failed to compile module");

        // Serialize
        let serialized = WasmtimeModuleSerializer::serialize(&module);
        assert!(serialized.is_some());

        // Deserialize
        if let Some(bytes) = serialized {
            let deserialized =
                unsafe { WasmtimeModuleSerializer::deserialize(executor.engine(), &bytes) };
            assert!(deserialized.is_some());
        }
    }

    #[test]
    fn test_cache_different_configs_different_keys() {
        // Same bytecode with different config should have different keys
        let bytecode = b"\x00asm\x01\x00\x00\x00";

        let key1 = CacheKey::from_bytecode_and_config(bytecode, 0);
        let key2 = CacheKey::from_bytecode_and_config(bytecode, 1);
        let key3 = CacheKey::from_bytecode_and_config(bytecode, 0);

        assert_ne!(key1, key2);
        assert_eq!(key1, key3);
    }

    #[cfg_attr(miri, ignore)]
    #[test]
    fn test_cached_executor_lru_eviction() {
        let config = CacheConfig {
            max_entries: 2,
            max_bytes: usize::MAX,
            ttl: None,
            eviction_policy: EvictionPolicy::Lru,
        };

        let executor =
            CachedExecutor::with_cache_config(config).expect("Failed to create executor");

        let wasm1 = wat::parse_str("(module)").expect("Failed to parse WAT");
        let wasm2 = wat::parse_str("(module (func))").expect("Failed to parse WAT");
        let wasm3 = wat::parse_str("(module (func) (func))").expect("Failed to parse WAT");

        // Fill cache
        let _ = executor.compile_module(&wasm1);
        let _ = executor.compile_module(&wasm2);
        assert_eq!(executor.cache_stats().entries, 2);

        // Access wasm1 to make it recently used
        let _ = executor.compile_module(&wasm1);

        // Add wasm3 - should evict wasm2 (LRU)
        let _ = executor.compile_module(&wasm3);

        // Cache should still have 2 entries
        assert_eq!(executor.cache_stats().entries, 2);

        // wasm1 should still be cached (was accessed)
        let stats_before = executor.cache_stats();
        let _ = executor.compile_module(&wasm1);
        let stats_after = executor.cache_stats();
        assert_eq!(stats_after.hits, stats_before.hits + 1);
    }
}
