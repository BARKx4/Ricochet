use std::collections::BTreeMap;

use ricochet_sandbox::{
    chunk_wire_bytes, ApprovalActor, Architecture, ArtifactKind, AuditEventKind, AuditPolicy,
    BrokerEvent, BrokerRequest, BrokerResponse, CancelSessionRequest, CatalogGeneration,
    CatalogPathNormalizer, CatalogRecord, CatalogSnapshot, ConfirmedExecutionCapabilities,
    ConnectionNonce, CreateSessionRequest, DiagnosticMetadata, EnforcementState, EnvironmentPolicy,
    ExecutableRef, ExecutionAccess, ExecutionPolicyRequest, FailedGuarantee, HandshakeRequest,
    HashedArtifact, LaunchEnvironment, MockBackendConfig, MockFailurePoint, MockSandboxBackend,
    OperatingSystem, OperationErrorCode, PlatformId, ProcessId, ProcessLaunchRequest,
    ProcessReadRequest, ProcessRequest, ProcessSnapshot, ProcessStatus, ProcessTreeId,
    ProcessWriteRequest, PtyId, PtyLaunchRequest, PtyReadRequest, PtyRequest, PtyResizeRequest,
    PtySnapshot, PtyStatus, PtyWriteRequest, Remediation, ResourceLimitKind, ResourceLimits,
    SandboxBackend, SandboxError, SandboxSession, ScratchDisposition, ScratchId, SessionCommand,
    SessionId, SessionRequest, SessionState, Sha256Digest, TerminationReason, ToolId, UnixMillis,
    ValidatedCatalogSnapshot, ValidatedExecutionPolicy, WireBytes, WorkspaceIdentity,
    WorkspaceIdentityResolver, WorkspaceRequest, CATALOG_SCHEMA_V1, MAX_IO_CHUNK_BYTES,
    POLICY_SCHEMA_V1,
};

struct FixturePathNormalizer;

impl CatalogPathNormalizer for FixturePathNormalizer {
    fn normalize(&self, _platform: &PlatformId, path: &str) -> Result<String, SandboxError> {
        Ok(path.replace('/', "\\").to_ascii_lowercase())
    }
}

struct FixtureWorkspaceResolver;

impl WorkspaceIdentityResolver for FixtureWorkspaceResolver {
    fn resolve(&self, request: &WorkspaceRequest) -> Result<WorkspaceIdentity, SandboxError> {
        Ok(WorkspaceIdentity {
            requested_root: request.requested_root.clone(),
            canonical_root: "C:/workspace".to_owned(),
            native_object_identity: "volume-7:file-42".to_owned(),
        })
    }
}

fn session_id(value: &str) -> SessionId {
    SessionId::parse(value).unwrap()
}

fn scratch_id(value: &str) -> ScratchId {
    ScratchId::parse(value).unwrap()
}

fn tool_id(value: &str) -> ToolId {
    ToolId::parse(value).unwrap()
}

fn generation() -> CatalogGeneration {
    CatalogGeneration::new(7).unwrap()
}

fn platform() -> PlatformId {
    PlatformId {
        os: OperatingSystem::Windows,
        arch: Architecture::X86_64,
    }
}

fn limits() -> ResourceLimits {
    ResourceLimits {
        descendant_processes: 8,
        memory_bytes: 64 * 1024 * 1024,
        cpu_time_ms: 20_000,
        wall_time_ms: 10_000,
        open_descriptors_or_handles: 128,
        captured_output_bytes: (MAX_IO_CHUNK_BYTES as u64) * 4,
    }
}

fn catalog() -> ValidatedCatalogSnapshot {
    CatalogSnapshot {
        schema_version: CATALOG_SCHEMA_V1,
        generation: generation(),
        platform: platform(),
        records: vec![CatalogRecord {
            schema_version: CATALOG_SCHEMA_V1,
            generation: generation(),
            tool_id: tool_id("git"),
            platform: platform(),
            original_source_path: r"C:\source\git.exe".to_owned(),
            executable: HashedArtifact {
                logical_name: "git-executable".to_owned(),
                managed_canonical_path: r"C:\broker-private\git.exe".to_owned(),
                sha256: Sha256Digest::from_bytes([0x33; 32]),
                kind: ArtifactKind::Executable,
            },
            helpers: Vec::new(),
            non_system_libraries: Vec::new(),
            resources: Vec::new(),
            transport_adapter: None,
            approval_actor: ApprovalActor {
                display_name: "Sandbox Administrator".to_owned(),
                mechanism: "interactive-consent".to_owned(),
            },
            approved_at: UnixMillis::new(1_783_987_200_000),
            replaces: None,
        }],
        revoked_tools: Vec::new(),
    }
    .validate(&FixturePathNormalizer)
    .unwrap()
}

fn policy_request(
    access: ExecutionAccess,
    allow_process: bool,
    allow_pty: bool,
) -> ExecutionPolicyRequest {
    let constrained = access != ExecutionAccess::Full;
    ExecutionPolicyRequest {
        schema_version: POLICY_SCHEMA_V1,
        access,
        allow_process,
        allow_pty,
        workspace: constrained.then(|| WorkspaceRequest {
            requested_root: "C:/workspace".to_owned(),
        }),
        scratch_disposition: ScratchDisposition::DeleteOnCleanCloseRetainOtherwise,
        catalog_generation: generation(),
        activated_tools: constrained.then(|| tool_id("git")).into_iter().collect(),
        destinations: Vec::new(),
        environment: EnvironmentPolicy { base: Vec::new() },
        resource_limits: constrained.then(limits),
        audit_policy: AuditPolicy {
            arguments: ricochet_sandbox::ArgumentAuditMode::CountOnly,
        },
    }
}

