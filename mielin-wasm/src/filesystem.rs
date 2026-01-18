//! Filesystem Host Functions for WASM Agents
//!
//! Provides sandboxed filesystem access with capability-based security.
//!
//! ## Features
//!
//! - Read/write file operations with size limits
//! - Directory listing and creation
//! - Path sanitization to prevent directory traversal
//! - Capability-based access control
//! - Per-agent isolated filesystem roots
//! - File descriptor management

use std::collections::{BTreeMap, HashSet};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

/// Filesystem error codes returned to WASM
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum FsError {
    /// Operation succeeded
    Success = 0,
    /// Permission denied (capability not granted)
    PermissionDenied = -1,
    /// File not found
    NotFound = -2,
    /// Path is invalid or contains traversal
    InvalidPath = -3,
    /// File already exists
    AlreadyExists = -4,
    /// Not a directory
    NotADirectory = -5,
    /// Not a file
    NotAFile = -6,
    /// Directory not empty
    DirectoryNotEmpty = -7,
    /// I/O error
    IoError = -8,
    /// Invalid file descriptor
    InvalidFd = -9,
    /// File too large
    FileTooLarge = -10,
    /// Buffer too small
    BufferTooSmall = -11,
    /// Read-only filesystem
    ReadOnly = -12,
    /// Invalid argument
    InvalidArgument = -13,
    /// Too many open files
    TooManyOpenFiles = -14,
}

impl From<std::io::Error> for FsError {
    fn from(err: std::io::Error) -> Self {
        use std::io::ErrorKind;
        match err.kind() {
            ErrorKind::NotFound => FsError::NotFound,
            ErrorKind::PermissionDenied => FsError::PermissionDenied,
            ErrorKind::AlreadyExists => FsError::AlreadyExists,
            ErrorKind::InvalidInput => FsError::InvalidArgument,
            _ => FsError::IoError,
        }
    }
}

/// File open mode flags
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenFlags(u32);

impl OpenFlags {
    /// Read access
    pub const READ: u32 = 1 << 0;
    /// Write access
    pub const WRITE: u32 = 1 << 1;
    /// Create if doesn't exist
    pub const CREATE: u32 = 1 << 2;
    /// Truncate existing file
    pub const TRUNCATE: u32 = 1 << 3;
    /// Append mode
    pub const APPEND: u32 = 1 << 4;
    /// Exclusive create (fail if exists)
    pub const EXCLUSIVE: u32 = 1 << 5;

    pub fn new(flags: u32) -> Self {
        Self(flags)
    }

    pub fn has_read(&self) -> bool {
        self.0 & Self::READ != 0
    }

    pub fn has_write(&self) -> bool {
        self.0 & Self::WRITE != 0
    }

    pub fn has_create(&self) -> bool {
        self.0 & Self::CREATE != 0
    }

    pub fn has_truncate(&self) -> bool {
        self.0 & Self::TRUNCATE != 0
    }

    pub fn has_append(&self) -> bool {
        self.0 & Self::APPEND != 0
    }

    pub fn has_exclusive(&self) -> bool {
        self.0 & Self::EXCLUSIVE != 0
    }
}

/// Seek origin for file positioning
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum SeekOrigin {
    /// From beginning of file
    Start = 0,
    /// From current position
    Current = 1,
    /// From end of file
    End = 2,
}

impl SeekOrigin {
    pub fn from_u32(value: u32) -> Option<Self> {
        match value {
            0 => Some(SeekOrigin::Start),
            1 => Some(SeekOrigin::Current),
            2 => Some(SeekOrigin::End),
            _ => None,
        }
    }
}

/// File type enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum FileType {
    /// Regular file
    File = 1,
    /// Directory
    Directory = 2,
    /// Symbolic link
    Symlink = 3,
    /// Unknown type
    Unknown = 0,
}

