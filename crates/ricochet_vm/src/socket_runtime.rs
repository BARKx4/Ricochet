use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use tungstenite::stream::MaybeTlsStream;
use tungstenite::{client_tls_with_config, Message, WebSocket};

const MAX_RETAINED_TCP_CONNECTIONS: usize = 64;
const MAX_RETAINED_TCP_LISTENERS: usize = 64;
const MAX_RETAINED_WEBSOCKET_CONNECTIONS: usize = 64;
const MAX_RETAINED_WEBSOCKET_LISTENERS: usize = 64;

type BlockingWebSocket = WebSocket<MaybeTlsStream<TcpStream>>;

#[derive(Clone)]
pub struct TcpSocketRegistry {
    inner: Arc<Mutex<TcpSocketRegistryState>>,
}

struct TcpSocketRegistryState {
    next_id: u64,
    pending_starts: usize,
    max_retained: usize,
    connections: BTreeMap<u64, Arc<TcpConnection>>,
}

struct TcpConnection {
    state: Mutex<TcpConnectionState>,
}

struct TcpConnectionState {
    id: u64,
    host: String,
    port: u16,
    started_at_ms: i64,
    status: SocketStatus,
    local_addr: Option<String>,
    peer_addr: Option<String>,
    error: Option<String>,
    bytes_read: usize,
    bytes_written: usize,
    stream: Option<TcpStream>,
}

#[derive(Clone)]
pub struct TcpListenerRegistry {
    inner: Arc<Mutex<TcpListenerRegistryState>>,
}

struct TcpListenerRegistryState {
    next_id: u64,
    pending_starts: usize,
    max_retained: usize,
    listeners: BTreeMap<u64, Arc<TcpListenerHandle>>,
}

struct TcpListenerHandle {
    state: Mutex<TcpListenerState>,
}

