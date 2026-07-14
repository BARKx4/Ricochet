#![allow(clippy::result_large_err)]

use std::collections::{BTreeMap, BTreeSet};

use crate::audit::{
    AuditContext, AuditEventKind, AuditRecord, EnforcementState, ExecutionAuditIdentity,
};
use crate::backend::{
    BackendCapabilities, BackendSelfTest, BackendSelfTestFailure, SandboxBackend, SandboxSession,
    SessionCommand,
};
use crate::error::{
    DiagnosticMetadata, FailedGuarantee, Remediation, SandboxError, TerminationReason,
};
use crate::identity::{
    BackendFeatureId, BackendIdentity, ProcessId, ProcessTreeId, ScratchId, SessionId, ToolId,
    UnixMillis,
};
use crate::lifecycle::{SessionLifecycle, SessionState};
use crate::policy::{ExecutionSurface, ValidatedExecutionPolicy};
use crate::protocol::{
    BrokerEvent, BrokerRequest, BrokerResponse, ConfirmedExecutionCapabilities, ExecutableRef,
    OperationError, OperationErrorCode, OperationSubject, ProcessLaunchRequest,
    ProcessReadSnapshot, ProcessSnapshot, ProcessStatus, PtyLaunchRequest, PtyReadSnapshot,
    PtySnapshot, PtyStatus, TerminationNotice, WireBytes,
};
use crate::version::PROTOCOL_V1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MockFailurePoint {
    SelfTest,
    Prepare,
    Launch,
    Write,
    Resize,
    Cancel,
    Close,
    Revocation,
}

#[derive(Clone, Default)]
pub struct MockBackendConfig {
    pub failures: BTreeMap<MockFailurePoint, SandboxError>,
    pub scripted_exit_code: i64,
    pub scripted_stdout: Vec<u8>,
    pub scripted_stderr: Vec<u8>,
}

pub struct MockSandboxBackend {
    config: MockBackendConfig,
}

impl MockSandboxBackend {
    pub fn new(config: MockBackendConfig) -> Self {
        Self { config }
    }

    fn mock_identity() -> BackendIdentity {
        BackendIdentity::new("mock", "1").expect("the fixed mock identity is valid")
    }

    fn capabilities() -> BackendCapabilities {
        BackendCapabilities {
            process: true,
            pty: true,
            filesystem_read: true,
            filesystem_write: true,
            executable_closure: true,
            descendant_confinement: true,
            destination_transport: true,
            resource_limits: true,
            scratch_isolation: true,
        }
    }
}

impl SandboxBackend for MockSandboxBackend {
    fn identity(&self) -> BackendIdentity {
        Self::mock_identity()
    }

    fn self_test(&self) -> BackendSelfTest {
        let failures = self
            .config
            .failures
            .get(&MockFailurePoint::SelfTest)
            .map(|error| BackendSelfTestFailure {
                feature: BackendFeatureId::parse("mock-self-test")
                    .expect("the fixed mock feature ID is valid"),
                guarantee: FailedGuarantee::BrokerAvailability,
                remediation: error.remediation(),
            })
            .into_iter()
            .collect();
        BackendSelfTest {
            identity: self.identity(),
            capabilities: Self::capabilities(),
            production_enforcement: false,
            failures,
        }
    }

    fn prepare(
        &self,
        session_id: SessionId,
        scratch_id: ScratchId,
        policy: ValidatedExecutionPolicy,
    ) -> Result<Box<dyn SandboxSession>, SandboxError> {
        if let Some(error) = self.config.failures.get(&MockFailurePoint::Prepare) {
            return Err(error.clone());
        }
        let identity = self.identity();
        let capabilities = ConfirmedExecutionCapabilities::new(
            session_id.clone(),
            scratch_id.clone(),
            identity.clone(),
            PROTOCOL_V1,
            EnforcementState::MockOnly,
            &policy,
        )?;
        let audit_context = AuditContext::new(
            session_id.clone(),
            scratch_id,
            identity,
            PROTOCOL_V1,
            EnforcementState::MockOnly,
            &policy,
        )?;
        let mut lifecycle = SessionLifecycle::new();
        lifecycle.transition(SessionState::Ready)?;
        let events = vec![
            BrokerEvent::Audit(AuditRecord::new(
                UnixMillis::new(0),
                audit_context.clone(),
                AuditEventKind::StateTransition {
                    from: SessionState::Preparing,
                    to: SessionState::Ready,
                },
            )),
            BrokerEvent::SessionState(SessionState::Ready),
        ];
        Ok(Box::new(MockSandboxSession {
            config: self.config.clone(),
            session_id,
            policy,
            capabilities,
            audit_context,
            lifecycle,
            events,
            next_timestamp: 1,
            next_process_id: 0,
            next_pty_id: 0,
            next_process_tree_id: 0,
            processes: BTreeMap::new(),
            ptys: BTreeMap::new(),
            revoked_tools: BTreeSet::new(),
        }))
    }
}

struct MockSandboxSession {
    config: MockBackendConfig,
    session_id: SessionId,
    policy: ValidatedExecutionPolicy,
    capabilities: ConfirmedExecutionCapabilities,
    audit_context: AuditContext,
    lifecycle: SessionLifecycle,
    events: Vec<BrokerEvent>,
    next_timestamp: u64,
    next_process_id: u64,
    next_pty_id: u64,
    next_process_tree_id: u64,
    processes: BTreeMap<ProcessId, MockProcess>,
    ptys: BTreeMap<crate::identity::PtyId, MockPty>,
    revoked_tools: BTreeSet<ToolId>,
}

struct MockProcess {
    snapshot: ProcessSnapshot,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    stdin: Vec<u8>,
    tool_id: Option<ToolId>,
}

struct MockPty {
    snapshot: PtySnapshot,
    output: Vec<u8>,
    input: Vec<u8>,
    tool_id: Option<ToolId>,
}

#[derive(Clone, Copy)]
enum MockExecution {
    Process(ProcessId),
    Pty(crate::identity::PtyId),
}

struct MockTimestampBatch {
    cursor: u64,
    remaining: u64,
    next_timestamp: u64,
}

impl MockTimestampBatch {
    fn new(next_timestamp: u64, count: usize) -> Result<Self, SandboxError> {
        let count = u64::try_from(count).map_err(|_| counter_exhausted())?;
        let reserved_next = next_timestamp
            .checked_add(count)
            .ok_or_else(counter_exhausted)?;
        Ok(Self {
            cursor: next_timestamp,
            remaining: count,
            next_timestamp: reserved_next,
        })
    }

    fn take(&mut self) -> Result<UnixMillis, SandboxError> {
        self.remaining = self
            .remaining
            .checked_sub(1)
            .ok_or_else(counter_exhausted)?;
        let timestamp = UnixMillis::new(self.cursor);
        self.cursor = self.cursor.checked_add(1).ok_or_else(counter_exhausted)?;
        Ok(timestamp)
    }

    fn finish(self) -> Result<u64, SandboxError> {
        if self.remaining == 0 && self.cursor == self.next_timestamp {
            Ok(self.next_timestamp)
        } else {
            Err(counter_exhausted())
        }
    }
}

fn counter_exhausted() -> SandboxError {
    SandboxError::unavailable(
        None,
        FailedGuarantee::BrokerAvailability,
        Remediation::RetryAfterBrokerRestart,
        DiagnosticMetadata::empty(),
    )
}

fn reserve_numeric_id(next_id: u64) -> Result<(u64, u64), SandboxError> {
    let reserved_next = next_id.checked_add(1).ok_or_else(counter_exhausted)?;
    Ok((next_id, reserved_next))
}

