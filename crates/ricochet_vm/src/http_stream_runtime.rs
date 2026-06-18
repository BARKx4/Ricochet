use std::collections::BTreeMap;
use std::io::Read;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::Value as JsonValue;

const HTTP_STREAM_CHUNK_BYTES: usize = 8192;

#[derive(Clone, Default)]
pub struct HttpStreamRegistry {
    inner: Arc<Mutex<HttpStreamRegistryState>>,
}

#[derive(Default)]
struct HttpStreamRegistryState {
    next_id: u64,
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

#[derive(Clone)]
pub struct HttpStreamRequest {
    pub method: reqwest::Method,
    pub url: String,
    pub headers: reqwest::header::HeaderMap,
    pub json: Option<JsonValue>,
    pub body: Option<String>,
    pub timeout: Duration,
    pub max_response_bytes: usize,
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
    pub offset: usize,
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
    pub fn start(
        &self,
        request: HttpStreamRequest,
    ) -> Result<HttpStreamSnapshot, HttpStreamRuntimeError> {
        let id = {
            let mut state = self
                .inner
                .lock()
                .expect("HTTP stream registry lock should not be poisoned");
            let id = state.next_id;
            state.next_id += 1;
            id
        };
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
        self.inner
            .lock()
            .expect("HTTP stream registry lock should not be poisoned")
            .streams
            .insert(id, job.clone());
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

    pub fn read(&self, id: u64, offset: usize) -> Option<HttpStreamRead> {
        let job = self.stream_arc(id)?;
        Some(job.read(offset))
    }

    pub fn cancel(&self, id: u64) -> Option<HttpStreamSnapshot> {
        let job = self.stream_arc(id)?;
        job.cancel();
        Some(job.snapshot())
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

impl HttpStreamJob {
    fn snapshot(&self) -> HttpStreamSnapshot {
        let state = self
            .state
            .lock()
            .expect("HTTP stream job lock should not be poisoned");
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

    fn read(&self, offset: usize) -> HttpStreamRead {
        let snapshot = self.snapshot();
        let state = self
            .state
            .lock()
            .expect("HTTP stream job lock should not be poisoned");
        let offset = offset.min(state.body.len());
        HttpStreamRead {
            snapshot,
            body: String::from_utf8_lossy(&state.body[offset..]).into_owned(),
            offset: state.body.len(),
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
    let client = reqwest::blocking::Client::builder()
        .timeout(request.timeout)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|error| HttpStreamRuntimeError::new("HttpError", error.to_string()))?;
    if job.cancelled() {
        return Ok(());
    }
    let mut builder = client
        .request(request.method, request.url)
        .headers(request.headers);
    if let Some(json) = request.json {
        builder = builder.json(&json);
    } else if let Some(body) = request.body {
        builder = builder.body(body);
    }
    let mut response = builder
        .send()
        .map_err(|error| HttpStreamRuntimeError::new("HttpError", error.to_string()))?;
    let status_code = i64::from(response.status().as_u16());
    let headers = response
        .headers()
        .iter()
        .map(|(name, value)| {
            (
                name.as_str().to_string(),
                value.to_str().unwrap_or_default().to_string(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    job.mark_running(status_code, headers);

    let mut buffer = [0u8; HTTP_STREAM_CHUNK_BYTES];
    loop {
        if job.cancelled() {
            return Ok(());
        }
        let count = response
            .read(&mut buffer)
            .map_err(|error| HttpStreamRuntimeError::new("HttpError", error.to_string()))?;
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

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| duration.as_millis().try_into().ok())
        .unwrap_or_default()
}