struct TcpListenerState {
    id: u64,
    host: String,
    port: u16,
    started_at_ms: i64,
    status: SocketStatus,
    local_addr: Option<String>,
    error: Option<String>,
    accepted_connections: usize,
    nodelay: bool,
    listener: Option<TcpListener>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum SocketStatus {
    Listening,
    Connected,
    Closed,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TcpConnectRequest {
    pub host: String,
    pub port: u16,
    pub timeout: Duration,
    pub nodelay: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TcpListenRequest {
    pub host: String,
    pub port: u16,
    pub nodelay: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TcpSocketSnapshot {
    pub id: u64,
    pub host: String,
    pub port: u16,
    pub started_at_ms: i64,
    pub status: String,
    pub connected: bool,
    pub closed: bool,
    pub local_addr: Option<String>,
    pub peer_addr: Option<String>,
    pub error: Option<String>,
    pub bytes_read: usize,
    pub bytes_written: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TcpListenerSnapshot {
    pub id: u64,
    pub host: String,
    pub port: u16,
    pub started_at_ms: i64,
    pub status: String,
    pub listening: bool,
    pub closed: bool,
    pub local_addr: Option<String>,
    pub error: Option<String>,
    pub accepted_connections: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TcpSocketRead {
    pub snapshot: TcpSocketSnapshot,
    pub data: String,
    pub bytes: usize,
}

#[derive(Clone)]
pub struct WebSocketRegistry {
    inner: Arc<Mutex<WebSocketRegistryState>>,
}

struct WebSocketRegistryState {
    next_id: u64,
    pending_starts: usize,
    max_retained: usize,
    connections: BTreeMap<u64, Arc<WebSocketConnection>>,
}

struct WebSocketConnection {
    state: Mutex<WebSocketConnectionState>,
}

struct WebSocketConnectionState {
    id: u64,
    url: String,
    host: String,
    started_at_ms: i64,
    status: SocketStatus,
    response_status: Option<i64>,
    response_headers: BTreeMap<String, String>,
    error: Option<String>,
    messages_sent: usize,
    messages_received: usize,
    socket: Option<BlockingWebSocket>,
}

#[derive(Clone)]
pub struct WebSocketListenerRegistry {
    inner: Arc<Mutex<WebSocketListenerRegistryState>>,
}

struct WebSocketListenerRegistryState {
    next_id: u64,
    pending_starts: usize,
    max_retained: usize,
    listeners: BTreeMap<u64, Arc<WebSocketListenerHandle>>,
}

struct WebSocketListenerHandle {
    state: Mutex<WebSocketListenerState>,
}

struct WebSocketListenerState {
    id: u64,
    host: String,
    port: u16,
    started_at_ms: i64,
    status: SocketStatus,
    local_addr: Option<String>,
    error: Option<String>,
    accepted_connections: usize,
    listener: Option<TcpListener>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WebSocketConnectRequest {
    pub url: String,
    pub host: String,
    pub port: u16,
    pub timeout: Duration,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WebSocketListenRequest {
    pub host: String,
    pub port: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WebSocketSnapshot {
    pub id: u64,
    pub url: String,
    pub host: String,
    pub started_at_ms: i64,
    pub status: String,
    pub connected: bool,
    pub closed: bool,
    pub response_status: Option<i64>,
    pub response_headers: BTreeMap<String, String>,
    pub error: Option<String>,
    pub messages_sent: usize,
    pub messages_received: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WebSocketListenerSnapshot {
    pub id: u64,
    pub host: String,
    pub port: u16,
    pub started_at_ms: i64,
    pub status: String,
    pub listening: bool,
    pub closed: bool,
    pub local_addr: Option<String>,
    pub error: Option<String>,
    pub accepted_connections: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WebSocketRead {
    pub snapshot: WebSocketSnapshot,
    pub message_type: String,
    pub message: String,
    pub bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SocketRuntimeError {
    pub kind: &'static str,
    pub message: String,
}

impl SocketRuntimeError {
    fn new(kind: &'static str, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

impl TcpSocketRegistry {
    pub fn new() -> Self {
        Self::with_max_retained(MAX_RETAINED_TCP_CONNECTIONS)
    }

    pub fn with_max_retained(max_retained: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(TcpSocketRegistryState {
                next_id: 0,
                pending_starts: 0,
                max_retained,
                connections: BTreeMap::new(),
            })),
        }
    }

    pub fn connect(
        &self,
        request: TcpConnectRequest,
    ) -> Result<TcpSocketSnapshot, SocketRuntimeError> {
        let id = self.reserve_connection_slot()?;
        let addresses = match (request.host.as_str(), request.port).to_socket_addrs() {
            Ok(addresses) => addresses.collect::<Vec<_>>(),
            Err(error) => {
                self.release_pending_start();
                return Err(SocketRuntimeError::new("TcpSocketError", error.to_string()));
            }
        };
        if addresses.is_empty() {
            self.release_pending_start();
            return Err(SocketRuntimeError::new(
                "TcpSocketError",
                format!(
                    "no socket addresses resolved for {}:{}",
                    request.host, request.port
                ),
            ));
        }

        let mut last_error = None;
        for address in addresses {
            match TcpStream::connect_timeout(&address, request.timeout) {
                Ok(stream) => {
                    if let Err(error) = stream.set_nodelay(request.nodelay) {
                        self.release_pending_start();
                        return Err(SocketRuntimeError::new("TcpSocketError", error.to_string()));
                    }
                    let local_addr = stream.local_addr().ok().map(|addr| addr.to_string());
                    let peer_addr = stream.peer_addr().ok().map(|addr| addr.to_string());
                    let connection = Arc::new(TcpConnection {
                        state: Mutex::new(TcpConnectionState {
                            id,
                            host: request.host.clone(),
                            port: request.port,
                            started_at_ms: now_millis(),
                            status: SocketStatus::Connected,
                            local_addr,
                            peer_addr,
                            error: None,
                            bytes_read: 0,
                            bytes_written: 0,
                            stream: Some(stream),
                        }),
                    });
                    let snapshot = connection.snapshot();
                    self.finish_connection_start(id, connection);
                    return Ok(snapshot);
                }
                Err(error) => last_error = Some(error),
            }
        }

        self.release_pending_start();
        Err(SocketRuntimeError::new(
            "TcpSocketError",
            last_error
                .map(|error| error.to_string())
                .unwrap_or_else(|| "TCP connection failed".to_string()),
        ))
    }

    pub fn retain_accepted(
        &self,
        host: String,
        port: u16,
        stream: TcpStream,
    ) -> Result<TcpSocketSnapshot, SocketRuntimeError> {
        let id = self.reserve_connection_slot()?;
        let local_addr = stream.local_addr().ok().map(|addr| addr.to_string());
        let peer_addr = stream.peer_addr().ok().map(|addr| addr.to_string());
        let connection = Arc::new(TcpConnection {
            state: Mutex::new(TcpConnectionState {
                id,
                host,
                port,
                started_at_ms: now_millis(),
                status: SocketStatus::Connected,
                local_addr,
                peer_addr,
                error: None,
                bytes_read: 0,
                bytes_written: 0,
                stream: Some(stream),
            }),
        });
        let snapshot = connection.snapshot();
        self.finish_connection_start(id, connection);
        Ok(snapshot)
    }

    pub fn connections(&self) -> Vec<TcpSocketSnapshot> {
        self.inner
            .lock()
            .expect("TCP socket registry lock should not be poisoned")
            .connections
            .values()
            .map(|connection| connection.snapshot())
            .collect()
    }

    pub fn connection(&self, id: u64) -> Option<TcpSocketSnapshot> {
        self.connection_arc(id)
            .map(|connection| connection.snapshot())
    }

    pub fn write(
        &self,
        id: u64,
        data: &str,
        timeout: Duration,
    ) -> Option<Result<TcpSocketSnapshot, SocketRuntimeError>> {
        let connection = self.connection_arc(id)?;
        Some(connection.write(data.as_bytes(), timeout))
    }

    pub fn read(
        &self,
        id: u64,
        max_bytes: usize,
        timeout: Duration,
    ) -> Option<Result<TcpSocketRead, SocketRuntimeError>> {
        let connection = self.connection_arc(id)?;
        Some(connection.read(max_bytes, timeout))
    }

    pub fn close(&self, id: u64) -> Option<Result<TcpSocketSnapshot, SocketRuntimeError>> {
        let connection = self.connection_arc(id)?;
        Some(connection.close())
    }

    pub fn release(&self, id: u64) -> Result<bool, SocketRuntimeError> {
        let Some(connection) = self.connection_arc(id) else {
            return Ok(false);
        };
        if connection.connected() {
            return Err(SocketRuntimeError::new(
                "TcpSocketOpen",
                format!("TCP socket {id} is still connected; close it before tcp_release"),
            ));
        }
        Ok(self
            .inner
            .lock()
            .expect("TCP socket registry lock should not be poisoned")
            .connections
            .remove(&id)
            .is_some())
    }

    pub fn len(&self) -> usize {
        self.inner
            .lock()
            .expect("TCP socket registry lock should not be poisoned")
            .connections
            .len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner
            .lock()
            .expect("TCP socket registry lock should not be poisoned")
            .connections
            .is_empty()
    }

    fn connection_arc(&self, id: u64) -> Option<Arc<TcpConnection>> {
        self.inner
            .lock()
            .expect("TCP socket registry lock should not be poisoned")
            .connections
            .get(&id)
            .cloned()
    }

    fn reserve_connection_slot(&self) -> Result<u64, SocketRuntimeError> {
        let mut state = self
            .inner
            .lock()
            .expect("TCP socket registry lock should not be poisoned");
        if state.connections.len() + state.pending_starts >= state.max_retained {
            return Err(SocketRuntimeError::new(
                "RegistryFull",
                format!(
                    "TCP socket registry retained connection limit of {} reached; release closed TCP sockets before starting another connection",
                    state.max_retained
                ),
            ));
        }
        let id = state.next_id;
        state.next_id += 1;
        state.pending_starts += 1;
        Ok(id)
    }

    fn release_pending_start(&self) {
        let mut state = self
            .inner
            .lock()
            .expect("TCP socket registry lock should not be poisoned");
        state.pending_starts = state.pending_starts.saturating_sub(1);
    }

    fn finish_connection_start(&self, id: u64, connection: Arc<TcpConnection>) {
        let mut state = self
            .inner
            .lock()
            .expect("TCP socket registry lock should not be poisoned");
        state.pending_starts = state.pending_starts.saturating_sub(1);
        state.connections.insert(id, connection);
    }
}

impl Default for TcpSocketRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl TcpListenerRegistry {
    pub fn new() -> Self {
        Self::with_max_retained(MAX_RETAINED_TCP_LISTENERS)
    }

    pub fn with_max_retained(max_retained: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(TcpListenerRegistryState {
                next_id: 0,
                pending_starts: 0,
                max_retained,
                listeners: BTreeMap::new(),
            })),
        }
    }

    pub fn listen(
        &self,
        request: TcpListenRequest,
    ) -> Result<TcpListenerSnapshot, SocketRuntimeError> {
        let id = self.reserve_listener_slot()?;
        let listener = match TcpListener::bind((request.host.as_str(), request.port)) {
            Ok(listener) => listener,
            Err(error) => {
                self.release_pending_start();
                return Err(SocketRuntimeError::new(
                    "TcpListenerError",
                    error.to_string(),
                ));
            }
        };
        if let Err(error) = listener.set_nonblocking(true) {
            self.release_pending_start();
            return Err(SocketRuntimeError::new(
                "TcpListenerError",
                error.to_string(),
            ));
        }
        let local_addr = listener.local_addr().ok();
        let port = local_addr.map(|addr| addr.port()).unwrap_or(request.port);
        let listener = Arc::new(TcpListenerHandle {
            state: Mutex::new(TcpListenerState {
                id,
                host: request.host,
                port,
                started_at_ms: now_millis(),
                status: SocketStatus::Listening,
                local_addr: local_addr.map(|addr| addr.to_string()),
                error: None,
                accepted_connections: 0,
                nodelay: request.nodelay,
                listener: Some(listener),
            }),
        });
        let snapshot = listener.snapshot();
        self.finish_listener_start(id, listener);
        Ok(snapshot)
    }

    pub fn listeners(&self) -> Vec<TcpListenerSnapshot> {
        self.inner
            .lock()
            .expect("TCP listener registry lock should not be poisoned")
            .listeners
            .values()
            .map(|listener| listener.snapshot())
            .collect()
    }

    pub fn listener(&self, id: u64) -> Option<TcpListenerSnapshot> {
        self.listener_arc(id).map(|listener| listener.snapshot())
    }

    pub fn accept(
        &self,
        id: u64,
        timeout: Duration,
        tcp_registry: &TcpSocketRegistry,
    ) -> Option<Result<TcpSocketSnapshot, SocketRuntimeError>> {
        let listener = self.listener_arc(id)?;
        Some(listener.accept(timeout, tcp_registry))
    }

    pub fn close(&self, id: u64) -> Option<Result<TcpListenerSnapshot, SocketRuntimeError>> {
        let listener = self.listener_arc(id)?;
        Some(listener.close())
    }

    pub fn release(&self, id: u64) -> Result<bool, SocketRuntimeError> {
        let Some(listener) = self.listener_arc(id) else {
            return Ok(false);
        };
        if listener.listening() {
            return Err(SocketRuntimeError::new(
                "TcpListenerOpen",
                format!(
                    "TCP listener {id} is still listening; close it before tcp_listener_release"
                ),
            ));
        }
        Ok(self
            .inner
            .lock()
            .expect("TCP listener registry lock should not be poisoned")
            .listeners
            .remove(&id)
            .is_some())
    }

    pub fn len(&self) -> usize {
        self.inner
            .lock()
            .expect("TCP listener registry lock should not be poisoned")
            .listeners
            .len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner
            .lock()
            .expect("TCP listener registry lock should not be poisoned")
            .listeners
            .is_empty()
    }

    fn listener_arc(&self, id: u64) -> Option<Arc<TcpListenerHandle>> {
        self.inner
            .lock()
            .expect("TCP listener registry lock should not be poisoned")
            .listeners
            .get(&id)
            .cloned()
    }

    fn reserve_listener_slot(&self) -> Result<u64, SocketRuntimeError> {
        let mut state = self
            .inner
            .lock()
            .expect("TCP listener registry lock should not be poisoned");
        if state.listeners.len() + state.pending_starts >= state.max_retained {
            return Err(SocketRuntimeError::new(
                "RegistryFull",
                format!(
                    "TCP listener registry retained listener limit of {} reached; release closed TCP listeners before starting another listener",
                    state.max_retained
                ),
            ));
        }
        let id = state.next_id;
        state.next_id += 1;
        state.pending_starts += 1;
        Ok(id)
    }

    fn release_pending_start(&self) {
        let mut state = self
            .inner
            .lock()
            .expect("TCP listener registry lock should not be poisoned");
        state.pending_starts = state.pending_starts.saturating_sub(1);
    }

    fn finish_listener_start(&self, id: u64, listener: Arc<TcpListenerHandle>) {
        let mut state = self
            .inner
            .lock()
            .expect("TCP listener registry lock should not be poisoned");
        state.pending_starts = state.pending_starts.saturating_sub(1);
        state.listeners.insert(id, listener);
    }
}

impl Default for TcpListenerRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl TcpListenerHandle {
    fn snapshot(&self) -> TcpListenerSnapshot {
        self.state
            .lock()
            .expect("TCP listener lock should not be poisoned")
            .snapshot()
    }

    fn listening(&self) -> bool {
        self.state
            .lock()
            .expect("TCP listener lock should not be poisoned")
            .status
            == SocketStatus::Listening
    }

    fn accept(
        &self,
        timeout: Duration,
        tcp_registry: &TcpSocketRegistry,
    ) -> Result<TcpSocketSnapshot, SocketRuntimeError> {
        let (listener, nodelay) = {
            let mut state = self
                .state
                .lock()
                .expect("TCP listener lock should not be poisoned");
            if state.status != SocketStatus::Listening {
                return Err(SocketRuntimeError::new(
                    "TcpListenerClosed",
                    "cannot accept from a closed TCP listener",
                ));
            }
            let Some(listener) = state.listener.as_ref() else {
                return Err(SocketRuntimeError::new(
                    "TcpListenerClosed",
                    "cannot accept from a closed TCP listener",
                ));
            };
            match listener.try_clone() {
                Ok(listener) => (listener, state.nodelay),
                Err(error) => return Err(state.fail(error)),
            }
        };

        let (stream, peer_addr) = accept_tcp_stream(&listener, timeout)?;
        if let Err(error) = stream.set_nonblocking(false) {
            return Err(SocketRuntimeError::new(
                "TcpListenerError",
                error.to_string(),
            ));
        }
        if let Err(error) = stream.set_nodelay(nodelay) {
            return Err(SocketRuntimeError::new(
                "TcpListenerError",
                error.to_string(),
            ));
        }
        let snapshot =
            tcp_registry.retain_accepted(peer_addr.ip().to_string(), peer_addr.port(), stream)?;
        let mut state = self
            .state
            .lock()
            .expect("TCP listener lock should not be poisoned");
        state.accepted_connections = state.accepted_connections.saturating_add(1);
        Ok(snapshot)
    }

    fn close(&self) -> Result<TcpListenerSnapshot, SocketRuntimeError> {
        let mut state = self
            .state
            .lock()
            .expect("TCP listener lock should not be poisoned");
        state.listener = None;
        state.status = SocketStatus::Closed;
        Ok(state.snapshot())
    }
}

impl TcpListenerState {
    fn snapshot(&self) -> TcpListenerSnapshot {
        TcpListenerSnapshot {
            id: self.id,
            host: self.host.clone(),
            port: self.port,
            started_at_ms: self.started_at_ms,
            status: socket_status_name(&self.status).to_string(),
            listening: self.status == SocketStatus::Listening,
            closed: self.status == SocketStatus::Closed,
            local_addr: self.local_addr.clone(),
            error: self.error.clone(),
            accepted_connections: self.accepted_connections,
        }
    }

    fn fail(&mut self, error: std::io::Error) -> SocketRuntimeError {
        self.status = SocketStatus::Failed;
        self.listener = None;
        self.error = Some(error.to_string());
        SocketRuntimeError::new("TcpListenerError", error.to_string())
    }
}

impl TcpConnection {
    fn snapshot(&self) -> TcpSocketSnapshot {
        self.state
            .lock()
            .expect("TCP socket lock should not be poisoned")
            .snapshot()
    }

    fn connected(&self) -> bool {
        self.state
            .lock()
            .expect("TCP socket lock should not be poisoned")
            .status
            == SocketStatus::Connected
    }

    fn write(
        &self,
        data: &[u8],
        timeout: Duration,
    ) -> Result<TcpSocketSnapshot, SocketRuntimeError> {
        let mut state = self
            .state
            .lock()
            .expect("TCP socket lock should not be poisoned");
        let Some(stream) = state.stream.as_mut() else {
            return Err(SocketRuntimeError::new(
                "TcpSocketClosed",
                "cannot write to a closed TCP socket",
            ));
        };
        if let Err(error) = stream.set_write_timeout(Some(timeout)) {
            return Err(state.fail(error));
        }
        if let Err(error) = stream.write_all(data).and_then(|()| stream.flush()) {
            return Err(state.fail(error));
        }
        state.bytes_written = state.bytes_written.saturating_add(data.len());
        Ok(state.snapshot())
    }

    fn read(
        &self,
        max_bytes: usize,
        timeout: Duration,
    ) -> Result<TcpSocketRead, SocketRuntimeError> {
        let mut state = self
            .state
            .lock()
            .expect("TCP socket lock should not be poisoned");
        let Some(stream) = state.stream.as_mut() else {
            return Err(SocketRuntimeError::new(
                "TcpSocketClosed",
                "cannot read from a closed TCP socket",
            ));
        };
        if let Err(error) = stream.set_read_timeout(Some(timeout)) {
            return Err(state.fail(error));
        }
        let mut buffer = vec![0_u8; max_bytes];
        match stream.read(&mut buffer) {
            Ok(0) => {
                state.stream = None;
                state.status = SocketStatus::Closed;
                Ok(TcpSocketRead {
                    snapshot: state.snapshot(),
                    data: String::new(),
                    bytes: 0,
                })
            }
            Ok(count) => {
                state.bytes_read = state.bytes_read.saturating_add(count);
                Ok(TcpSocketRead {
                    snapshot: state.snapshot(),
                    data: String::from_utf8_lossy(&buffer[..count]).into_owned(),
                    bytes: count,
                })
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                Ok(TcpSocketRead {
                    snapshot: state.snapshot(),
                    data: String::new(),
                    bytes: 0,
                })
            }
            Err(error) => Err(state.fail(error)),
        }
    }

    fn close(&self) -> Result<TcpSocketSnapshot, SocketRuntimeError> {
        let mut state = self
            .state
            .lock()
            .expect("TCP socket lock should not be poisoned");
        if let Some(stream) = state.stream.take() {
            let _ = stream.shutdown(Shutdown::Both);
        }
        state.status = SocketStatus::Closed;
        Ok(state.snapshot())
    }
}

impl TcpConnectionState {
    fn snapshot(&self) -> TcpSocketSnapshot {
        TcpSocketSnapshot {
            id: self.id,
            host: self.host.clone(),
            port: self.port,
            started_at_ms: self.started_at_ms,
            status: socket_status_name(&self.status).to_string(),
            connected: self.status == SocketStatus::Connected,
            closed: self.status == SocketStatus::Closed,
            local_addr: self.local_addr.clone(),
            peer_addr: self.peer_addr.clone(),
            error: self.error.clone(),
            bytes_read: self.bytes_read,
            bytes_written: self.bytes_written,
        }
    }

    fn fail(&mut self, error: std::io::Error) -> SocketRuntimeError {
        self.status = SocketStatus::Failed;
        self.stream = None;
        self.error = Some(error.to_string());
        SocketRuntimeError::new("TcpSocketError", error.to_string())
    }
}

impl WebSocketRegistry {
    pub fn new() -> Self {
        Self::with_max_retained(MAX_RETAINED_WEBSOCKET_CONNECTIONS)
    }

    pub fn with_max_retained(max_retained: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(WebSocketRegistryState {
                next_id: 0,
                pending_starts: 0,
                max_retained,
                connections: BTreeMap::new(),
            })),
        }
    }

    pub fn connect(
        &self,
        request: WebSocketConnectRequest,
    ) -> Result<WebSocketSnapshot, SocketRuntimeError> {
        let id = self.reserve_connection_slot()?;
        let addresses = match (request.host.as_str(), request.port).to_socket_addrs() {
            Ok(addresses) => addresses.collect::<Vec<_>>(),
            Err(error) => {
                self.release_pending_start();
                return Err(SocketRuntimeError::new("WebSocketError", error.to_string()));
            }
        };
        if addresses.is_empty() {
            self.release_pending_start();
            return Err(SocketRuntimeError::new(
                "WebSocketError",
                format!(
                    "no socket addresses resolved for {}:{}",
                    request.host, request.port
                ),
            ));
        }

        let mut last_error = None;
        let mut connection_result = None;
        for address in addresses {
            match TcpStream::connect_timeout(&address, request.timeout) {
                Ok(stream) => {
                    if let Err(error) = stream
                        .set_nodelay(true)
                        .and_then(|()| stream.set_read_timeout(Some(request.timeout)))
                        .and_then(|()| stream.set_write_timeout(Some(request.timeout)))
                    {
                        self.release_pending_start();
                        return Err(SocketRuntimeError::new("WebSocketError", error.to_string()));
                    }
                    match client_tls_with_config(request.url.as_str(), stream, None, None) {
                        Ok(connection) => {
                            connection_result = Some(connection);
                            break;
                        }
                        Err(error) => {
                            self.release_pending_start();
                            return Err(SocketRuntimeError::new(
                                "WebSocketError",
                                error.to_string(),
                            ));
                        }
                    }
                }
                Err(error) => last_error = Some(error),
            }
        }

        let Some((mut socket, response)) = connection_result else {
            self.release_pending_start();
            return Err(SocketRuntimeError::new(
                "WebSocketError",
                last_error
                    .map(|error| error.to_string())
                    .unwrap_or_else(|| "WebSocket connection failed".to_string()),
            ));
        };

        set_websocket_timeouts(&mut socket, request.timeout)?;
        let response_headers = response
            .headers()
            .iter()
            .map(|(name, value)| {
                (
                    name.as_str().to_ascii_lowercase(),
                    value.to_str().unwrap_or_default().to_string(),
                )
            })
            .collect::<BTreeMap<_, _>>();
        let connection = Arc::new(WebSocketConnection {
            state: Mutex::new(WebSocketConnectionState {
                id,
                url: request.url,
                host: request.host,
                started_at_ms: now_millis(),
                status: SocketStatus::Connected,
                response_status: Some(i64::from(response.status().as_u16())),
                response_headers,
                error: None,
                messages_sent: 0,
                messages_received: 0,
                socket: Some(socket),
            }),
        });
        let snapshot = connection.snapshot();
        self.finish_connection_start(id, connection);
        Ok(snapshot)
    }

    pub fn retain_accepted(
        &self,
        url: String,
        host: String,
        socket: BlockingWebSocket,
    ) -> Result<WebSocketSnapshot, SocketRuntimeError> {
        let id = self.reserve_connection_slot()?;
        let connection = Arc::new(WebSocketConnection {
            state: Mutex::new(WebSocketConnectionState {
                id,
                url,
                host,
                started_at_ms: now_millis(),
                status: SocketStatus::Connected,
                response_status: Some(101),
                response_headers: BTreeMap::new(),
                error: None,
                messages_sent: 0,
                messages_received: 0,
                socket: Some(socket),
            }),
        });
        let snapshot = connection.snapshot();
        self.finish_connection_start(id, connection);
        Ok(snapshot)
    }

    pub fn connections(&self) -> Vec<WebSocketSnapshot> {
        self.inner
            .lock()
            .expect("WebSocket registry lock should not be poisoned")
            .connections
            .values()
            .map(|connection| connection.snapshot())
            .collect()
    }

    pub fn connection(&self, id: u64) -> Option<WebSocketSnapshot> {
        self.connection_arc(id)
            .map(|connection| connection.snapshot())
    }

    pub fn send(
        &self,
        id: u64,
        message: &str,
        timeout: Duration,
    ) -> Option<Result<WebSocketSnapshot, SocketRuntimeError>> {
        let connection = self.connection_arc(id)?;
        Some(connection.send(message, timeout))
    }

    pub fn read(
        &self,
        id: u64,
        timeout: Duration,
    ) -> Option<Result<WebSocketRead, SocketRuntimeError>> {
        let connection = self.connection_arc(id)?;
        Some(connection.read(timeout))
    }

    pub fn close(&self, id: u64) -> Option<Result<WebSocketSnapshot, SocketRuntimeError>> {
        let connection = self.connection_arc(id)?;
        Some(connection.close())
    }

    pub fn release(&self, id: u64) -> Result<bool, SocketRuntimeError> {
        let Some(connection) = self.connection_arc(id) else {
            return Ok(false);
        };
        if connection.connected() {
            return Err(SocketRuntimeError::new(
                "WebSocketOpen",
                format!("WebSocket {id} is still connected; close it before ws_release"),
            ));
        }
        Ok(self
            .inner
            .lock()
            .expect("WebSocket registry lock should not be poisoned")
            .connections
            .remove(&id)
            .is_some())
    }

    pub fn len(&self) -> usize {
        self.inner
            .lock()
            .expect("WebSocket registry lock should not be poisoned")
            .connections
            .len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner
            .lock()
            .expect("WebSocket registry lock should not be poisoned")
            .connections
            .is_empty()
    }

    fn connection_arc(&self, id: u64) -> Option<Arc<WebSocketConnection>> {
        self.inner
            .lock()
            .expect("WebSocket registry lock should not be poisoned")
            .connections
            .get(&id)
            .cloned()
    }

    fn reserve_connection_slot(&self) -> Result<u64, SocketRuntimeError> {
        let mut state = self
            .inner
            .lock()
            .expect("WebSocket registry lock should not be poisoned");
        if state.connections.len() + state.pending_starts >= state.max_retained {
            return Err(SocketRuntimeError::new(
                "RegistryFull",
                format!(
                    "WebSocket registry retained connection limit of {} reached; release closed WebSockets before starting another connection",
                    state.max_retained
                ),
            ));
        }
        let id = state.next_id;
        state.next_id += 1;
        state.pending_starts += 1;
        Ok(id)
    }

    fn release_pending_start(&self) {
        let mut state = self
            .inner
            .lock()
            .expect("WebSocket registry lock should not be poisoned");
        state.pending_starts = state.pending_starts.saturating_sub(1);
    }

    fn finish_connection_start(&self, id: u64, connection: Arc<WebSocketConnection>) {
        let mut state = self
            .inner
            .lock()
            .expect("WebSocket registry lock should not be poisoned");
        state.pending_starts = state.pending_starts.saturating_sub(1);
        state.connections.insert(id, connection);
    }
}

impl Default for WebSocketRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl WebSocketListenerRegistry {
    pub fn new() -> Self {
        Self::with_max_retained(MAX_RETAINED_WEBSOCKET_LISTENERS)
    }

    pub fn with_max_retained(max_retained: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(WebSocketListenerRegistryState {
                next_id: 0,
                pending_starts: 0,
                max_retained,
                listeners: BTreeMap::new(),
            })),
        }
    }

    pub fn listen(
        &self,
        request: WebSocketListenRequest,
    ) -> Result<WebSocketListenerSnapshot, SocketRuntimeError> {
        let id = self.reserve_listener_slot()?;
        let listener = match TcpListener::bind((request.host.as_str(), request.port)) {
            Ok(listener) => listener,
            Err(error) => {
                self.release_pending_start();
                return Err(SocketRuntimeError::new(
                    "WebSocketListenerError",
                    error.to_string(),
                ));
            }
        };
        if let Err(error) = listener.set_nonblocking(true) {
            self.release_pending_start();
            return Err(SocketRuntimeError::new(
                "WebSocketListenerError",
                error.to_string(),
            ));
        }
        let local_addr = listener.local_addr().ok();
        let port = local_addr.map(|addr| addr.port()).unwrap_or(request.port);
        let listener = Arc::new(WebSocketListenerHandle {
            state: Mutex::new(WebSocketListenerState {
                id,
                host: request.host,
                port,
                started_at_ms: now_millis(),
                status: SocketStatus::Listening,
                local_addr: local_addr.map(|addr| addr.to_string()),
                error: None,
                accepted_connections: 0,
                listener: Some(listener),
            }),
        });
        let snapshot = listener.snapshot();
        self.finish_listener_start(id, listener);
        Ok(snapshot)
    }

    pub fn listeners(&self) -> Vec<WebSocketListenerSnapshot> {
        self.inner
            .lock()
            .expect("WebSocket listener registry lock should not be poisoned")
            .listeners
            .values()
            .map(|listener| listener.snapshot())
            .collect()
    }

    pub fn listener(&self, id: u64) -> Option<WebSocketListenerSnapshot> {
        self.listener_arc(id).map(|listener| listener.snapshot())
    }

    pub fn accept(
        &self,
        id: u64,
        timeout: Duration,
        websocket_registry: &WebSocketRegistry,
    ) -> Option<Result<WebSocketSnapshot, SocketRuntimeError>> {
        let listener = self.listener_arc(id)?;
        Some(listener.accept(timeout, websocket_registry))
    }

    pub fn close(&self, id: u64) -> Option<Result<WebSocketListenerSnapshot, SocketRuntimeError>> {
        let listener = self.listener_arc(id)?;
        Some(listener.close())
    }

    pub fn release(&self, id: u64) -> Result<bool, SocketRuntimeError> {
        let Some(listener) = self.listener_arc(id) else {
            return Ok(false);
        };
        if listener.listening() {
            return Err(SocketRuntimeError::new(
                "WebSocketListenerOpen",
                format!("WebSocket listener {id} is still listening; close it before ws_listener_release"),
            ));
        }
        Ok(self
            .inner
            .lock()
            .expect("WebSocket listener registry lock should not be poisoned")
            .listeners
            .remove(&id)
            .is_some())
    }

    pub fn len(&self) -> usize {
        self.inner
            .lock()
            .expect("WebSocket listener registry lock should not be poisoned")
            .listeners
            .len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner
            .lock()
            .expect("WebSocket listener registry lock should not be poisoned")
            .listeners
            .is_empty()
    }

    fn listener_arc(&self, id: u64) -> Option<Arc<WebSocketListenerHandle>> {
        self.inner
            .lock()
            .expect("WebSocket listener registry lock should not be poisoned")
            .listeners
            .get(&id)
            .cloned()
    }

    fn reserve_listener_slot(&self) -> Result<u64, SocketRuntimeError> {
        let mut state = self
            .inner
            .lock()
            .expect("WebSocket listener registry lock should not be poisoned");
        if state.listeners.len() + state.pending_starts >= state.max_retained {
            return Err(SocketRuntimeError::new(
                "RegistryFull",
                format!(
                    "WebSocket listener registry retained listener limit of {} reached; release closed WebSocket listeners before starting another listener",
                    state.max_retained
                ),
            ));
        }
        let id = state.next_id;
        state.next_id += 1;
        state.pending_starts += 1;
        Ok(id)
    }

    fn release_pending_start(&self) {
        let mut state = self
            .inner
            .lock()
            .expect("WebSocket listener registry lock should not be poisoned");
        state.pending_starts = state.pending_starts.saturating_sub(1);
    }

    fn finish_listener_start(&self, id: u64, listener: Arc<WebSocketListenerHandle>) {
        let mut state = self
            .inner
            .lock()
            .expect("WebSocket listener registry lock should not be poisoned");
        state.pending_starts = state.pending_starts.saturating_sub(1);
        state.listeners.insert(id, listener);
    }
}

impl Default for WebSocketListenerRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl WebSocketListenerHandle {
    fn snapshot(&self) -> WebSocketListenerSnapshot {
        self.state
            .lock()
            .expect("WebSocket listener lock should not be poisoned")
            .snapshot()
    }

    fn listening(&self) -> bool {
        self.state
            .lock()
            .expect("WebSocket listener lock should not be poisoned")
            .status
            == SocketStatus::Listening
    }

    fn accept(
        &self,
        timeout: Duration,
        websocket_registry: &WebSocketRegistry,
    ) -> Result<WebSocketSnapshot, SocketRuntimeError> {
        let listener = {
            let mut state = self
                .state
                .lock()
                .expect("WebSocket listener lock should not be poisoned");
            if state.status != SocketStatus::Listening {
                return Err(SocketRuntimeError::new(
                    "WebSocketListenerClosed",
                    "cannot accept from a closed WebSocket listener",
                ));
            }
            let Some(listener) = state.listener.as_ref() else {
                return Err(SocketRuntimeError::new(
                    "WebSocketListenerClosed",
                    "cannot accept from a closed WebSocket listener",
                ));
            };
            match listener.try_clone() {
                Ok(listener) => listener,
                Err(error) => return Err(state.fail(error)),
            }
        };

        let (stream, peer_addr) = accept_tcp_stream(&listener, timeout)?;
        if let Err(error) = stream
            .set_nonblocking(false)
            .and_then(|()| stream.set_read_timeout(Some(timeout)))
            .and_then(|()| stream.set_write_timeout(Some(timeout)))
        {
            return Err(SocketRuntimeError::new(
                "WebSocketListenerError",
                error.to_string(),
            ));
        }
        let socket = match tungstenite::accept(MaybeTlsStream::Plain(stream)) {
            Ok(socket) => socket,
            Err(error) => {
                return Err(SocketRuntimeError::new(
                    "WebSocketListenerError",
                    error.to_string(),
                ));
            }
        };
        let url = self
            .snapshot()
            .local_addr
            .map(|addr| format!("ws://{addr}"))
            .unwrap_or_else(|| "ws://unknown".to_string());
        let snapshot =
            websocket_registry.retain_accepted(url, peer_addr.ip().to_string(), socket)?;
        let mut state = self
            .state
            .lock()
            .expect("WebSocket listener lock should not be poisoned");
        state.accepted_connections = state.accepted_connections.saturating_add(1);
        Ok(snapshot)
    }

    fn close(&self) -> Result<WebSocketListenerSnapshot, SocketRuntimeError> {
        let mut state = self
            .state
            .lock()
            .expect("WebSocket listener lock should not be poisoned");
        state.listener = None;
        state.status = SocketStatus::Closed;
        Ok(state.snapshot())
    }
}

impl WebSocketListenerState {
    fn snapshot(&self) -> WebSocketListenerSnapshot {
        WebSocketListenerSnapshot {
            id: self.id,
            host: self.host.clone(),
            port: self.port,
            started_at_ms: self.started_at_ms,
            status: socket_status_name(&self.status).to_string(),
            listening: self.status == SocketStatus::Listening,
            closed: self.status == SocketStatus::Closed,
            local_addr: self.local_addr.clone(),
            error: self.error.clone(),
            accepted_connections: self.accepted_connections,
        }
    }

    fn fail(&mut self, error: std::io::Error) -> SocketRuntimeError {
        self.status = SocketStatus::Failed;
        self.listener = None;
        self.error = Some(error.to_string());
        SocketRuntimeError::new("WebSocketListenerError", error.to_string())
    }
}

impl WebSocketConnection {
    fn snapshot(&self) -> WebSocketSnapshot {
        self.state
            .lock()
            .expect("WebSocket lock should not be poisoned")
            .snapshot()
    }

    fn connected(&self) -> bool {
        self.state
            .lock()
            .expect("WebSocket lock should not be poisoned")
            .status
            == SocketStatus::Connected
    }

    fn send(
        &self,
        message: &str,
        timeout: Duration,
    ) -> Result<WebSocketSnapshot, SocketRuntimeError> {
        let mut state = self
            .state
            .lock()
            .expect("WebSocket lock should not be poisoned");
        let Some(socket) = state.socket.as_mut() else {
            return Err(SocketRuntimeError::new(
                "WebSocketClosed",
                "cannot send to a closed WebSocket",
            ));
        };
        set_websocket_timeouts(socket, timeout)?;
        if let Err(error) = socket.send(Message::Text(message.to_string().into())) {
            return Err(state.fail(error));
        }
        state.messages_sent = state.messages_sent.saturating_add(1);
        Ok(state.snapshot())
    }

    fn read(&self, timeout: Duration) -> Result<WebSocketRead, SocketRuntimeError> {
        let mut state = self
            .state
            .lock()
            .expect("WebSocket lock should not be poisoned");
        let Some(socket) = state.socket.as_mut() else {
            return Err(SocketRuntimeError::new(
                "WebSocketClosed",
                "cannot read from a closed WebSocket",
            ));
        };
        set_websocket_timeouts(socket, timeout)?;
        match socket.read() {
            Ok(message) => {
                let (message_type, message, bytes) = websocket_message_parts(message);
                if message_type == "close" {
                    state.status = SocketStatus::Closed;
                    state.socket = None;
                } else {
                    state.messages_received = state.messages_received.saturating_add(1);
                }
                Ok(WebSocketRead {
                    snapshot: state.snapshot(),
                    message_type,
                    message,
                    bytes,
                })
            }
            Err(tungstenite::Error::Io(error))
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                Ok(WebSocketRead {
                    snapshot: state.snapshot(),
                    message_type: "none".to_string(),
                    message: String::new(),
                    bytes: 0,
                })
            }
            Err(tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed) => {
                state.status = SocketStatus::Closed;
                state.socket = None;
                Ok(WebSocketRead {
                    snapshot: state.snapshot(),
                    message_type: "close".to_string(),
                    message: String::new(),
                    bytes: 0,
                })
            }
            Err(error) => Err(state.fail(error)),
        }
    }

    fn close(&self) -> Result<WebSocketSnapshot, SocketRuntimeError> {
        let mut state = self
            .state
            .lock()
            .expect("WebSocket lock should not be poisoned");
        if let Some(mut socket) = state.socket.take() {
            let _ = socket.close(None);
        }
        state.status = SocketStatus::Closed;
        Ok(state.snapshot())
    }
}

impl WebSocketConnectionState {
    fn snapshot(&self) -> WebSocketSnapshot {
        WebSocketSnapshot {
            id: self.id,
            url: self.url.clone(),
            host: self.host.clone(),
            started_at_ms: self.started_at_ms,
            status: socket_status_name(&self.status).to_string(),
            connected: self.status == SocketStatus::Connected,
            closed: self.status == SocketStatus::Closed,
            response_status: self.response_status,
            response_headers: self.response_headers.clone(),
            error: self.error.clone(),
            messages_sent: self.messages_sent,
            messages_received: self.messages_received,
        }
    }

    fn fail(&mut self, error: tungstenite::Error) -> SocketRuntimeError {
        self.status = SocketStatus::Failed;
        self.socket = None;
        self.error = Some(error.to_string());
        SocketRuntimeError::new("WebSocketError", error.to_string())
    }
}

fn websocket_message_parts(message: Message) -> (String, String, usize) {
    match message {
        Message::Text(text) => {
            let text = text.to_string();
            let bytes = text.len();
            ("text".to_string(), text, bytes)
        }
        Message::Binary(bytes) => {
            let len = bytes.len();
            (
                "binary".to_string(),
                String::from_utf8_lossy(&bytes).into_owned(),
                len,
            )
        }
        Message::Ping(bytes) => {
            let len = bytes.len();
            (
                "ping".to_string(),
                String::from_utf8_lossy(&bytes).into_owned(),
                len,
            )
        }
        Message::Pong(bytes) => {
            let len = bytes.len();
            (
                "pong".to_string(),
                String::from_utf8_lossy(&bytes).into_owned(),
                len,
            )
        }
        Message::Close(frame) => {
            let message = frame
                .map(|frame| frame.reason.to_string())
                .unwrap_or_default();
            let bytes = message.len();
            ("close".to_string(), message, bytes)
        }
        Message::Frame(_) => ("frame".to_string(), String::new(), 0),
    }
}

fn set_websocket_timeouts(
    socket: &mut BlockingWebSocket,
    timeout: Duration,
) -> Result<(), SocketRuntimeError> {
    let stream = socket.get_mut();
    set_maybe_tls_read_timeout(stream, timeout)
        .and_then(|()| set_maybe_tls_write_timeout(stream, timeout))
        .map_err(|error| SocketRuntimeError::new("WebSocketError", error.to_string()))
}

fn set_maybe_tls_read_timeout(
    stream: &mut MaybeTlsStream<TcpStream>,
    timeout: Duration,
) -> std::io::Result<()> {
    match stream {
        MaybeTlsStream::Plain(stream) => stream.set_read_timeout(Some(timeout)),
        MaybeTlsStream::Rustls(stream) => stream.sock.set_read_timeout(Some(timeout)),
        _ => Ok(()),
    }
}

fn set_maybe_tls_write_timeout(
    stream: &mut MaybeTlsStream<TcpStream>,
    timeout: Duration,
) -> std::io::Result<()> {
    match stream {
        MaybeTlsStream::Plain(stream) => stream.set_write_timeout(Some(timeout)),
        MaybeTlsStream::Rustls(stream) => stream.sock.set_write_timeout(Some(timeout)),
        _ => Ok(()),
    }
}

fn socket_status_name(status: &SocketStatus) -> &'static str {
    match status {
        SocketStatus::Listening => "listening",
        SocketStatus::Connected => "connected",
        SocketStatus::Closed => "closed",
        SocketStatus::Failed => "failed",
    }
}

fn accept_tcp_stream(
    listener: &TcpListener,
    timeout: Duration,
) -> Result<(TcpStream, SocketAddr), SocketRuntimeError> {
    let start = Instant::now();
    loop {
        match listener.accept() {
            Ok((stream, peer_addr)) => return Ok((stream, peer_addr)),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                if start.elapsed() >= timeout {
                    return Err(SocketRuntimeError::new(
                        "SocketAcceptTimeout",
                        format!(
                            "timed out after {} ms waiting for socket connection",
                            timeout.as_millis()
                        ),
                    ));
                }
                let remaining = timeout.saturating_sub(start.elapsed());
                thread::sleep(Duration::from_millis(10).min(remaining));
            }
            Err(error) if error.kind() == std::io::ErrorKind::TimedOut => {
                return Err(SocketRuntimeError::new(
                    "SocketAcceptTimeout",
                    format!(
                        "timed out after {} ms waiting for socket connection",
                        timeout.as_millis()
                    ),
                ));
            }
            Err(error) => {
                return Err(SocketRuntimeError::new(
                    "SocketAcceptError",
                    error.to_string(),
                ));
            }
        }
    }
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(i64::MAX)
}
