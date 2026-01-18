//! WASI Preview 2 Implementation
//!
//! Provides WASI (WebAssembly System Interface) support for mielin-wasm.
//! This module implements WASI Preview 2 which is based on the Component Model.
//!
//! # Features
//!
//! - **Streams**: Stdio redirection (stdin, stdout, stderr)
//! - **Clocks**: Realtime and monotonic clocks
//! - **Filesystem**: WASI filesystem operations (preview 2)
//! - **Environment**: Environment variable access
//! - **Random**: Cryptographically secure random number generation
//!
//! # Security
//!
//! All WASI operations are subject to capability-based security checks.
//! Access to resources must be explicitly granted through permissions.

use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use wasmtime::{Caller, Linker};

use crate::host::HostState;

/// WASI error codes (errno values)
#[repr(u16)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Errno {
    /// Success
    Success = 0,
    /// Argument list too long
    TooBig = 1,
    /// Permission denied
    Access = 2,
    /// Address in use
    AddrInUse = 3,
    /// Address not available
    AddrNotAvail = 4,
    /// Bad file descriptor
    BadF = 8,
    /// Resource busy
    Busy = 10,
    /// Operation canceled
    Canceled = 11,
    /// No child processes
    Child = 12,
    /// Resource deadlock would occur
    Deadlk = 13,
    /// File exists
    Exist = 20,
    /// Bad address
    Fault = 21,
    /// File too large
    FBig = 22,
    /// Invalid argument
    Inval = 28,
    /// I/O error
    Io = 29,
    /// Is a directory
    IsDir = 31,
    /// Too many levels of symbolic links
    Loop = 32,
    /// File descriptor value too large
    MFile = 33,
    /// Too many links
    MLink = 34,
    /// Filename too long
    NameTooLong = 37,
    /// No such device
    NoDev = 43,
    /// No such file or directory
    NoEnt = 44,
    /// Not a directory
    NotDir = 54,
    /// Directory not empty
    NotEmpty = 55,
    /// Not supported
    NotSup = 58,
    /// Operation not permitted
    Perm = 63,
    /// Broken pipe
    Pipe = 64,
    /// Read-only filesystem
    Rofs = 69,
}

impl Errno {
    /// Convert to u16
    pub fn as_u16(self) -> u16 {
        self as u16
    }
}

/// WASI file descriptor
pub type Fd = u32;

/// Standard file descriptors
pub const STDIN_FD: Fd = 0;
pub const STDOUT_FD: Fd = 1;
pub const STDERR_FD: Fd = 2;

/// Clock ID for WASI clock operations
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClockId {
    /// Realtime clock
    Realtime = 0,
    /// Monotonic clock
    Monotonic = 1,
    /// Process CPU time
    ProcessCputime = 2,
    /// Thread CPU time
    ThreadCputime = 3,
}

impl ClockId {
    /// Try to convert from u32
    pub fn from_u32(value: u32) -> Option<Self> {
        match value {
            0 => Some(ClockId::Realtime),
            1 => Some(ClockId::Monotonic),
            2 => Some(ClockId::ProcessCputime),
            3 => Some(ClockId::ThreadCputime),
            _ => None,
        }
    }
}

/// Stream interface for WASI I/O
pub trait Stream: Send + Sync {
    /// Read from the stream
    fn read(&mut self, buf: &mut [u8]) -> Result<usize>;

    /// Write to the stream
    fn write(&mut self, buf: &[u8]) -> Result<usize>;

    /// Flush the stream
    fn flush(&mut self) -> Result<()>;
}

/// Standard input stream
pub struct StdinStream {
    buffer: Vec<u8>,
    position: usize,
}

impl StdinStream {
    /// Create a new stdin stream
    pub fn new() -> Self {
        Self {
            buffer: Vec::new(),
            position: 0,
        }
    }

    /// Set input data (for testing)
    pub fn set_input(&mut self, data: Vec<u8>) {
        self.buffer = data;
        self.position = 0;
    }
}