fn wire_len(bytes: &[u8]) -> Result<u64, SandboxError> {
    u64::try_from(bytes.len()).map_err(|_| counter_exhausted())
}

impl MockSandboxSession {
    fn planned_audit(
        &self,
        timestamps: &mut MockTimestampBatch,
        event: AuditEventKind,
    ) -> Result<BrokerEvent, SandboxError> {
        Ok(BrokerEvent::Audit(AuditRecord::new(
            timestamps.take()?,
            self.audit_context.clone(),
            event,
        )))
    }

    fn validate_request(&self, request: BrokerRequest) -> Result<BrokerRequest, SandboxError> {
        request.validate_against(&self.session_id, &self.policy)?;
        Ok(request)
    }

    fn operation_error(
        code: OperationErrorCode,
        subject: OperationSubject,
    ) -> Result<BrokerResponse, SandboxError> {
        Ok(BrokerResponse::OperationError(OperationError::new(
            code, subject,
        )?))
    }

    fn handle_process_start(
        &mut self,
        request: ProcessLaunchRequest,
    ) -> Result<BrokerResponse, SandboxError> {
        if let Some(error) = self.config.failures.get(&MockFailurePoint::Launch) {
            return Err(error.clone());
        }

        let tool_id = match &request.executable {
            ExecutableRef::ManagedTool(tool_id) => Some(tool_id.clone()),
            ExecutableRef::HostCommand(_) => None,
        };
        let command_display = match &request.executable {
            ExecutableRef::ManagedTool(tool_id) => format!("managed:{}", tool_id.as_str()),
            ExecutableRef::HostCommand(_) => "host:[REDACTED]".to_owned(),
        };
        let transition_for_launch = self.lifecycle.state() == SessionState::Ready;
        let timestamp_count = if transition_for_launch { 4 } else { 3 };
        let mut timestamps = MockTimestampBatch::new(self.next_timestamp, timestamp_count)?;
        let launch_event = self.planned_audit(
            &mut timestamps,
            AuditEventKind::LaunchRequested {
                surface: ExecutionSurface::Process,
                tool_id: tool_id.clone(),
                argument_count: request.arguments.len(),
            },
        )?;
        let (process_id, next_process_id) = reserve_numeric_id(self.next_process_id)?;
        let process_id = ProcessId::new(process_id);
        let (process_tree_id, next_process_tree_id) =
            reserve_numeric_id(self.next_process_tree_id)?;
        let process_tree_id = ProcessTreeId::new(process_tree_id);
        let started_at = timestamps.take()?;
        let stdout = truncate_to_cap(&self.config.scripted_stdout, request.stdout_max_bytes);
        let stderr = truncate_to_cap(&self.config.scripted_stderr, request.stderr_max_bytes);
        let snapshot = ProcessSnapshot {
            id: process_id,
            process_tree_id,
            command_display,
            argument_count: request.arguments.len(),
            arguments: request.arguments,
            cwd: request.cwd,
            started_at,
            status: ProcessStatus::Running,
            running: true,
            success: false,
            exit_code: None,
            error: None,
            stdout_len: wire_len(&stdout)?,
            stderr_len: wire_len(&stderr)?,
            stdout_truncated: stdout.len() != self.config.scripted_stdout.len(),
            stderr_truncated: stderr.len() != self.config.scripted_stderr.len(),
            stdin_open: request.stdin_open,
            timed_out: false,
            cancelled: false,
        };
        snapshot.validate()?;
        let process_tree_event = self.planned_audit(
            &mut timestamps,
            AuditEventKind::ProcessTreeStarted { process_tree_id },
        )?;
        let (next_lifecycle, transition_event) = if transition_for_launch {
            let from = self.lifecycle.state();
            let mut lifecycle = self.lifecycle;
            lifecycle.transition(SessionState::Running)?;
            let event = self.planned_audit(
                &mut timestamps,
                AuditEventKind::StateTransition {
                    from,
                    to: SessionState::Running,
                },
            )?;
            (lifecycle, Some(event))
        } else {
            (self.lifecycle, None)
        };
        let next_timestamp = timestamps.finish()?;

        self.next_timestamp = next_timestamp;
        self.next_process_id = next_process_id;
        self.next_process_tree_id = next_process_tree_id;
        self.lifecycle = next_lifecycle;
        self.processes.insert(
            process_id,
            MockProcess {
                snapshot: snapshot.clone(),
                stdout,
                stderr,
                stdin: Vec::new(),
                tool_id,
            },
        );
        self.events.push(launch_event);
        self.events.push(process_tree_event);
        if let Some(event) = transition_event {
            self.events.push(event);
        }
        Ok(BrokerResponse::Process(snapshot))
    }

    fn process_read(
        &self,
        process_id: ProcessId,
        stdout_offset: u64,
        stderr_offset: u64,
        max_bytes: u32,
    ) -> Result<BrokerResponse, SandboxError> {
        let Some(process) = self.processes.get(&process_id) else {
            return Self::operation_error(
                OperationErrorCode::ProcessNotFound,
                OperationSubject::Process(process_id),
            );
        };
        let (stdout, stdout_offset) = read_chunk(&process.stdout, stdout_offset, max_bytes)?;
        let (stderr, stderr_offset) = read_chunk(&process.stderr, stderr_offset, max_bytes)?;
        let snapshot = ProcessReadSnapshot {
            snapshot: process.snapshot.clone(),
            stdout: WireBytes::new(stdout)?,
            stderr: WireBytes::new(stderr)?,
            stdout_offset,
            stderr_offset,
        };
        snapshot.validate()?;
        Ok(BrokerResponse::ProcessRead(snapshot))
    }

    fn process_write(
        &mut self,
        process_id: ProcessId,
        bytes: &[u8],
        close_stdin: bool,
    ) -> Result<BrokerResponse, SandboxError> {
        if let Some(error) = self.config.failures.get(&MockFailurePoint::Write) {
            return Err(error.clone());
        }
        let Some(process) = self.processes.get(&process_id) else {
            return Self::operation_error(
                OperationErrorCode::ProcessNotFound,
                OperationSubject::Process(process_id),
            );
        };
        if !process.snapshot.running {
            return Self::operation_error(
                OperationErrorCode::ProcessNotRunning,
                OperationSubject::Process(process_id),
            );
        }
        if !process.snapshot.stdin_open {
            return Self::operation_error(
                OperationErrorCode::ProcessStdinClosed,
                OperationSubject::Process(process_id),
            );
        }
        let (next_timestamp, exit_event) = if close_stdin {
            let mut timestamps = MockTimestampBatch::new(self.next_timestamp, 1)?;
            let event = self.planned_audit(
                &mut timestamps,
                AuditEventKind::Exited {
                    execution: ExecutionAuditIdentity::process(
                        process.snapshot.process_tree_id,
                        process.snapshot.id,
                    ),
                    exit_code: Some(self.config.scripted_exit_code),
                    success: self.config.scripted_exit_code == 0,
                },
            )?;
            (Some(timestamps.finish()?), Some(event))
        } else {
            (None, None)
        };

        let process = self
            .processes
            .get_mut(&process_id)
            .ok_or_else(counter_exhausted)?;
        process.stdin.extend_from_slice(bytes);
        if close_stdin {
            process.snapshot.stdin_open = false;
            process.snapshot.running = false;
            process.snapshot.status = ProcessStatus::Exited;
            process.snapshot.exit_code = Some(self.config.scripted_exit_code);
            process.snapshot.success = self.config.scripted_exit_code == 0;
        }
        let snapshot = process.snapshot.clone();
        if let Some(next_timestamp) = next_timestamp {
            self.next_timestamp = next_timestamp;
        }
        if let Some(event) = exit_event {
            self.events.push(event);
        }
        Ok(BrokerResponse::Process(snapshot))
    }