/// File metadata/stat information
#[derive(Debug, Clone)]
pub struct FileStat {
    /// File size in bytes
    pub size: u64,
    /// File type
    pub file_type: FileType,
    /// Creation time (Unix timestamp)
    pub created: u64,
    /// Modification time (Unix timestamp)
    pub modified: u64,
    /// Access time (Unix timestamp)
    pub accessed: u64,
    /// Is read-only
    pub readonly: bool,
}

impl FileStat {
    /// Serialize to bytes for WASM (40 bytes total)
    pub fn to_bytes(&self) -> [u8; 40] {
        let mut bytes = [0u8; 40];
        bytes[0..8].copy_from_slice(&self.size.to_le_bytes());
        bytes[8..12].copy_from_slice(&(self.file_type as u32).to_le_bytes());
        bytes[12..20].copy_from_slice(&self.created.to_le_bytes());
        bytes[20..28].copy_from_slice(&self.modified.to_le_bytes());
        bytes[28..36].copy_from_slice(&self.accessed.to_le_bytes());
        bytes[36..40].copy_from_slice(&(self.readonly as u32).to_le_bytes());
        bytes
    }
}

/// Directory entry
#[derive(Debug, Clone)]
pub struct DirEntry {
    /// Entry name (without path)
    pub name: String,
    /// File type
    pub file_type: FileType,
    /// File size
    pub size: u64,
}

impl DirEntry {
    /// Serialize to bytes (variable length)
    pub fn to_bytes(&self) -> Vec<u8> {
        let name_bytes = self.name.as_bytes();
        let mut bytes = Vec::with_capacity(16 + name_bytes.len());

        bytes.extend_from_slice(&(name_bytes.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&(self.file_type as u32).to_le_bytes());
        bytes.extend_from_slice(&self.size.to_le_bytes());
        bytes.extend_from_slice(name_bytes);

        bytes
    }
}

/// Open file handle
struct OpenFile {
    /// The underlying file
    file: std::fs::File,
    /// Path (for debugging and operations)
    #[allow(dead_code)]
    path: PathBuf,
    /// Open flags
    flags: OpenFlags,
    /// Current position
    position: u64,
}

/// Filesystem capability permissions
#[derive(Debug, Clone)]
pub struct FsPermissions {
    /// Can read files
    pub can_read: bool,
    /// Can write files
    pub can_write: bool,
    /// Can create files
    pub can_create: bool,
    /// Can delete files
    pub can_delete: bool,
    /// Can list directories
    pub can_list: bool,
    /// Maximum file size (0 = unlimited)
    pub max_file_size: u64,
    /// Maximum total storage (0 = unlimited)
    pub max_total_size: u64,
    /// Allowed path patterns (empty = all allowed)
    pub allowed_patterns: HashSet<String>,
}

impl Default for FsPermissions {
    fn default() -> Self {
        Self {
            can_read: true,
            can_write: true,
            can_create: true,
            can_delete: true,
            can_list: true,
            max_file_size: 100 * 1024 * 1024,   // 100MB default
            max_total_size: 1024 * 1024 * 1024, // 1GB default
            allowed_patterns: HashSet::new(),
        }
    }
}

impl FsPermissions {
    /// Read-only permissions
    pub fn read_only() -> Self {
        Self {
            can_read: true,
            can_write: false,
            can_create: false,
            can_delete: false,
            can_list: true,
            max_file_size: 0,
            max_total_size: 0,
            allowed_patterns: HashSet::new(),
        }
    }

    /// Full access
    pub fn full_access() -> Self {
        Self::default()
    }

