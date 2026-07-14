#![allow(clippy::result_large_err)]

use std::collections::BTreeMap;

use crate::audit::{
    AuditContext, AuditEventKind, AuditRecord, EnforcementState, ExecutionAuditIdentity,
};
use crate::backend::{
    BackendCapabilities, BackendSelfTest, BackendSelfTestFailure, SandboxBackend, SandboxSession,
    SessionCommand,
};
use crate::error::{FailedGuarantee, SandboxError, TerminationReason};
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

impl MockSandboxSession {
    fn timestamp(&mut self) -> UnixMillis {
        let value = self.next_timestamp;
        self.next_timestamp = self
            .next_timestamp
            .checked_add(1)
            .expect("the deterministic mock timestamp must not overflow");
        UnixMillis::new(value)
    }

    fn audit(&mut self, event: AuditEventKind) {
        let at = self.timestamp();
        self.events.push(BrokerEvent::Audit(AuditRecord::new(
            at,
            self.audit_context.clone(),
            event,
        )));
    }

    fn transition(&mut self, next: SessionState) -> Result<(), SandboxError> {
        let from = self.lifecycle.state();
        self.lifecycle.transition(next)?;
        self.audit(AuditEventKind::StateTransition { from, to: next });
        self.events.push(BrokerEvent::SessionState(next));
        Ok(())
    }

