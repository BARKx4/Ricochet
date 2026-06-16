use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use portable_pty::{native_pty_system, CommandBuilder, PtySize};

#[derive(Clone, Default)]
pub struct PtyRegistry {
    inner: Arc<Mutex<PtyRegistryState>>,
}

#[derive(Default)]
struct PtyRegistryState {
    next_id: u64,
    sessions: BTreeMap<u64, Arc<PtySession>>,
}

struct PtySession {
    state: Mutex<PtySessionState>,
}

struct PtySessionState {
    id: u64,
    command: String,
    args: Vec<String>,
    cwd: Option<PathBuf>,
    started_at_ms: i64,
    status: PtyStatus,
    exit_code: Option<i64>,
    error: Option<String>,
    output: Vec<u8>,
    output_max_bytes: usize,
    output_truncated: bool,
    stop_requested: bool,
    rows: u16,
    cols: u16,
    process_id: Option<u32>,
    master: Option<Box<dyn portable_pty::MasterPty + Send>>,
    writer: Option<Box<dyn Write + Send>>,
    child: Option<Box<dyn portable_pty::Child + Send + Sync>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum PtyStatus {
    Running,
    Exited,
    Failed,
    Stopped,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PtyRequest {
    pub command: String,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub clear_env: bool,
    pub env: BTreeMap<String, String>,
    pub rows: u16,
    pub cols: u16,
    pub output_max_bytes: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PtySnapshot {
    pub id: u64,
    pub command: String,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub started_at_ms: i64,
    pub status: String,
    pub running: bool,
    pub success: bool,
    pub exit_code: Option<i64>,
    pub error: Option<String>,
    pub output_len: usize,
    pub output_truncated: bool,
    pub rows: u16,
    pub cols: u16,
    pub process_id: Option<u32>,
    pub stopped: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PtyRead {
    pub snapshot: PtySnapshot,
    pub output: String,
    pub offset: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PtyRuntimeError {
    pub kind: &'static str,
    pub message: String,
}

impl PtyRuntimeError {
    fn new(kind: &'static str, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

impl PtyRegistry {
    pub fn start(&self, request: PtyRequest) -> Result<PtySnapshot, PtyRuntimeError> {
        let size = PtySize {
            rows: request.rows,
            cols: request.cols,
            pixel_width: 0,
            pixel_height: 0,
        };
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(size)
            .map_err(|error| PtyRuntimeError::new("PtyError", error.to_string()))?;
        let mut command = CommandBuilder::new(&request.command);
        command.args(&request.args);
        if let Some(cwd) = &request.cwd {
            command.cwd(cwd);
        }
        if request.clear_env {
            command.env_clear();
        }
        for (name, value) in &request.env {
            command.env(name, value);
        }

        let child = pair
            .slave
            .spawn_command(command)
            .map_err(|error| PtyRuntimeError::new("PtyError", error.to_string()))?;
        let process_id = child.process_id();
        let reader = pair
            .master
            .try_clone_reader()
            .map_err(|error| PtyRuntimeError::new("PtyError", error.to_string()))?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|error| PtyRuntimeError::new("PtyError", error.to_string()))?;

        let id = {
            let mut state = self
                .inner
                .lock()
                .expect("pty registry lock should not be poisoned");
            let id = state.next_id;
            state.next_id += 1;
            id
        };

        let session = Arc::new(PtySession {
            state: Mutex::new(PtySessionState {
                id,
                command: request.command.clone(),
                args: request.args.clone(),
                cwd: request.cwd.clone(),
                started_at_ms: now_millis(),
                status: PtyStatus::Running,
                exit_code: None,
                error: None,
                output: Vec::new(),
                output_max_bytes: request.output_max_bytes,
                output_truncated: false,
                stop_requested: false,
                rows: request.rows,
                cols: request.cols,
                process_id,
                master: Some(pair.master),
                writer: Some(writer),
                child: Some(child),
            }),
        });

        spawn_reader(session.clone(), reader);
        spawn_waiter(session.clone());

        let snapshot = session.snapshot();
        self.inner
            .lock()
            .expect("pty registry lock should not be poisoned")
            .sessions
            .insert(id, session);
        Ok(snapshot)
    }

    pub fn sessions(&self) -> Vec<PtySnapshot> {
        self.inner
            .lock()
            .expect("pty registry lock should not be poisoned")
            .sessions
            .values()
            .map(|session| session.snapshot())
            .collect()
    }

    pub fn session(&self, id: u64) -> Option<PtySnapshot> {
        self.session_arc(id).map(|session| session.snapshot())
    }

    pub fn read(&self, id: u64, offset: usize) -> Option<PtyRead> {
        let session = self.session_arc(id)?;
        Some(session.read(offset))
    }

    pub fn write(&self, id: u64, input: &str) -> Option<Result<PtySnapshot, PtyRuntimeError>> {
        let session = self.session_arc(id)?;
        Some(session.write(input))
    }

    pub fn resize(
        &self,
        id: u64,
        cols: u16,
        rows: u16,
    ) -> Option<Result<PtySnapshot, PtyRuntimeError>> {
        let session = self.session_arc(id)?;
        Some(session.resize(cols, rows))
    }

    pub fn stop(&self, id: u64) -> Option<Result<PtySnapshot, PtyRuntimeError>> {
        let session = self.session_arc(id)?;
        Some(session.stop())
    }

    pub fn len(&self) -> usize {
        self.inner
            .lock()
            .expect("pty registry lock should not be poisoned")
            .sessions
            .len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner
            .lock()
            .expect("pty registry lock should not be poisoned")
            .sessions
            .is_empty()
    }

    fn session_arc(&self, id: u64) -> Option<Arc<PtySession>> {
        self.inner
            .lock()
            .expect("pty registry lock should not be poisoned")
            .sessions
            .get(&id)
            .cloned()
    }
}

impl PtySession {
    fn snapshot(&self) -> PtySnapshot {
        let state = self
            .state
            .lock()
            .expect("pty session lock should not be poisoned");
        state.snapshot()
    }

    fn read(&self, offset: usize) -> PtyRead {
        let state = self
            .state
            .lock()
            .expect("pty session lock should not be poisoned");
        let offset = offset.min(state.output.len());
        PtyRead {
            snapshot: state.snapshot(),
            output: String::from_utf8_lossy(&state.output[offset..]).into_owned(),
            offset: state.output.len(),
        }
    }

    fn write(&self, input: &str) -> Result<PtySnapshot, PtyRuntimeError> {
        let mut state = self
            .state
            .lock()
            .expect("pty session lock should not be poisoned");
        if state.status != PtyStatus::Running {
            return Err(PtyRuntimeError::new(
                "PtyClosed",
                "cannot write to a stopped PTY session",
            ));
        }
        let Some(writer) = state.writer.as_mut() else {
            return Err(PtyRuntimeError::new("PtyClosed", "PTY writer is closed"));
        };
        writer
            .write_all(input.as_bytes())
            .map_err(|error| PtyRuntimeError::new("PtyError", error.to_string()))?;
        writer
            .flush()
            .map_err(|error| PtyRuntimeError::new("PtyError", error.to_string()))?;
        Ok(state.snapshot())
    }

    fn resize(&self, cols: u16, rows: u16) -> Result<PtySnapshot, PtyRuntimeError> {
        let mut state = self
            .state
            .lock()
            .expect("pty session lock should not be poisoned");
        let Some(master) = state.master.as_ref() else {
            return Err(PtyRuntimeError::new("PtyClosed", "PTY master is closed"));
        };
        master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|error| PtyRuntimeError::new("PtyError", error.to_string()))?;
        state.rows = rows;
        state.cols = cols;
        Ok(state.snapshot())
    }

    fn stop(&self) -> Result<PtySnapshot, PtyRuntimeError> {
        let mut state = self
            .state
            .lock()
            .expect("pty session lock should not be poisoned");
        state.stop_requested = true;
        state.writer = None;
        if let Some(child) = state.child.as_mut() {
            child
                .kill()
                .map_err(|error| PtyRuntimeError::new("PtyError", error.to_string()))?;
        }
        Ok(state.snapshot())
    }
}

impl PtySessionState {
    fn snapshot(&self) -> PtySnapshot {
        PtySnapshot {
            id: self.id,
            command: self.command.clone(),
            args: self.args.clone(),
            cwd: self.cwd.clone(),
            started_at_ms: self.started_at_ms,
            status: pty_status_name(&self.status).to_string(),
            running: self.status == PtyStatus::Running,
            success: self.status == PtyStatus::Exited && self.exit_code == Some(0),
            exit_code: self.exit_code,
            error: self.error.clone(),
            output_len: self.output.len(),
            output_truncated: self.output_truncated,
            rows: self.rows,
            cols: self.cols,
            process_id: self.process_id,
            stopped: self.stop_requested || self.status == PtyStatus::Stopped,
        }
    }
}

fn spawn_reader(session: Arc<PtySession>, mut reader: Box<dyn Read + Send>) {
    thread::spawn(move || {
        let mut buffer = [0_u8; 8192];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => return,
                Ok(count) => append_output(&session, &buffer[..count]),
                Err(error) => {
                    let mut state = session
                        .state
                        .lock()
                        .expect("pty session lock should not be poisoned");
                    if state.status == PtyStatus::Running {
                        state.error = Some(error.to_string());
                    }
                    return;
                }
            }
        }
    });
}

fn append_output(session: &PtySession, bytes: &[u8]) {
    let mut state = session
        .state
        .lock()
        .expect("pty session lock should not be poisoned");
    let should_report_cursor_position = bytes.windows(4).any(|window| window == b"\x1b[6n");
    let max_bytes = state.output_max_bytes;
    if state.output.len() >= max_bytes {
        state.output_truncated = true;
    } else {
        let available = max_bytes - state.output.len();
        let take = bytes.len().min(available);
        state.output.extend_from_slice(&bytes[..take]);
        if take < bytes.len() {
            state.output_truncated = true;
        }
    }
    if should_report_cursor_position {
        if let Some(writer) = state.writer.as_mut() {
            let _ = writer.write_all(b"\x1b[1;1R");
            let _ = writer.flush();
        }
    }
}

fn spawn_waiter(session: Arc<PtySession>) {
    thread::spawn(move || loop {
        let wait_result = {
            let mut state = session
                .state
                .lock()
                .expect("pty session lock should not be poisoned");
            if state.status != PtyStatus::Running {
                return;
            }
            match state.child.as_mut() {
                Some(child) => child.try_wait(),
                None => return,
            }
        };

        match wait_result {
            Ok(Some(status)) => {
                let mut state = session
                    .state
                    .lock()
                    .expect("pty session lock should not be poisoned");
                state.exit_code = Some(status.exit_code().into());
                state.status = if state.stop_requested {
                    PtyStatus::Stopped
                } else if status.success() {
                    PtyStatus::Exited
                } else {
                    PtyStatus::Failed
                };
                state.child = None;
                state.writer = None;
                return;
            }
            Ok(None) => thread::sleep(Duration::from_millis(25)),
            Err(error) => {
                let mut state = session
                    .state
                    .lock()
                    .expect("pty session lock should not be poisoned");
                state.status = PtyStatus::Failed;
                state.error = Some(error.to_string());
                state.child = None;
                state.writer = None;
                return;
            }
        }
    });
}

fn pty_status_name(status: &PtyStatus) -> &'static str {
    match status {
        PtyStatus::Running => "running",
        PtyStatus::Exited => "exited",
        PtyStatus::Failed => "failed",
        PtyStatus::Stopped => "stopped",
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