fn read_policy() -> ValidatedExecutionPolicy {
    policy_request(ExecutionAccess::Read, true, true)
        .validate(&catalog(), &FixtureWorkspaceResolver)
        .unwrap()
}

fn process_launch(session_id: &SessionId, stdin_open: bool) -> ProcessLaunchRequest {
    ProcessLaunchRequest {
        session_id: session_id.clone(),
        executable: ExecutableRef::ManagedTool(tool_id("git")),
        arguments: vec!["status".to_owned(), "--short".to_owned()],
        cwd: Some("C:/workspace".to_owned()),
        stdin_open,
        environment: LaunchEnvironment {
            clear_environment: true,
            entries: Vec::new(),
        },
        timeout_ms: 10_000,
        stdout_max_bytes: (MAX_IO_CHUNK_BYTES as u64) * 3,
        stderr_max_bytes: MAX_IO_CHUNK_BYTES as u64,
    }
}

fn pty_launch(session_id: &SessionId) -> PtyLaunchRequest {
    PtyLaunchRequest {
        session_id: session_id.clone(),
        executable: ExecutableRef::ManagedTool(tool_id("git")),
        arguments: vec!["status".to_owned()],
        cwd: Some("C:/workspace".to_owned()),
        environment: LaunchEnvironment {
            clear_environment: true,
            entries: Vec::new(),
        },
        rows: 24,
        cols: 80,
        output_max_bytes: (MAX_IO_CHUNK_BYTES as u64) * 4,
    }
}

fn backend(config: MockBackendConfig) -> MockSandboxBackend {
    MockSandboxBackend::new(config)
}

fn prepared_session(config: MockBackendConfig, id: &SessionId) -> Box<dyn SandboxSession> {
    backend(config)
        .prepare(id.clone(), scratch_id("scratch-01"), read_policy())
        .unwrap()
}

fn process_response(response: BrokerResponse) -> ProcessSnapshot {
    match response {
        BrokerResponse::Process(snapshot) => snapshot,
        other => panic!("expected process response, got {other:?}"),
    }
}

fn pty_response(response: BrokerResponse) -> PtySnapshot {
    match response {
        BrokerResponse::Pty(snapshot) => snapshot,
        other => panic!("expected PTY response, got {other:?}"),
    }
}

fn operation_error(response: BrokerResponse) -> OperationErrorCode {
    match response {
        BrokerResponse::OperationError(error) => error.code(),
        other => panic!("expected operation error, got {other:?}"),
    }
}

fn start_process(session: &mut dyn SandboxSession, id: &SessionId) -> ProcessSnapshot {
    process_response(
        session
            .handle(SessionCommand::ProcessStart(process_launch(id, true)))
            .unwrap(),
    )
}

fn start_pty(session: &mut dyn SandboxSession, id: &SessionId) -> PtySnapshot {
    pty_response(
        session
            .handle(SessionCommand::PtyStart(pty_launch(id)))
            .unwrap(),
    )
}

fn injected_error() -> SandboxError {
    SandboxError::unavailable(
        None,
        FailedGuarantee::BrokerAvailability,
        Remediation::InspectSandboxDoctor,
        DiagnosticMetadata::empty(),
    )
}

fn config_with_failure(point: MockFailurePoint) -> MockBackendConfig {
    let mut failures = BTreeMap::new();
    failures.insert(point, injected_error());
    MockBackendConfig {
        failures,
        ..MockBackendConfig::default()
    }
}

fn assert_state_transition(event: &BrokerEvent, from: SessionState, to: SessionState) {
    let BrokerEvent::Audit(record) = event else {
        panic!("expected audit event, got {event:?}");
    };
    assert!(matches!(
        record.event(),
        AuditEventKind::StateTransition {
            from: actual_from,
            to: actual_to,
        } if *actual_from == from && *actual_to == to
    ));
}

#[test]
fn backend_and_session_traits_are_object_safe_and_have_required_send_bounds() {
    fn accept_backend(_: &dyn SandboxBackend) {}
    fn assert_send<T: Send + ?Sized>() {}
    fn assert_sync<T: Sync + ?Sized>() {}

    let backend = backend(MockBackendConfig::default());
    accept_backend(&backend);
    assert_send::<MockSandboxBackend>();
    assert_sync::<MockSandboxBackend>();
    assert_send::<dyn SandboxSession>();
}

#[test]
fn mock_backend_can_never_claim_real_enforcement() {
    let backend = backend(MockBackendConfig::default());
    let self_test = backend.self_test();
    assert_eq!(self_test.identity, backend.identity());
    assert!(!self_test.production_enforcement);
    assert!(self_test.failures.is_empty());
    assert!(self_test.capabilities.supports_complete_contract());

    let session_id = session_id("session-01");
    let session = backend
        .prepare(session_id, scratch_id("scratch-01"), read_policy())
        .unwrap();
    assert_eq!(
        session.confirmed_capabilities().enforcement(),
        EnforcementState::MockOnly
    );
    assert!(!session.confirmed_capabilities().enforcement().enforced());
}