    /// Minimal (list only)
    pub fn list_only() -> Self {
        Self {
            can_read: false,
            can_write: false,
            can_create: false,
            can_delete: false,
            can_list: true,
            max_file_size: 0,
            max_total_size: 0,
            allowed_patterns: HashSet::new(),
        }
    }
}

/// Filesystem configuration
#[derive(Debug, Clone)]
pub struct FsConfig {
    /// Root directory for this filesystem sandbox
    pub root: PathBuf,
    /// Maximum number of open files
    pub max_open_files: usize,
    /// Maximum path length
    pub max_path_length: usize,
    /// Permissions
    pub permissions: FsPermissions,
}

impl Default for FsConfig {
    fn default() -> Self {
        Self {
            root: PathBuf::from("/tmp/mielin-sandbox"),
            max_open_files: 64,
            max_path_length: 4096,
            permissions: FsPermissions::default(),
        }
    }
}

impl FsConfig {
    /// Create config with specific root
    pub fn with_root(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            ..Default::default()
        }
    }

    /// Set permissions
    pub fn with_permissions(mut self, permissions: FsPermissions) -> Self {
        self.permissions = permissions;
        self
    }
}

/// Sandboxed filesystem state
pub struct FilesystemState {
    /// Configuration
    config: FsConfig,
    /// Open file handles
    open_files: BTreeMap<u32, OpenFile>,
    /// Next file descriptor
    next_fd: AtomicU32,
    /// Total bytes written (for quota tracking)
    total_written: u64,
}

impl FilesystemState {
    /// Create a new filesystem state
    pub fn new(config: FsConfig) -> Self {
        Self {
            config,
            open_files: BTreeMap::new(),
            next_fd: AtomicU32::new(3), // 0, 1, 2 reserved for stdin/out/err
            total_written: 0,
        }
    }

    /// Get configuration
    pub fn config(&self) -> &FsConfig {
        &self.config
    }

    /// Sanitize and resolve a path within the sandbox
    pub fn resolve_path(&self, path: &str) -> Result<PathBuf, FsError> {
        // Check path length
        if path.len() > self.config.max_path_length {
            return Err(FsError::InvalidPath);
        }

        // Parse the path
        let path = Path::new(path);

        // Build resolved path, rejecting any traversal attempts
        let mut resolved = self.config.root.clone();

        for component in path.components() {
            match component {
                Component::Normal(name) => {
                    // Check for hidden traversal in name
                    let name_str = name.to_string_lossy();
                    if name_str.contains('\0') || name_str == "." || name_str == ".." {
                        return Err(FsError::InvalidPath);
                    }
                    resolved.push(name);
                }
                Component::CurDir => {
                    // Allow "." but don't add it
                }
                Component::ParentDir => {
                    // Reject ".." - no directory traversal
                    return Err(FsError::InvalidPath);
                }
                Component::RootDir => {
                    // Ignore leading "/" - we're always relative to sandbox root
                }
                Component::Prefix(_) => {
                    // Windows-specific, reject
                    return Err(FsError::InvalidPath);
                }
            }
        }

        // Verify the resolved path is within sandbox root
        if !resolved.starts_with(&self.config.root) {
            return Err(FsError::InvalidPath);
        }

        Ok(resolved)
    }

    /// Open a file
    pub fn open(&mut self, path: &str, flags: OpenFlags) -> Result<u32, FsError> {
        // Check permissions
        if flags.has_read() && !self.config.permissions.can_read {
            return Err(FsError::PermissionDenied);
        }
        if flags.has_write() && !self.config.permissions.can_write {
            return Err(FsError::PermissionDenied);
        }
        if flags.has_create() && !self.config.permissions.can_create {
            return Err(FsError::PermissionDenied);
        }

        // Check max open files
        if self.open_files.len() >= self.config.max_open_files {
            return Err(FsError::TooManyOpenFiles);
        }

        let resolved = self.resolve_path(path)?;

        // Build file open options
        let mut options = std::fs::OpenOptions::new();
        options.read(flags.has_read());
        options.write(flags.has_write());
        options.create(flags.has_create());
        options.truncate(flags.has_truncate());
        options.append(flags.has_append());

        if flags.has_exclusive() {
            options.create_new(true);
        }

        // Ensure parent directory exists if creating
        if flags.has_create() {
            if let Some(parent) = resolved.parent() {
                std::fs::create_dir_all(parent).map_err(FsError::from)?;
            }
        }

        let file = options.open(&resolved).map_err(FsError::from)?;

        let fd = self.next_fd.fetch_add(1, Ordering::Relaxed);
        self.open_files.insert(
            fd,
            OpenFile {
                file,
                path: resolved,
                flags,
                position: 0,
            },
        );

        Ok(fd)
    }

