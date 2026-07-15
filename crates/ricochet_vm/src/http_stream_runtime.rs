use std::collections::BTreeMap;
use std::io::Read;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ricochet_secrets::{PreparedSecretHttpRequest, SecretHttpResponseStream, SecretsHttpExecutor};
use serde_json::Value as JsonValue;

const HTTP_STREAM_CHUNK_BYTES: usize = 8192;
const MAX_RETAINED_HTTP_STREAMS: usize = 64;

#[derive(Clone)]
pub struct HttpStreamRegistry {
    inner: Arc<Mutex<HttpStreamRegistryState>>,
}

struct HttpStreamRegistryState {
    next_id: u64,
    max_retained: usize,
    streams: BTreeMap<u64, Arc<HttpStreamJob>>,
}

struct HttpStreamJob {
    state: Mutex<HttpStreamJobState>,
}

struct HttpStreamJobState {
    id: u64,
    method: String,
    url: String,
    started_at_ms: i64,
    status: HttpStreamStatus,
    status_code: Option<i64>,
    headers: BTreeMap<String, String>,
    error: Option<String>,
    body: Vec<u8>,
    body_max_bytes: usize,
    body_truncated: bool,
    cancel_requested: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum HttpStreamStatus {
    Connecting,
    Running,
    Completed,
    Failed,
    Cancelled,
}

pub struct HttpStreamRequest {
    pub method: reqwest::Method,
    pub url: String,
    pub headers: reqwest::header::HeaderMap,
    pub json: Option<JsonValue>,
    pub body: Option<String>,
    pub timeout: Duration,
    pub max_response_bytes: usize,
    pub resolved_destination: Option<HttpResolvedDestination>,
    pub prepared_secret_request: Option<PreparedSecretHttpRequest>,
    pub secrets_http_executor: SecretsHttpExecutor,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpResolvedDestination {
    pub host: String,
    pub addresses: Vec<SocketAddr>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpStreamSnapshot {
    pub id: u64,
    pub method: String,
    pub url: String,
    pub started_at_ms: i64,
    pub status: String,
    pub running: bool,
    pub success: bool,
    pub status_code: Option<i64>,
    pub headers: BTreeMap<String, String>,
    pub error: Option<String>,
    pub body_len: usize,
    pub body_truncated: bool,
    pub cancelled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpStreamRead {
    pub snapshot: HttpStreamSnapshot,
    pub body: String,
    pub from_offset: usize,
    pub next_offset: usize,
    pub offset: usize,
    pub bytes_len: usize,
    pub done: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpStreamRuntimeError {
    pub kind: &'static str,
    pub message: String,
}

impl HttpStreamRuntimeError {
    fn new(kind: &'static str, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

impl HttpStreamRegistry {
    pub fn new() -> Self {
        Self::with_max_retained(MAX_RETAINED_HTTP_STREAMS)
    }

    pub fn with_max_retained(max_retained: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(HttpStreamRegistryState {
                next_id: 0,
                max_retained,
                streams: BTreeMap::new(),
            })),
        }
    }

    pub fn start(
        &self,
        request: HttpStreamRequest,
    ) -> Result<HttpStreamSnapshot, HttpStreamRuntimeError> {
        let (job, snapshot) = {
            let mut state = self
                .inner
                .lock()
                .expect("HTTP stream registry lock should not be poisoned");
            if state.streams.len() >= state.max_retained {
                return Err(HttpStreamRuntimeError::new(
                    "RegistryFull",
                    format!(
                        "HTTP stream registry retained stream limit of {} reached; release completed HTTP streams before starting another stream",
                        state.max_retained
                    ),
                ));
            }
            let id = state.next_id;
            state.next_id += 1;
            let job = Arc::new(HttpStreamJob {
                state: Mutex::new(HttpStreamJobState {
                    id,
                    method: request.method.as_str().to_string(),
                    url: request.url.clone(),
                    started_at_ms: now_millis(),
                    status: HttpStreamStatus::Connecting,
                    status_code: None,
                    headers: BTreeMap::new(),
                    error: None,
                    body: Vec::new(),
                    body_max_bytes: request.max_response_bytes,
                    body_truncated: false,
                    cancel_requested: false,
                }),
            });
            let snapshot = job.snapshot();
            state.streams.insert(id, job.clone());
            (job, snapshot)
        };
        spawn_http_stream_worker(job, request);
        Ok(snapshot)
    }

    pub fn streams(&self) -> Vec<HttpStreamSnapshot> {
        self.inner
            .lock()
            .expect("HTTP stream registry lock should not be poisoned")
            .streams
            .values()
            .map(|job| job.snapshot())
            .collect()
    }

    pub fn stream(&self, id: u64) -> Option<HttpStreamSnapshot> {
        self.stream_arc(id).map(|job| job.snapshot())
    }

    pub fn read(&self, id: u64, offset: usize, max_bytes: Option<usize>) -> Option<HttpStreamRead> {
        let job = self.stream_arc(id)?;
        Some(job.read(offset, max_bytes))
    }

    pub fn cancel(&self, id: u64) -> Option<HttpStreamSnapshot> {
        let job = self.stream_arc(id)?;
        job.cancel();
        Some(job.snapshot())
    }

    pub fn release(&self, id: u64) -> Result<bool, HttpStreamRuntimeError> {
        let Some(job) = self.stream_arc(id) else {
            return Ok(false);
        };
        if job.running() {
            return Err(HttpStreamRuntimeError::new(
                "HttpStreamRunning",
                format!(
                    "HTTP stream {id} is still running; cancel or wait before http_stream_release"
                ),
            ));
        }
        Ok(self
            .inner
            .lock()
            .expect("HTTP stream registry lock should not be poisoned")
            .streams
            .remove(&id)
            .is_some())
    }

    pub fn len(&self) -> usize {
        self.inner
            .lock()
            .expect("HTTP stream registry lock should not be poisoned")
            .streams
            .len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn stream_arc(&self, id: u64) -> Option<Arc<HttpStreamJob>> {
        self.inner
            .lock()
            .expect("HTTP stream registry lock should not be poisoned")
            .streams
            .get(&id)
            .cloned()
    }
}

impl Default for HttpStreamRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpStreamJob {
    fn snapshot(&self) -> HttpStreamSnapshot {
        let state = self
            .state
            .lock()
            .expect("HTTP stream job lock should not be poisoned");
        Self::snapshot_from_state(&state)
    }

    fn snapshot_from_state(state: &HttpStreamJobState) -> HttpStreamSnapshot {
        HttpStreamSnapshot {
            id: state.id,
            method: state.method.clone(),
            url: state.url.clone(),
            started_at_ms: state.started_at_ms,
            status: state.status.as_str().to_string(),
            running: matches!(
                state.status,
                HttpStreamStatus::Connecting | HttpStreamStatus::Running
            ),
            success: matches!(state.status, HttpStreamStatus::Completed),
            status_code: state.status_code,
            headers: state.headers.clone(),
            error: state.error.clone(),
            body_len: state.body.len(),
            body_truncated: state.body_truncated,
            cancelled: matches!(state.status, HttpStreamStatus::Cancelled),
        }
    }

    fn read(&self, offset: usize, max_bytes: Option<usize>) -> HttpStreamRead {
        let state = self
            .state
            .lock()
            .expect("HTTP stream job lock should not be poisoned");
        let snapshot = Self::snapshot_from_state(&state);
        let from_offset = offset.min(state.body.len());
        let available_bytes = state.body.len() - from_offset;
        let bytes_len = max_bytes
            .map(|max_bytes| available_bytes.min(max_bytes))
            .unwrap_or(available_bytes);
        let next_offset = from_offset + bytes_len;
        let done = !matches!(
            state.status,
            HttpStreamStatus::Connecting | HttpStreamStatus::Running
        ) && next_offset >= state.body.len();
        HttpStreamRead {
            snapshot,
            body: String::from_utf8_lossy(&state.body[from_offset..next_offset]).into_owned(),
            from_offset,
            next_offset,
            offset: next_offset,
            bytes_len,
            done,
        }
    }

    fn cancel(&self) {
        let mut state = self
            .state
            .lock()
            .expect("HTTP stream job lock should not be poisoned");
        state.cancel_requested = true;
        if matches!(
            state.status,
            HttpStreamStatus::Connecting | HttpStreamStatus::Running
        ) {
            state.status = HttpStreamStatus::Cancelled;
        }
    }

    fn running(&self) -> bool {
        matches!(
            self.state
                .lock()
                .expect("HTTP stream job lock should not be poisoned")
                .status,
            HttpStreamStatus::Connecting | HttpStreamStatus::Running
        )
    }

    fn cancelled(&self) -> bool {
        self.state
            .lock()
            .expect("HTTP stream job lock should not be poisoned")
            .cancel_requested
    }

    fn mark_failed(&self, error: impl Into<String>) {
        let mut state = self
            .state
            .lock()
            .expect("HTTP stream job lock should not be poisoned");
        if !matches!(state.status, HttpStreamStatus::Cancelled) {
            state.status = HttpStreamStatus::Failed;
            state.error = Some(error.into());
        }
    }

    fn mark_running(&self, status_code: i64, headers: BTreeMap<String, String>) {
        let mut state = self
            .state
            .lock()
            .expect("HTTP stream job lock should not be poisoned");
        if !matches!(state.status, HttpStreamStatus::Cancelled) {
            state.status = HttpStreamStatus::Running;
            state.status_code = Some(status_code);
            state.headers = headers;
        }
    }

    fn append_body(&self, bytes: &[u8]) -> bool {
        let mut state = self
            .state
            .lock()
            .expect("HTTP stream job lock should not be poisoned");
        if state.cancel_requested || matches!(state.status, HttpStreamStatus::Cancelled) {
            state.status = HttpStreamStatus::Cancelled;
            return false;
        }
        let remaining = state.body_max_bytes.saturating_sub(state.body.len());
        if remaining == 0 {
            state.body_truncated = true;
            return false;
        }
        let keep = remaining.min(bytes.len());
        state.body.extend_from_slice(&bytes[..keep]);
        if keep < bytes.len() {
            state.body_truncated = true;
            return false;
        }
        true
    }

    fn mark_completed(&self) {
        let mut state = self
            .state
            .lock()
            .expect("HTTP stream job lock should not be poisoned");
        if state.cancel_requested || matches!(state.status, HttpStreamStatus::Cancelled) {
            state.status = HttpStreamStatus::Cancelled;
        } else if !matches!(state.status, HttpStreamStatus::Failed) {
            state.status = HttpStreamStatus::Completed;
        }
    }
}

impl HttpStreamStatus {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Connecting => "connecting",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
}

fn spawn_http_stream_worker(job: Arc<HttpStreamJob>, request: HttpStreamRequest) {
    thread::spawn(move || {
        if let Err(error) = run_http_stream(job.clone(), request) {
            job.mark_failed(error.message);
        }
    });
}

fn run_http_stream(
    job: Arc<HttpStreamJob>,
    request: HttpStreamRequest,
) -> Result<(), HttpStreamRuntimeError> {
    if job.cancelled() {
        return Ok(());
    }
    let mut response = if let Some(prepared) = request.prepared_secret_request {
        HttpStreamResponse::Secret(
            request
                .secrets_http_executor
                .execute_stream(prepared)
                .map_err(|error| HttpStreamRuntimeError::new(error.kind(), error.message()))?,
        )
    } else {
        let mut client = reqwest::blocking::Client::builder()
            .timeout(request.timeout)
            .redirect(reqwest::redirect::Policy::none());
        if let Some(destination) = &request.resolved_destination {
            client = client.resolve_to_addrs(&destination.host, &destination.addresses);
        }
        let client = client
            .build()
            .map_err(|error| HttpStreamRuntimeError::new("HttpError", error.to_string()))?;
        let mut builder = client
            .request(request.method, request.url)
            .headers(request.headers);
        if let Some(json) = request.json {
            builder = builder.json(&json);
        } else if let Some(body) = request.body {
            builder = builder.body(body);
        }
        HttpStreamResponse::Ordinary(
            builder
                .send()
                .map_err(|error| HttpStreamRuntimeError::new("HttpError", error.to_string()))?,
        )
    };
    let status_code = i64::from(response.status());
    let headers = response.headers();
    job.mark_running(status_code, headers);

    let mut buffer = [0u8; HTTP_STREAM_CHUNK_BYTES];
    loop {
        if job.cancelled() {
            return Ok(());
        }
        let count = response.read_chunk(&mut buffer)?;
        if count == 0 {
            break;
        }
        if !job.append_body(&buffer[..count]) {
            break;
        }
    }
    job.mark_completed();
    Ok(())
}

enum HttpStreamResponse {
    Ordinary(reqwest::blocking::Response),
    Secret(SecretHttpResponseStream),
}

impl HttpStreamResponse {
    fn status(&self) -> u16 {
        match self {
            Self::Ordinary(response) => response.status().as_u16(),
            Self::Secret(response) => response.status(),
        }
    }

    fn headers(&self) -> BTreeMap<String, String> {
        match self {
            Self::Ordinary(response) => response
                .headers()
                .iter()
                .map(|(name, value)| {
                    (
                        name.as_str().to_string(),
                        value.to_str().unwrap_or_default().to_string(),
                    )
                })
                .collect(),
            Self::Secret(response) => response.headers().clone(),
        }
    }

    fn read_chunk(&mut self, buffer: &mut [u8]) -> Result<usize, HttpStreamRuntimeError> {
        match self {
            Self::Ordinary(response) => response
                .read(buffer)
                .map_err(|error| HttpStreamRuntimeError::new("HttpError", error.to_string())),
            Self::Secret(response) => response
                .read_chunk(buffer)
                .map_err(|error| HttpStreamRuntimeError::new(error.kind(), error.message())),
        }
    }
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| duration.as_millis().try_into().ok())
        .unwrap_or_default()
}

#[cfg(test)]
pub(crate) struct TestHttpsCaptureServer {
    address: SocketAddr,
    requests: Arc<Mutex<Vec<String>>>,
    shutdown: Arc<std::sync::atomic::AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}

#[cfg(test)]
impl TestHttpsCaptureServer {
    pub(crate) fn new(status: u16, response_headers: &[(&str, &str)]) -> Self {
        use std::io::{Read as _, Write as _};
        use std::net::TcpListener;
        use std::sync::atomic::Ordering;

        let certified = rcgen::generate_simple_self_signed(vec!["phase0.test".to_string()])
            .expect("test TLS certificate should generate");
        let certificate = certified.cert.der().clone();
        let private_key =
            rustls::pki_types::PrivatePkcs8KeyDer::from(certified.key_pair.serialize_der());
        let tls = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![certificate], private_key.into())
            .expect("test TLS server config should build");
        let tls = Arc::new(tls);
        let listener =
            TcpListener::bind("127.0.0.1:0").expect("test HTTPS capture listener should bind");
        listener
            .set_nonblocking(true)
            .expect("test HTTPS capture listener should become nonblocking");
        let address = listener
            .local_addr()
            .expect("test HTTPS capture listener should expose its address");
        let requests = Arc::new(Mutex::new(Vec::new()));
        let captured = Arc::clone(&requests);
        let shutdown = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stop = Arc::clone(&shutdown);
        let response_headers = response_headers
            .iter()
            .map(|(name, value)| ((*name).to_string(), (*value).to_string()))
            .collect::<Vec<_>>();
        let worker = thread::spawn(move || {
            while !stop.load(Ordering::Acquire) {
                let (stream, _) = match listener.accept() {
                    Ok(accepted) => accepted,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                        continue;
                    }
                    Err(_) => break,
                };
                let _ = stream.set_nonblocking(false);
                let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
                let connection = match rustls::ServerConnection::new(Arc::clone(&tls)) {
                    Ok(connection) => connection,
                    Err(_) => continue,
                };
                let mut stream = rustls::StreamOwned::new(connection, stream);
                let mut request = Vec::new();
                let mut buffer = [0_u8; 1024];
                while !request.windows(4).any(|window| window == b"\r\n\r\n") {
                    match stream.read(&mut buffer) {
                        Ok(0) => break,
                        Ok(count) => request.extend_from_slice(&buffer[..count]),
                        Err(_) => break,
                    }
                    if request.len() > 64 * 1024 {
                        break;
                    }
                }
                captured
                    .lock()
                    .expect("test HTTPS requests lock should not be poisoned")
                    .push(String::from_utf8_lossy(&request).into_owned());
                let reason = match status {
                    200 => "OK",
                    302 => "Found",
                    503 => "Service Unavailable",
                    _ => "Test Response",
                };
                let mut response = format!(
                    "HTTP/1.1 {status} {reason}\r\nContent-Length: 2\r\nConnection: close\r\n"
                );
                for (name, value) in &response_headers {
                    response.push_str(name);
                    response.push_str(": ");
                    response.push_str(value);
                    response.push_str("\r\n");
                }
                response.push_str("\r\nok");
                let _ = stream.write_all(response.as_bytes());
                let _ = stream.flush();
            }
        });
        Self {
            address,
            requests,
            shutdown,
            worker: Some(worker),
        }
    }

    pub(crate) fn address(&self) -> SocketAddr {
        self.address
    }

    pub(crate) fn wait_for_requests(&self, expected: usize) -> Vec<String> {
        for _ in 0..200 {
            let requests = self
                .requests
                .lock()
                .expect("test HTTPS requests lock should not be poisoned")
                .clone();
            if requests.len() >= expected {
                return requests;
            }
            thread::sleep(Duration::from_millis(10));
        }
        self.requests
            .lock()
            .expect("test HTTPS requests lock should not be poisoned")
            .clone()
    }
}

#[cfg(test)]
impl Drop for TestHttpsCaptureServer {
    fn drop(&mut self) {
        use std::sync::atomic::Ordering;

        self.shutdown.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            worker.join().expect("test HTTPS worker should stop");
        }
    }
}

#[cfg(test)]
pub(crate) struct TestHttpsProtocolNackServer {
    address: SocketAddr,
    attempts: Arc<std::sync::atomic::AtomicUsize>,
    credential_attempts: Arc<std::sync::atomic::AtomicUsize>,
    shutdown: Arc<std::sync::atomic::AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}

#[cfg(test)]
impl TestHttpsProtocolNackServer {
    pub(crate) fn new() -> Self {
        use std::net::TcpListener;
        use std::sync::atomic::Ordering;

        let certified = rcgen::generate_simple_self_signed(vec!["phase0.test".to_string()])
            .expect("test HTTP/2 TLS certificate should generate");
        let certificate = certified.cert.der().clone();
        let private_key =
            rustls::pki_types::PrivatePkcs8KeyDer::from(certified.key_pair.serialize_der());
        let mut tls = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![certificate], private_key.into())
            .expect("test HTTP/2 TLS server config should build");
        tls.alpn_protocols = vec![b"h2".to_vec()];
        let tls = Arc::new(tls);