#[test]
fn prepare_enters_ready_with_exact_capabilities_and_ordered_typed_events() {
    let backend = backend(MockBackendConfig::default());
    let id = session_id("session-01");
    let scratch = scratch_id("scratch-01");
    let mut session = backend
        .prepare(id.clone(), scratch.clone(), read_policy())
        .unwrap();

    assert_eq!(session.state(), SessionState::Ready);
    let capabilities: &ConfirmedExecutionCapabilities = session.confirmed_capabilities();
    assert_eq!(capabilities.session_id(), &id);
    assert_eq!(capabilities.scratch_id(), &scratch);
    assert_eq!(capabilities.backend(), &backend.identity());
    assert_eq!(capabilities.access(), ExecutionAccess::Read);
    assert_eq!(capabilities.enforcement(), EnforcementState::MockOnly);
    assert_eq!(capabilities.tools().len(), 1);
    assert_eq!(capabilities.tools()[0].tool_id, tool_id("git"));

    let events = session.drain_events();
    assert_eq!(events.len(), 2);
    assert_state_transition(&events[0], SessionState::Preparing, SessionState::Ready);
    let BrokerEvent::Audit(record) = &events[0] else {
        unreachable!()
    };
    assert_eq!(record.context().session_id(), &id);
    assert_eq!(record.context().enforcement(), EnforcementState::MockOnly);
    assert!(matches!(
        events[1],
        BrokerEvent::SessionState(SessionState::Ready)
    ));
    assert!(session.drain_events().is_empty());
}

#[test]
fn request_conversion_admits_every_session_command_and_rejects_broker_only_requests() {
    let id = session_id("session-01");
    let process = ProcessRequest {
        session_id: id.clone(),
        process_id: ProcessId::new(0),
    };
    let pty = PtyRequest {
        session_id: id.clone(),
        pty_id: PtyId::new(0),
    };
    let admitted = vec![
        BrokerRequest::CloseSession(SessionRequest {
            session_id: id.clone(),
        }),
        BrokerRequest::CancelSession(CancelSessionRequest {
            session_id: id.clone(),
        }),
        BrokerRequest::ProcessStart(process_launch(&id, true)),
        BrokerRequest::ProcessList(SessionRequest {
            session_id: id.clone(),
        }),
        BrokerRequest::ProcessDetail(process.clone()),
        BrokerRequest::ProcessRead(ProcessReadRequest {
            session_id: id.clone(),
            process_id: ProcessId::new(0),
            stdout_offset: 0,
            stderr_offset: 0,
            max_bytes_per_stream: 1,
        }),
        BrokerRequest::ProcessWrite(ProcessWriteRequest {
            session_id: id.clone(),
            process_id: ProcessId::new(0),
            bytes: WireBytes::new(vec![1]).unwrap(),
            close_stdin: false,
        }),
        BrokerRequest::ProcessCancel(process.clone()),
        BrokerRequest::ProcessRelease(process),
        BrokerRequest::PtyStart(pty_launch(&id)),
        BrokerRequest::PtyList(SessionRequest {
            session_id: id.clone(),
        }),
        BrokerRequest::PtyDetail(pty.clone()),
        BrokerRequest::PtyRead(PtyReadRequest {
            session_id: id.clone(),
            pty_id: PtyId::new(0),
            offset: 0,
            max_bytes: 1,
        }),
        BrokerRequest::PtyWrite(PtyWriteRequest {
            session_id: id.clone(),
            pty_id: PtyId::new(0),
            bytes: WireBytes::new(vec![1]).unwrap(),
        }),
        BrokerRequest::PtyResize(PtyResizeRequest {
            session_id: id.clone(),
            pty_id: PtyId::new(0),
            rows: 30,
            cols: 100,
        }),
        BrokerRequest::PtyStop(pty.clone()),
        BrokerRequest::PtyRelease(pty),
    ];
    for request in admitted {
        assert!(SessionCommand::try_from(request).is_ok());
    }

    let rejected = vec![
        BrokerRequest::Handshake(HandshakeRequest {
            supported_protocol_versions: vec![1],
            connection_nonce: ConnectionNonce::from_bytes([0; 32]),
            channel_binding: Sha256Digest::from_bytes([1; 32]),
        }),
        BrokerRequest::CreateSession(CreateSessionRequest {
            session_id: id,
            policy: policy_request(ExecutionAccess::Read, true, true),
        }),
        BrokerRequest::CatalogPublicSnapshot,
        BrokerRequest::Ping,
    ];
    for request in rejected {
        let error = match SessionCommand::try_from(request) {
            Ok(_) => panic!("broker-only request crossed the session boundary"),
            Err(error) => error,
        };
        assert_eq!(error.kind(), "BrokerProtocolError");
    }
}

