//! Network Host Functions for WASM Agents
//!
//! Provides sandboxed network access with capability-based security.
//!
//! ## Features
//!
//! - TCP socket operations (connect, listen, accept, read, write)
//! - UDP socket operations (bind, send_to, recv_from)
//! - DNS resolution
//! - Network capability checks
//! - Connection limits and rate limiting

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream, ToSocketAddrs, UdpSocket};

#[cfg(test)]
use std::net::{Ipv4Addr, Ipv6Addr};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// Network error codes returned to WASM
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum NetError {
    /// Operation succeeded
    Success = 0,
    /// Permission denied (capability not granted)
    PermissionDenied = -1,
    /// Connection refused
    ConnectionRefused = -2,
    /// Connection reset
    ConnectionReset = -3,
    /// Connection timed out
    TimedOut = -4,
    /// Host not found
    HostNotFound = -5,
    /// Network unreachable
    NetworkUnreachable = -6,
    /// Address already in use
    AddressInUse = -7,
    /// Address not available
    AddressNotAvailable = -8,
    /// Invalid socket descriptor
    InvalidSocket = -9,
    /// Socket not connected
    NotConnected = -10,
    /// Already connected
    AlreadyConnected = -11,
    /// Would block (for non-blocking)
    WouldBlock = -12,
    /// Invalid address format
    InvalidAddress = -13,
    /// I/O error
    IoError = -14,
    /// Too many connections
    TooManyConnections = -15,
    /// Rate limit exceeded
    RateLimitExceeded = -16,
    /// Buffer too small
    BufferTooSmall = -17,
    /// Invalid argument
    InvalidArgument = -18,
}

impl From<std::io::Error> for NetError {
    fn from(err: std::io::Error) -> Self {
        use std::io::ErrorKind;
        match err.kind() {
            ErrorKind::ConnectionRefused => NetError::ConnectionRefused,
            ErrorKind::ConnectionReset => NetError::ConnectionReset,
            ErrorKind::TimedOut => NetError::TimedOut,
            ErrorKind::AddrInUse => NetError::AddressInUse,
            ErrorKind::AddrNotAvailable => NetError::AddressNotAvailable,
            ErrorKind::NotConnected => NetError::NotConnected,
            ErrorKind::AlreadyExists => NetError::AlreadyConnected,
            ErrorKind::WouldBlock => NetError::WouldBlock,
            ErrorKind::PermissionDenied => NetError::PermissionDenied,
            _ => NetError::IoError,
        }
    }
}

/// Socket type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum SocketType {
    /// TCP stream socket
    Tcp = 1,
    /// UDP datagram socket
    Udp = 2,
}

impl SocketType {
    pub fn from_u32(value: u32) -> Option<Self> {
        match value {
            1 => Some(SocketType::Tcp),
            2 => Some(SocketType::Udp),
            _ => None,
        }
    }
}

/// Socket state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SocketState {
    /// Socket created but not bound
    Created,
    /// Socket bound to address
    Bound,
    /// Listening for connections (TCP)
    Listening,
    /// Connected to peer (TCP)
    Connected,
    /// Closed
    Closed,
}

/// Resolved address information
#[derive(Debug, Clone)]
pub struct AddressInfo {
    /// IP address
    pub address: IpAddr,
    /// Port number
    pub port: u16,
    /// Is IPv6
    pub is_ipv6: bool,
}

impl AddressInfo {
    /// Serialize to bytes (24 bytes)
    pub fn to_bytes(&self) -> [u8; 24] {
        let mut bytes = [0u8; 24];

        match self.address {
            IpAddr::V4(ipv4) => {
                bytes[0] = 4; // IPv4 marker
                bytes[4..8].copy_from_slice(&ipv4.octets());
            }
            IpAddr::V6(ipv6) => {
                bytes[0] = 6; // IPv6 marker
                bytes[4..20].copy_from_slice(&ipv6.octets());
            }
        }

        bytes[20..22].copy_from_slice(&self.port.to_le_bytes());
        bytes[22] = self.is_ipv6 as u8;

        bytes
    }