    fn handle_pty_start(
        &mut self,
        request: PtyLaunchRequest,
    ) -> Result<BrokerResponse, SandboxError> {
        if let Some(error) = self.config.failures.get(&MockFailurePoint::Launch) {
            return Err(error.clone());
        }
        let tool_id = match &request.executable {
            ExecutableRef::ManagedTool(tool_id) => Some(tool_id.clone()),
            ExecutableRef::HostCommand(_) => None,
        };
        let command_display = match &request.executable {
            ExecutableRef::ManagedTool(tool_id) => format!("managed:{}", tool_id.as_str()),
            ExecutableRef::HostCommand(_) => "host:[REDACTED]".to_owned(),
        };
        let transition_for_launch = self.lifecycle.state() == SessionState::Ready;
        let timestamp_count = if transition_for_launch { 4 } else { 3 };
        let mut timestamps = MockTimestampBatch::new(self.next_timestamp, timestamp_count)?;
        let launch_event = self.planned_audit(
            &mut timestamps,
            AuditEventKind::LaunchRequested {
                surface: ExecutionSurface::Pty,
                tool_id: tool_id.clone(),
                argument_count: request.arguments.len(),
            },
        )?;
        let (pty_id, next_pty_id) = reserve_numeric_id(self.next_pty_id)?;
        let pty_id = crate::identity::PtyId::new(pty_id);
        let (process_tree_id, next_process_tree_id) =
            reserve_numeric_id(self.next_process_tree_id)?;
        let process_tree_id = ProcessTreeId::new(process_tree_id);
        let native_process_id = pty_id
            .get()
            .checked_add(10_000)
            .ok_or_else(counter_exhausted)
            .and_then(|value| u32::try_from(value).map_err(|_| counter_exhausted()))?;
        let started_at = timestamps.take()?;
        let mut scripted_output = self.config.scripted_stdout.clone();
        scripted_output.extend_from_slice(&self.config.scripted_stderr);
        let output = truncate_to_cap(&scripted_output, request.output_max_bytes);
        let snapshot = PtySnapshot {
            id: pty_id,
            process_tree_id,
            command_display,
            argument_count: request.arguments.len(),
            arguments: request.arguments,
            cwd: request.cwd,
            started_at,
            status: PtyStatus::Running,
            running: true,
            success: false,
            exit_code: None,
            error: None,
            output_len: wire_len(&output)?,
            output_truncated: output.len() != scripted_output.len(),
            rows: request.rows,
            cols: request.cols,
            native_process_id: Some(native_process_id),
            stopped: false,
        };
        snapshot.validate()?;
        let process_tree_event = self.planned_audit(
            &mut timestamps,
            AuditEventKind::ProcessTreeStarted { process_tree_id },
        )?;
        let (next_lifecycle, transition_event) = if transition_for_launch {
            let from = self.lifecycle.state();
            let mut lifecycle = self.lifecycle;
            lifecycle.transition(SessionState::Running)?;
            let event = self.planned_audit(
                &mut timestamps,
                AuditEventKind::StateTransition {
                    from,
                    to: SessionState::Running,
                },
            )?;
            (lifecycle, Some(event))
        } else {
            (self.lifecycle, None)
        };
        let next_timestamp = timestamps.finish()?;

        self.next_timestamp = next_timestamp;
        self.next_pty_id = next_pty_id;
        self.next_process_tree_id = next_process_tree_id;
        self.lifecycle = next_lifecycle;
        self.ptys.insert(
            pty_id,
            MockPty {
                snapshot: snapshot.clone(),
                output,
                input: Vec::new(),
                tool_id,
            },
        );
        self.events.push(launch_event);
        self.events.push(process_tree_event);
        if let Some(event) = transition_event {
            self.events.push(event);
        }
        Ok(BrokerResponse::Pty(snapshot))
    }

    fn pty_read(
        &self,
        pty_id: crate::identity::PtyId,
        offset: u64,
        max_bytes: u32,
    ) -> Result<BrokerResponse, SandboxError> {
        let Some(pty) = self.ptys.get(&pty_id) else {
            return Self::operation_error(
                OperationErrorCode::PtyNotFound,
                OperationSubject::Pty(pty_id),
            );
        };
        let (output, offset) = read_chunk(&pty.output, offset, max_bytes)?;
        let snapshot = PtyReadSnapshot {
            snapshot: pty.snapshot.clone(),
            output: WireBytes::new(output)?,
            offset,
        };
        snapshot.validate()?;
        Ok(BrokerResponse::PtyRead(snapshot))
    }

    fn pty_write(
        &mut self,
        pty_id: crate::identity::PtyId,
        bytes: &[u8],
    ) -> Result<BrokerResponse, SandboxError> {
        if let Some(error) = self.config.failures.get(&MockFailurePoint::Write) {
            return Err(error.clone());
        }
        let Some(pty) = self.ptys.get_mut(&pty_id) else {
            return Self::operation_error(
                OperationErrorCode::PtyNotFound,
                OperationSubject::Pty(pty_id),
            );
        };
        if !pty.snapshot.running {
            return Self::operation_error(
                OperationErrorCode::PtyClosed,
                OperationSubject::Pty(pty_id),
            );
        }
        pty.input.extend_from_slice(bytes);
        Ok(BrokerResponse::Pty(pty.snapshot.clone()))
    }

    fn pty_resize(
        &mut self,
        pty_id: crate::identity::PtyId,
        rows: u16,
        cols: u16,
    ) -> Result<BrokerResponse, SandboxError> {
        if let Some(error) = self.config.failures.get(&MockFailurePoint::Resize) {
            return Err(error.clone());
        }
        let Some(pty) = self.ptys.get_mut(&pty_id) else {
            return Self::operation_error(
                OperationErrorCode::PtyNotFound,
                OperationSubject::Pty(pty_id),
            );
        };
        if !pty.snapshot.running {
            return Self::operation_error(
                OperationErrorCode::PtyClosed,
                OperationSubject::Pty(pty_id),
            );
        }
        pty.snapshot.rows = rows;
        pty.snapshot.cols = cols;
        Ok(BrokerResponse::Pty(pty.snapshot.clone()))
    }

    fn pty_stop(&mut self, pty_id: crate::identity::PtyId) -> Result<BrokerResponse, SandboxError> {
        if let Some(error) = self.config.failures.get(&MockFailurePoint::Cancel) {
            return Err(error.clone());
        }
        let Some(pty) = self.ptys.get(&pty_id) else {
            return Self::operation_error(
                OperationErrorCode::PtyNotFound,
                OperationSubject::Pty(pty_id),
            );
        };
        if !pty.snapshot.running {
            return Ok(BrokerResponse::Pty(pty.snapshot.clone()));
        }
        let execution = ExecutionAuditIdentity::pty(pty.snapshot.process_tree_id, pty.snapshot.id);
        let tree_id = pty.snapshot.process_tree_id;
        let mut timestamps = MockTimestampBatch::new(self.next_timestamp, 1)?;
        let audit_event = self.planned_audit(
            &mut timestamps,
            AuditEventKind::Cancelled {
                execution,
                reason: TerminationReason::CancelledByHost,
            },
        )?;
        let next_timestamp = timestamps.finish()?;
        let termination_event =
            self.termination_event(TerminationReason::CancelledByHost, vec![tree_id]);

        let pty = self.ptys.get_mut(&pty_id).ok_or_else(counter_exhausted)?;
        pty.snapshot.status = PtyStatus::Stopped;
        pty.snapshot.running = false;
        pty.snapshot.success = false;
        pty.snapshot.stopped = true;
        let snapshot = pty.snapshot.clone();
        self.next_timestamp = next_timestamp;
        self.events.push(audit_event);
        self.events.push(termination_event);
        Ok(BrokerResponse::Pty(snapshot))
    }