    /// Close a file
    pub fn close(&mut self, fd: u32) -> Result<(), FsError> {
        self.open_files.remove(&fd).ok_or(FsError::InvalidFd)?;
        Ok(())
    }

    /// Read from a file
    pub fn read(&mut self, fd: u32, buffer: &mut [u8]) -> Result<usize, FsError> {
        let file = self.open_files.get_mut(&fd).ok_or(FsError::InvalidFd)?;

        if !file.flags.has_read() {
            return Err(FsError::PermissionDenied);
        }

        let bytes_read = file.file.read(buffer).map_err(FsError::from)?;
        file.position += bytes_read as u64;

        Ok(bytes_read)
    }

    /// Write to a file
    pub fn write(&mut self, fd: u32, data: &[u8]) -> Result<usize, FsError> {
        // Check file size limits
        if self.config.permissions.max_file_size > 0 {
            let file = self.open_files.get(&fd).ok_or(FsError::InvalidFd)?;
            let new_size = file.position + data.len() as u64;
            if new_size > self.config.permissions.max_file_size {
                return Err(FsError::FileTooLarge);
            }
        }

        // Check total quota
        if self.config.permissions.max_total_size > 0
            && self.total_written + data.len() as u64 > self.config.permissions.max_total_size
        {
            return Err(FsError::FileTooLarge);
        }

        let file = self.open_files.get_mut(&fd).ok_or(FsError::InvalidFd)?;

        if !file.flags.has_write() {
            return Err(FsError::PermissionDenied);
        }

        let bytes_written = file.file.write(data).map_err(FsError::from)?;
        file.position += bytes_written as u64;
        self.total_written += bytes_written as u64;

        Ok(bytes_written)
    }

    /// Seek in a file
    pub fn seek(&mut self, fd: u32, offset: i64, origin: SeekOrigin) -> Result<u64, FsError> {
        let file = self.open_files.get_mut(&fd).ok_or(FsError::InvalidFd)?;

        let seek_from = match origin {
            SeekOrigin::Start => SeekFrom::Start(offset as u64),
            SeekOrigin::Current => SeekFrom::Current(offset),
            SeekOrigin::End => SeekFrom::End(offset),
        };

        let new_pos = file.file.seek(seek_from).map_err(FsError::from)?;
        file.position = new_pos;

        Ok(new_pos)
    }

    /// Get file size
    pub fn file_size(&self, fd: u32) -> Result<u64, FsError> {
        let file = self.open_files.get(&fd).ok_or(FsError::InvalidFd)?;
        let metadata = file.file.metadata().map_err(FsError::from)?;
        Ok(metadata.len())
    }

    /// Flush file buffers
    pub fn flush(&mut self, fd: u32) -> Result<(), FsError> {
        let file = self.open_files.get_mut(&fd).ok_or(FsError::InvalidFd)?;
        file.file.flush().map_err(FsError::from)?;
        Ok(())
    }

    /// Get file stat
    pub fn stat(&self, path: &str) -> Result<FileStat, FsError> {
        if !self.config.permissions.can_read {
            return Err(FsError::PermissionDenied);
        }

        let resolved = self.resolve_path(path)?;
        let metadata = std::fs::metadata(&resolved).map_err(FsError::from)?;

        let file_type = if metadata.is_file() {
            FileType::File
        } else if metadata.is_dir() {
            FileType::Directory
        } else if metadata.file_type().is_symlink() {
            FileType::Symlink
        } else {
            FileType::Unknown
        };

        let created = metadata
            .created()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let modified = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let accessed = metadata
            .accessed()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);