    /// From socket address
    pub fn from_socket_addr(addr: SocketAddr) -> Self {
        Self {
            address: addr.ip(),
            port: addr.port(),
            is_ipv6: addr.is_ipv6(),
        }
    }
}

/// TCP socket wrapper
struct TcpSocketState {
    /// Stream (for connected sockets)
    stream: Option<TcpStream>,
    /// Listener (for server sockets)
    listener: Option<TcpListener>,
    /// Socket state
    state: SocketState,
    /// Local address
    local_addr: Option<SocketAddr>,
    /// Remote address
    remote_addr: Option<SocketAddr>,
    /// Bytes sent
    bytes_sent: u64,
    /// Bytes received
    bytes_received: u64,
    /// Non-blocking mode
    non_blocking: bool,
}

/// UDP socket wrapper
struct UdpSocketState {
    /// The UDP socket
    socket: Option<UdpSocket>,
    /// Socket state
    state: SocketState,
    /// Local address
    local_addr: Option<SocketAddr>,
    /// Bytes sent
    bytes_sent: u64,
    /// Bytes received
    bytes_received: u64,
    /// Non-blocking mode
    non_blocking: bool,
}

/// Socket wrapper enum
enum SocketWrapper {
    Tcp(TcpSocketState),
    Udp(UdpSocketState),
}

/// Network capability permissions
#[derive(Debug, Clone)]
pub struct NetPermissions {
    /// Can create TCP connections
    pub can_tcp_connect: bool,
    /// Can create TCP listeners
    pub can_tcp_listen: bool,
    /// Can use UDP
    pub can_udp: bool,
    /// Can resolve DNS
    pub can_dns: bool,
    /// Allowed hosts (empty = all)
    pub allowed_hosts: Vec<String>,
    /// Allowed ports (empty = all)
    pub allowed_ports: Vec<u16>,
    /// Denied hosts
    pub denied_hosts: Vec<String>,
    /// Maximum connections
    pub max_connections: usize,
    /// Maximum bandwidth (bytes/sec, 0 = unlimited)
    pub max_bandwidth: u64,
    /// Connection timeout
    pub connect_timeout: Duration,
    /// Read timeout
    pub read_timeout: Duration,
    /// Write timeout
    pub write_timeout: Duration,
}

impl Default for NetPermissions {
    fn default() -> Self {
        Self {
            can_tcp_connect: true,
            can_tcp_listen: true,
            can_udp: true,
            can_dns: true,
            allowed_hosts: Vec::new(),
            allowed_ports: Vec::new(),
            denied_hosts: vec![
                "localhost".to_string(),
                "127.0.0.1".to_string(),
                "::1".to_string(),
            ],
            max_connections: 16,
            max_bandwidth: 10 * 1024 * 1024, // 10 MB/s
            connect_timeout: Duration::from_secs(30),
            read_timeout: Duration::from_secs(60),
            write_timeout: Duration::from_secs(60),
        }
    }
}

impl NetPermissions {
    /// Full network access (be careful!)
    pub fn full_access() -> Self {
        Self {
            can_tcp_connect: true,
            can_tcp_listen: true,
            can_udp: true,
            can_dns: true,
            allowed_hosts: Vec::new(),
            allowed_ports: Vec::new(),
            denied_hosts: Vec::new(),
            max_connections: 256,
            max_bandwidth: 0,
            connect_timeout: Duration::from_secs(60),
            read_timeout: Duration::from_secs(120),
            write_timeout: Duration::from_secs(120),
        }
    }

    /// Outbound only (no listening)
    pub fn outbound_only() -> Self {
        Self {
            can_tcp_connect: true,
            can_tcp_listen: false,
            can_udp: true,
            can_dns: true,
            ..Default::default()
        }
    }

    /// HTTP only (ports 80, 443)
    pub fn http_only() -> Self {
        Self {
            can_tcp_connect: true,
            can_tcp_listen: false,
            can_udp: false,
            can_dns: true,
            allowed_hosts: Vec::new(),
            allowed_ports: vec![80, 443],
            denied_hosts: vec!["localhost".to_string(), "127.0.0.1".to_string()],
            max_connections: 8,
            max_bandwidth: 1024 * 1024, // 1 MB/s
            ..Default::default()
        }
    }