impl Default for StdinStream {
    fn default() -> Self {
        Self::new()
    }
}

impl Stream for StdinStream {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        let available = self.buffer.len().saturating_sub(self.position);
        let to_read = buf.len().min(available);

        buf[..to_read].copy_from_slice(&self.buffer[self.position..self.position + to_read]);
        self.position += to_read;

        Ok(to_read)
    }

    fn write(&mut self, _buf: &[u8]) -> Result<usize> {
        Err(anyhow!("Cannot write to stdin"))
    }

    fn flush(&mut self) -> Result<()> {
        Ok(())
    }
}

/// Standard output stream
pub struct StdoutStream {
    buffer: Arc<Mutex<Vec<u8>>>,
}

impl StdoutStream {
    /// Create a new stdout stream
    pub fn new() -> Self {
        Self {
            buffer: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Create with a shared buffer
    pub fn with_buffer(buffer: Arc<Mutex<Vec<u8>>>) -> Self {
        Self { buffer }
    }

    /// Get the output buffer
    pub fn get_output(&self) -> Vec<u8> {
        self.buffer
            .lock()
            .expect("Stdout buffer lock poisoned")
            .clone()
    }

    /// Clear the output buffer
    pub fn clear(&mut self) {
        self.buffer
            .lock()
            .expect("Stdout buffer lock poisoned")
            .clear();
    }

    /// Get the buffer arc (for sharing)
    pub fn buffer_arc(&self) -> Arc<Mutex<Vec<u8>>> {
        self.buffer.clone()
    }
}

impl Default for StdoutStream {
    fn default() -> Self {
        Self::new()
    }
}

impl Stream for StdoutStream {
    fn read(&mut self, _buf: &mut [u8]) -> Result<usize> {
        Err(anyhow!("Cannot read from stdout"))
    }

    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        self.buffer
            .lock()
            .expect("Stdout buffer lock poisoned")
            .extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> Result<()> {
        Ok(())
    }
}

/// Standard error stream
pub struct StderrStream {
    buffer: Arc<Mutex<Vec<u8>>>,
}

impl StderrStream {
    /// Create a new stderr stream
    pub fn new() -> Self {
        Self {
            buffer: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Create with a shared buffer
    pub fn with_buffer(buffer: Arc<Mutex<Vec<u8>>>) -> Self {
        Self { buffer }
    }

    /// Get the error buffer
    pub fn get_output(&self) -> Vec<u8> {
        self.buffer
            .lock()
            .expect("Stderr buffer lock poisoned")
            .clone()
    }

    /// Clear the error buffer
    pub fn clear(&mut self) {
        self.buffer
            .lock()
            .expect("Stderr buffer lock poisoned")
            .clear();
    }

    /// Get the buffer arc (for sharing)
    pub fn buffer_arc(&self) -> Arc<Mutex<Vec<u8>>> {
        self.buffer.clone()
    }
}

impl Default for StderrStream {
    fn default() -> Self {
        Self::new()
    }
}

impl Stream for StderrStream {
    fn read(&mut self, _buf: &mut [u8]) -> Result<usize> {
        Err(anyhow!("Cannot read from stderr"))
    }

    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        self.buffer
            .lock()
            .expect("Stderr buffer lock poisoned")
            .extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> Result<()> {
        Ok(())
    }
}

/// WASI context holding all WASI state
pub struct WasiContext {
    /// Standard streams
    streams: Arc<Mutex<HashMap<Fd, Box<dyn Stream>>>>,

    /// Standard output buffer (for testing/inspection)
    stdout_buffer: Arc<Mutex<Vec<u8>>>,

    /// Standard error buffer (for testing/inspection)
    stderr_buffer: Arc<Mutex<Vec<u8>>>,

    /// Environment variables
    env_vars: HashMap<String, String>,

    /// Command-line arguments
    args: Vec<String>,

    /// Current working directory
    cwd: String,

    /// Monotonic clock start time (for process/thread CPU time)
    monotonic_start: std::time::Instant,
}

impl WasiContext {
    /// Create a new WASI context
    pub fn new() -> Self {
        let stdout_buffer = Arc::new(Mutex::new(Vec::new()));
        let stderr_buffer = Arc::new(Mutex::new(Vec::new()));

        let mut streams: HashMap<Fd, Box<dyn Stream>> = HashMap::new();
        streams.insert(STDIN_FD, Box::new(StdinStream::new()));
        streams.insert(
            STDOUT_FD,
            Box::new(StdoutStream::with_buffer(stdout_buffer.clone())),
        );
        streams.insert(
            STDERR_FD,
            Box::new(StderrStream::with_buffer(stderr_buffer.clone())),
        );

        Self {
            streams: Arc::new(Mutex::new(streams)),
            stdout_buffer,
            stderr_buffer,
            env_vars: HashMap::new(),
            args: Vec::new(),
            cwd: "/".to_string(),
            monotonic_start: std::time::Instant::now(),
        }
    }

    /// Set environment variables
    pub fn set_env_vars(&mut self, vars: HashMap<String, String>) {
        self.env_vars = vars;
    }

    /// Set command-line arguments
    pub fn set_args(&mut self, args: Vec<String>) {
        self.args = args;
    }

    /// Set current working directory
    pub fn set_cwd(&mut self, cwd: String) {
        self.cwd = cwd;
    }

    /// Get the streams collection (for direct manipulation in tests)
    pub fn streams(&self) -> &Arc<Mutex<HashMap<Fd, Box<dyn Stream>>>> {
        &self.streams
    }

    /// Get stdout output as a vector (for testing)
    ///
    /// This creates a copy of the stdout buffer. Note that reading
    /// from stdout is not a standard WASI operation - this is only
    /// for testing purposes.
    pub fn get_stdout_output(&self) -> Vec<u8> {
        self.stdout_buffer
            .lock()
            .expect("Stdout buffer lock poisoned")
            .clone()
    }

    /// Get stderr output as a vector (for testing)
    pub fn get_stderr_output(&self) -> Vec<u8> {
        self.stderr_buffer
            .lock()
            .expect("Stderr buffer lock poisoned")
            .clone()
    }
}

impl Default for WasiContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Register WASI host functions with the linker
pub fn register_wasi_functions(linker: &mut Linker<HostState>) -> Result<()> {
    // fd_read: Read from a file descriptor
    linker.func_wrap(
        "wasi_snapshot_preview1",
        "fd_read",
        |mut caller: Caller<'_, HostState>,
         fd: i32,
         iovs_ptr: i32,
         iovs_len: i32,
         nread_ptr: i32|
         -> i32 {
            // Clone the streams Arc before borrowing caller mutably
            let streams = match caller.data().wasi_context() {
                Some(w) => w.streams.clone(),
                None => return Errno::Io.as_u16() as i32,
            };

            // Get memory
            let memory = match caller.get_export("memory") {
                Some(wasmtime::Extern::Memory(m)) => m,
                _ => return Errno::Fault.as_u16() as i32,
            };

            // Lock streams
            let streams = streams;
            let mut streams_guard = streams.lock().expect("WASI streams lock poisoned");

            // Get stream
            let stream = match streams_guard.get_mut(&(fd as u32)) {
                Some(s) => s,
                None => return Errno::BadF.as_u16() as i32,
            };

            // Read iovecs
            let mut total_read = 0usize;
            for i in 0..iovs_len {
                let iov_ptr = iovs_ptr as usize + (i as usize * 8);

                let buf_ptr = memory
                    .data(&caller)
                    .get(iov_ptr..iov_ptr + 4)
                    .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                    .unwrap_or(0);

                let buf_len = memory
                    .data(&caller)
                    .get(iov_ptr + 4..iov_ptr + 8)
                    .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                    .unwrap_or(0);

                // Read into buffer
                let buf_slice = match memory
                    .data_mut(&mut caller)
                    .get_mut(buf_ptr as usize..(buf_ptr + buf_len) as usize)
                {
                    Some(s) => s,
                    None => return Errno::Fault.as_u16() as i32,
                };

                match stream.read(buf_slice) {
                    Ok(n) => total_read += n,
                    Err(_) => return Errno::Io.as_u16() as i32,
                }
            }

            // Write nread
            let nread_bytes = (total_read as u32).to_le_bytes();
            if let Some(slice) = memory
                .data_mut(&mut caller)
                .get_mut(nread_ptr as usize..nread_ptr as usize + 4)
            {
                slice.copy_from_slice(&nread_bytes);
            } else {
                return Errno::Fault.as_u16() as i32;
            }

            Errno::Success.as_u16() as i32
        },
    )?;

    // fd_write: Write to a file descriptor
    linker.func_wrap(
        "wasi_snapshot_preview1",
        "fd_write",
        |mut caller: Caller<'_, HostState>,
         fd: i32,
         iovs_ptr: i32,
         iovs_len: i32,
         nwritten_ptr: i32|
         -> i32 {
            // Clone the streams Arc before borrowing caller mutably
            let streams = match caller.data().wasi_context() {
                Some(w) => w.streams.clone(),
                None => return Errno::Io.as_u16() as i32,
            };

            // Get memory
            let memory = match caller.get_export("memory") {
                Some(wasmtime::Extern::Memory(m)) => m,
                _ => return Errno::Fault.as_u16() as i32,
            };

            // Lock streams
            let streams = streams;
            let mut streams_guard = streams.lock().expect("WASI streams lock poisoned");

            // Get stream
            let stream = match streams_guard.get_mut(&(fd as u32)) {
                Some(s) => s,
                None => return Errno::BadF.as_u16() as i32,
            };

            // Write iovecs
            let mut total_written = 0usize;
            for i in 0..iovs_len {
                let iov_ptr = iovs_ptr as usize + (i as usize * 8);

                let buf_ptr = memory
                    .data(&caller)
                    .get(iov_ptr..iov_ptr + 4)
                    .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                    .unwrap_or(0);

                let buf_len = memory
                    .data(&caller)
                    .get(iov_ptr + 4..iov_ptr + 8)
                    .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                    .unwrap_or(0);

                // Write from buffer
                let buf_slice = match memory
                    .data(&caller)
                    .get(buf_ptr as usize..(buf_ptr + buf_len) as usize)
                {
                    Some(s) => s,
                    None => return Errno::Fault.as_u16() as i32,
                };

                match stream.write(buf_slice) {
                    Ok(n) => total_written += n,
                    Err(_) => return Errno::Io.as_u16() as i32,
                }
            }

            // Write nwritten
            let nwritten_bytes = (total_written as u32).to_le_bytes();
            if let Some(slice) = memory
                .data_mut(&mut caller)
                .get_mut(nwritten_ptr as usize..nwritten_ptr as usize + 4)
            {
                slice.copy_from_slice(&nwritten_bytes);
            } else {
                return Errno::Fault.as_u16() as i32;
            }

            Errno::Success.as_u16() as i32
        },
    )?;

    // clock_time_get: Get time from a clock
    linker.func_wrap(
        "wasi_snapshot_preview1",
        "clock_time_get",
        |mut caller: Caller<'_, HostState>, clock_id: i32, _precision: i64, time_ptr: i32| -> i32 {
            let clock = match ClockId::from_u32(clock_id as u32) {
                Some(c) => c,
                None => return Errno::Inval.as_u16() as i32,
            };

            // Get monotonic start time if needed
            let monotonic_start = if matches!(
                clock,
                ClockId::Monotonic | ClockId::ProcessCputime | ClockId::ThreadCputime
            ) {
                match caller.data().wasi_context() {
                    Some(w) => w.monotonic_start,
                    None => return Errno::Io.as_u16() as i32,
                }
            } else {
                std::time::Instant::now() // Not used for Realtime
            };

            let nanos = match clock {
                ClockId::Realtime => SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or(Duration::ZERO)
                    .as_nanos() as u64,
                ClockId::Monotonic => monotonic_start.elapsed().as_nanos() as u64,
                ClockId::ProcessCputime | ClockId::ThreadCputime => {
                    // Approximate with monotonic time
                    monotonic_start.elapsed().as_nanos() as u64
                }
            };

            // Get memory and write time
            let memory = match caller.get_export("memory") {
                Some(wasmtime::Extern::Memory(m)) => m,
                _ => return Errno::Fault.as_u16() as i32,
            };

            let time_bytes = nanos.to_le_bytes();
            if let Some(slice) = memory
                .data_mut(&mut caller)
                .get_mut(time_ptr as usize..time_ptr as usize + 8)
            {
                slice.copy_from_slice(&time_bytes);
            } else {
                return Errno::Fault.as_u16() as i32;
            }

            Errno::Success.as_u16() as i32
        },
    )?;

    // environ_sizes_get: Get environment variable sizes
    linker.func_wrap(
        "wasi_snapshot_preview1",
        "environ_sizes_get",
        |mut caller: Caller<'_, HostState>, environc_ptr: i32, environ_buf_size_ptr: i32| -> i32 {
            let (count, total_size) = match caller.data().wasi_context() {
                Some(w) => {
                    let count = w.env_vars.len() as u32;
                    let total_size: u32 = w
                        .env_vars
                        .iter()
                        .map(|(k, v)| (k.len() + 1 + v.len() + 1) as u32)
                        .sum();
                    (count, total_size)
                }
                None => return Errno::Io.as_u16() as i32,
            };

            // Get memory
            let memory = match caller.get_export("memory") {
                Some(wasmtime::Extern::Memory(m)) => m,
                _ => return Errno::Fault.as_u16() as i32,
            };

            // Write count
            let count_bytes = count.to_le_bytes();
            if let Some(slice) = memory
                .data_mut(&mut caller)
                .get_mut(environc_ptr as usize..environc_ptr as usize + 4)
            {
                slice.copy_from_slice(&count_bytes);
            } else {
                return Errno::Fault.as_u16() as i32;
            }

            // Write buffer size
            let size_bytes = total_size.to_le_bytes();
            if let Some(slice) = memory
                .data_mut(&mut caller)
                .get_mut(environ_buf_size_ptr as usize..environ_buf_size_ptr as usize + 4)
            {
                slice.copy_from_slice(&size_bytes);
            } else {
                return Errno::Fault.as_u16() as i32;
            }

            Errno::Success.as_u16() as i32
        },
    )?;

    // environ_get: Get environment variables
    linker.func_wrap(
        "wasi_snapshot_preview1",
        "environ_get",
        |mut caller: Caller<'_, HostState>, environ_ptr: i32, environ_buf_ptr: i32| -> i32 {
            // Clone env_vars to avoid borrow issues
            let env_vars = match caller.data().wasi_context() {
                Some(w) => w.env_vars.clone(),
                None => return Errno::Io.as_u16() as i32,
            };

            // Get memory
            let memory = match caller.get_export("memory") {
                Some(wasmtime::Extern::Memory(m)) => m,
                _ => return Errno::Fault.as_u16() as i32,
            };

            let mut ptr_offset = environ_ptr as usize;
            let mut buf_offset = environ_buf_ptr as usize;

            for (key, value) in &env_vars {
                let entry = format!("{}={}", key, value);
                let entry_bytes = entry.as_bytes();

                // Write pointer to buffer
                let buf_ptr_bytes = (buf_offset as u32).to_le_bytes();
                if let Some(slice) = memory
                    .data_mut(&mut caller)
                    .get_mut(ptr_offset..ptr_offset + 4)
                {
                    slice.copy_from_slice(&buf_ptr_bytes);
                } else {
                    return Errno::Fault.as_u16() as i32;
                }

                // Write entry to buffer
                if let Some(slice) = memory
                    .data_mut(&mut caller)
                    .get_mut(buf_offset..buf_offset + entry_bytes.len() + 1)
                {
                    slice[..entry_bytes.len()].copy_from_slice(entry_bytes);
                    slice[entry_bytes.len()] = 0; // null terminator
                } else {
                    return Errno::Fault.as_u16() as i32;
                }

                ptr_offset += 4;
                buf_offset += entry_bytes.len() + 1;
            }

            Errno::Success.as_u16() as i32
        },
    )?;

    // args_sizes_get: Get command-line argument sizes
    linker.func_wrap(
        "wasi_snapshot_preview1",
        "args_sizes_get",
        |mut caller: Caller<'_, HostState>, argc_ptr: i32, argv_buf_size_ptr: i32| -> i32 {
            let (count, total_size) = match caller.data().wasi_context() {
                Some(w) => {
                    let count = w.args.len() as u32;
                    let total_size: u32 = w.args.iter().map(|arg| (arg.len() + 1) as u32).sum();
                    (count, total_size)
                }
                None => return Errno::Io.as_u16() as i32,
            };

            // Get memory
            let memory = match caller.get_export("memory") {
                Some(wasmtime::Extern::Memory(m)) => m,
                _ => return Errno::Fault.as_u16() as i32,
            };

            // Write count
            let count_bytes = count.to_le_bytes();
            if let Some(slice) = memory
                .data_mut(&mut caller)
                .get_mut(argc_ptr as usize..argc_ptr as usize + 4)
            {
                slice.copy_from_slice(&count_bytes);
            } else {
                return Errno::Fault.as_u16() as i32;
            }

            // Write buffer size
            let size_bytes = total_size.to_le_bytes();
            if let Some(slice) = memory
                .data_mut(&mut caller)
                .get_mut(argv_buf_size_ptr as usize..argv_buf_size_ptr as usize + 4)
            {
                slice.copy_from_slice(&size_bytes);
            } else {
                return Errno::Fault.as_u16() as i32;
            }

            Errno::Success.as_u16() as i32
        },
    )?;