#[test]
fn retained_process_operations_preserve_ids_arguments_offsets_status_and_release_rules() {
    let id = session_id("session-01");
    let mut session = prepared_session(
        MockBackendConfig {
            scripted_exit_code: 7,
            scripted_stdout: b"abcdef".to_vec(),
            scripted_stderr: b"XYZ".to_vec(),
            ..MockBackendConfig::default()
        },
        &id,
    );
    session.drain_events();

    let started = start_process(session.as_mut(), &id);
    assert_eq!(session.state(), SessionState::Running);
    assert_eq!(started.id, ProcessId::new(0));
    assert_eq!(started.process_tree_id, ProcessTreeId::new(0));
    assert_eq!(started.command_display, "managed:git");
    assert_eq!(started.arguments, ["status", "--short"]);
    assert_eq!(started.argument_count, 2);
    assert_eq!(started.status, ProcessStatus::Running);
    assert!(started.running);
    assert!(started.stdin_open);
    assert_eq!(started.stdout_len, 6);
    assert_eq!(started.stderr_len, 3);

    let listed = session
        .handle(SessionCommand::ProcessList(SessionRequest {
            session_id: id.clone(),
        }))
        .unwrap();
    let BrokerResponse::Processes(listed) = listed else {
        panic!("expected process list")
    };
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, started.id);

    let detailed = process_response(
        session
            .handle(SessionCommand::ProcessDetail(ProcessRequest {
                session_id: id.clone(),
                process_id: started.id,
            }))
            .unwrap(),
    );
    assert_eq!(detailed.process_tree_id, started.process_tree_id);

    let read = session
        .handle(SessionCommand::ProcessRead(ProcessReadRequest {
            session_id: id.clone(),
            process_id: started.id,
            stdout_offset: 1,
            stderr_offset: 1,
            max_bytes_per_stream: 2,
        }))
        .unwrap();
    let BrokerResponse::ProcessRead(read) = read else {
        panic!("expected process read")
    };
    assert_eq!(read.stdout_offset, 1);
    assert_eq!(read.stderr_offset, 1);
    assert_eq!(read.stdout.as_slice(), b"bc");
    assert_eq!(read.stderr.as_slice(), b"YZ");

    let written = process_response(
        session
            .handle(SessionCommand::ProcessWrite(ProcessWriteRequest {
                session_id: id.clone(),
                process_id: started.id,
                bytes: WireBytes::new(b"first chunk".to_vec()).unwrap(),
                close_stdin: false,
            }))
            .unwrap(),
    );
    assert!(written.running);
    assert!(written.stdin_open);
    assert_eq!(
        operation_error(
            session
                .handle(SessionCommand::ProcessRelease(ProcessRequest {
                    session_id: id.clone(),
                    process_id: started.id,
                }))
                .unwrap(),
        ),
        OperationErrorCode::ProcessRunning
    );

    let exited = process_response(
        session
            .handle(SessionCommand::ProcessWrite(ProcessWriteRequest {
                session_id: id.clone(),
                process_id: started.id,
                bytes: WireBytes::new(b"last chunk".to_vec()).unwrap(),
                close_stdin: true,
            }))
            .unwrap(),
    );
    assert_eq!(exited.status, ProcessStatus::Exited);
    assert!(!exited.running);
    assert!(!exited.stdin_open);
    assert!(!exited.success);
    assert_eq!(exited.exit_code, Some(7));
    assert_eq!(
        operation_error(
            session
                .handle(SessionCommand::ProcessWrite(ProcessWriteRequest {
                    session_id: id.clone(),
                    process_id: started.id,
                    bytes: WireBytes::new(b"too late".to_vec()).unwrap(),
                    close_stdin: false,
                }))
                .unwrap(),
        ),
        OperationErrorCode::ProcessNotRunning
    );

    assert!(matches!(
        session
            .handle(SessionCommand::ProcessRelease(ProcessRequest {
                session_id: id.clone(),
                process_id: started.id,
            }))
            .unwrap(),
        BrokerResponse::Acknowledged
    ));
    let BrokerResponse::Processes(listed) = session
        .handle(SessionCommand::ProcessList(SessionRequest {
            session_id: id.clone(),
        }))
        .unwrap()
    else {
        panic!("expected process list")
    };
    assert!(listed.is_empty());
    assert_eq!(
        operation_error(
            session
                .handle(SessionCommand::ProcessDetail(ProcessRequest {
                    session_id: id,
                    process_id: started.id,
                }))
                .unwrap(),
        ),
        OperationErrorCode::ProcessNotFound
    );
}

#[test]
fn chunked_initial_stdin_supports_keep_open_and_close_one_shot_sequence() {
    let id = session_id("session-01");
    let scripted_stdout = (0..(MAX_IO_CHUNK_BYTES * 2 + 17))
        .map(|index| (index % 251) as u8)
        .collect::<Vec<_>>();
    let mut session = prepared_session(
        MockBackendConfig {
            scripted_stdout: scripted_stdout.clone(),
            ..MockBackendConfig::default()
        },
        &id,
    );
    session.drain_events();

    let started = start_process(session.as_mut(), &id);
    let initial_stdin = vec![0x5a; MAX_IO_CHUNK_BYTES + 19];
    let chunks = chunk_wire_bytes(&initial_stdin);
    assert_eq!(chunks.len(), 2);
    assert_eq!(chunks[0].as_slice().len(), MAX_IO_CHUNK_BYTES);
    assert_eq!(chunks[1].as_slice().len(), 19);
    let mut latest = started;
    for chunk in chunks {
        latest = process_response(
            session
                .handle(SessionCommand::ProcessWrite(ProcessWriteRequest {
                    session_id: id.clone(),
                    process_id: latest.id,
                    bytes: chunk,
                    close_stdin: false,
                }))
                .unwrap(),
        );
    }
    assert!(
        latest.running,
        "keep-open initial stdin must retain the job"
    );
    assert!(latest.stdin_open);

    let exited = process_response(
        session
            .handle(SessionCommand::ProcessWrite(ProcessWriteRequest {
                session_id: id.clone(),
                process_id: latest.id,
                bytes: WireBytes::new(Vec::new()).unwrap(),
                close_stdin: true,
            }))
            .unwrap(),
    );
    assert_eq!(exited.status, ProcessStatus::Exited);
    assert_eq!(exited.exit_code, Some(0));
    assert!(!exited.stdin_open);

    let mut reconstructed = Vec::new();
    let mut offset = 0_u64;
    while offset < exited.stdout_len {
        let response = session
            .handle(SessionCommand::ProcessRead(ProcessReadRequest {
                session_id: id.clone(),
                process_id: exited.id,
                stdout_offset: offset,
                stderr_offset: 0,
                max_bytes_per_stream: MAX_IO_CHUNK_BYTES as u32,
            }))
            .unwrap();
        let BrokerResponse::ProcessRead(read) = response else {
            panic!("expected process read")
        };
        assert_eq!(read.stdout_offset, offset);
        assert!(read.stdout.as_slice().len() <= MAX_IO_CHUNK_BYTES);
        reconstructed.extend_from_slice(read.stdout.as_slice());
        offset += read.stdout.as_slice().len() as u64;
    }
    assert_eq!(reconstructed, scripted_stdout);
    assert!(matches!(
        session
            .handle(SessionCommand::ProcessRelease(ProcessRequest {
                session_id: id,
                process_id: exited.id,
            }))
            .unwrap(),
        BrokerResponse::Acknowledged
    ));
}