        Ok(FileStat {
            size: metadata.len(),
            file_type,
            created,
            modified,
            accessed,
            readonly: metadata.permissions().readonly(),
        })
    }

    /// List directory contents
    pub fn read_dir(&self, path: &str) -> Result<Vec<DirEntry>, FsError> {
        if !self.config.permissions.can_list {
            return Err(FsError::PermissionDenied);
        }

        let resolved = self.resolve_path(path)?;

        if !resolved.is_dir() {
            return Err(FsError::NotADirectory);
        }

        let entries = std::fs::read_dir(&resolved).map_err(FsError::from)?;

        let mut result = Vec::new();
        for entry in entries {
            let entry = entry.map_err(FsError::from)?;
            let metadata = entry.metadata().map_err(FsError::from)?;

            let file_type = if metadata.is_file() {
                FileType::File
            } else if metadata.is_dir() {
                FileType::Directory
            } else {
                FileType::Unknown
            };

            result.push(DirEntry {
                name: entry.file_name().to_string_lossy().to_string(),
                file_type,
                size: metadata.len(),
            });
        }

        Ok(result)
    }

    /// Create a directory
    pub fn create_dir(&self, path: &str) -> Result<(), FsError> {
        if !self.config.permissions.can_create {
            return Err(FsError::PermissionDenied);
        }

        let resolved = self.resolve_path(path)?;
        std::fs::create_dir_all(&resolved).map_err(FsError::from)?;

        Ok(())
    }

    /// Remove a file
    pub fn remove_file(&self, path: &str) -> Result<(), FsError> {
        if !self.config.permissions.can_delete {
            return Err(FsError::PermissionDenied);
        }

        let resolved = self.resolve_path(path)?;

        if resolved.is_dir() {
            return Err(FsError::NotAFile);
        }

        std::fs::remove_file(&resolved).map_err(FsError::from)?;

        Ok(())
    }

    /// Remove a directory (must be empty)
    pub fn remove_dir(&self, path: &str) -> Result<(), FsError> {
        if !self.config.permissions.can_delete {
            return Err(FsError::PermissionDenied);
        }

        let resolved = self.resolve_path(path)?;

        if !resolved.is_dir() {
            return Err(FsError::NotADirectory);
        }

        std::fs::remove_dir(&resolved).map_err(|e| {
            if e.kind() == std::io::ErrorKind::Other {
                FsError::DirectoryNotEmpty
            } else {
                FsError::from(e)
            }
        })?;

        Ok(())
    }

    /// Check if path exists
    pub fn exists(&self, path: &str) -> Result<bool, FsError> {
        let resolved = self.resolve_path(path)?;
        Ok(resolved.exists())
    }

    /// Rename/move a file
    pub fn rename(&self, from: &str, to: &str) -> Result<(), FsError> {
        if !self.config.permissions.can_write {
            return Err(FsError::PermissionDenied);
        }

        let from_resolved = self.resolve_path(from)?;
        let to_resolved = self.resolve_path(to)?;

        std::fs::rename(from_resolved, to_resolved).map_err(FsError::from)?;

        Ok(())
    }

    /// Get open file count
    pub fn open_file_count(&self) -> usize {
        self.open_files.len()
    }

    /// Get total bytes written
    pub fn total_written(&self) -> u64 {
        self.total_written
    }
}