    /// No network access
    pub fn none() -> Self {
        Self {
            can_tcp_connect: false,
            can_tcp_listen: false,
            can_udp: false,
            can_dns: false,
            allowed_hosts: Vec::new(),
            allowed_ports: Vec::new(),
            denied_hosts: Vec::new(),
            max_connections: 0,
            max_bandwidth: 0,
            connect_timeout: Duration::ZERO,
            read_timeout: Duration::ZERO,
            write_timeout: Duration::ZERO,
        }
    }

    /// Check if host is allowed
    pub fn is_host_allowed(&self, host: &str) -> bool {
        // Check denied list first
        if self.denied_hosts.iter().any(|h| h == host) {
            return false;
        }

        // If allowed list is empty, all hosts are allowed
        if self.allowed_hosts.is_empty() {
            return true;
        }

        // Check allowed list
        self.allowed_hosts.iter().any(|h| h == host)
    }

    /// Check if port is allowed
    pub fn is_port_allowed(&self, port: u16) -> bool {
        if self.allowed_ports.is_empty() {
            return true;
        }
        self.allowed_ports.contains(&port)
    }
}

/// Rate limiter for bandwidth control
#[derive(Debug)]
pub struct RateLimiter {
    /// Maximum bytes per second
    max_bytes_per_sec: u64,
    /// Bytes sent this window
    bytes_this_window: AtomicU64,
    /// Window start time
    window_start: Instant,
}

impl RateLimiter {
    pub fn new(max_bytes_per_sec: u64) -> Self {
        Self {
            max_bytes_per_sec,
            bytes_this_window: AtomicU64::new(0),
            window_start: Instant::now(),
        }
    }

    /// Check if we can send/receive bytes
    pub fn can_transfer(&self, bytes: u64) -> bool {
        if self.max_bytes_per_sec == 0 {
            return true; // Unlimited
        }

        let elapsed = self.window_start.elapsed();
        if elapsed >= Duration::from_secs(1) {
            // Window expired, would reset
            return bytes <= self.max_bytes_per_sec;
        }

        let current = self.bytes_this_window.load(Ordering::Relaxed);
        current + bytes <= self.max_bytes_per_sec
    }

    /// Record a transfer
    pub fn record_transfer(&self, bytes: u64) {
        self.bytes_this_window.fetch_add(bytes, Ordering::Relaxed);
    }

    /// Reset window if needed
    pub fn maybe_reset(&mut self) {
        if self.window_start.elapsed() >= Duration::from_secs(1) {
            self.bytes_this_window.store(0, Ordering::Relaxed);
            self.window_start = Instant::now();
        }
    }
}

/// Network configuration
#[derive(Debug, Clone, Default)]
pub struct NetConfig {
    /// Permissions
    pub permissions: NetPermissions,
}

impl NetConfig {
    /// Create with specific permissions
    pub fn with_permissions(permissions: NetPermissions) -> Self {
        Self { permissions }
    }
}

/// Network statistics
#[derive(Debug, Clone, Default)]
pub struct NetStats {
    /// Total TCP connections made
    pub tcp_connections: u64,
    /// Total UDP sockets created
    pub udp_sockets: u64,
    /// Total bytes sent
    pub bytes_sent: u64,
    /// Total bytes received
    pub bytes_received: u64,
    /// DNS queries made
    pub dns_queries: u64,
    /// Failed connections
    pub failed_connections: u64,
    /// Active connections
    pub active_connections: usize,
}

/// Network state for WASM agents
pub struct NetworkState {
    /// Configuration
    config: NetConfig,
    /// Open sockets
    sockets: BTreeMap<u32, SocketWrapper>,
    /// Next socket descriptor
    next_fd: AtomicU32,
    /// Statistics
    stats: NetStats,
    /// Rate limiter
    rate_limiter: RateLimiter,
}

impl NetworkState {
    /// Create new network state
    pub fn new(config: NetConfig) -> Self {
        let max_bandwidth = config.permissions.max_bandwidth;
        Self {
            config,
            sockets: BTreeMap::new(),
            next_fd: AtomicU32::new(100), // Start at 100 to avoid conflicts
            stats: NetStats::default(),
            rate_limiter: RateLimiter::new(max_bandwidth),
        }
    }