        let listener =
            TcpListener::bind("127.0.0.1:0").expect("test protocol NACK listener should bind");
        listener
            .set_nonblocking(true)
            .expect("test protocol NACK listener should become nonblocking");
        let address = listener
            .local_addr()
            .expect("test protocol NACK listener should expose its address");
        let attempts = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let observed_attempts = Arc::clone(&attempts);
        let credential_attempts = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let observed_credentials = Arc::clone(&credential_attempts);
        let shutdown = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stop = Arc::clone(&shutdown);

        let worker = thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("test protocol NACK runtime should build");
            runtime.block_on(async move {
                let listener = tokio::net::TcpListener::from_std(listener)
                    .expect("test protocol NACK listener should enter Tokio");
                let acceptor = tokio_rustls::TlsAcceptor::from(tls);
                while !stop.load(Ordering::Acquire) {
                    let accepted =
                        tokio::time::timeout(Duration::from_millis(20), listener.accept()).await;
                    let (stream, _) = match accepted {
                        Ok(Ok(accepted)) => accepted,
                        Ok(Err(_)) => break,
                        Err(_) => continue,
                    };
                    let tls_stream =
                        match tokio::time::timeout(Duration::from_secs(2), acceptor.accept(stream))
                            .await
                        {
                            Ok(Ok(stream)) => stream,
                            Ok(Err(_)) | Err(_) => continue,
                        };
                    let mut connection = match h2::server::handshake(tls_stream).await {
                        Ok(connection) => connection,
                        Err(_) => continue,
                    };
                    loop {
                        match tokio::time::timeout(Duration::from_millis(20), connection.accept())
                            .await
                        {
                            Ok(Some(Ok((request, mut respond)))) => {
                                observed_attempts.fetch_add(1, Ordering::AcqRel);
                                if request.headers().contains_key("authorization") {
                                    observed_credentials.fetch_add(1, Ordering::AcqRel);
                                }
                                respond.send_reset(h2::Reason::REFUSED_STREAM);
                            }
                            Ok(Some(Err(_))) | Ok(None) => break,
                            Err(_) if stop.load(Ordering::Acquire) => break,
                            Err(_) => continue,
                        }
                    }
                }
            });
        });