impl Default for FilesystemState {
    fn default() -> Self {
        Self::new(FsConfig::default())
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_fs() -> (FilesystemState, TempDir) {
        let temp = TempDir::new().unwrap();
        let config = FsConfig::with_root(temp.path());
        (FilesystemState::new(config), temp)
    }

    #[test]
    fn test_path_sanitization_basic() {
        let (fs, _temp) = create_test_fs();

        // Valid paths
        assert!(fs.resolve_path("test.txt").is_ok());
        assert!(fs.resolve_path("subdir/test.txt").is_ok());
        assert!(fs.resolve_path("/test.txt").is_ok());
    }

    #[test]
    fn test_path_sanitization_traversal() {
        let (fs, _temp) = create_test_fs();

        // Invalid paths (traversal attempts)
        assert_eq!(
            fs.resolve_path("../test.txt").unwrap_err(),
            FsError::InvalidPath
        );
        assert_eq!(
            fs.resolve_path("subdir/../../../etc/passwd").unwrap_err(),
            FsError::InvalidPath
        );
        assert_eq!(fs.resolve_path("..").unwrap_err(), FsError::InvalidPath);
    }

    #[test]
    fn test_open_flags() {
        let flags = OpenFlags::new(OpenFlags::READ | OpenFlags::WRITE | OpenFlags::CREATE);
        assert!(flags.has_read());
        assert!(flags.has_write());
        assert!(flags.has_create());
        assert!(!flags.has_truncate());
        assert!(!flags.has_append());
    }

    #[test]
    fn test_file_create_and_write() {
        let (mut fs, _temp) = create_test_fs();

        // Create and write
        let fd = fs
            .open(
                "test.txt",
                OpenFlags::new(OpenFlags::WRITE | OpenFlags::CREATE),
            )
            .unwrap();
        let written = fs.write(fd, b"Hello, World!").unwrap();
        assert_eq!(written, 13);
        fs.close(fd).unwrap();

        // Read back
        let fd = fs
            .open("test.txt", OpenFlags::new(OpenFlags::READ))
            .unwrap();
        let mut buffer = [0u8; 20];
        let read = fs.read(fd, &mut buffer).unwrap();
        assert_eq!(read, 13);
        assert_eq!(&buffer[..13], b"Hello, World!");
        fs.close(fd).unwrap();
    }

    #[test]
    fn test_file_stat() {
        let (mut fs, _temp) = create_test_fs();

        // Create a file
        let fd = fs
            .open(
                "stat_test.txt",
                OpenFlags::new(OpenFlags::WRITE | OpenFlags::CREATE),
            )
            .unwrap();
        fs.write(fd, b"Test content").unwrap();
        fs.close(fd).unwrap();

        // Get stat
        let stat = fs.stat("stat_test.txt").unwrap();
        assert_eq!(stat.size, 12);
        assert_eq!(stat.file_type, FileType::File);
    }

    #[test]
    fn test_directory_operations() {
        let (mut fs, _temp) = create_test_fs();

        // Create directory
        fs.create_dir("testdir").unwrap();

        // Check it exists
        assert!(fs.exists("testdir").unwrap());

        // Create file in directory
        let fd = fs
            .open(
                "testdir/file.txt",
                OpenFlags::new(OpenFlags::WRITE | OpenFlags::CREATE),
            )
            .unwrap();
        fs.write(fd, b"In subdir").unwrap();
        fs.close(fd).unwrap();

        // List directory
        let entries = fs.read_dir("testdir").unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "file.txt");
    }

    #[test]
    fn test_file_seek() {
        let (mut fs, _temp) = create_test_fs();

        // Create file with content
        let fd = fs
            .open(
                "seek_test.txt",
                OpenFlags::new(OpenFlags::READ | OpenFlags::WRITE | OpenFlags::CREATE),
            )
            .unwrap();
        fs.write(fd, b"0123456789").unwrap();

        // Seek to beginning
        let pos = fs.seek(fd, 0, SeekOrigin::Start).unwrap();
        assert_eq!(pos, 0);

        // Read first 5 bytes
        let mut buffer = [0u8; 5];
        fs.read(fd, &mut buffer).unwrap();
        assert_eq!(&buffer, b"01234");

        // Seek from current
        fs.seek(fd, 2, SeekOrigin::Current).unwrap();
        let mut buffer = [0u8; 3];
        fs.read(fd, &mut buffer).unwrap();
        assert_eq!(&buffer, b"789");

        fs.close(fd).unwrap();
    }