    /// Get configuration
    pub fn config(&self) -> &NetConfig {
        &self.config
    }

    /// Get statistics
    pub fn stats(&self) -> &NetStats {
        &self.stats
    }

    /// Create a TCP socket
    pub fn tcp_socket(&mut self) -> Result<u32, NetError> {
        if !self.config.permissions.can_tcp_connect && !self.config.permissions.can_tcp_listen {
            return Err(NetError::PermissionDenied);
        }

        if self.sockets.len() >= self.config.permissions.max_connections {
            return Err(NetError::TooManyConnections);
        }

        let fd = self.next_fd.fetch_add(1, Ordering::Relaxed);
        self.sockets.insert(
            fd,
            SocketWrapper::Tcp(TcpSocketState {
                stream: None,
                listener: None,
                state: SocketState::Created,
                local_addr: None,
                remote_addr: None,
                bytes_sent: 0,
                bytes_received: 0,
                non_blocking: false,
            }),
        );

        Ok(fd)
    }

    /// Create a UDP socket
    pub fn udp_socket(&mut self) -> Result<u32, NetError> {
        if !self.config.permissions.can_udp {
            return Err(NetError::PermissionDenied);
        }

        if self.sockets.len() >= self.config.permissions.max_connections {
            return Err(NetError::TooManyConnections);
        }

        let fd = self.next_fd.fetch_add(1, Ordering::Relaxed);
        self.sockets.insert(
            fd,
            SocketWrapper::Udp(UdpSocketState {
                socket: None,
                state: SocketState::Created,
                local_addr: None,
                bytes_sent: 0,
                bytes_received: 0,
                non_blocking: false,
            }),
        );

        self.stats.udp_sockets += 1;
        Ok(fd)
    }

    /// Connect TCP socket
    pub fn tcp_connect(&mut self, fd: u32, host: &str, port: u16) -> Result<(), NetError> {
        if !self.config.permissions.can_tcp_connect {
            return Err(NetError::PermissionDenied);
        }

        if !self.config.permissions.is_host_allowed(host) {
            return Err(NetError::PermissionDenied);
        }

        if !self.config.permissions.is_port_allowed(port) {
            return Err(NetError::PermissionDenied);
        }

        let socket = self.sockets.get_mut(&fd).ok_or(NetError::InvalidSocket)?;

        let tcp_state = match socket {
            SocketWrapper::Tcp(state) => state,
            SocketWrapper::Udp(_) => return Err(NetError::InvalidArgument),
        };

        if tcp_state.state != SocketState::Created {
            return Err(NetError::AlreadyConnected);
        }

        // Resolve address
        let addr_str = format!("{}:{}", host, port);
        let addr = addr_str
            .to_socket_addrs()
            .map_err(|_| NetError::HostNotFound)?
            .next()
            .ok_or(NetError::HostNotFound)?;

        // Connect with timeout
        let stream = TcpStream::connect_timeout(&addr, self.config.permissions.connect_timeout)
            .map_err(NetError::from)?;

        // Set timeouts
        stream
            .set_read_timeout(Some(self.config.permissions.read_timeout))
            .ok();
        stream
            .set_write_timeout(Some(self.config.permissions.write_timeout))
            .ok();

        tcp_state.stream = Some(stream);
        tcp_state.state = SocketState::Connected;
        tcp_state.remote_addr = Some(addr);

        self.stats.tcp_connections += 1;
        self.stats.active_connections += 1;

        Ok(())
    }

    /// Listen on TCP socket
    pub fn tcp_listen(&mut self, fd: u32, host: &str, port: u16) -> Result<(), NetError> {
        if !self.config.permissions.can_tcp_listen {
            return Err(NetError::PermissionDenied);
        }

        let socket = self.sockets.get_mut(&fd).ok_or(NetError::InvalidSocket)?;

        let tcp_state = match socket {
            SocketWrapper::Tcp(state) => state,
            SocketWrapper::Udp(_) => return Err(NetError::InvalidArgument),
        };

        let addr: SocketAddr = format!("{}:{}", host, port)
            .parse()
            .map_err(|_| NetError::InvalidAddress)?;

        let listener = TcpListener::bind(addr).map_err(NetError::from)?;
        tcp_state.listener = Some(listener);
        tcp_state.state = SocketState::Listening;
        tcp_state.local_addr = Some(addr);

        Ok(())
    }