        Self {
            address,
            attempts,
            credential_attempts,
            shutdown,
            worker: Some(worker),
        }
    }

    pub(crate) fn address(&self) -> SocketAddr {
        self.address
    }

    pub(crate) fn wait_for_attempts(&self, expected: usize) -> (usize, usize) {
        use std::sync::atomic::Ordering;

        for _ in 0..200 {
            let attempts = self.attempts.load(Ordering::Acquire);
            if attempts >= expected {
                return (attempts, self.credential_attempts.load(Ordering::Acquire));
            }
            thread::sleep(Duration::from_millis(10));
        }
        (
            self.attempts.load(Ordering::Acquire),
            self.credential_attempts.load(Ordering::Acquire),
        )
    }
}

#[cfg(test)]
impl Drop for TestHttpsProtocolNackServer {
    fn drop(&mut self) {
        use std::sync::atomic::Ordering;

        self.shutdown.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            worker
                .join()
                .expect("test protocol NACK worker should stop");
        }
    }
}

#[cfg(test)]
pub(crate) struct TestConnectionCaptureServer {
    address: SocketAddr,
    connections: Arc<std::sync::atomic::AtomicUsize>,
    shutdown: Arc<std::sync::atomic::AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}

#[cfg(test)]
impl TestConnectionCaptureServer {
    pub(crate) fn new() -> Self {
        Self::with_stalled_connections(false)
    }