#[test]
fn pty_operations_preserve_ids_arguments_output_offsets_dimensions_stop_and_release() {
    let id = session_id("session-01");
    let mut session = prepared_session(
        MockBackendConfig {
            scripted_stdout: b"pty-out".to_vec(),
            scripted_stderr: b"+err".to_vec(),
            ..MockBackendConfig::default()
        },
        &id,
    );
    session.drain_events();

    let started = start_pty(session.as_mut(), &id);
    assert_eq!(started.id, PtyId::new(0));
    assert_eq!(started.process_tree_id, ProcessTreeId::new(0));
    assert_eq!(started.command_display, "managed:git");
    assert_eq!(started.arguments, ["status"]);
    assert_eq!(started.argument_count, 1);
    assert_eq!(started.status, PtyStatus::Running);
    assert!(started.running);
    assert_eq!((started.rows, started.cols), (24, 80));
    assert_eq!(started.native_process_id, Some(10_000));
    assert_eq!(started.output_len, 11);

    let BrokerResponse::Ptys(listed) = session
        .handle(SessionCommand::PtyList(SessionRequest {
            session_id: id.clone(),
        }))
        .unwrap()
    else {
        panic!("expected PTY list")
    };
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, started.id);
    let detailed = pty_response(
        session
            .handle(SessionCommand::PtyDetail(PtyRequest {
                session_id: id.clone(),
                pty_id: started.id,
            }))
            .unwrap(),
    );
    assert_eq!(detailed.process_tree_id, started.process_tree_id);

    let BrokerResponse::PtyRead(read) = session
        .handle(SessionCommand::PtyRead(PtyReadRequest {
            session_id: id.clone(),
            pty_id: started.id,
            offset: 2,
            max_bytes: 3,
        }))
        .unwrap()
    else {
        panic!("expected PTY read")
    };
    assert_eq!(read.offset, 2);
    assert_eq!(read.output.as_slice(), b"y-o");

    let written = pty_response(
        session
            .handle(SessionCommand::PtyWrite(PtyWriteRequest {
                session_id: id.clone(),
                pty_id: started.id,
                bytes: WireBytes::new(b"input".to_vec()).unwrap(),
            }))
            .unwrap(),
    );
    assert!(written.running);
    assert_eq!((written.rows, written.cols), (24, 80));

    let resized = pty_response(
        session
            .handle(SessionCommand::PtyResize(PtyResizeRequest {
                session_id: id.clone(),
                pty_id: started.id,
                rows: 40,
                cols: 120,
            }))
            .unwrap(),
    );
    assert_eq!((resized.rows, resized.cols), (40, 120));
    assert_eq!(
        operation_error(
            session
                .handle(SessionCommand::PtyRelease(PtyRequest {
                    session_id: id.clone(),
                    pty_id: started.id,
                }))
                .unwrap(),
        ),
        OperationErrorCode::PtyRunning
    );

    let stopped = pty_response(
        session
            .handle(SessionCommand::PtyStop(PtyRequest {
                session_id: id.clone(),
                pty_id: started.id,
            }))
            .unwrap(),
    );
    assert_eq!(stopped.status, PtyStatus::Stopped);
    assert!(stopped.stopped);
    assert!(!stopped.running);
    assert_eq!(
        operation_error(
            session
                .handle(SessionCommand::PtyWrite(PtyWriteRequest {
                    session_id: id.clone(),
                    pty_id: started.id,
                    bytes: WireBytes::new(b"too late".to_vec()).unwrap(),
                }))
                .unwrap(),
        ),
        OperationErrorCode::PtyClosed
    );
    assert!(matches!(
        session
            .handle(SessionCommand::PtyRelease(PtyRequest {
                session_id: id.clone(),
                pty_id: started.id,
            }))
            .unwrap(),
        BrokerResponse::Acknowledged
    ));
    let BrokerResponse::Ptys(listed) = session
        .handle(SessionCommand::PtyList(SessionRequest {
            session_id: id.clone(),
        }))
        .unwrap()
    else {
        panic!("expected PTY list")
    };
    assert!(listed.is_empty());
    assert_eq!(
        operation_error(
            session
                .handle(SessionCommand::PtyDetail(PtyRequest {
                    session_id: id,
                    pty_id: started.id,
                }))
                .unwrap(),
        ),
        OperationErrorCode::PtyNotFound
    );
}