    #[test]
    fn test_permission_denied_read_only() {
        let temp = TempDir::new().unwrap();
        let config = FsConfig::with_root(temp.path()).with_permissions(FsPermissions::read_only());
        let mut fs = FilesystemState::new(config);

        // Create a file manually
        let file_path = temp.path().join("readonly.txt");
        std::fs::write(&file_path, b"test").unwrap();

        // Can read
        let fd = fs
            .open("readonly.txt", OpenFlags::new(OpenFlags::READ))
            .unwrap();
        let mut buffer = [0u8; 10];
        let read = fs.read(fd, &mut buffer).unwrap();
        assert_eq!(read, 4);
        fs.close(fd).unwrap();

        // Cannot write
        let result = fs.open(
            "new.txt",
            OpenFlags::new(OpenFlags::WRITE | OpenFlags::CREATE),
        );
        assert_eq!(result.unwrap_err(), FsError::PermissionDenied);
    }

    #[test]
    fn test_file_too_large() {
        let temp = TempDir::new().unwrap();
        let perms = FsPermissions {
            max_file_size: 10, // 10 bytes max
            ..FsPermissions::default()
        };
        let config = FsConfig::with_root(temp.path()).with_permissions(perms);
        let mut fs = FilesystemState::new(config);

        let fd = fs
            .open(
                "large.txt",
                OpenFlags::new(OpenFlags::WRITE | OpenFlags::CREATE),
            )
            .unwrap();

        // First write succeeds
        fs.write(fd, b"12345").unwrap();

        // Second write exceeds limit
        let result = fs.write(fd, b"678901");
        assert_eq!(result.unwrap_err(), FsError::FileTooLarge);

        fs.close(fd).unwrap();
    }

    #[test]
    fn test_invalid_fd() {
        let (mut fs, _temp) = create_test_fs();

        let result = fs.read(999, &mut [0u8; 10]);
        assert_eq!(result.unwrap_err(), FsError::InvalidFd);

        let result = fs.close(999);
        assert_eq!(result.unwrap_err(), FsError::InvalidFd);
    }

    #[test]
    fn test_remove_file() {
        let (mut fs, _temp) = create_test_fs();

        // Create file
        let fd = fs
            .open(
                "to_delete.txt",
                OpenFlags::new(OpenFlags::WRITE | OpenFlags::CREATE),
            )
            .unwrap();
        fs.write(fd, b"delete me").unwrap();
        fs.close(fd).unwrap();

        assert!(fs.exists("to_delete.txt").unwrap());

        fs.remove_file("to_delete.txt").unwrap();

        assert!(!fs.exists("to_delete.txt").unwrap());
    }

    #[test]
    fn test_remove_directory() {
        let (fs, _temp) = create_test_fs();

        fs.create_dir("to_delete_dir").unwrap();
        assert!(fs.exists("to_delete_dir").unwrap());

        fs.remove_dir("to_delete_dir").unwrap();

        assert!(!fs.exists("to_delete_dir").unwrap());
    }

    #[test]
    fn test_rename() {
        let (mut fs, _temp) = create_test_fs();

        // Create file
        let fd = fs
            .open(
                "original.txt",
                OpenFlags::new(OpenFlags::WRITE | OpenFlags::CREATE),
            )
            .unwrap();
        fs.write(fd, b"content").unwrap();
        fs.close(fd).unwrap();

        fs.rename("original.txt", "renamed.txt").unwrap();

        assert!(!fs.exists("original.txt").unwrap());
        assert!(fs.exists("renamed.txt").unwrap());
    }

    #[test]
    fn test_file_stat_serialization() {
        let stat = FileStat {
            size: 1234,
            file_type: FileType::File,
            created: 1000,
            modified: 2000,
            accessed: 3000,
            readonly: false,
        };

        let bytes = stat.to_bytes();
        assert_eq!(bytes.len(), 40);

        // Verify size
        let size = u64::from_le_bytes(bytes[0..8].try_into().unwrap());
        assert_eq!(size, 1234);
    }