    pub(crate) fn new_stalled() -> Self {
        Self::with_stalled_connections(true)
    }

    fn with_stalled_connections(stall_connections: bool) -> Self {
        use std::net::TcpListener;
        use std::sync::atomic::Ordering;

        let listener =
            TcpListener::bind("127.0.0.1:0").expect("test connection capture listener should bind");
        listener
            .set_nonblocking(true)
            .expect("test connection capture listener should become nonblocking");
        let address = listener
            .local_addr()
            .expect("test connection capture listener should expose its address");
        let connections = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let captured = Arc::clone(&connections);
        let shutdown = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stop = Arc::clone(&shutdown);
        let worker = thread::spawn(move || {
            let mut stalled = Vec::new();
            while !stop.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        captured.fetch_add(1, Ordering::AcqRel);
                        if stall_connections {
                            stalled.push(stream);
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(_) => break,
                }
            }
        });
        Self {
            address,
            connections,
            shutdown,
            worker: Some(worker),
        }
    }

    pub(crate) fn address(&self) -> SocketAddr {
        self.address
    }

    pub(crate) fn connection_count(&self) -> usize {
        self.connections.load(std::sync::atomic::Ordering::Acquire)
    }

    pub(crate) fn wait_for_connections(&self, expected: usize) -> usize {
        for _ in 0..200 {
            let count = self.connection_count();
            if count >= expected {
                return count;
            }
            thread::sleep(Duration::from_millis(5));
        }
        self.connection_count()
    }
}