#[test]
fn targeted_process_cancel_terminates_only_its_process_tree() {
    let id = session_id("session-01");
    let mut session = prepared_session(MockBackendConfig::default(), &id);
    session.drain_events();
    let first = start_process(session.as_mut(), &id);
    let second = start_process(session.as_mut(), &id);
    assert_eq!(first.process_tree_id, ProcessTreeId::new(0));
    assert_eq!(second.process_tree_id, ProcessTreeId::new(1));
    session.drain_events();

    let cancelled = process_response(
        session
            .handle(SessionCommand::ProcessCancel(ProcessRequest {
                session_id: id.clone(),
                process_id: first.id,
            }))
            .unwrap(),
    );
    assert_eq!(cancelled.status, ProcessStatus::Cancelled);
    assert!(cancelled.cancelled);
    assert!(!cancelled.running);
    let survivor = process_response(
        session
            .handle(SessionCommand::ProcessDetail(ProcessRequest {
                session_id: id,
                process_id: second.id,
            }))
            .unwrap(),
    );
    assert!(survivor.running);

    let events = session.drain_events();
    assert_eq!(events.len(), 2);
    let BrokerEvent::Audit(record) = &events[0] else {
        panic!("cancel must emit audit first")
    };
    assert!(matches!(
        record.event(),
        AuditEventKind::Cancelled { execution, reason }
            if *execution == ricochet_sandbox::ExecutionAuditIdentity::process(
                first.process_tree_id,
                first.id,
            ) && *reason == TerminationReason::CancelledByHost
    ));
    let BrokerEvent::Terminated(notice) = &events[1] else {
        panic!("cancel must emit a typed termination notice second")
    };
    assert_eq!(notice.reason, TerminationReason::CancelledByHost);
    assert_eq!(notice.process_tree_ids, [first.process_tree_id]);
}

#[test]
fn host_session_cancel_derives_cancelled_by_host_and_closes_after_all_descendants() {
    let id = session_id("session-01");
    let mut session = prepared_session(MockBackendConfig::default(), &id);
    session.drain_events();
    let process = start_process(session.as_mut(), &id);
    let pty = start_pty(session.as_mut(), &id);
    session.drain_events();

    assert!(matches!(
        session
            .handle(SessionCommand::Cancel(CancelSessionRequest {
                session_id: id,
            }))
            .unwrap(),
        BrokerResponse::Acknowledged
    ));
    assert_eq!(session.state(), SessionState::Closed);
    let events = session.drain_events();
    assert_eq!(events.len(), 7);
    assert_state_transition(&events[0], SessionState::Running, SessionState::Stopping);
    assert!(matches!(
        events[1],
        BrokerEvent::SessionState(SessionState::Stopping)
    ));
    for event in &events[2..4] {
        let BrokerEvent::Audit(record) = event else {
            panic!("descendants must be audited before closure")
        };
        assert!(matches!(
            record.event(),
            AuditEventKind::Cancelled {
                reason: TerminationReason::CancelledByHost,
                ..
            }
        ));
    }
    let BrokerEvent::Terminated(notice) = &events[4] else {
        panic!("expected consolidated termination notice")
    };
    assert_eq!(notice.reason, TerminationReason::CancelledByHost);
    assert_eq!(
        notice.process_tree_ids,
        [process.process_tree_id, pty.process_tree_id]
    );
    assert_state_transition(&events[5], SessionState::Stopping, SessionState::Closed);
    assert!(matches!(
        events[6],
        BrokerEvent::SessionState(SessionState::Closed)
    ));
}

#[test]
fn broker_termination_supports_every_broker_owned_reason_with_complete_tree_events() {
    let reasons = [
        TerminationReason::TimedOut,
        TerminationReason::BrokerShutdown,
        TerminationReason::PolicyEnforcement,
        TerminationReason::ResourceLimit(ResourceLimitKind::CapturedOutput),
        TerminationReason::SessionClosed,
    ];
    for (index, reason) in reasons.into_iter().enumerate() {
        let id = session_id(&format!("session-{index}"));
        let mut session = prepared_session(MockBackendConfig::default(), &id);
        session.drain_events();
        let process = start_process(session.as_mut(), &id);
        session.drain_events();

        session.terminate(reason).unwrap();
        assert_eq!(session.state(), SessionState::Closed);
        let events = session.drain_events();
        let notice = events.iter().find_map(|event| match event {
            BrokerEvent::Terminated(notice) => Some(notice),
            _ => None,
        });
        let notice = notice.expect("broker termination must emit a typed notice");
        assert_eq!(notice.reason, reason);
        assert_eq!(notice.process_tree_ids, [process.process_tree_id]);
        let terminal_audit = events.iter().find_map(|event| match event {
            BrokerEvent::Audit(record)
                if !matches!(record.event(), AuditEventKind::StateTransition { .. }) =>
            {
                Some(record.event())
            }
            _ => None,
        });
        match reason {
            TerminationReason::TimedOut => {
                assert!(matches!(
                    terminal_audit,
                    Some(AuditEventKind::TimedOut { .. })
                ))
            }
            TerminationReason::ResourceLimit(limit) => assert!(matches!(
                terminal_audit,
                Some(AuditEventKind::ResourceLimit {
                    limit: actual,
                    ..
                }) if *actual == limit
            )),
            other => assert!(matches!(
                terminal_audit,
                Some(AuditEventKind::Cancelled { reason, .. }) if *reason == other
            )),
        }
    }
}

#[test]
fn close_derives_session_closed_and_orders_stopping_termination_then_closed() {
    let id = session_id("session-01");
    let mut session = prepared_session(MockBackendConfig::default(), &id);
    session.drain_events();
    let process = start_process(session.as_mut(), &id);
    session.drain_events();

    assert!(matches!(
        session
            .handle(SessionCommand::Close(SessionRequest { session_id: id }))
            .unwrap(),
        BrokerResponse::Acknowledged
    ));
    let events = session.drain_events();
    assert_eq!(events.len(), 6);
    assert_state_transition(&events[0], SessionState::Running, SessionState::Stopping);
    assert!(matches!(
        events[1],
        BrokerEvent::SessionState(SessionState::Stopping)
    ));
    let BrokerEvent::Audit(record) = &events[2] else {
        panic!("descendant termination audit must precede Closed")
    };
    assert!(matches!(
        record.event(),
        AuditEventKind::Cancelled {
            execution,
            reason: TerminationReason::SessionClosed,
        } if *execution == ricochet_sandbox::ExecutionAuditIdentity::process(
            process.process_tree_id,
            process.id,
        )
    ));
    let BrokerEvent::Terminated(notice) = &events[3] else {
        panic!("termination notice must precede Closed")
    };
    assert_eq!(notice.reason, TerminationReason::SessionClosed);
    assert_state_transition(&events[4], SessionState::Stopping, SessionState::Closed);
    assert!(matches!(
        events[5],
        BrokerEvent::SessionState(SessionState::Closed)
    ));
    assert_eq!(session.state(), SessionState::Closed);
}