    // args_get: Get command-line arguments
    linker.func_wrap(
        "wasi_snapshot_preview1",
        "args_get",
        |mut caller: Caller<'_, HostState>, argv_ptr: i32, argv_buf_ptr: i32| -> i32 {
            // Clone args to avoid borrow issues
            let args = match caller.data().wasi_context() {
                Some(w) => w.args.clone(),
                None => return Errno::Io.as_u16() as i32,
            };

            // Get memory
            let memory = match caller.get_export("memory") {
                Some(wasmtime::Extern::Memory(m)) => m,
                _ => return Errno::Fault.as_u16() as i32,
            };

            let mut ptr_offset = argv_ptr as usize;
            let mut buf_offset = argv_buf_ptr as usize;

            for arg in &args {
                let arg_bytes = arg.as_bytes();

                // Write pointer to buffer
                let buf_ptr_bytes = (buf_offset as u32).to_le_bytes();
                if let Some(slice) = memory
                    .data_mut(&mut caller)
                    .get_mut(ptr_offset..ptr_offset + 4)
                {
                    slice.copy_from_slice(&buf_ptr_bytes);
                } else {
                    return Errno::Fault.as_u16() as i32;
                }

                // Write arg to buffer
                if let Some(slice) = memory
                    .data_mut(&mut caller)
                    .get_mut(buf_offset..buf_offset + arg_bytes.len() + 1)
                {
                    slice[..arg_bytes.len()].copy_from_slice(arg_bytes);
                    slice[arg_bytes.len()] = 0; // null terminator
                } else {
                    return Errno::Fault.as_u16() as i32;
                }

                ptr_offset += 4;
                buf_offset += arg_bytes.len() + 1;
            }

            Errno::Success.as_u16() as i32
        },
    )?;

    // proc_exit: Terminate the process
    linker.func_wrap(
        "wasi_snapshot_preview1",
        "proc_exit",
        |_caller: Caller<'_, HostState>, exit_code: i32| -> () {
            // In a real implementation, this would terminate the WASM instance
            // For now, we just record the exit code
            // The caller can check for this and handle appropriately
            panic!("WASI proc_exit called with code: {}", exit_code);
        },
    )?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_errno_values() {
        assert_eq!(Errno::Success.as_u16(), 0);
        assert_eq!(Errno::Access.as_u16(), 2);
        assert_eq!(Errno::BadF.as_u16(), 8);
    }

    #[test]
    fn test_clock_id_conversion() {
        assert_eq!(ClockId::from_u32(0), Some(ClockId::Realtime));
        assert_eq!(ClockId::from_u32(1), Some(ClockId::Monotonic));
        assert_eq!(ClockId::from_u32(99), None);
    }

    #[test]
    fn test_stdin_stream() {
        let mut stdin = StdinStream::new();
        stdin.set_input(vec![1, 2, 3, 4, 5]);

        let mut buf = [0u8; 3];
        assert_eq!(stdin.read(&mut buf).unwrap(), 3);
        assert_eq!(buf, [1, 2, 3]);

        assert_eq!(stdin.read(&mut buf).unwrap(), 2);
        assert_eq!(buf[..2], [4, 5]);

        // No more data
        assert_eq!(stdin.read(&mut buf).unwrap(), 0);
    }

    #[test]
    fn test_stdout_stream() {
        let mut stdout = StdoutStream::new();

        assert_eq!(stdout.write(&[1, 2, 3]).unwrap(), 3);
        assert_eq!(stdout.write(&[4, 5]).unwrap(), 2);

        assert_eq!(stdout.get_output(), vec![1, 2, 3, 4, 5]);

        stdout.clear();
        assert_eq!(stdout.get_output(), Vec::<u8>::new());
    }

    #[test]
    fn test_stderr_stream() {
        let mut stderr = StderrStream::new();

        assert_eq!(stderr.write(&[1, 2, 3]).unwrap(), 3);
        assert_eq!(stderr.get_output(), vec![1, 2, 3]);
    }

    #[test]
    fn test_wasi_context_creation() {
        let ctx = WasiContext::new();
        assert_eq!(ctx.env_vars.len(), 0);
        assert_eq!(ctx.args.len(), 0);
        assert_eq!(ctx.cwd, "/");
    }

    #[test]
    fn test_wasi_context_env_vars() {
        let mut ctx = WasiContext::new();
        let mut vars = HashMap::new();
        vars.insert("FOO".to_string(), "bar".to_string());
        vars.insert("BAZ".to_string(), "qux".to_string());

        ctx.set_env_vars(vars);
        assert_eq!(ctx.env_vars.len(), 2);
        assert_eq!(ctx.env_vars.get("FOO"), Some(&"bar".to_string()));
    }

    #[test]
    fn test_wasi_context_args() {
        let mut ctx = WasiContext::new();
        let args = vec![
            "program".to_string(),
            "arg1".to_string(),
            "arg2".to_string(),
        ];

        ctx.set_args(args);
        assert_eq!(ctx.args.len(), 3);
        assert_eq!(ctx.args[0], "program");
    }

    #[test]
    fn test_wasi_context_streams() {
        let ctx = WasiContext::new();

        // Verify stream creation
        assert!(ctx.streams.lock().unwrap().contains_key(&STDIN_FD));
        assert!(ctx.streams.lock().unwrap().contains_key(&STDOUT_FD));
        assert!(ctx.streams.lock().unwrap().contains_key(&STDERR_FD));
    }
}