    fn transition_for_launch(&mut self) -> Result<(), SandboxError> {
        let from = self.lifecycle.state();
        self.lifecycle.transition(SessionState::Running)?;
        self.audit(AuditEventKind::StateTransition {
            from,
            to: SessionState::Running,
        });
        Ok(())
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
        self.audit(AuditEventKind::LaunchRequested {
            surface: ExecutionSurface::Process,
            tool_id: tool_id.clone(),
            argument_count: request.arguments.len(),
        });

        let process_id = ProcessId::new(self.next_process_id);
        self.next_process_id = self
            .next_process_id
            .checked_add(1)
            .expect("the deterministic mock process ID must not overflow");
        let process_tree_id = ProcessTreeId::new(self.next_process_tree_id);
        self.next_process_tree_id = self
            .next_process_tree_id
            .checked_add(1)
            .expect("the deterministic mock process-tree ID must not overflow");
        let started_at = self.timestamp();
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
            stdout_len: stdout.len() as u64,
            stderr_len: stderr.len() as u64,
            stdout_truncated: stdout.len() != self.config.scripted_stdout.len(),
            stderr_truncated: stderr.len() != self.config.scripted_stderr.len(),
            stdin_open: request.stdin_open,
            timed_out: false,
            cancelled: false,
        };
        snapshot.validate()?;
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
        self.audit(AuditEventKind::ProcessTreeStarted { process_tree_id });
        if self.lifecycle.state() == SessionState::Ready {
            self.transition_for_launch()?;
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
        let (stdout, stdout_offset) = read_chunk(&process.stdout, stdout_offset, max_bytes);
        let (stderr, stderr_offset) = read_chunk(&process.stderr, stderr_offset, max_bytes);
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
        let Some(process) = self.processes.get_mut(&process_id) else {
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
        process.stdin.extend_from_slice(bytes);
        let mut exited = None;
        if close_stdin {
            process.snapshot.stdin_open = false;
            process.snapshot.running = false;
            process.snapshot.status = ProcessStatus::Exited;
            process.snapshot.exit_code = Some(self.config.scripted_exit_code);
            process.snapshot.success = self.config.scripted_exit_code == 0;
            exited = Some((
                ExecutionAuditIdentity::process(
                    process.snapshot.process_tree_id,
                    process.snapshot.id,
                ),
                process.snapshot.exit_code,
                process.snapshot.success,
            ));
        }
        let snapshot = process.snapshot.clone();
        if let Some((execution, exit_code, success)) = exited {
            self.audit(AuditEventKind::Exited {
                execution,
                exit_code,
                success,
            });
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
        self.audit(AuditEventKind::LaunchRequested {
            surface: ExecutionSurface::Pty,
            tool_id: tool_id.clone(),
            argument_count: request.arguments.len(),
        });

        let pty_id = crate::identity::PtyId::new(self.next_pty_id);
        self.next_pty_id = self
            .next_pty_id
            .checked_add(1)
            .expect("the deterministic mock PTY ID must not overflow");
        let process_tree_id = ProcessTreeId::new(self.next_process_tree_id);
        self.next_process_tree_id = self
            .next_process_tree_id
            .checked_add(1)
            .expect("the deterministic mock process-tree ID must not overflow");
        let started_at = self.timestamp();
        let mut scripted_output = self.config.scripted_stdout.clone();
        scripted_output.extend_from_slice(&self.config.scripted_stderr);
        let output = truncate_to_cap(&scripted_output, request.output_max_bytes);
        let native_process_id = u32::try_from(10_000_u64 + pty_id.get()).ok();
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
            output_len: output.len() as u64,
            output_truncated: output.len() != scripted_output.len(),
            rows: request.rows,
            cols: request.cols,
            native_process_id,
            stopped: false,
        };
        snapshot.validate()?;
        self.ptys.insert(
            pty_id,
            MockPty {
                snapshot: snapshot.clone(),
                output,
                input: Vec::new(),
                tool_id,
            },
        );
        self.audit(AuditEventKind::ProcessTreeStarted { process_tree_id });
        if self.lifecycle.state() == SessionState::Ready {
            self.transition_for_launch()?;
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
        let (output, offset) = read_chunk(&pty.output, offset, max_bytes);
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
        let Some(pty) = self.ptys.get_mut(&pty_id) else {
            return Self::operation_error(
                OperationErrorCode::PtyNotFound,
                OperationSubject::Pty(pty_id),
            );
        };
        if !pty.snapshot.running {
            return Ok(BrokerResponse::Pty(pty.snapshot.clone()));
        }
        pty.snapshot.status = PtyStatus::Stopped;
        pty.snapshot.running = false;
        pty.snapshot.success = false;
        pty.snapshot.stopped = true;
        let snapshot = pty.snapshot.clone();
        let execution = ExecutionAuditIdentity::pty(snapshot.process_tree_id, snapshot.id);
        self.audit(AuditEventKind::Cancelled {
            execution,
            reason: TerminationReason::CancelledByHost,
        });
        self.events.push(BrokerEvent::Terminated(TerminationNotice {
            reason: TerminationReason::CancelledByHost,
            process_tree_ids: vec![snapshot.process_tree_id],
            error: Some(SandboxError::terminated(
                TerminationReason::CancelledByHost,
                self.session_id.clone(),
            )),
        }));
        Ok(BrokerResponse::Pty(snapshot))
    }

    fn audit_execution_termination(
        &mut self,
        execution: ExecutionAuditIdentity,
        reason: TerminationReason,
    ) {
        match reason {
            TerminationReason::TimedOut => self.audit(AuditEventKind::TimedOut { execution }),
            TerminationReason::ResourceLimit(limit) => {
                self.audit(AuditEventKind::ResourceLimit { execution, limit })
            }
            _ => self.audit(AuditEventKind::Cancelled { execution, reason }),
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

    fn terminate_execution(
        &mut self,
        execution: MockExecution,
        reason: TerminationReason,
    ) -> Option<ProcessTreeId> {
        let (tree_id, audit_identity) = match execution {
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
                (
                    process.snapshot.process_tree_id,
                    ExecutionAuditIdentity::process(
                        process.snapshot.process_tree_id,
                        process.snapshot.id,
                    ),
                )
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
                (
                    pty.snapshot.process_tree_id,
                    ExecutionAuditIdentity::pty(pty.snapshot.process_tree_id, pty.snapshot.id),
                )
            }
        };
        self.audit_execution_termination(audit_identity, reason);
        Some(tree_id)
    }

    fn termination_notice(&mut self, reason: TerminationReason, tree_ids: Vec<ProcessTreeId>) {
        self.events.push(BrokerEvent::Terminated(TerminationNotice {
            reason,
            process_tree_ids: tree_ids,
            error: Some(SandboxError::terminated(reason, self.session_id.clone())),
        }));
    }

    fn terminate_session(&mut self, reason: TerminationReason) -> Result<(), SandboxError> {
        if matches!(self.lifecycle.state(), SessionState::Closed) {
            return Ok(());
        }
        self.transition(SessionState::Stopping)?;
        let tree_ids = self
            .active_executions()
            .into_iter()
            .filter_map(|(_, execution)| self.terminate_execution(execution, reason))
            .collect::<Vec<_>>();
        self.termination_notice(reason, tree_ids);
        self.transition(SessionState::Closed)
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
        self.terminate_execution(
            MockExecution::Process(process_id),
            TerminationReason::CancelledByHost,
        );
        self.termination_notice(TerminationReason::CancelledByHost, vec![tree_id]);
        Ok(BrokerResponse::Process(
            self.processes[&process_id].snapshot.clone(),
        ))
    }

    fn tool_closure_contains(&self, root: &ToolId, expected: &ToolId) -> bool {
        let tools = self.policy.prepared_catalog().tools();
        let mut pending = vec![root];
        let mut visited = std::collections::BTreeSet::new();
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
        self.audit(AuditEventKind::Revoked {
            tool_id: tool_id.clone(),
            affected_process_trees: affected_tree_ids.clone(),
        });
        for (_, execution) in affected {
            self.terminate_execution(execution, TerminationReason::ToolRevoked);
        }
        if !affected_tree_ids.is_empty() {
            self.termination_notice(TerminationReason::ToolRevoked, affected_tree_ids);
        }
        Ok(())
    }
}

fn truncate_to_cap(bytes: &[u8], cap: u64) -> Vec<u8> {
    let cap = usize::try_from(cap).unwrap_or(usize::MAX);
    bytes[..bytes.len().min(cap)].to_vec()
}

fn read_chunk(bytes: &[u8], requested_offset: u64, max_bytes: u32) -> (Vec<u8>, u64) {
    let buffer_len = u64::try_from(bytes.len()).expect("mock buffer length must fit the wire");
    let offset = requested_offset.min(buffer_len);
    let start = usize::try_from(offset).expect("clamped mock read offset must fit usize");
    let end = start
        .saturating_add(usize::try_from(max_bytes).expect("u32 must fit usize"))
        .min(bytes.len());
    (bytes[start..end].to_vec(), offset)
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