#[test]
fn tool_revocation_terminates_every_affected_process_and_pty_tree_with_typed_audit() {
    let id = session_id("session-01");
    let mut session = prepared_session(MockBackendConfig::default(), &id);
    session.drain_events();
    let process = start_process(session.as_mut(), &id);
    let pty = start_pty(session.as_mut(), &id);
    session.drain_events();

    session.revoke(&tool_id("git")).unwrap();
    assert_eq!(session.state(), SessionState::Running);
    let process = process_response(
        session
            .handle(SessionCommand::ProcessDetail(ProcessRequest {
                session_id: id.clone(),
                process_id: process.id,
            }))
            .unwrap(),
    );
    let pty = pty_response(
        session
            .handle(SessionCommand::PtyDetail(PtyRequest {
                session_id: id,
                pty_id: pty.id,
            }))
            .unwrap(),
    );
    assert_eq!(process.status, ProcessStatus::Cancelled);
    assert_eq!(pty.status, PtyStatus::Stopped);

    let events = session.drain_events();
    assert_eq!(events.len(), 4);
    let BrokerEvent::Audit(record) = &events[0] else {
        panic!("revocation audit must be first")
    };
    assert!(matches!(
        record.event(),
        AuditEventKind::Revoked {
            tool_id: revoked,
            affected_process_trees,
        } if *revoked == tool_id("git")
            && affected_process_trees == &[process.process_tree_id, pty.process_tree_id]
    ));
    for event in &events[1..3] {
        let BrokerEvent::Audit(record) = event else {
            panic!("affected execution must have a terminal audit")
        };
        assert!(matches!(
            record.event(),
            AuditEventKind::Cancelled {
                reason: TerminationReason::ToolRevoked,
                ..
            }
        ));
    }
    let BrokerEvent::Terminated(notice) = &events[3] else {
        panic!("revocation must emit consolidated termination notice")
    };
    assert_eq!(notice.reason, TerminationReason::ToolRevoked);
    assert_eq!(
        notice.process_tree_ids,
        [process.process_tree_id, pty.process_tree_id]
    );
}

#[test]
fn wrong_session_commands_fail_before_mutating_state() {
    let id = session_id("session-01");
    let wrong = session_id("session-02");
    let mut session = prepared_session(MockBackendConfig::default(), &id);
    session.drain_events();
    start_process(session.as_mut(), &id);
    session.drain_events();

    for command in [
        SessionCommand::ProcessList(SessionRequest {
            session_id: wrong.clone(),
        }),
        SessionCommand::PtyList(SessionRequest {
            session_id: wrong.clone(),
        }),
        SessionCommand::Close(SessionRequest { session_id: wrong }),
    ] {
        assert_eq!(
            session.handle(command).unwrap_err().kind(),
            "BrokerProtocolError"
        );
    }
    assert_eq!(session.state(), SessionState::Running);
    assert!(session.drain_events().is_empty());
}