    fn termination_audit_kind(
        execution: ExecutionAuditIdentity,
        reason: TerminationReason,
    ) -> AuditEventKind {
        match reason {
            TerminationReason::TimedOut => AuditEventKind::TimedOut { execution },
            TerminationReason::ResourceLimit(limit) => {
                AuditEventKind::ResourceLimit { execution, limit }
            }
            _ => AuditEventKind::Cancelled { execution, reason },
        }
    }

    fn execution_identity(
        &self,
        execution: MockExecution,
    ) -> Option<(ProcessTreeId, ExecutionAuditIdentity)> {
        match execution {
            MockExecution::Process(process_id) => self.processes.get(&process_id).map(|process| {
                (
                    process.snapshot.process_tree_id,
                    ExecutionAuditIdentity::process(
                        process.snapshot.process_tree_id,
                        process.snapshot.id,
                    ),
                )
            }),
            MockExecution::Pty(pty_id) => self.ptys.get(&pty_id).map(|pty| {
                (
                    pty.snapshot.process_tree_id,
                    ExecutionAuditIdentity::pty(pty.snapshot.process_tree_id, pty.snapshot.id),
                )
            }),
        }
    }

    fn active_executions(&self) -> Vec<(ProcessTreeId, MockExecution)> {
        let mut executions = self
            .processes
            .values()
            .filter(|process| process.snapshot.running)
            .map(|process| {
                (
                    process.snapshot.process_tree_id,
                    MockExecution::Process(process.snapshot.id),
                )
            })
            .chain(
                self.ptys
                    .values()
                    .filter(|pty| pty.snapshot.running)
                    .map(|pty| {
                        (
                            pty.snapshot.process_tree_id,
                            MockExecution::Pty(pty.snapshot.id),
                        )
                    }),
            )
            .collect::<Vec<_>>();
        executions.sort_by_key(|(tree_id, _)| *tree_id);
        executions
    }

    fn apply_execution_termination(
        &mut self,
        execution: MockExecution,
        reason: TerminationReason,
    ) -> Option<ProcessTreeId> {
        let tree_id = match execution {
            MockExecution::Process(process_id) => {
                let process = self.processes.get_mut(&process_id)?;
                if !process.snapshot.running {
                    return None;
                }
                process.snapshot.running = false;
                process.snapshot.success = false;
                process.snapshot.exit_code = None;
                process.snapshot.stdin_open = false;
                process.snapshot.error = None;
                process.snapshot.cancelled = reason != TerminationReason::TimedOut;
                process.snapshot.timed_out = reason == TerminationReason::TimedOut;
                process.snapshot.status = if reason == TerminationReason::TimedOut {
                    ProcessStatus::TimedOut
                } else {
                    ProcessStatus::Cancelled
                };
                process.snapshot.process_tree_id
            }
            MockExecution::Pty(pty_id) => {
                let pty = self.ptys.get_mut(&pty_id)?;
                if !pty.snapshot.running {
                    return None;
                }
                pty.snapshot.running = false;
                pty.snapshot.success = false;
                pty.snapshot.exit_code = None;
                pty.snapshot.error = None;
                pty.snapshot.stopped = true;
                pty.snapshot.status = PtyStatus::Stopped;
                pty.snapshot.process_tree_id
            }
        };
        Some(tree_id)
    }

    fn termination_event(
        &self,
        reason: TerminationReason,
        tree_ids: Vec<ProcessTreeId>,
    ) -> BrokerEvent {
        BrokerEvent::Terminated(TerminationNotice {
            reason,
            process_tree_ids: tree_ids,
            error: Some(SandboxError::terminated(reason, self.session_id.clone())),
        })
    }

    fn terminate_session(&mut self, reason: TerminationReason) -> Result<(), SandboxError> {
        if matches!(self.lifecycle.state(), SessionState::Closed) {
            return Ok(());
        }
        let active = self.active_executions();
        let timestamp_count = active.len().checked_add(2).ok_or_else(counter_exhausted)?;
        let mut timestamps = MockTimestampBatch::new(self.next_timestamp, timestamp_count)?;
        let from = self.lifecycle.state();
        let mut next_lifecycle = self.lifecycle;
        next_lifecycle.transition(SessionState::Stopping)?;
        next_lifecycle.transition(SessionState::Closed)?;

        let mut events =
            Vec::with_capacity(active.len().checked_add(5).ok_or_else(counter_exhausted)?);
        events.push(self.planned_audit(
            &mut timestamps,
            AuditEventKind::StateTransition {
                from,
                to: SessionState::Stopping,
            },
        )?);
        events.push(BrokerEvent::SessionState(SessionState::Stopping));
        let mut tree_ids = Vec::with_capacity(active.len());
        for (tree_id, execution) in &active {
            let (_, identity) = self
                .execution_identity(*execution)
                .ok_or_else(counter_exhausted)?;
            tree_ids.push(*tree_id);
            events.push(self.planned_audit(
                &mut timestamps,
                Self::termination_audit_kind(identity, reason),
            )?);
        }
        events.push(self.termination_event(reason, tree_ids));
        events.push(self.planned_audit(
            &mut timestamps,
            AuditEventKind::StateTransition {
                from: SessionState::Stopping,
                to: SessionState::Closed,
            },
        )?);
        events.push(BrokerEvent::SessionState(SessionState::Closed));
        let next_timestamp = timestamps.finish()?;

        for (_, execution) in active {
            self.apply_execution_termination(execution, reason);
        }
        self.lifecycle = next_lifecycle;
        self.next_timestamp = next_timestamp;
        self.events.extend(events);
        Ok(())
    }

    fn process_cancel(&mut self, process_id: ProcessId) -> Result<BrokerResponse, SandboxError> {
        if let Some(error) = self.config.failures.get(&MockFailurePoint::Cancel) {
            return Err(error.clone());
        }
        let Some(process) = self.processes.get(&process_id) else {
            return Self::operation_error(
                OperationErrorCode::ProcessNotFound,
                OperationSubject::Process(process_id),
            );
        };
        if !process.snapshot.running {
            return Ok(BrokerResponse::Process(process.snapshot.clone()));
        }
        let tree_id = process.snapshot.process_tree_id;
        let execution = ExecutionAuditIdentity::process(tree_id, process.snapshot.id);
        let mut timestamps = MockTimestampBatch::new(self.next_timestamp, 1)?;
        let audit_event = self.planned_audit(
            &mut timestamps,
            Self::termination_audit_kind(execution, TerminationReason::CancelledByHost),
        )?;
        let next_timestamp = timestamps.finish()?;
        let termination_event =
            self.termination_event(TerminationReason::CancelledByHost, vec![tree_id]);

        self.apply_execution_termination(
            MockExecution::Process(process_id),
            TerminationReason::CancelledByHost,
        );
        self.next_timestamp = next_timestamp;
        self.events.push(audit_event);
        self.events.push(termination_event);
        Ok(BrokerResponse::Process(
            self.processes[&process_id].snapshot.clone(),
        ))
    }

    fn tool_closure_contains(&self, root: &ToolId, expected: &ToolId) -> bool {
        let tools = self.policy.prepared_catalog().tools();
        let mut pending = vec![root];
        let mut visited = BTreeSet::new();
        while let Some(tool_id) = pending.pop() {
            if tool_id == expected {
                return true;
            }
            if !visited.insert(tool_id) {
                continue;
            }
            if let Some(tool) = tools.get(tool_id) {
                pending.extend(tool.helper_ids());
            }
        }
        false
    }