    /// Accept TCP connection
    pub fn tcp_accept(&mut self, fd: u32) -> Result<u32, NetError> {
        if self.sockets.len() >= self.config.permissions.max_connections {
            return Err(NetError::TooManyConnections);
        }

        // First, get the local_addr and validate state
        let local_addr = {
            let socket = self.sockets.get(&fd).ok_or(NetError::InvalidSocket)?;
            let tcp_state = match socket {
                SocketWrapper::Tcp(state) => state,
                SocketWrapper::Udp(_) => return Err(NetError::InvalidArgument),
            };

            if tcp_state.state != SocketState::Listening {
                return Err(NetError::NotConnected);
            }

            tcp_state.local_addr
        };

        // Get mutable reference to accept
        let socket = self.sockets.get_mut(&fd).ok_or(NetError::InvalidSocket)?;
        let tcp_state = match socket {
            SocketWrapper::Tcp(state) => state,
            SocketWrapper::Udp(_) => return Err(NetError::InvalidArgument),
        };

        let listener = tcp_state.listener.as_ref().ok_or(NetError::NotConnected)?;
        let (stream, peer_addr) = listener.accept().map_err(NetError::from)?;

        // Set timeouts on accepted connection
        stream
            .set_read_timeout(Some(self.config.permissions.read_timeout))
            .ok();
        stream
            .set_write_timeout(Some(self.config.permissions.write_timeout))
            .ok();

        // Create new socket for the connection
        let new_fd = self.next_fd.fetch_add(1, Ordering::Relaxed);
        self.sockets.insert(
            new_fd,
            SocketWrapper::Tcp(TcpSocketState {
                stream: Some(stream),
                listener: None,
                state: SocketState::Connected,
                local_addr,
                remote_addr: Some(peer_addr),
                bytes_sent: 0,
                bytes_received: 0,
                non_blocking: false,
            }),
        );

        self.stats.tcp_connections += 1;
        self.stats.active_connections += 1;

        Ok(new_fd)
    }

    /// Send data on TCP socket
    pub fn tcp_send(&mut self, fd: u32, data: &[u8]) -> Result<usize, NetError> {
        // Rate limiting
        if !self.rate_limiter.can_transfer(data.len() as u64) {
            return Err(NetError::RateLimitExceeded);
        }

        let socket = self.sockets.get_mut(&fd).ok_or(NetError::InvalidSocket)?;

        let tcp_state = match socket {
            SocketWrapper::Tcp(state) => state,
            SocketWrapper::Udp(_) => return Err(NetError::InvalidArgument),
        };

        if tcp_state.state != SocketState::Connected {
            return Err(NetError::NotConnected);
        }

        let stream = tcp_state.stream.as_mut().ok_or(NetError::NotConnected)?;
        let sent = stream.write(data).map_err(NetError::from)?;

        tcp_state.bytes_sent += sent as u64;
        self.stats.bytes_sent += sent as u64;
        self.rate_limiter.record_transfer(sent as u64);

        Ok(sent)
    }

    /// Receive data on TCP socket
    pub fn tcp_recv(&mut self, fd: u32, buffer: &mut [u8]) -> Result<usize, NetError> {
        let socket = self.sockets.get_mut(&fd).ok_or(NetError::InvalidSocket)?;

        let tcp_state = match socket {
            SocketWrapper::Tcp(state) => state,
            SocketWrapper::Udp(_) => return Err(NetError::InvalidArgument),
        };

        if tcp_state.state != SocketState::Connected {
            return Err(NetError::NotConnected);
        }

        let stream = tcp_state.stream.as_mut().ok_or(NetError::NotConnected)?;
        let received = stream.read(buffer).map_err(NetError::from)?;

        tcp_state.bytes_received += received as u64;
        self.stats.bytes_received += received as u64;

        Ok(received)
    }