#[test]
fn launch_event_and_audit_order_is_deterministic_and_timestamps_are_monotonic() {
    let id = session_id("session-01");
    let mut session = prepared_session(MockBackendConfig::default(), &id);
    session.drain_events();
    let process = start_process(session.as_mut(), &id);

    let events = session.drain_events();
    assert_eq!(events.len(), 4);
    let BrokerEvent::Audit(launch) = &events[0] else {
        panic!("launch audit must be first")
    };
    assert!(matches!(
        launch.event(),
        AuditEventKind::LaunchRequested {
            surface: ricochet_sandbox::ExecutionSurface::Process,
            tool_id: Some(tool),
            argument_count: 2,
        } if *tool == tool_id("git")
    ));
    let BrokerEvent::Audit(tree) = &events[1] else {
        panic!("process-tree audit must be second")
    };
    assert!(matches!(
        tree.event(),
        AuditEventKind::ProcessTreeStarted { process_tree_id }
            if *process_tree_id == process.process_tree_id
    ));
    assert_state_transition(&events[2], SessionState::Ready, SessionState::Running);
    assert!(matches!(
        events[3],
        BrokerEvent::SessionState(SessionState::Running)
    ));
    let audit_times = events
        .iter()
        .filter_map(|event| match event {
            BrokerEvent::Audit(record) => Some(
                serde_json::to_value(record).unwrap()["at"]
                    .as_u64()
                    .unwrap(),
            ),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(audit_times.windows(2).all(|pair| pair[0] < pair[1]));
    for event in events {
        if let BrokerEvent::Audit(record) = event {
            assert_eq!(record.context().session_id(), &id);
            assert_eq!(record.context().enforcement(), EnforcementState::MockOnly);
        }
    }
}

#[test]
fn every_mock_failure_point_is_reachable_and_fails_before_mutation() {
    let self_test_backend = backend(config_with_failure(MockFailurePoint::SelfTest));
    let self_test = self_test_backend.self_test();
    assert!(!self_test.production_enforcement);
    assert_eq!(self_test.failures.len(), 1);
    assert_eq!(self_test.failures[0].feature.as_str(), "mock-self-test");
    assert_eq!(
        self_test.failures[0].guarantee,
        FailedGuarantee::BrokerAvailability
    );

    let id = session_id("session-01");
    let prepare_error = match backend(config_with_failure(MockFailurePoint::Prepare)).prepare(
        id.clone(),
        scratch_id("scratch-01"),
        read_policy(),
    ) {
        Ok(_) => panic!("prepare failure injection was ignored"),
        Err(error) => error,
    };
    assert_eq!(prepare_error.kind(), "SandboxUnavailable");

    let mut launch = prepared_session(config_with_failure(MockFailurePoint::Launch), &id);
    launch.drain_events();
    assert_eq!(
        launch
            .handle(SessionCommand::ProcessStart(process_launch(&id, true)))
            .unwrap_err()
            .kind(),
        "SandboxUnavailable"
    );
    assert_eq!(launch.state(), SessionState::Ready);
    assert!(launch.drain_events().is_empty());

    let mut write = prepared_session(config_with_failure(MockFailurePoint::Write), &id);
    write.drain_events();
    let process = start_process(write.as_mut(), &id);
    write.drain_events();
    assert_eq!(
        write
            .handle(SessionCommand::ProcessWrite(ProcessWriteRequest {
                session_id: id.clone(),
                process_id: process.id,
                bytes: WireBytes::new(b"blocked".to_vec()).unwrap(),
                close_stdin: true,
            }))
            .unwrap_err()
            .kind(),
        "SandboxUnavailable"
    );
    let process = process_response(
        write
            .handle(SessionCommand::ProcessDetail(ProcessRequest {
                session_id: id.clone(),
                process_id: process.id,
            }))
            .unwrap(),
    );
    assert!(process.running);
    assert!(process.stdin_open);
    assert!(write.drain_events().is_empty());

    let mut resize = prepared_session(config_with_failure(MockFailurePoint::Resize), &id);
    resize.drain_events();
    let pty = start_pty(resize.as_mut(), &id);
    resize.drain_events();
    assert_eq!(
        resize
            .handle(SessionCommand::PtyResize(PtyResizeRequest {
                session_id: id.clone(),
                pty_id: pty.id,
                rows: 40,
                cols: 120,
            }))
            .unwrap_err()
            .kind(),
        "SandboxUnavailable"
    );
    let pty = pty_response(
        resize
            .handle(SessionCommand::PtyDetail(PtyRequest {
                session_id: id.clone(),
                pty_id: pty.id,
            }))
            .unwrap(),
    );
    assert_eq!((pty.rows, pty.cols), (24, 80));
    assert!(resize.drain_events().is_empty());

    let mut cancel = prepared_session(config_with_failure(MockFailurePoint::Cancel), &id);
    cancel.drain_events();
    let process = start_process(cancel.as_mut(), &id);
    cancel.drain_events();
    assert_eq!(
        cancel
            .handle(SessionCommand::ProcessCancel(ProcessRequest {
                session_id: id.clone(),
                process_id: process.id,
            }))
            .unwrap_err()
            .kind(),
        "SandboxUnavailable"
    );
    assert!(
        process_response(
            cancel
                .handle(SessionCommand::ProcessDetail(ProcessRequest {
                    session_id: id.clone(),
                    process_id: process.id,
                }))
                .unwrap(),
        )
        .running
    );
    assert!(cancel.drain_events().is_empty());

    let mut close = prepared_session(config_with_failure(MockFailurePoint::Close), &id);
    close.drain_events();
    start_process(close.as_mut(), &id);
    close.drain_events();
    assert_eq!(
        close
            .handle(SessionCommand::Close(SessionRequest {
                session_id: id.clone(),
            }))
            .unwrap_err()
            .kind(),
        "SandboxUnavailable"
    );
    assert_eq!(close.state(), SessionState::Running);
    assert!(close.drain_events().is_empty());

    let mut revocation = prepared_session(config_with_failure(MockFailurePoint::Revocation), &id);
    revocation.drain_events();
    let process = start_process(revocation.as_mut(), &id);
    let pty = start_pty(revocation.as_mut(), &id);
    revocation.drain_events();
    assert_eq!(
        revocation.revoke(&tool_id("git")).unwrap_err().kind(),
        "SandboxUnavailable"
    );
    assert!(
        process_response(
            revocation
                .handle(SessionCommand::ProcessDetail(ProcessRequest {
                    session_id: id.clone(),
                    process_id: process.id,
                }))
                .unwrap(),
        )
        .running
    );
    assert!(
        pty_response(
            revocation
                .handle(SessionCommand::PtyDetail(PtyRequest {
                    session_id: id,
                    pty_id: pty.id,
                }))
                .unwrap(),
        )
        .running
    );
    assert!(revocation.drain_events().is_empty());
}

#[test]
fn closed_session_is_terminal_and_rejects_new_execution_work() {
    let id = session_id("session-01");
    let mut session = prepared_session(MockBackendConfig::default(), &id);
    session.drain_events();
    session
        .handle(SessionCommand::Close(SessionRequest {
            session_id: id.clone(),
        }))
        .unwrap();
    session.drain_events();

    assert_eq!(
        session
            .handle(SessionCommand::ProcessStart(process_launch(&id, true)))
            .unwrap_err()
            .kind(),
        "SandboxTerminated"
    );
    assert_eq!(session.state(), SessionState::Closed);
    assert!(session.drain_events().is_empty());
}