    #[test]
    fn test_dir_entry_serialization() {
        let entry = DirEntry {
            name: "test.txt".to_string(),
            file_type: FileType::File,
            size: 100,
        };

        let bytes = entry.to_bytes();

        // 4 (name len) + 4 (type) + 8 (size) + 8 (name) = 24 bytes
        assert_eq!(bytes.len(), 24);
    }

    #[test]
    fn test_seek_origin_from_u32() {
        assert_eq!(SeekOrigin::from_u32(0), Some(SeekOrigin::Start));
        assert_eq!(SeekOrigin::from_u32(1), Some(SeekOrigin::Current));
        assert_eq!(SeekOrigin::from_u32(2), Some(SeekOrigin::End));
        assert_eq!(SeekOrigin::from_u32(99), None);
    }

    #[test]
    fn test_fs_permissions_presets() {
        let ro = FsPermissions::read_only();
        assert!(ro.can_read);
        assert!(!ro.can_write);

        let full = FsPermissions::full_access();
        assert!(full.can_read);
        assert!(full.can_write);
        assert!(full.can_create);
        assert!(full.can_delete);

        let list = FsPermissions::list_only();
        assert!(!list.can_read);
        assert!(list.can_list);
    }

    #[test]
    fn test_open_file_count() {
        let (mut fs, _temp) = create_test_fs();

        assert_eq!(fs.open_file_count(), 0);

        let fd1 = fs
            .open(
                "file1.txt",
                OpenFlags::new(OpenFlags::WRITE | OpenFlags::CREATE),
            )
            .unwrap();
        assert_eq!(fs.open_file_count(), 1);

        let fd2 = fs
            .open(
                "file2.txt",
                OpenFlags::new(OpenFlags::WRITE | OpenFlags::CREATE),
            )
            .unwrap();
        assert_eq!(fs.open_file_count(), 2);

        fs.close(fd1).unwrap();
        assert_eq!(fs.open_file_count(), 1);

        fs.close(fd2).unwrap();
        assert_eq!(fs.open_file_count(), 0);
    }

    #[test]
    fn test_flush() {
        let (mut fs, _temp) = create_test_fs();

        let fd = fs
            .open(
                "flush_test.txt",
                OpenFlags::new(OpenFlags::WRITE | OpenFlags::CREATE),
            )
            .unwrap();
        fs.write(fd, b"buffered data").unwrap();
        fs.flush(fd).unwrap();
        fs.close(fd).unwrap();
    }

    #[test]
    fn test_not_a_directory() {
        let (mut fs, _temp) = create_test_fs();

        // Create a file
        let fd = fs
            .open(
                "file.txt",
                OpenFlags::new(OpenFlags::WRITE | OpenFlags::CREATE),
            )
            .unwrap();
        fs.close(fd).unwrap();

        // Try to list it as directory
        let result = fs.read_dir("file.txt");
        assert_eq!(result.unwrap_err(), FsError::NotADirectory);
    }

    #[test]
    fn test_nested_directories() {
        let (mut fs, _temp) = create_test_fs();

        // Create nested structure
        fs.create_dir("a/b/c").unwrap();

        assert!(fs.exists("a").unwrap());
        assert!(fs.exists("a/b").unwrap());
        assert!(fs.exists("a/b/c").unwrap());

        // Create file in nested dir
        let fd = fs
            .open(
                "a/b/c/deep.txt",
                OpenFlags::new(OpenFlags::WRITE | OpenFlags::CREATE),
            )
            .unwrap();
        fs.write(fd, b"deep file").unwrap();
        fs.close(fd).unwrap();

        assert!(fs.exists("a/b/c/deep.txt").unwrap());
    }
}