    /// Bind UDP socket
    pub fn udp_bind(&mut self, fd: u32, host: &str, port: u16) -> Result<(), NetError> {
        let socket = self.sockets.get_mut(&fd).ok_or(NetError::InvalidSocket)?;

        let udp_state = match socket {
            SocketWrapper::Udp(state) => state,
            SocketWrapper::Tcp(_) => return Err(NetError::InvalidArgument),
        };

        let addr: SocketAddr = format!("{}:{}", host, port)
            .parse()
            .map_err(|_| NetError::InvalidAddress)?;

        let udp_socket = UdpSocket::bind(addr).map_err(NetError::from)?;
        udp_state.socket = Some(udp_socket);
        udp_state.state = SocketState::Bound;
        udp_state.local_addr = Some(addr);

        Ok(())
    }

    /// Send UDP datagram
    pub fn udp_send_to(
        &mut self,
        fd: u32,
        data: &[u8],
        host: &str,
        port: u16,
    ) -> Result<usize, NetError> {
        if !self.config.permissions.is_host_allowed(host) {
            return Err(NetError::PermissionDenied);
        }

        if !self.config.permissions.is_port_allowed(port) {
            return Err(NetError::PermissionDenied);
        }

        if !self.rate_limiter.can_transfer(data.len() as u64) {
            return Err(NetError::RateLimitExceeded);
        }

        let socket = self.sockets.get_mut(&fd).ok_or(NetError::InvalidSocket)?;

        let udp_state = match socket {
            SocketWrapper::Udp(state) => state,
            SocketWrapper::Tcp(_) => return Err(NetError::InvalidArgument),
        };

        let udp_socket = udp_state.socket.as_ref().ok_or(NetError::NotConnected)?;

        let addr: SocketAddr = format!("{}:{}", host, port)
            .parse()
            .map_err(|_| NetError::InvalidAddress)?;

        let sent = udp_socket.send_to(data, addr).map_err(NetError::from)?;

        udp_state.bytes_sent += sent as u64;
        self.stats.bytes_sent += sent as u64;
        self.rate_limiter.record_transfer(sent as u64);

        Ok(sent)
    }

    /// Receive UDP datagram
    pub fn udp_recv_from(
        &mut self,
        fd: u32,
        buffer: &mut [u8],
    ) -> Result<(usize, SocketAddr), NetError> {
        let socket = self.sockets.get_mut(&fd).ok_or(NetError::InvalidSocket)?;

        let udp_state = match socket {
            SocketWrapper::Udp(state) => state,
            SocketWrapper::Tcp(_) => return Err(NetError::InvalidArgument),
        };

        let udp_socket = udp_state.socket.as_ref().ok_or(NetError::NotConnected)?;
        let (received, addr) = udp_socket.recv_from(buffer).map_err(NetError::from)?;

        udp_state.bytes_received += received as u64;
        self.stats.bytes_received += received as u64;

        Ok((received, addr))
    }

    /// Resolve DNS name
    pub fn dns_resolve(&mut self, hostname: &str) -> Result<Vec<IpAddr>, NetError> {
        if !self.config.permissions.can_dns {
            return Err(NetError::PermissionDenied);
        }

        self.stats.dns_queries += 1;

        // Simple DNS resolution using std
        let addrs: Vec<IpAddr> = (hostname, 0u16)
            .to_socket_addrs()
            .map_err(|_| NetError::HostNotFound)?
            .map(|addr| addr.ip())
            .collect();

        if addrs.is_empty() {
            return Err(NetError::HostNotFound);
        }

        Ok(addrs)
    }

    /// Close socket
    pub fn close(&mut self, fd: u32) -> Result<(), NetError> {
        let socket = self.sockets.remove(&fd).ok_or(NetError::InvalidSocket)?;

        match socket {
            SocketWrapper::Tcp(tcp) => {
                if tcp.state == SocketState::Connected {
                    self.stats.active_connections = self.stats.active_connections.saturating_sub(1);
                }
            }
            SocketWrapper::Udp(_) => {}
        }

        Ok(())
    }