    fn reject_revoked_executable(&self, executable: &ExecutableRef) -> Result<(), SandboxError> {
        let ExecutableRef::ManagedTool(root) = executable else {
            return Ok(());
        };
        match self
            .revoked_tools
            .iter()
            .find(|revoked| self.tool_closure_contains(root, revoked))
        {
            Some(revoked) => Err(SandboxError::tool_not_approved(revoked.clone())),
            None => Ok(()),
        }
    }

    fn revoke_tool(&mut self, tool_id: &ToolId) -> Result<(), SandboxError> {
        if let Some(error) = self.config.failures.get(&MockFailurePoint::Revocation) {
            return Err(error.clone());
        }
        let mut affected = self
            .active_executions()
            .into_iter()
            .filter(|(_, execution)| match execution {
                MockExecution::Process(process_id) => self.processes[process_id]
                    .tool_id
                    .as_ref()
                    .is_some_and(|root| self.tool_closure_contains(root, tool_id)),
                MockExecution::Pty(pty_id) => self.ptys[pty_id]
                    .tool_id
                    .as_ref()
                    .is_some_and(|root| self.tool_closure_contains(root, tool_id)),
            })
            .collect::<Vec<_>>();
        affected.sort_by_key(|(tree_id, _)| *tree_id);
        let affected_tree_ids = affected
            .iter()
            .map(|(tree_id, _)| *tree_id)
            .collect::<Vec<_>>();
        let timestamp_count = affected
            .len()
            .checked_add(1)
            .ok_or_else(counter_exhausted)?;
        let mut timestamps = MockTimestampBatch::new(self.next_timestamp, timestamp_count)?;
        let mut events = Vec::with_capacity(
            affected
                .len()
                .checked_add(2)
                .ok_or_else(counter_exhausted)?,
        );
        events.push(self.planned_audit(
            &mut timestamps,
            AuditEventKind::Revoked {
                tool_id: tool_id.clone(),
                affected_process_trees: affected_tree_ids.clone(),
            },
        )?);
        for (_, execution) in &affected {
            let (_, identity) = self
                .execution_identity(*execution)
                .ok_or_else(counter_exhausted)?;
            events.push(self.planned_audit(
                &mut timestamps,
                Self::termination_audit_kind(identity, TerminationReason::ToolRevoked),
            )?);
        }
        if !affected_tree_ids.is_empty() {
            events.push(self.termination_event(TerminationReason::ToolRevoked, affected_tree_ids));
        }
        let next_timestamp = timestamps.finish()?;

        self.revoked_tools.insert(tool_id.clone());
        for (_, execution) in affected {
            self.apply_execution_termination(execution, TerminationReason::ToolRevoked);
        }
        self.next_timestamp = next_timestamp;
        self.events.extend(events);
        Ok(())
    }
}

fn truncate_to_cap(bytes: &[u8], cap: u64) -> Vec<u8> {
    let cap = usize::try_from(cap).unwrap_or(usize::MAX);
    bytes[..bytes.len().min(cap)].to_vec()
}

fn read_chunk(
    bytes: &[u8],
    requested_offset: u64,
    max_bytes: u32,
) -> Result<(Vec<u8>, u64), SandboxError> {
    let buffer_len = u64::try_from(bytes.len()).map_err(|_| counter_exhausted())?;
    let offset = requested_offset.min(buffer_len);
    let start = usize::try_from(offset).map_err(|_| counter_exhausted())?;
    let max_bytes = usize::try_from(max_bytes).map_err(|_| counter_exhausted())?;
    let end = start
        .checked_add(max_bytes)
        .ok_or_else(counter_exhausted)?
        .min(bytes.len());
    Ok((bytes[start..end].to_vec(), offset))
}

impl SandboxSession for MockSandboxSession {
    fn confirmed_capabilities(&self) -> &ConfirmedExecutionCapabilities {
        &self.capabilities
    }

    fn state(&self) -> SessionState {
        self.lifecycle.state()
    }

    fn handle(&mut self, command: SessionCommand) -> Result<BrokerResponse, SandboxError> {
        if self.lifecycle.state() == SessionState::Closed {
            if command.session_id() != &self.session_id {
                return Err(SandboxError::protocol(Default::default()));
            }
            return match command {
                SessionCommand::Close(_) | SessionCommand::Cancel(_) => {
                    Ok(BrokerResponse::Acknowledged)
                }
                _ => Err(SandboxError::terminated(
                    TerminationReason::SessionClosed,
                    self.session_id.clone(),
                )),
            };
        }
        match command {
            SessionCommand::Close(request) => {
                self.validate_request(BrokerRequest::CloseSession(request))?;
                if let Some(error) = self.config.failures.get(&MockFailurePoint::Close) {
                    return Err(error.clone());
                }
                self.terminate_session(TerminationReason::SessionClosed)?;
                Ok(BrokerResponse::Acknowledged)
            }
            SessionCommand::Cancel(request) => {
                self.validate_request(BrokerRequest::CancelSession(request))?;
                if let Some(error) = self.config.failures.get(&MockFailurePoint::Cancel) {
                    return Err(error.clone());
                }
                self.terminate_session(TerminationReason::CancelledByHost)?;
                Ok(BrokerResponse::Acknowledged)
            }
            SessionCommand::ProcessStart(request) => {
                let request = self.validate_request(BrokerRequest::ProcessStart(request))?;
                let BrokerRequest::ProcessStart(request) = request else {
                    unreachable!()
                };
                self.reject_revoked_executable(&request.executable)?;
                self.handle_process_start(request)
            }
            SessionCommand::ProcessList(request) => {
                self.validate_request(BrokerRequest::ProcessList(request))?;
                Ok(BrokerResponse::Processes(
                    self.processes
                        .values()
                        .map(|process| process.snapshot.clone())
                        .collect(),
                ))
            }
            SessionCommand::ProcessDetail(request) => {
                let request = self.validate_request(BrokerRequest::ProcessDetail(request))?;
                let BrokerRequest::ProcessDetail(request) = request else {
                    unreachable!()
                };
                match self.processes.get(&request.process_id) {
                    Some(process) => Ok(BrokerResponse::Process(process.snapshot.clone())),
                    None => Self::operation_error(
                        OperationErrorCode::ProcessNotFound,
                        OperationSubject::Process(request.process_id),
                    ),
                }
            }
            SessionCommand::ProcessRead(request) => {
                let request = self.validate_request(BrokerRequest::ProcessRead(request))?;
                let BrokerRequest::ProcessRead(request) = request else {
                    unreachable!()
                };
                self.process_read(
                    request.process_id,
                    request.stdout_offset,
                    request.stderr_offset,
                    request.max_bytes_per_stream,
                )
            }
            SessionCommand::ProcessWrite(request) => {
                let request = self.validate_request(BrokerRequest::ProcessWrite(request))?;
                let BrokerRequest::ProcessWrite(request) = request else {
                    unreachable!()
                };
                self.process_write(
                    request.process_id,
                    request.bytes.as_slice(),
                    request.close_stdin,
                )
            }
            SessionCommand::ProcessCancel(request) => {
                let request = self.validate_request(BrokerRequest::ProcessCancel(request))?;
                let BrokerRequest::ProcessCancel(request) = request else {
                    unreachable!()
                };
                self.process_cancel(request.process_id)
            }
            SessionCommand::ProcessRelease(request) => {
                let request = self.validate_request(BrokerRequest::ProcessRelease(request))?;
                let BrokerRequest::ProcessRelease(request) = request else {
                    unreachable!()
                };
                let Some(process) = self.processes.get(&request.process_id) else {
                    return Self::operation_error(
                        OperationErrorCode::ProcessNotFound,
                        OperationSubject::Process(request.process_id),
                    );
                };
                if process.snapshot.running {
                    return Self::operation_error(
                        OperationErrorCode::ProcessRunning,
                        OperationSubject::Process(request.process_id),
                    );
                }
                self.processes.remove(&request.process_id);
                Ok(BrokerResponse::Acknowledged)
            }
            SessionCommand::PtyStart(request) => {
                let request = self.validate_request(BrokerRequest::PtyStart(request))?;
                let BrokerRequest::PtyStart(request) = request else {
                    unreachable!()
                };
                self.reject_revoked_executable(&request.executable)?;
                self.handle_pty_start(request)
            }
            SessionCommand::PtyList(request) => {
                self.validate_request(BrokerRequest::PtyList(request))?;
                Ok(BrokerResponse::Ptys(
                    self.ptys.values().map(|pty| pty.snapshot.clone()).collect(),
                ))
            }
            SessionCommand::PtyDetail(request) => {
                let request = self.validate_request(BrokerRequest::PtyDetail(request))?;
                let BrokerRequest::PtyDetail(request) = request else {
                    unreachable!()
                };
                match self.ptys.get(&request.pty_id) {
                    Some(pty) => Ok(BrokerResponse::Pty(pty.snapshot.clone())),
                    None => Self::operation_error(
                        OperationErrorCode::PtyNotFound,
                        OperationSubject::Pty(request.pty_id),
                    ),
                }
            }
            SessionCommand::PtyRead(request) => {
                let request = self.validate_request(BrokerRequest::PtyRead(request))?;
                let BrokerRequest::PtyRead(request) = request else {
                    unreachable!()
                };
                self.pty_read(request.pty_id, request.offset, request.max_bytes)
            }
            SessionCommand::PtyWrite(request) => {
                let request = self.validate_request(BrokerRequest::PtyWrite(request))?;
                let BrokerRequest::PtyWrite(request) = request else {
                    unreachable!()
                };
                self.pty_write(request.pty_id, request.bytes.as_slice())
            }
            SessionCommand::PtyResize(request) => {
                let request = self.validate_request(BrokerRequest::PtyResize(request))?;
                let BrokerRequest::PtyResize(request) = request else {
                    unreachable!()
                };
                self.pty_resize(request.pty_id, request.rows, request.cols)
            }
            SessionCommand::PtyStop(request) => {
                let request = self.validate_request(BrokerRequest::PtyStop(request))?;
                let BrokerRequest::PtyStop(request) = request else {
                    unreachable!()
                };
                self.pty_stop(request.pty_id)
            }
            SessionCommand::PtyRelease(request) => {
                let request = self.validate_request(BrokerRequest::PtyRelease(request))?;
                let BrokerRequest::PtyRelease(request) = request else {
                    unreachable!()
                };
                let Some(pty) = self.ptys.get(&request.pty_id) else {
                    return Self::operation_error(
                        OperationErrorCode::PtyNotFound,
                        OperationSubject::Pty(request.pty_id),
                    );
                };
                if pty.snapshot.running {
                    return Self::operation_error(
                        OperationErrorCode::PtyRunning,
                        OperationSubject::Pty(request.pty_id),
                    );
                }
                self.ptys.remove(&request.pty_id);
                Ok(BrokerResponse::Acknowledged)
            }
        }
    }

