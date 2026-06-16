use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[derive(Clone, Default)]
pub struct ProcessRegistry {
    inner: Arc<Mutex<ProcessRegistryState>>,
}

#[derive(Default)]
struct ProcessRegistryState {
    next_id: u64,
    jobs: BTreeMap<u64, Arc<ProcessJob>>,
}

struct ProcessJob {
    state: Mutex<ProcessJobState>,
}

struct ProcessJobState {
    id: u64,
    command: String,
    args: Vec<String>,
    cwd: Option<PathBuf>,
    started_at_ms: i64,
    status: ProcessStatus,
    exit_code: Option<i32>,
    error: Option<String>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    stdout_max_bytes: usize,
    stderr_max_bytes: usize,
    stdout_truncated: bool,
    stderr_truncated: bool,
    cancel_requested: bool,
    timed_out: bool,
    child: Option<std::process::Child>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ProcessStatus {
    Running,
    Exited,
    Failed,
    Cancelled,
    TimedOut,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessRequest {
    pub command: String,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub stdin: Option<String>,
    pub timeout: Duration,
    pub clear_env: bool,
    pub env: BTreeMap<String, String>,
    pub stdout_max_bytes: usize,
    pub stderr_max_bytes: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessSnapshot {
    pub id: u64,
    pub command: String,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub started_at_ms: i64,
    pub status: String,
    pub running: bool,
    pub success: bool,
    pub exit_code: Option<i32>,
    pub error: Option<String>,
    pub stdout_len: usize,
    pub stderr_len: usize,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
    pub timed_out: bool,
    pub cancelled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessRead {
    pub snapshot: ProcessSnapshot,
    pub stdout: String,
    pub stderr: String,
    pub stdout_offset: usize,
    pub stderr_offset: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessRuntimeError {
    pub kind: &'static str,
    pub message: String,
}

impl ProcessRuntimeError {
    fn new(kind: &'static str, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

impl ProcessRegistry {
    pub fn start(&self, request: ProcessRequest) -> Result<ProcessSnapshot, ProcessRuntimeError> {
        let mut command = Command::new(&request.command);
        command.args(&request.args);
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());
        command.stdin(if request.stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        });

        if let Some(cwd) = &request.cwd {
            command.current_dir(cwd);
        }
        if request.clear_env {
            command.env_clear();
        }
        for (name, value) in &request.env {
            command.env(name, value);
        }

        let mut child = command
            .spawn()
            .map_err(|error| ProcessRuntimeError::new("ProcessError", error.to_string()))?;

        let stdout = child.stdout.take();
        let stderr = child.stderr.take();

        if let Some(input) = &request.stdin {
            if let Some(mut stdin) = child.stdin.take() {
                if let Err(error) = stdin.write_all(input.as_bytes()) {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(ProcessRuntimeError::new(
                        "ProcessError",
                        format!("failed to write stdin: {error}"),
                    ));
                }
            }
        }

        let id = {
            let mut state = self
                .inner
                .lock()
                .expect("process registry lock should not be poisoned");
            let id = state.next_id;
            state.next_id += 1;
            id
        };

        let job = Arc::new(ProcessJob {
            state: Mutex::new(ProcessJobState {
                id,
                command: request.command.clone(),
                args: request.args.clone(),
                cwd: request.cwd.clone(),
                started_at_ms: now_millis(),
                status: ProcessStatus::Running,
                exit_code: None,
                error: None,
                stdout: Vec::new(),
                stderr: Vec::new(),
                stdout_max_bytes: request.stdout_max_bytes,
                stderr_max_bytes: request.stderr_max_bytes,
                stdout_truncated: false,
                stderr_truncated: false,
                cancel_requested: false,
                timed_out: false,
                child: Some(child),
            }),
        });

        if let Some(stdout) = stdout {
            spawn_output_reader(job.clone(), OutputStream::Stdout, stdout);
        }
        if let Some(stderr) = stderr {
            spawn_output_reader(job.clone(), OutputStream::Stderr, stderr);
        }
        spawn_waiter(job.clone(), request.timeout);

        let snapshot = job.snapshot();
        self.inner
            .lock()
            .expect("process registry lock should not be poisoned")
            .jobs
            .insert(id, job);
        Ok(snapshot)
    }

    pub fn job(&self, id: u64) -> Option<ProcessSnapshot> {
        self.job_arc(id).map(|job| job.snapshot())
    }

    pub fn jobs(&self) -> Vec<ProcessSnapshot> {
        self.inner
            .lock()
            .expect("process registry lock should not be poisoned")
            .jobs
            .values()
            .map(|job| job.snapshot())
            .collect()
    }

    pub fn read(&self, id: u64, stdout_offset: usize, stderr_offset: usize) -> Option<ProcessRead> {
        let job = self.job_arc(id)?;
        Some(job.read(stdout_offset, stderr_offset))
    }

    pub fn cancel(&self, id: u64) -> Option<ProcessSnapshot> {
        let job = self.job_arc(id)?;
        job.cancel();
        Some(job.snapshot())
    }

    pub fn len(&self) -> usize {
        self.inner
            .lock()
            .expect("process registry lock should not be poisoned")
            .jobs
            .len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner
            .lock()
            .expect("process registry lock should not be poisoned")
            .jobs
            .is_empty()
    }

    fn job_arc(&self, id: u64) -> Option<Arc<ProcessJob>> {
        self.inner
            .lock()
            .expect("process registry lock should not be poisoned")
            .jobs
            .get(&id)
            .cloned()
    }
}

impl ProcessJob {
    fn snapshot(&self) -> ProcessSnapshot {
        let state = self
            .state
            .lock()
            .expect("process job lock should not be poisoned");
        state.snapshot()
    }

    fn read(&self, stdout_offset: usize, stderr_offset: usize) -> ProcessRead {
        let state = self
            .state
            .lock()
            .expect("process job lock should not be poisoned");
        let stdout_offset = stdout_offset.min(state.stdout.len());
        let stderr_offset = stderr_offset.min(state.stderr.len());
        ProcessRead {
            snapshot: state.snapshot(),
            stdout: String::from_utf8_lossy(&state.stdout[stdout_offset..]).into_owned(),
            stderr: String::from_utf8_lossy(&state.stderr[stderr_offset..]).into_owned(),
            stdout_offset: state.stdout.len(),
            stderr_offset: state.stderr.len(),
        }
    }

    fn cancel(&self) {
        let mut state = self
            .state
            .lock()
            .expect("process job lock should not be poisoned");
        if state.status != ProcessStatus::Running {
            return;
        }
        state.cancel_requested = true;
        if let Some(child) = state.child.as_mut() {
            if let Err(error) = child.kill() {
                state.error = Some(error.to_string());
            }
        }
    }
}

impl ProcessJobState {
    fn snapshot(&self) -> ProcessSnapshot {
        ProcessSnapshot {
            id: self.id,
            command: self.command.clone(),
            args: self.args.clone(),
            cwd: self.cwd.clone(),
            started_at_ms: self.started_at_ms,
            status: process_status_name(&self.status).to_string(),
            running: self.status == ProcessStatus::Running,
            success: self.status == ProcessStatus::Exited && self.exit_code == Some(0),
            exit_code: self.exit_code,
            error: self.error.clone(),
            stdout_len: self.stdout.len(),
            stderr_len: self.stderr.len(),
            stdout_truncated: self.stdout_truncated,
            stderr_truncated: self.stderr_truncated,
            timed_out: self.timed_out,
            cancelled: self.cancel_requested || self.status == ProcessStatus::Cancelled,
        }
    }
}

enum OutputStream {
    Stdout,
    Stderr,
}

fn spawn_output_reader(
    job: Arc<ProcessJob>,
    stream: OutputStream,
    mut reader: impl Read + Send + 'static,
) {
    thread::spawn(move || {
        let mut buffer = [0_u8; 8192];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => return,
                Ok(count) => append_output(&job, &stream, &buffer[..count]),
                Err(error) => {
                    let mut state = job
                        .state
                        .lock()
                        .expect("process job lock should not be poisoned");
                    state.error = Some(error.to_string());
                    return;
                }
            }
        }
    });
}

fn append_output(job: &ProcessJob, stream: &OutputStream, bytes: &[u8]) {
    let mut state = job
        .state
        .lock()
        .expect("process job lock should not be poisoned");
    match stream {
        OutputStream::Stdout => {
            let max_bytes = state.stdout_max_bytes;
            if state.stdout.len() >= max_bytes {
                state.stdout_truncated = true;
                return;
            }
            let available = max_bytes - state.stdout.len();
            let take = bytes.len().min(available);
            state.stdout.extend_from_slice(&bytes[..take]);
            if take < bytes.len() {
                state.stdout_truncated = true;
            }
        }
        OutputStream::Stderr => {
            let max_bytes = state.stderr_max_bytes;
            if state.stderr.len() >= max_bytes {
                state.stderr_truncated = true;
                return;
            }
            let available = max_bytes - state.stderr.len();
            let take = bytes.len().min(available);
            state.stderr.extend_from_slice(&bytes[..take]);
            if take < bytes.len() {
                state.stderr_truncated = true;
            }
        }
    }
}

fn spawn_waiter(job: Arc<ProcessJob>, timeout: Duration) {
    thread::spawn(move || {
        let started = Instant::now();
        loop {
            let wait_result = {
                let mut state = job
                    .state
                    .lock()
                    .expect("process job lock should not be poisoned");
                if state.status != ProcessStatus::Running {
                    return;
                }
                match state.child.as_mut() {
                    Some(child) => child.try_wait(),
                    None => return,
                }
            };

            match wait_result {
                Ok(Some(status)) => {
                    let mut state = job
                        .state
                        .lock()
                        .expect("process job lock should not be poisoned");
                    state.exit_code = status.code();
                    if state.cancel_requested {
                        state.status = ProcessStatus::Cancelled;
                    } else {
                        state.status = ProcessStatus::Exited;
                    }
                    state.child = None;
                    return;
                }
                Ok(None) if started.elapsed() >= timeout => {
                    let mut state = job
                        .state
                        .lock()
                        .expect("process job lock should not be poisoned");
                    state.timed_out = true;
                    state.status = ProcessStatus::TimedOut;
                    if let Some(mut child) = state.child.take() {
                        if let Err(error) = child.kill() {
                            state.error = Some(error.to_string());
                        }
                        let _ = child.wait();
                    }
                    return;
                }
                Ok(None) => thread::sleep(Duration::from_millis(25)),
                Err(error) => {
                    let mut state = job
                        .state
                        .lock()
                        .expect("process job lock should not be poisoned");
                    state.status = ProcessStatus::Failed;
                    state.error = Some(error.to_string());
                    state.child = None;
                    return;
                }
            }
        }
    });
}

fn process_status_name(status: &ProcessStatus) -> &'static str {
    match status {
        ProcessStatus::Running => "running",
        ProcessStatus::Exited => "exited",
        ProcessStatus::Failed => "failed",
        ProcessStatus::Cancelled => "cancelled",
        ProcessStatus::TimedOut => "timed_out",
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