    /// Set socket to non-blocking mode
    pub fn set_nonblocking(&mut self, fd: u32, nonblocking: bool) -> Result<(), NetError> {
        let socket = self.sockets.get_mut(&fd).ok_or(NetError::InvalidSocket)?;

        match socket {
            SocketWrapper::Tcp(tcp) => {
                if let Some(stream) = &tcp.stream {
                    stream
                        .set_nonblocking(nonblocking)
                        .map_err(NetError::from)?;
                }
                tcp.non_blocking = nonblocking;
            }
            SocketWrapper::Udp(udp) => {
                if let Some(socket) = &udp.socket {
                    socket
                        .set_nonblocking(nonblocking)
                        .map_err(NetError::from)?;
                }
                udp.non_blocking = nonblocking;
            }
        }

        Ok(())
    }

    /// Get local address
    pub fn local_addr(&self, fd: u32) -> Result<SocketAddr, NetError> {
        let socket = self.sockets.get(&fd).ok_or(NetError::InvalidSocket)?;

        match socket {
            SocketWrapper::Tcp(tcp) => tcp.local_addr.ok_or(NetError::NotConnected),
            SocketWrapper::Udp(udp) => udp.local_addr.ok_or(NetError::NotConnected),
        }
    }

    /// Get remote address (TCP only)
    pub fn remote_addr(&self, fd: u32) -> Result<SocketAddr, NetError> {
        let socket = self.sockets.get(&fd).ok_or(NetError::InvalidSocket)?;

        match socket {
            SocketWrapper::Tcp(tcp) => tcp.remote_addr.ok_or(NetError::NotConnected),
            SocketWrapper::Udp(_) => Err(NetError::InvalidArgument),
        }
    }

    /// Get socket count
    pub fn socket_count(&self) -> usize {
        self.sockets.len()
    }
}

impl Default for NetworkState {
    fn default() -> Self {
        Self::new(NetConfig::default())
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_net_error_from_io() {
        use std::io::{Error, ErrorKind};

        assert_eq!(
            NetError::from(Error::new(ErrorKind::ConnectionRefused, "test")),
            NetError::ConnectionRefused
        );
        assert_eq!(
            NetError::from(Error::new(ErrorKind::TimedOut, "test")),
            NetError::TimedOut
        );
        assert_eq!(
            NetError::from(Error::new(ErrorKind::AddrInUse, "test")),
            NetError::AddressInUse
        );
    }

    #[test]
    fn test_socket_type_from_u32() {
        assert_eq!(SocketType::from_u32(1), Some(SocketType::Tcp));
        assert_eq!(SocketType::from_u32(2), Some(SocketType::Udp));
        assert_eq!(SocketType::from_u32(99), None);
    }

    #[test]
    fn test_address_info_ipv4() {
        let info = AddressInfo {
            address: IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)),
            port: 8080,
            is_ipv6: false,
        };