#[cfg(test)]
impl Drop for TestConnectionCaptureServer {
    fn drop(&mut self) {
        use std::sync::atomic::Ordering;

        self.shutdown.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            worker
                .join()
                .expect("test connection capture worker should stop");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::header::HeaderMap;
    use ricochet_sandbox::DestinationGrant;
    use ricochet_secrets::test_host::{TestEnvironmentValue, TestSecretsHttpHost};
    use ricochet_secrets::{
        DeferredHttpCredentials, DeferredSecretSource, EnvironmentCredentialPolicy,
        SecretHttpPolicySnapshot,
    };
    use std::collections::{BTreeMap, BTreeSet};
    use std::thread;

    #[test]
    fn retained_stream_limit_requires_release() {
        let registry = HttpStreamRegistry::with_max_retained(1);
        let first = registry
            .start(unreachable_request())
            .expect("first stream starts");
        wait_for_stream(&registry, first.id);

        let error = registry
            .start(unreachable_request())
            .expect_err("retained stream cap should reject another start");
        assert_eq!(error.kind, "RegistryFull");
        assert!(error.message.contains("release completed HTTP streams"));

        assert!(
            registry
                .release(first.id)
                .expect("completed stream releases"),
            "release should report a removed stream"
        );
        assert!(registry.stream(first.id).is_none());

        let second = registry
            .start(unreachable_request())
            .expect("cap frees after release");
        wait_for_stream(&registry, second.id);
        assert!(registry.release(second.id).expect("second stream releases"));
    }

    #[test]
    fn http_stream_deferred_credential_resolves_once_and_sends_once() {
        let server = TestHttpsCaptureServer::new(200, &[]);
        let test_host = TestSecretsHttpHost::new(
            "phase0.test",
            server.address(),
            BTreeMap::from([(
                "provider.api-key".to_string(),
                TestEnvironmentValue::unicode("stream-synthetic-secret".to_string()),
            )]),
        );
        let executor = test_host.executor();
        let credentials = DeferredHttpCredentials::bearer(DeferredSecretSource::environment(
            ricochet_application::SecretName::parse("provider.api-key")
                .expect("stream fixture name should parse"),
        ));
        let port = server.address().port();
        let policy = SecretHttpPolicySnapshot::new(
            true,
            Some(BTreeSet::from(["phase0.test".to_string()])),
            BTreeSet::from([DestinationGrant::new("phase0.test", port)
                .expect("stream exact destination should parse")]),
            EnvironmentCredentialPolicy::new(
                true,
                Some(BTreeSet::from(["provider.api-key".to_string()])),
            ),
        );
        let prepared = executor
            .prepare(
                credentials,
                reqwest::Method::GET,
                format!("https://phase0.test:{port}/stream"),
                HeaderMap::new(),
                None,
                None,
                Duration::from_secs(2),
                1024,
                Some(BTreeSet::from(["phase0.test".to_string()])),
                Some(BTreeSet::from(["https".to_string()])),
                policy,
            )
            .expect("authorized stream should prepare without resolving");
        assert_eq!(test_host.credential_resolution_count(), 0);
        assert_eq!(test_host.environment_source_access_count(), 0);

        let registry = HttpStreamRegistry::new();
        let started = registry
            .start(HttpStreamRequest {
                method: reqwest::Method::GET,
                url: format!("https://phase0.test:{port}/stream"),
                headers: HeaderMap::new(),
                json: None,
                body: None,
                timeout: Duration::from_secs(2),
                max_response_bytes: 1024,
                resolved_destination: None,
                prepared_secret_request: Some(prepared),
                secrets_http_executor: executor,
            })
            .expect("authorized deferred stream should start");
        let completed = wait_for_stream(&registry, started.id);
        assert!(completed.success, "stream should complete: {completed:?}");
        assert_eq!(completed.status_code, Some(200));
        assert_eq!(test_host.credential_resolution_count(), 1);
        assert_eq!(test_host.environment_source_access_count(), 1);
        let requests = server.wait_for_requests(1);
        assert_eq!(requests.len(), 1);
        assert_eq!(
            requests[0]
                .lines()
                .filter(|line| line.to_ascii_lowercase().starts_with("authorization:"))
                .count(),
            1
        );
        assert!(requests[0].contains("Bearer stream-synthetic-secret"));
    }

    #[test]
    fn http_stream_deferred_credential_literal_resolves_once_without_source_access() {
        let server = TestHttpsCaptureServer::new(200, &[]);
        let test_host = TestSecretsHttpHost::new("phase0.test", server.address(), BTreeMap::new());
        let executor = test_host.executor();
        let credentials = DeferredHttpCredentials::bearer(
            DeferredSecretSource::literal("stream-literal-synthetic-secret".to_string())
                .expect("stream literal fixture should construct"),
        );
        let port = server.address().port();
        let policy = SecretHttpPolicySnapshot::new(
            true,
            Some(BTreeSet::from(["phase0.test".to_string()])),
            BTreeSet::from([DestinationGrant::new("phase0.test", port)
                .expect("stream literal exact destination should parse")]),
            EnvironmentCredentialPolicy::new(false, None),
        );
        let prepared = executor
            .prepare(
                credentials,
                reqwest::Method::GET,
                format!("https://phase0.test:{port}/literal-stream"),
                HeaderMap::new(),
                None,
                None,
                Duration::from_secs(2),
                1024,
                Some(BTreeSet::from(["phase0.test".to_string()])),
                Some(BTreeSet::from(["https".to_string()])),
                policy,
            )
            .expect("authorized literal stream should prepare");
        assert_eq!(test_host.credential_resolution_count(), 0);
        assert_eq!(test_host.environment_source_access_count(), 0);

        let registry = HttpStreamRegistry::new();
        let started = registry
            .start(HttpStreamRequest {
                method: reqwest::Method::GET,
                url: format!("https://phase0.test:{port}/literal-stream"),
                headers: HeaderMap::new(),
                json: None,
                body: None,
                timeout: Duration::from_secs(2),
                max_response_bytes: 1024,
                resolved_destination: None,
                prepared_secret_request: Some(prepared),
                secrets_http_executor: executor,
            })
            .expect("authorized literal stream should start");
        let completed = wait_for_stream(&registry, started.id);
        assert!(completed.success, "literal stream should complete");
        assert_eq!(test_host.credential_resolution_count(), 1);
        assert_eq!(test_host.environment_source_access_count(), 0);
        let requests = server.wait_for_requests(1);
        assert_eq!(requests.len(), 1);
        assert!(requests[0].contains("Bearer stream-literal-synthetic-secret"));
    }

    fn unreachable_request() -> HttpStreamRequest {
        HttpStreamRequest {
            method: reqwest::Method::GET,
            url: "http://127.0.0.1:9/ricochet-test".to_string(),
            headers: HeaderMap::new(),
            json: None,
            body: None,
            timeout: Duration::from_millis(250),
            max_response_bytes: 1024,
            resolved_destination: None,
            prepared_secret_request: None,
            secrets_http_executor: ricochet_secrets::SecretsHttpExecutor::new(),
        }
    }

    fn wait_for_stream(registry: &HttpStreamRegistry, id: u64) -> HttpStreamSnapshot {
        for _ in 0..100 {
            let snapshot = registry.stream(id).expect("stream should remain retained");
            if !snapshot.running {
                return snapshot;
            }
            thread::sleep(Duration::from_millis(25));
        }
        panic!("HTTP stream {id} did not finish in time");
    }
}