    fn terminate(&mut self, reason: TerminationReason) -> Result<(), SandboxError> {
        if matches!(
            reason,
            TerminationReason::CancelledByHost | TerminationReason::ToolRevoked
        ) {
            return Err(SandboxError::protocol(Default::default()));
        }
        if let Some(error) = self.config.failures.get(&MockFailurePoint::Cancel) {
            return Err(error.clone());
        }
        self.terminate_session(reason)
    }

    fn revoke(&mut self, tool_id: &ToolId) -> Result<(), SandboxError> {
        self.revoke_tool(tool_id)
    }

    fn drain_events(&mut self) -> Vec<BrokerEvent> {
        std::mem::take(&mut self.events)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{
        Architecture, CatalogPathNormalizer, CatalogSnapshot, OperatingSystem, PlatformId,
    };
    use crate::error::{DiagnosticMetadata, Remediation};
    use crate::identity::CatalogGeneration;
    use crate::policy::{
        ArgumentAuditMode, AuditPolicy, EnvironmentPolicy, ExecutionAccess, ExecutionPolicyRequest,
        LaunchEnvironment, ScratchDisposition, WorkspaceIdentity, WorkspaceIdentityResolver,
        WorkspaceRequest,
    };
    use crate::protocol::{
        CancelSessionRequest, ProcessLaunchRequest, ProcessRequest, ProcessWriteRequest,
        PtyLaunchRequest, PtyRequest, SessionRequest,
    };
    use crate::version::{CATALOG_SCHEMA_V1, POLICY_SCHEMA_V1};

    struct TestPathNormalizer;

    impl CatalogPathNormalizer for TestPathNormalizer {
        fn normalize(&self, _platform: &PlatformId, path: &str) -> Result<String, SandboxError> {
            Ok(path.to_owned())
        }
    }

    struct TestWorkspaceResolver;

    impl WorkspaceIdentityResolver for TestWorkspaceResolver {
        fn resolve(&self, request: &WorkspaceRequest) -> Result<WorkspaceIdentity, SandboxError> {
            Ok(WorkspaceIdentity {
                requested_root: request.requested_root.clone(),
                canonical_root: request.requested_root.clone(),
                native_object_identity: "test-workspace".to_owned(),
            })
        }
    }

    fn generation() -> CatalogGeneration {
        CatalogGeneration::new(1).unwrap()
    }

    fn full_policy() -> ValidatedExecutionPolicy {
        let catalog = CatalogSnapshot {
            schema_version: CATALOG_SCHEMA_V1,
            generation: generation(),
            platform: PlatformId {
                os: OperatingSystem::Windows,
                arch: Architecture::X86_64,
            },
            records: Vec::new(),
            revoked_tools: Vec::new(),
        }
        .validate(&TestPathNormalizer)
        .unwrap();
        ExecutionPolicyRequest {
            schema_version: POLICY_SCHEMA_V1,
            access: ExecutionAccess::Full,
            allow_process: true,
            allow_pty: true,
            workspace: None,
            scratch_disposition: ScratchDisposition::DeleteOnCleanCloseRetainOtherwise,
            catalog_generation: generation(),
            activated_tools: Vec::new(),
            destinations: Vec::new(),
            environment: EnvironmentPolicy { base: Vec::new() },
            resource_limits: None,
            audit_policy: AuditPolicy {
                arguments: ArgumentAuditMode::CountOnly,
            },
        }
        .validate(&catalog, &TestWorkspaceResolver)
        .unwrap()
    }

    fn session_id() -> SessionId {
        SessionId::parse("counter-test-session").unwrap()
    }

    fn prepared_session() -> MockSandboxSession {
        let session_id = session_id();
        let scratch_id = ScratchId::parse("counter-test-scratch").unwrap();
        let policy = full_policy();
        let identity = MockSandboxBackend::mock_identity();
        let capabilities = ConfirmedExecutionCapabilities::new(
            session_id.clone(),
            scratch_id.clone(),
            identity.clone(),
            PROTOCOL_V1,
            EnforcementState::MockOnly,
            &policy,
        )
        .unwrap();
        let audit_context = AuditContext::new(
            session_id.clone(),
            scratch_id,
            identity,
            PROTOCOL_V1,
            EnforcementState::MockOnly,
            &policy,
        )
        .unwrap();
        let mut lifecycle = SessionLifecycle::new();
        lifecycle.transition(SessionState::Ready).unwrap();
        MockSandboxSession {
            config: MockBackendConfig::default(),
            session_id,
            policy,
            capabilities,
            audit_context,
            lifecycle,
            events: Vec::new(),
            next_timestamp: 1,
            next_process_id: 0,
            next_pty_id: 0,
            next_process_tree_id: 0,
            processes: BTreeMap::new(),
            ptys: BTreeMap::new(),
            revoked_tools: BTreeSet::new(),
        }
    }

    fn process_launch() -> ProcessLaunchRequest {
        ProcessLaunchRequest {
            session_id: session_id(),
            executable: ExecutableRef::HostCommand("test-command".to_owned()),
            arguments: Vec::new(),
            cwd: None,
            stdin_open: true,
            environment: LaunchEnvironment {
                clear_environment: true,
                entries: Vec::new(),
            },
            timeout_ms: 1,
            stdout_max_bytes: 1,
            stderr_max_bytes: 1,
        }
    }

    fn pty_launch() -> PtyLaunchRequest {
        PtyLaunchRequest {
            session_id: session_id(),
            executable: ExecutableRef::HostCommand("test-command".to_owned()),
            arguments: Vec::new(),
            cwd: None,
            environment: LaunchEnvironment {
                clear_environment: true,
                entries: Vec::new(),
            },
            rows: 24,
            cols: 80,
            output_max_bytes: 1,
        }
    }

    fn start_process(session: &mut MockSandboxSession) -> ProcessId {
        let BrokerResponse::Process(snapshot) = session
            .handle(SessionCommand::ProcessStart(process_launch()))
            .unwrap()
        else {
            panic!("process start must return a process snapshot")
        };
        session.events.clear();
        snapshot.id
    }

    fn start_pty(session: &mut MockSandboxSession) -> crate::identity::PtyId {
        let BrokerResponse::Pty(snapshot) = session
            .handle(SessionCommand::PtyStart(pty_launch()))
            .unwrap()
        else {
            panic!("PTY start must return a PTY snapshot")
        };
        session.events.clear();
        snapshot.id
    }

    fn assert_counter_error(error: &SandboxError) {
        assert_eq!(error.kind(), "SandboxUnavailable");
        assert_eq!(
            error.remediation(),
            Some(Remediation::RetryAfterBrokerRestart)
        );
        assert_eq!(error.metadata(), &DiagnosticMetadata::empty());
        let wire = serde_json::to_value(error).unwrap();
        assert_eq!(wire["failed_guarantee"], "broker_availability");
        assert_eq!(wire["phase"], "setup");
    }

    fn assert_empty_launch_state(
        session: &MockSandboxSession,
        timestamp: u64,
        process_id: u64,
        pty_id: u64,
        process_tree_id: u64,
    ) {
        assert_eq!(session.lifecycle.state(), SessionState::Ready);
        assert_eq!(session.next_timestamp, timestamp);
        assert_eq!(session.next_process_id, process_id);
        assert_eq!(session.next_pty_id, pty_id);
        assert_eq!(session.next_process_tree_id, process_tree_id);
        assert!(session.processes.is_empty());
        assert!(session.ptys.is_empty());
        assert!(session.events.is_empty());
    }

    #[test]
    fn timestamp_exhaustion_is_typed_and_atomic() {
        let mut session = prepared_session();
        session.next_timestamp = u64::MAX;

        let error = session
            .handle(SessionCommand::ProcessStart(process_launch()))
            .unwrap_err();

        assert_counter_error(&error);
        assert_eq!(session.lifecycle.state(), SessionState::Ready);
        assert_eq!(session.next_timestamp, u64::MAX);
        assert_eq!(session.next_process_id, 0);
        assert_eq!(session.next_pty_id, 0);
        assert_eq!(session.next_process_tree_id, 0);
        assert!(session.processes.is_empty());
        assert!(session.ptys.is_empty());
        assert!(session.events.is_empty());
    }

    #[test]
    fn launch_preflights_the_complete_timestamp_batch_before_mutation() {
        let mut session = prepared_session();
        session.next_timestamp = u64::MAX - 2;

        let error = session
            .handle(SessionCommand::ProcessStart(process_launch()))
            .unwrap_err();

        assert_counter_error(&error);
        assert_empty_launch_state(&session, u64::MAX - 2, 0, 0, 0);
    }

    #[test]
    fn process_id_exhaustion_is_typed_and_atomic() {
        let mut session = prepared_session();
        session.next_process_id = u64::MAX;

        let error = session
            .handle(SessionCommand::ProcessStart(process_launch()))
            .unwrap_err();

        assert_counter_error(&error);
        assert_empty_launch_state(&session, 1, u64::MAX, 0, 0);
    }

    #[test]
    fn process_id_last_value_is_allocated_once_before_typed_exhaustion() {
        let mut session = prepared_session();
        session.next_process_id = u64::MAX - 1;
        let process_id = start_process(&mut session);
        assert_eq!(process_id.get(), u64::MAX - 1);
        let before = serde_json::to_value(&session.processes[&process_id].snapshot).unwrap();
        let counters_before = (
            session.next_timestamp,
            session.next_process_id,
            session.next_pty_id,
            session.next_process_tree_id,
        );

        let error = session
            .handle(SessionCommand::ProcessStart(process_launch()))
            .unwrap_err();

        assert_counter_error(&error);
        assert_eq!(session.processes.len(), 1);
        assert_eq!(
            serde_json::to_value(&session.processes[&process_id].snapshot).unwrap(),
            before
        );
        assert_eq!(
            (
                session.next_timestamp,
                session.next_process_id,
                session.next_pty_id,
                session.next_process_tree_id,
            ),
            counters_before
        );
        assert!(session.events.is_empty());
    }

    #[test]
    fn process_tree_id_exhaustion_is_typed_and_atomic_for_both_surfaces() {
        for command in [
            SessionCommand::ProcessStart(process_launch()),
            SessionCommand::PtyStart(pty_launch()),
        ] {
            let mut session = prepared_session();
            session.next_process_id = 7;
            session.next_pty_id = 11;
            session.next_process_tree_id = u64::MAX;

            let error = session.handle(command).unwrap_err();

            assert_counter_error(&error);
            assert_empty_launch_state(&session, 1, 7, 11, u64::MAX);
        }
    }

    #[test]
    fn process_tree_last_value_is_allocated_once_before_typed_exhaustion() {
        let mut session = prepared_session();
        session.next_process_tree_id = u64::MAX - 1;
        let process_id = start_process(&mut session);
        assert_eq!(
            session.processes[&process_id]
                .snapshot
                .process_tree_id
                .get(),
            u64::MAX - 1
        );
        let before = serde_json::to_value(&session.processes[&process_id].snapshot).unwrap();
        let counters_before = (
            session.next_timestamp,
            session.next_process_id,
            session.next_pty_id,
            session.next_process_tree_id,
        );

        let error = session
            .handle(SessionCommand::PtyStart(pty_launch()))
            .unwrap_err();

        assert_counter_error(&error);
        assert_eq!(session.processes.len(), 1);
        assert!(session.ptys.is_empty());
        assert_eq!(
            serde_json::to_value(&session.processes[&process_id].snapshot).unwrap(),
            before
        );
        assert_eq!(
            (
                session.next_timestamp,
                session.next_process_id,
                session.next_pty_id,
                session.next_process_tree_id,
            ),
            counters_before
        );
        assert!(session.events.is_empty());
    }

    #[test]
    fn pty_id_exhaustion_is_typed_and_atomic() {
        let mut session = prepared_session();
        session.next_pty_id = u64::MAX;

        let error = session
            .handle(SessionCommand::PtyStart(pty_launch()))
            .unwrap_err();

        assert_counter_error(&error);
        assert_empty_launch_state(&session, 1, 0, u64::MAX, 0);
    }

    #[test]
    fn synthetic_pty_native_pid_conversion_exhaustion_is_typed_and_atomic() {
        let mut session = prepared_session();
        let first_unrepresentable_pty_id = u64::from(u32::MAX) - 9_999;
        session.next_pty_id = first_unrepresentable_pty_id;

        let error = session
            .handle(SessionCommand::PtyStart(pty_launch()))
            .unwrap_err();

        assert_counter_error(&error);
        assert_empty_launch_state(&session, 1, 0, first_unrepresentable_pty_id, 0);
    }

    #[test]
    fn pty_last_native_pid_is_allocated_once_before_typed_conversion_exhaustion() {
        let mut session = prepared_session();
        let last_representable_pty_id = u64::from(u32::MAX) - 10_000;
        session.next_pty_id = last_representable_pty_id;
        let pty_id = start_pty(&mut session);
        assert_eq!(pty_id.get(), last_representable_pty_id);
        assert_eq!(
            session.ptys[&pty_id].snapshot.native_process_id,
            Some(u32::MAX)
        );
        let before = serde_json::to_value(&session.ptys[&pty_id].snapshot).unwrap();
        let counters_before = (
            session.next_timestamp,
            session.next_process_id,
            session.next_pty_id,
            session.next_process_tree_id,
        );

        let error = session
            .handle(SessionCommand::PtyStart(pty_launch()))
            .unwrap_err();

        assert_counter_error(&error);
        assert_eq!(session.ptys.len(), 1);
        assert_eq!(
            serde_json::to_value(&session.ptys[&pty_id].snapshot).unwrap(),
            before
        );
        assert_eq!(
            (
                session.next_timestamp,
                session.next_process_id,
                session.next_pty_id,
                session.next_process_tree_id,
            ),
            counters_before
        );
        assert!(session.events.is_empty());
    }

    #[test]
    fn synthetic_pty_native_pid_addition_exhaustion_is_typed_and_atomic() {
        let mut session = prepared_session();
        let first_overflowing_pty_id = u64::MAX - 9_999;
        session.next_pty_id = first_overflowing_pty_id;

        let error = session
            .handle(SessionCommand::PtyStart(pty_launch()))
            .unwrap_err();

        assert_counter_error(&error);
        assert_empty_launch_state(&session, 1, 0, first_overflowing_pty_id, 0);
    }

    #[test]
    fn timestamp_exhaustion_preserves_process_write_and_cancel_state() {
        let mut write = prepared_session();
        let write_id = start_process(&mut write);
        let write_before = serde_json::to_value(&write.processes[&write_id].snapshot).unwrap();
        write.next_timestamp = u64::MAX;
        let error = write
            .handle(SessionCommand::ProcessWrite(ProcessWriteRequest {
                session_id: session_id(),
                process_id: write_id,
                bytes: WireBytes::new(b"input".to_vec()).unwrap(),
                close_stdin: true,
            }))
            .unwrap_err();
        assert_counter_error(&error);
        assert_eq!(
            serde_json::to_value(&write.processes[&write_id].snapshot).unwrap(),
            write_before
        );
        assert!(write.processes[&write_id].stdin.is_empty());
        assert_eq!(write.next_timestamp, u64::MAX);
        assert!(write.events.is_empty());

        let mut cancel = prepared_session();
        let cancel_id = start_process(&mut cancel);
        let cancel_before = serde_json::to_value(&cancel.processes[&cancel_id].snapshot).unwrap();
        cancel.next_timestamp = u64::MAX;
        let error = cancel
            .handle(SessionCommand::ProcessCancel(ProcessRequest {
                session_id: session_id(),
                process_id: cancel_id,
            }))
            .unwrap_err();
        assert_counter_error(&error);
        assert_eq!(
            serde_json::to_value(&cancel.processes[&cancel_id].snapshot).unwrap(),
            cancel_before
        );
        assert_eq!(cancel.next_timestamp, u64::MAX);
        assert!(cancel.events.is_empty());
    }

    #[test]
    fn timestamp_exhaustion_preserves_pty_stop_state_and_input() {
        let mut session = prepared_session();
        let pty_id = start_pty(&mut session);
        let before = serde_json::to_value(&session.ptys[&pty_id].snapshot).unwrap();
        session.next_timestamp = u64::MAX;

        let error = session
            .handle(SessionCommand::PtyStop(PtyRequest {
                session_id: session_id(),
                pty_id,
            }))
            .unwrap_err();

        assert_counter_error(&error);
        assert_eq!(
            serde_json::to_value(&session.ptys[&pty_id].snapshot).unwrap(),
            before
        );
        assert!(session.ptys[&pty_id].input.is_empty());
        assert_eq!(session.next_timestamp, u64::MAX);
        assert!(session.events.is_empty());
    }

    #[test]
    fn timestamp_exhaustion_is_atomic_for_every_session_termination_entry_point() {
        let mut close = prepared_session();
        let close_id = start_process(&mut close);
        let close_before = serde_json::to_value(&close.processes[&close_id].snapshot).unwrap();
        close.next_timestamp = u64::MAX;
        let error = close
            .handle(SessionCommand::Close(SessionRequest {
                session_id: session_id(),
            }))
            .unwrap_err();
        assert_counter_error(&error);
        assert_eq!(close.lifecycle.state(), SessionState::Running);
        assert_eq!(
            serde_json::to_value(&close.processes[&close_id].snapshot).unwrap(),
            close_before
        );
        assert_eq!(close.next_timestamp, u64::MAX);
        assert!(close.events.is_empty());

        let mut cancel = prepared_session();
        let cancel_id = start_process(&mut cancel);
        let cancel_before = serde_json::to_value(&cancel.processes[&cancel_id].snapshot).unwrap();
        cancel.next_timestamp = u64::MAX;
        let error = cancel
            .handle(SessionCommand::Cancel(CancelSessionRequest {
                session_id: session_id(),
            }))
            .unwrap_err();
        assert_counter_error(&error);
        assert_eq!(cancel.lifecycle.state(), SessionState::Running);
        assert_eq!(
            serde_json::to_value(&cancel.processes[&cancel_id].snapshot).unwrap(),
            cancel_before
        );
        assert_eq!(cancel.next_timestamp, u64::MAX);
        assert!(cancel.events.is_empty());

        let mut terminate = prepared_session();
        let terminate_id = start_process(&mut terminate);
        let terminate_before =
            serde_json::to_value(&terminate.processes[&terminate_id].snapshot).unwrap();
        terminate.next_timestamp = u64::MAX;
        let error = terminate
            .terminate(TerminationReason::TimedOut)
            .unwrap_err();
        assert_counter_error(&error);
        assert_eq!(terminate.lifecycle.state(), SessionState::Running);
        assert_eq!(
            serde_json::to_value(&terminate.processes[&terminate_id].snapshot).unwrap(),
            terminate_before
        );
        assert_eq!(terminate.next_timestamp, u64::MAX);
        assert!(terminate.events.is_empty());
    }

    #[test]
    fn timestamp_exhaustion_keeps_revocation_registry_and_events_unchanged() {
        let mut session = prepared_session();
        let tool_id = ToolId::parse("test-tool").unwrap();
        session.next_timestamp = u64::MAX;

        let error = session.revoke(&tool_id).unwrap_err();

        assert_counter_error(&error);
        assert!(session.revoked_tools.is_empty());
        assert_eq!(session.lifecycle.state(), SessionState::Ready);
        assert_eq!(session.next_timestamp, u64::MAX);
        assert_eq!(session.next_process_id, 0);
        assert_eq!(session.next_pty_id, 0);
        assert_eq!(session.next_process_tree_id, 0);
        assert!(session.events.is_empty());
    }
}