        let bytes = info.to_bytes();
        assert_eq!(bytes[0], 4); // IPv4 marker
        assert_eq!(bytes[4..8], [192, 168, 1, 1]);
    }

    #[test]
    fn test_address_info_ipv6() {
        let info = AddressInfo {
            address: IpAddr::V6(Ipv6Addr::LOCALHOST),
            port: 443,
            is_ipv6: true,
        };

        let bytes = info.to_bytes();
        assert_eq!(bytes[0], 6); // IPv6 marker
        assert_eq!(bytes[22], 1); // is_ipv6 true
    }

    #[test]
    fn test_net_permissions_default() {
        let perms = NetPermissions::default();
        assert!(perms.can_tcp_connect);
        assert!(perms.can_tcp_listen);
        assert!(perms.can_udp);
        assert!(perms.can_dns);
        assert_eq!(perms.max_connections, 16);
    }

    #[test]
    fn test_net_permissions_presets() {
        let none = NetPermissions::none();
        assert!(!none.can_tcp_connect);
        assert!(!none.can_tcp_listen);
        assert!(!none.can_udp);
        assert!(!none.can_dns);

        let full = NetPermissions::full_access();
        assert!(full.can_tcp_connect);
        assert!(full.can_tcp_listen);
        assert_eq!(full.max_connections, 256);

        let http = NetPermissions::http_only();
        assert!(http.can_tcp_connect);
        assert!(!http.can_tcp_listen);
        assert!(!http.can_udp);
        assert_eq!(http.allowed_ports, vec![80, 443]);
    }

    #[test]
    fn test_host_allowed() {
        let perms = NetPermissions::default();

        // localhost should be denied by default
        assert!(!perms.is_host_allowed("localhost"));
        assert!(!perms.is_host_allowed("127.0.0.1"));

        // Other hosts should be allowed
        assert!(perms.is_host_allowed("example.com"));
    }

    #[test]
    fn test_port_allowed() {
        let mut perms = NetPermissions::default();

        // All ports allowed by default
        assert!(perms.is_port_allowed(80));
        assert!(perms.is_port_allowed(22));

        // Restrict to specific ports
        perms.allowed_ports = vec![80, 443];
        assert!(perms.is_port_allowed(80));
        assert!(perms.is_port_allowed(443));
        assert!(!perms.is_port_allowed(22));
    }

    #[test]
    fn test_rate_limiter() {
        let limiter = RateLimiter::new(1000);

        // Should allow small transfers
        assert!(limiter.can_transfer(500));
        limiter.record_transfer(500);

        // Should allow another small transfer
        assert!(limiter.can_transfer(400));
        limiter.record_transfer(400);

        // Should deny transfer that exceeds limit
        assert!(!limiter.can_transfer(200));
    }

    #[test]
    fn test_rate_limiter_unlimited() {
        let limiter = RateLimiter::new(0); // Unlimited

        assert!(limiter.can_transfer(1_000_000_000));
    }

    #[test]
    fn test_network_state_create_sockets() {
        let mut net = NetworkState::new(NetConfig::default());

        // Create TCP socket
        let tcp_fd = net.tcp_socket().unwrap();
        assert!(tcp_fd >= 100);

        // Create UDP socket
        let udp_fd = net.udp_socket().unwrap();
        assert!(udp_fd > tcp_fd);

        assert_eq!(net.socket_count(), 2);
    }

    #[test]
    fn test_network_state_permission_denied() {
        let config = NetConfig::with_permissions(NetPermissions::none());
        let mut net = NetworkState::new(config);

        // Should fail - no TCP permission
        let result = net.tcp_socket();
        assert_eq!(result.unwrap_err(), NetError::PermissionDenied);

        // Should fail - no UDP permission
        let result = net.udp_socket();
        assert_eq!(result.unwrap_err(), NetError::PermissionDenied);
    }

    #[test]
    fn test_network_state_too_many_connections() {
        let perms = NetPermissions {
            max_connections: 2,
            ..NetPermissions::default()
        };
        let config = NetConfig::with_permissions(perms);
        let mut net = NetworkState::new(config);

        net.tcp_socket().unwrap();
        net.tcp_socket().unwrap();

        let result = net.tcp_socket();
        assert_eq!(result.unwrap_err(), NetError::TooManyConnections);
    }

    #[test]
    fn test_network_state_close() {
        let mut net = NetworkState::new(NetConfig::default());

        let fd = net.tcp_socket().unwrap();
        assert_eq!(net.socket_count(), 1);

        net.close(fd).unwrap();
        assert_eq!(net.socket_count(), 0);
    }

    #[test]
    fn test_network_state_invalid_socket() {
        let mut net = NetworkState::new(NetConfig::default());

        let result = net.close(9999);
        assert_eq!(result.unwrap_err(), NetError::InvalidSocket);
    }

    #[test]
    fn test_address_info_from_socket_addr() {
        let addr: SocketAddr = "192.168.1.1:8080".parse().unwrap();
        let info = AddressInfo::from_socket_addr(addr);

        assert_eq!(info.port, 8080);
        assert!(!info.is_ipv6);
    }

    #[test]
    fn test_net_stats() {
        let net = NetworkState::new(NetConfig::default());
        let stats = net.stats();

        assert_eq!(stats.tcp_connections, 0);
        assert_eq!(stats.bytes_sent, 0);
        assert_eq!(stats.bytes_received, 0);
    }

    #[test]
    fn test_outbound_only_permissions() {
        let perms = NetPermissions::outbound_only();
        assert!(perms.can_tcp_connect);
        assert!(!perms.can_tcp_listen);
        assert!(perms.can_udp);
        assert!(perms.can_dns);
    }
}
