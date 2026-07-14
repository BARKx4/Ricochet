use std::fmt::Debug;

use ricochet_sandbox::{
    chunk_wire_bytes, ApprovalActor, Architecture, ArtifactKind, AuditContext, AuditPolicy,
    AuthenticatedChannelContext, BackendIdentity, BrokerEvent, BrokerRequest, BrokerRequestKind,
    BrokerResponse, CancelSessionRequest, CatalogGeneration, CatalogPathNormalizer, CatalogRecord,
    CatalogSnapshot, ConfirmedExecutionCapabilities, ConnectionNonce, CreateSessionRequest,
    DiagnosticMetadata, EnforcementState, EnvironmentPolicy, EnvironmentVariable, ExecutableRef,
    ExecutionAccess, ExecutionPolicyRequest, FailedGuarantee, HandshakeRequest, HandshakeResponse,
    HashedArtifact, LaunchEnvironment, OperatingSystem, OperationError, OperationErrorCode,
    OperationSubject, PeerContextId, PlatformId, ProcessId, ProcessLaunchRequest,
    ProcessReadRequest, ProcessReadSnapshot, ProcessRequest, ProcessSnapshot, ProcessStatus,
    ProcessTreeId, ProtocolEnvelope, ProtocolMessage, PtyId, PtyLaunchRequest, PtyReadRequest,
    PtyReadSnapshot, PtyRequest, PtyResizeRequest, PtySnapshot, PtyStatus, PtyWriteRequest,
    PublicCatalogSnapshot, PublicToolRecord, RequestId, ResourceLimitKind, ResourceLimits,
    ResponseCorrelation, SandboxError, ScratchDisposition, ScratchId, SessionId, SessionRequest,
    Sha256Digest, TerminationNotice, TerminationReason, ToolId, UnixMillis,
    ValidatedCatalogSnapshot, ValidatedExecutionPolicy, WireBytes, WorkspaceIdentity,
    WorkspaceIdentityResolver, WorkspaceRequest, CATALOG_SCHEMA_V1, MAX_IO_CHUNK_BYTES,
    POLICY_SCHEMA_V1, PROTOCOL_V1,
};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{json, Value};

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

fn session(value: &str) -> SessionId {
    SessionId::parse(value).unwrap()
}

fn tool(value: &str) -> ToolId {
    ToolId::parse(value).unwrap()
}

fn generation(value: u64) -> CatalogGeneration {
    CatalogGeneration::new(value).unwrap()
}

fn nonce(byte: u8) -> ConnectionNonce {
    ConnectionNonce::from_bytes([byte; 32])
}

fn binding(byte: u8) -> Sha256Digest {
    Sha256Digest::from_bytes([byte; 32])
}

fn limits() -> ResourceLimits {
    ResourceLimits {
        descendant_processes: 4,
        memory_bytes: 64 * 1024 * 1024,
        cpu_time_ms: 10_000,
        wall_time_ms: 5_000,
        open_descriptors_or_handles: 64,
        captured_output_bytes: 4_096,
    }
}

fn platform() -> PlatformId {
    PlatformId {
        os: OperatingSystem::Windows,
        arch: Architecture::X86_64,
    }
}

fn catalog() -> ValidatedCatalogSnapshot {
    CatalogSnapshot {
        schema_version: CATALOG_SCHEMA_V1,
        generation: generation(7),
        platform: platform(),
        records: vec![CatalogRecord {
            schema_version: CATALOG_SCHEMA_V1,
            generation: generation(7),
            tool_id: tool("git"),
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
            transport_adapter: Some(ricochet_sandbox::TransportAdapter::HttpConnect),
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
        catalog_generation: generation(7),
        activated_tools: if constrained {
            vec![tool("git")]
        } else {
            Vec::new()
        },
        destinations: if constrained {
            vec!["github.com:443".parse().unwrap()]
        } else {
            Vec::new()
        },
        environment: EnvironmentPolicy { base: Vec::new() },
        resource_limits: constrained.then(limits),
        audit_policy: AuditPolicy {
            arguments: ricochet_sandbox::ArgumentAuditMode::CountOnly,
        },
    }
}

fn policy(
    access: ExecutionAccess,
    allow_process: bool,
    allow_pty: bool,
) -> ValidatedExecutionPolicy {
    policy_request(access, allow_process, allow_pty)
        .validate(&catalog(), &FixtureWorkspaceResolver)
        .unwrap()
}

fn environment() -> LaunchEnvironment {
    LaunchEnvironment {
        clear_environment: true,
        entries: vec![EnvironmentVariable {
            name: "SAFE_NAME".to_owned(),
            value: "environment-secret".to_owned(),
        }],
    }
}

fn process_launch() -> ProcessLaunchRequest {
    ProcessLaunchRequest {
        session_id: session("session-01"),
        executable: ExecutableRef::ManagedTool(tool("git")),
        arguments: vec!["argument-secret".to_owned(), "status".to_owned()],
        cwd: Some("C:/workspace".to_owned()),
        stdin_open: true,
        environment: environment(),
        timeout_ms: 5_000,
        stdout_max_bytes: 2_048,
        stderr_max_bytes: 2_048,
    }
}

fn pty_launch() -> PtyLaunchRequest {
    PtyLaunchRequest {
        session_id: session("session-01"),
        executable: ExecutableRef::ManagedTool(tool("git")),
        arguments: vec!["pty-argument-secret".to_owned()],
        cwd: Some("C:/workspace".to_owned()),
        environment: environment(),
        rows: 24,
        cols: 80,
        output_max_bytes: 4_096,
    }
}

fn process_snapshot() -> ProcessSnapshot {
    ProcessSnapshot {
        id: ProcessId::new(0),
        process_tree_id: ProcessTreeId::new(0),
        command_display: "managed:git".to_owned(),
        arguments: vec!["snapshot-argument-secret".to_owned()],
        argument_count: 1,
        cwd: Some("C:/workspace".to_owned()),
        started_at: UnixMillis::new(1_783_987_200_000),
        status: ProcessStatus::Running,
        running: true,
        success: false,
        exit_code: None,
        error: None,
        stdout_len: 4,
        stderr_len: 0,
        stdout_truncated: false,
        stderr_truncated: false,
        stdin_open: true,
        timed_out: false,
        cancelled: false,
    }
}

fn pty_snapshot() -> PtySnapshot {
    PtySnapshot {
        id: PtyId::new(0),
        process_tree_id: ProcessTreeId::new(0),
        command_display: "managed:git".to_owned(),
        arguments: vec!["pty-snapshot-argument-secret".to_owned()],
        argument_count: 1,
        cwd: Some("C:/workspace".to_owned()),
        started_at: UnixMillis::new(1_783_987_200_000),
        status: PtyStatus::Running,
        running: true,
        success: false,
        exit_code: None,
        error: None,
        output_len: 4,
        output_truncated: false,
        rows: 24,
        cols: 80,
        native_process_id: Some(42),
        stopped: false,
    }
}

fn round_trip<T>(value: &T)
where
    T: Serialize + DeserializeOwned,
{
    let encoded = serde_json::to_value(value).unwrap();
    let decoded: T = serde_json::from_value(encoded.clone()).unwrap();
    assert_eq!(serde_json::to_value(decoded).unwrap(), encoded);
}

fn assert_omission_rejected<T>(source: &Value, label: &str, remove: impl FnOnce(&mut Value))
where
    T: DeserializeOwned,
{
    assert!(
        serde_json::from_value::<T>(source.clone()).is_ok(),
        "valid explicit field source failed before removing {label}"
    );
    let mut forged = source.clone();
    remove(&mut forged);
    assert!(
        serde_json::from_value::<T>(forged).is_err(),
        "accepted omitted {label}"
    );
}

fn envelope(sequence: u64, message: ProtocolMessage) -> ProtocolEnvelope {
    ProtocolEnvelope {
        protocol_version: PROTOCOL_V1,
        sequence,
        message,
    }
}

fn decoded_termination_event(
    reason: TerminationReason,
    error: Option<SandboxError>,
) -> ProtocolEnvelope {
    let source = envelope(
        4,
        ProtocolMessage::event(
            session("session-01"),
            BrokerEvent::Terminated(TerminationNotice {
                reason,
                process_tree_ids: vec![ProcessTreeId::new(0)],
                error,
            }),
        ),
    );
    serde_json::from_value(serde_json::to_value(source).unwrap()).unwrap()
}

#[test]
fn every_request_variant_has_a_stable_tagged_round_trip() {
    let session_request = || SessionRequest {
        session_id: session("session-01"),
    };
    let process_request = || ProcessRequest {
        session_id: session("session-01"),
        process_id: ProcessId::new(0),
    };
    let pty_request = || PtyRequest {
        session_id: session("session-01"),
        pty_id: PtyId::new(0),
    };
    let requests = vec![
        BrokerRequest::Handshake(HandshakeRequest {
            supported_protocol_versions: vec![PROTOCOL_V1],
            connection_nonce: nonce(0),
            channel_binding: binding(0x11),
        }),
        BrokerRequest::CreateSession(CreateSessionRequest {
            session_id: session("session-01"),
            policy: policy_request(ExecutionAccess::Read, true, true),
        }),
        BrokerRequest::CloseSession(session_request()),
        BrokerRequest::CancelSession(CancelSessionRequest {
            session_id: session("session-01"),
        }),
        BrokerRequest::ProcessStart(process_launch()),
        BrokerRequest::ProcessList(session_request()),
        BrokerRequest::ProcessDetail(process_request()),
        BrokerRequest::ProcessRead(ProcessReadRequest {
            session_id: session("session-01"),
            process_id: ProcessId::new(0),
            stdout_offset: 0,
            stderr_offset: 0,
            max_bytes_per_stream: MAX_IO_CHUNK_BYTES as u32,
        }),
        BrokerRequest::ProcessWrite(ricochet_sandbox::ProcessWriteRequest {
            session_id: session("session-01"),
            process_id: ProcessId::new(0),
            bytes: WireBytes::new(vec![1, 2, 3]).unwrap(),
            close_stdin: false,
        }),
        BrokerRequest::ProcessCancel(process_request()),
        BrokerRequest::ProcessRelease(process_request()),
        BrokerRequest::PtyStart(pty_launch()),
        BrokerRequest::PtyList(session_request()),
        BrokerRequest::PtyDetail(pty_request()),
        BrokerRequest::PtyRead(PtyReadRequest {
            session_id: session("session-01"),
            pty_id: PtyId::new(0),
            offset: 0,
            max_bytes: MAX_IO_CHUNK_BYTES as u32,
        }),
        BrokerRequest::PtyWrite(PtyWriteRequest {
            session_id: session("session-01"),
            pty_id: PtyId::new(0),
            bytes: WireBytes::new(vec![4, 5, 6]).unwrap(),
        }),
        BrokerRequest::PtyResize(PtyResizeRequest {
            session_id: session("session-01"),
            pty_id: PtyId::new(0),
            rows: 24,
            cols: 80,
        }),
        BrokerRequest::PtyStop(pty_request()),
        BrokerRequest::PtyRelease(pty_request()),
        BrokerRequest::CatalogPublicSnapshot,
        BrokerRequest::Ping,
    ];

    let expected_kinds = [
        BrokerRequestKind::Handshake,
        BrokerRequestKind::CreateSession,
        BrokerRequestKind::CloseSession,
        BrokerRequestKind::CancelSession,
        BrokerRequestKind::ProcessStart,
        BrokerRequestKind::ProcessList,
        BrokerRequestKind::ProcessDetail,
        BrokerRequestKind::ProcessRead,
        BrokerRequestKind::ProcessWrite,
        BrokerRequestKind::ProcessCancel,
        BrokerRequestKind::ProcessRelease,
        BrokerRequestKind::PtyStart,
        BrokerRequestKind::PtyList,
        BrokerRequestKind::PtyDetail,
        BrokerRequestKind::PtyRead,
        BrokerRequestKind::PtyWrite,
        BrokerRequestKind::PtyResize,
        BrokerRequestKind::PtyStop,
        BrokerRequestKind::PtyRelease,
        BrokerRequestKind::CatalogPublicSnapshot,
        BrokerRequestKind::Ping,
    ];

    assert_eq!(requests.len(), expected_kinds.len());
    for (request, expected_kind) in requests.iter().zip(expected_kinds) {
        assert_eq!(request.kind(), expected_kind);
        round_trip(request);
        ProtocolMessage::request(
            RequestId::new(0),
            serde_json::from_value(serde_json::to_value(request).unwrap()).unwrap(),
        )
        .validate_for(ricochet_sandbox::EndpointRole::Host)
        .unwrap();
    }
}

#[test]
fn every_response_variant_has_a_stable_tagged_round_trip() {
    let read = ProcessReadSnapshot {
        snapshot: process_snapshot(),
        stdout: WireBytes::new(vec![1, 2, 3, 4]).unwrap(),
        stderr: WireBytes::new(Vec::new()).unwrap(),
        stdout_offset: 0,
        stderr_offset: 0,
    };
    let pty_read = PtyReadSnapshot {
        snapshot: pty_snapshot(),
        output: WireBytes::new(vec![1, 2, 3, 4]).unwrap(),
        offset: 0,
    };
    let constrained = policy(ExecutionAccess::Read, true, true);
    let capabilities = ConfirmedExecutionCapabilities::new(
        session("session-01"),
        ScratchId::parse("scratch-01").unwrap(),
        BackendIdentity::new("windows-lpac", "1").unwrap(),
        PROTOCOL_V1,
        EnforcementState::Enforced,
        &constrained,
    )
    .unwrap();
    let responses = vec![
        BrokerResponse::Handshake(HandshakeResponse {
            selected_protocol_version: PROTOCOL_V1,
            connection_nonce: nonce(0),
            broker_nonce: nonce(1),
            broker_identity: BackendIdentity::new("windows-lpac", "1").unwrap(),
            peer_context_id: PeerContextId::parse("peer-01").unwrap(),
            channel_binding: binding(0x11),
        }),
        BrokerResponse::SessionCreated(capabilities),
        BrokerResponse::Acknowledged,
        BrokerResponse::Process(process_snapshot()),
        BrokerResponse::Processes(vec![process_snapshot()]),
        BrokerResponse::ProcessRead(read),
        BrokerResponse::Pty(pty_snapshot()),
        BrokerResponse::Ptys(vec![pty_snapshot()]),
        BrokerResponse::PtyRead(pty_read),
        BrokerResponse::PublicCatalog(PublicCatalogSnapshot {
            schema_version: CATALOG_SCHEMA_V1,
            generation: generation(7),
            platform: platform(),
            records: Vec::new(),
            revoked_tools: Vec::new(),
        }),
        BrokerResponse::Pong,
        BrokerResponse::OperationError(
            OperationError::new(
                OperationErrorCode::ProcessNotFound,
                OperationSubject::Process(ProcessId::new(0)),
            )
            .unwrap(),
        ),
        BrokerResponse::Error(SandboxError::policy(
            FailedGuarantee::PolicyValidity,
            DiagnosticMetadata::empty(),
        )),
    ];

    for response in &responses {
        round_trip(response);
        ProtocolMessage::response(
            RequestId::new(0),
            serde_json::from_value(serde_json::to_value(response).unwrap()).unwrap(),
        )
        .validate_for(ricochet_sandbox::EndpointRole::Broker)
        .unwrap();
    }
}

#[test]
fn literal_v1_fixtures_decode_validate_and_reencode_exactly() {
    let fixtures = [
        include_str!("fixtures/protocol/v1/handshake.json"),
        include_str!("fixtures/protocol/v1/create_session.json"),
        include_str!("fixtures/protocol/v1/error_response.json"),
        include_str!("fixtures/protocol/v1/audit_event.json"),
    ];

    for fixture in fixtures {
        let expected: Value = serde_json::from_str(fixture).unwrap();
        let typed: ProtocolEnvelope = serde_json::from_value(expected.clone()).unwrap();
        let sender = match typed.message {
            ProtocolMessage::Request { .. } => ricochet_sandbox::EndpointRole::Host,
            ProtocolMessage::Response { .. } | ProtocolMessage::Event { .. } => {
                ricochet_sandbox::EndpointRole::Broker
            }
        };
        typed.message.validate_for(sender).unwrap();
        assert_eq!(serde_json::to_value(typed).unwrap(), expected);
    }
}

#[test]
fn unknown_fields_and_client_controlled_cancellation_reason_are_rejected() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/protocol/v1/handshake.json")).unwrap();
    let mut cases = Vec::new();

    let mut top = fixture.clone();
    top.as_object_mut()
        .unwrap()
        .insert("gremlin".to_owned(), json!(true));
    cases.push(top);

    let mut message_body = fixture.clone();
    message_body["message"]["body"]
        .as_object_mut()
        .unwrap()
        .insert("goblin".to_owned(), json!(true));
    cases.push(message_body);

    let mut request_body = fixture;
    request_body["message"]["body"]["request"]["body"]
        .as_object_mut()
        .unwrap()
        .insert("peer_context_id".to_owned(), json!("attacker-controlled"));
    cases.push(request_body);

    for case in cases {
        assert!(serde_json::from_value::<ProtocolEnvelope>(case).is_err());
    }

    let cancel = json!({
        "type": "cancel_session",
        "body": { "session_id": "session-01", "reason": "broker_shutdown" }
    });
    assert!(serde_json::from_value::<BrokerRequest>(cancel).is_err());
}

#[test]
fn envelope_and_wire_primitive_encodings_are_strict_v1() {
    let bad_version = json!({
        "protocol_version": 2,
        "sequence": 0,
        "message": { "type": "request", "body": { "request_id": 0, "request": { "type": "ping" } } }
    });
    assert!(serde_json::from_value::<ProtocolEnvelope>(bad_version).is_err());
    assert!(serde_json::to_value(ProtocolEnvelope {
        protocol_version: 2,
        sequence: 0,
        message: ProtocolMessage::request(RequestId::new(0), BrokerRequest::Ping),
    })
    .is_err());
    assert!(ConnectionNonce::parse_hex(&"AA".repeat(32)).is_err());
    assert_eq!(nonce(0xab).to_hex(), "ab".repeat(32));

    let bytes = WireBytes::new(vec![0xfb, 0xff, 0]).unwrap();
    assert_eq!(serde_json::to_value(&bytes).unwrap(), json!("+/8A"));
    round_trip(&bytes);
    assert!(serde_json::from_value::<WireBytes>(json!([1, 2, 3])).is_err());
    assert!(serde_json::from_value::<WireBytes>(json!("-_8")).is_err());
    assert!(WireBytes::new(vec![0; MAX_IO_CHUNK_BYTES + 1]).is_err());
}

#[test]
fn handshake_is_bound_to_the_native_channel_and_stored_offer() {
    let channel = AuthenticatedChannelContext::from_native_acceptor(
        PeerContextId::parse("peer-01").unwrap(),
        binding(0x11),
    );
    let request = HandshakeRequest {
        supported_protocol_versions: vec![PROTOCOL_V1],
        connection_nonce: nonce(0),
        channel_binding: binding(0x11),
    };
    request.validate_channel(&channel).unwrap();

    let mut correlation = ResponseCorrelation::default();
    assert!(correlation
        .record_request(
            RequestId::new(1),
            &BrokerRequest::Handshake(HandshakeRequest {
                supported_protocol_versions: vec![PROTOCOL_V1],
                connection_nonce: nonce(0),
                channel_binding: binding(0x11),
            })
        )
        .is_err());
    correlation
        .record_handshake(RequestId::new(1), &request, &channel)
        .unwrap();

    let response = HandshakeResponse {
        selected_protocol_version: PROTOCOL_V1,
        connection_nonce: nonce(0),
        broker_nonce: nonce(1),
        broker_identity: BackendIdentity::new("windows-lpac", "1").unwrap(),
        peer_context_id: PeerContextId::parse("peer-01").unwrap(),
        channel_binding: binding(0x11),
    };
    correlation
        .validate_and_complete(RequestId::new(1), &BrokerResponse::Handshake(response))
        .unwrap();

    for invalid_request in [
        HandshakeRequest {
            supported_protocol_versions: vec![PROTOCOL_V1, PROTOCOL_V1],
            connection_nonce: nonce(0),
            channel_binding: binding(0x11),
        },
        HandshakeRequest {
            supported_protocol_versions: vec![2],
            connection_nonce: nonce(0),
            channel_binding: binding(0x11),
        },
        HandshakeRequest {
            supported_protocol_versions: vec![PROTOCOL_V1],
            connection_nonce: nonce(0),
            channel_binding: binding(0x12),
        },
    ] {
        assert!(invalid_request.validate_channel(&channel).is_err());
    }

    for mutate in ["version", "nonce", "peer", "binding"] {
        let mut correlation = ResponseCorrelation::default();
        correlation
            .record_handshake(RequestId::new(2), &request, &channel)
            .unwrap();
        let mut response = HandshakeResponse {
            selected_protocol_version: PROTOCOL_V1,
            connection_nonce: nonce(0),
            broker_nonce: nonce(1),
            broker_identity: BackendIdentity::new("windows-lpac", "1").unwrap(),
            peer_context_id: PeerContextId::parse("peer-01").unwrap(),
            channel_binding: binding(0x11),
        };
        match mutate {
            "version" => response.selected_protocol_version = 2,
            "nonce" => response.connection_nonce = nonce(2),
            "peer" => response.peer_context_id = PeerContextId::parse("peer-02").unwrap(),
            "binding" => response.channel_binding = binding(0x12),
            _ => unreachable!(),
        }
        assert!(correlation
            .validate_and_complete(RequestId::new(2), &BrokerResponse::Handshake(response))
            .is_err());
    }
}

#[test]
fn request_validation_enforces_policy_session_surface_executable_and_caps() {
    let expected = session("session-01");
    let constrained = policy(ExecutionAccess::Read, true, true);
    BrokerRequest::ProcessStart(process_launch())
        .validate_against(&expected, &constrained)
        .unwrap();
    BrokerRequest::PtyStart(pty_launch())
        .validate_against(&expected, &constrained)
        .unwrap();

    let full = policy(ExecutionAccess::Full, true, true);
    let mut host = process_launch();
    host.executable = ExecutableRef::HostCommand("powershell.exe".to_owned());
    host.stdout_max_bytes = u64::MAX;
    host.stderr_max_bytes = 0;
    BrokerRequest::ProcessStart(host)
        .validate_against(&expected, &full)
        .unwrap();

    let mut forbidden_host = process_launch();
    forbidden_host.executable = ExecutableRef::HostCommand("cmd.exe".to_owned());
    assert!(BrokerRequest::ProcessStart(forbidden_host)
        .validate_against(&expected, &constrained)
        .is_err());

    let mut unknown_tool = process_launch();
    unknown_tool.executable = ExecutableRef::ManagedTool(tool("curl"));
    assert!(BrokerRequest::ProcessStart(unknown_tool)
        .validate_against(&expected, &constrained)
        .is_err());

    let mut wrong_session = process_launch();
    wrong_session.session_id = session("session-02");
    assert!(BrokerRequest::ProcessStart(wrong_session)
        .validate_against(&expected, &constrained)
        .is_err());

    let process_disabled = policy(ExecutionAccess::Read, false, true);
    assert!(BrokerRequest::ProcessStart(process_launch())
        .validate_against(&expected, &process_disabled)
        .is_err());
    assert!(BrokerRequest::ProcessList(SessionRequest {
        session_id: expected.clone(),
    })
    .validate_against(&expected, &process_disabled)
    .is_err());

    let pty_disabled = policy(ExecutionAccess::Read, true, false);
    assert!(BrokerRequest::PtyList(SessionRequest {
        session_id: expected.clone(),
    })
    .validate_against(&expected, &pty_disabled)
    .is_err());

    let mut zero_timeout = process_launch();
    zero_timeout.timeout_ms = 0;
    assert!(BrokerRequest::ProcessStart(zero_timeout)
        .validate_against(&expected, &constrained)
        .is_err());

    let mut zero_caps = process_launch();
    zero_caps.stdout_max_bytes = 0;
    zero_caps.stderr_max_bytes = 0;
    BrokerRequest::ProcessStart(zero_caps)
        .validate_against(&expected, &constrained)
        .unwrap();

    let mut over_time = process_launch();
    over_time.timeout_ms = 5_001;
    assert!(BrokerRequest::ProcessStart(over_time)
        .validate_against(&expected, &constrained)
        .is_err());

    let mut over_output = process_launch();
    over_output.stdout_max_bytes = 4_096;
    over_output.stderr_max_bytes = 1;
    assert!(BrokerRequest::ProcessStart(over_output)
        .validate_against(&expected, &constrained)
        .is_err());

    let mut overflow = process_launch();
    overflow.stdout_max_bytes = u64::MAX;
    overflow.stderr_max_bytes = 1;
    assert!(BrokerRequest::ProcessStart(overflow)
        .validate_against(&expected, &constrained)
        .is_err());

    let mut zero_pty = pty_launch();
    zero_pty.rows = 0;
    assert!(BrokerRequest::PtyStart(zero_pty)
        .validate_against(&expected, &constrained)
        .is_err());
}

#[test]
fn bounded_io_and_chunking_preserve_all_bytes_and_offsets() {
    let input = (0..(MAX_IO_CHUNK_BYTES * 2 + 37))
        .map(|index| (index % 251) as u8)
        .collect::<Vec<_>>();
    let chunks = chunk_wire_bytes(&input);
    assert_eq!(chunks.len(), 3);
    assert!(chunks
        .iter()
        .all(|chunk| chunk.as_slice().len() <= MAX_IO_CHUNK_BYTES));
    assert_eq!(
        chunks
            .iter()
            .flat_map(|chunk| chunk.as_slice().iter().copied())
            .collect::<Vec<_>>(),
        input
    );
    assert!(chunk_wire_bytes(&[]).is_empty());

    for invalid in [0, MAX_IO_CHUNK_BYTES as u32 + 1] {
        let read = BrokerRequest::ProcessRead(ProcessReadRequest {
            session_id: session("session-01"),
            process_id: ProcessId::new(0),
            stdout_offset: 0,
            stderr_offset: 0,
            max_bytes_per_stream: invalid,
        });
        assert!(read
            .validate_against(
                &session("session-01"),
                &policy(ExecutionAccess::Read, true, true)
            )
            .is_err());
    }

    let empty_close = BrokerRequest::ProcessWrite(ricochet_sandbox::ProcessWriteRequest {
        session_id: session("session-01"),
        process_id: ProcessId::new(0),
        bytes: WireBytes::new(Vec::new()).unwrap(),
        close_stdin: true,
    });
    empty_close
        .validate_against(
            &session("session-01"),
            &policy(ExecutionAccess::Read, true, true),
        )
        .unwrap();

    let empty_keep_open = BrokerRequest::ProcessWrite(ricochet_sandbox::ProcessWriteRequest {
        session_id: session("session-01"),
        process_id: ProcessId::new(0),
        bytes: WireBytes::new(Vec::new()).unwrap(),
        close_stdin: false,
    });
    assert!(empty_keep_open
        .validate_against(
            &session("session-01"),
            &policy(ExecutionAccess::Read, true, true),
        )
        .is_err());

    let empty_pty = BrokerRequest::PtyWrite(PtyWriteRequest {
        session_id: session("session-01"),
        pty_id: PtyId::new(0),
        bytes: WireBytes::new(Vec::new()).unwrap(),
    });
    assert!(empty_pty
        .validate_against(
            &session("session-01"),
            &policy(ExecutionAccess::Read, true, true),
        )
        .is_err());

    let mut read = ProcessReadSnapshot {
        snapshot: process_snapshot(),
        stdout: WireBytes::new(vec![1, 2, 3, 4]).unwrap(),
        stderr: WireBytes::new(Vec::new()).unwrap(),
        stdout_offset: 0,
        stderr_offset: 0,
    };
    read.validate().unwrap();
    read.stdout_offset = 1;
    assert!(read.validate().is_err());
}

#[test]
fn operation_errors_are_typed_fixed_and_pascal_case() {
    let process = OperationSubject::Process(ProcessId::new(7));
    let pty = OperationSubject::Pty(PtyId::new(8));
    let process_error = OperationError::new(OperationErrorCode::ProcessNotFound, process).unwrap();
    assert_eq!(process_error.kind(), "ProcessNotFound");
    assert_eq!(process_error.message(), "sandbox process was not found");
    assert_eq!(
        serde_json::to_value(&process_error).unwrap()["code"],
        "ProcessNotFound"
    );
    assert!(OperationError::new(OperationErrorCode::ProcessError, pty).is_err());
    assert!(OperationError::new(
        OperationErrorCode::RegistryFull,
        OperationSubject::Process(ProcessId::new(0))
    )
    .is_err());
    OperationError::new(
        OperationErrorCode::RegistryFull,
        OperationSubject::Registry(ricochet_sandbox::ExecutionSurface::Process),
    )
    .unwrap();

    let mut forged = serde_json::to_value(process_error).unwrap();
    forged["message"] = json!("native process detail leaked");
    assert!(serde_json::from_value::<OperationError>(forged).is_err());
}

fn assert_correlates(request: BrokerRequest, response: BrokerResponse) {
    let request_id = RequestId::new(77);
    let mut correlation = ResponseCorrelation::default();
    correlation.record_request(request_id, &request).unwrap();
    correlation
        .validate_and_complete(request_id, &response)
        .unwrap();
    assert!(correlation
        .validate_and_complete(request_id, &response)
        .is_err());
}

#[test]
fn response_correlation_enforces_every_success_and_operation_error_column() {
    let session_request = || SessionRequest {
        session_id: session("session-01"),
    };
    let process_request = || ProcessRequest {
        session_id: session("session-01"),
        process_id: ProcessId::new(0),
    };
    let pty_request = || PtyRequest {
        session_id: session("session-01"),
        pty_id: PtyId::new(0),
    };
    let constrained = policy(ExecutionAccess::Read, true, true);
    assert_correlates(
        BrokerRequest::CreateSession(CreateSessionRequest {
            session_id: session("session-01"),
            policy: policy_request(ExecutionAccess::Read, true, true),
        }),
        BrokerResponse::SessionCreated(
            ConfirmedExecutionCapabilities::new(
                session("session-01"),
                ScratchId::parse("scratch-01").unwrap(),
                BackendIdentity::new("windows-lpac", "1").unwrap(),
                PROTOCOL_V1,
                EnforcementState::Enforced,
                &constrained,
            )
            .unwrap(),
        ),
    );
    assert_correlates(
        BrokerRequest::CloseSession(session_request()),
        BrokerResponse::Acknowledged,
    );
    assert_correlates(
        BrokerRequest::CancelSession(CancelSessionRequest {
            session_id: session("session-01"),
        }),
        BrokerResponse::Acknowledged,
    );
    assert_correlates(
        BrokerRequest::ProcessStart(process_launch()),
        BrokerResponse::Process(process_snapshot()),
    );
    assert_correlates(
        BrokerRequest::ProcessList(session_request()),
        BrokerResponse::Processes(vec![]),
    );
    assert_correlates(
        BrokerRequest::ProcessDetail(process_request()),
        BrokerResponse::Process(process_snapshot()),
    );
    assert_correlates(
        BrokerRequest::ProcessRead(ProcessReadRequest {
            session_id: session("session-01"),
            process_id: ProcessId::new(0),
            stdout_offset: 0,
            stderr_offset: 0,
            max_bytes_per_stream: 1,
        }),
        BrokerResponse::ProcessRead(ProcessReadSnapshot {
            snapshot: process_snapshot(),
            stdout: WireBytes::new(vec![]).unwrap(),
            stderr: WireBytes::new(vec![]).unwrap(),
            stdout_offset: 4,
            stderr_offset: 0,
        }),
    );
    assert_correlates(
        BrokerRequest::ProcessWrite(ricochet_sandbox::ProcessWriteRequest {
            session_id: session("session-01"),
            process_id: ProcessId::new(0),
            bytes: WireBytes::new(vec![1]).unwrap(),
            close_stdin: false,
        }),
        BrokerResponse::Process(process_snapshot()),
    );
    assert_correlates(
        BrokerRequest::ProcessCancel(process_request()),
        BrokerResponse::Process(process_snapshot()),
    );
    assert_correlates(
        BrokerRequest::ProcessRelease(process_request()),
        BrokerResponse::Acknowledged,
    );
    assert_correlates(
        BrokerRequest::PtyStart(pty_launch()),
        BrokerResponse::Pty(pty_snapshot()),
    );
    assert_correlates(
        BrokerRequest::PtyList(session_request()),
        BrokerResponse::Ptys(vec![]),
    );
    assert_correlates(
        BrokerRequest::PtyDetail(pty_request()),
        BrokerResponse::Pty(pty_snapshot()),
    );
    assert_correlates(
        BrokerRequest::PtyRead(PtyReadRequest {
            session_id: session("session-01"),
            pty_id: PtyId::new(0),
            offset: 0,
            max_bytes: 1,
        }),
        BrokerResponse::PtyRead(PtyReadSnapshot {
            snapshot: pty_snapshot(),
            output: WireBytes::new(vec![]).unwrap(),
            offset: 4,
        }),
    );
    assert_correlates(
        BrokerRequest::PtyWrite(PtyWriteRequest {
            session_id: session("session-01"),
            pty_id: PtyId::new(0),
            bytes: WireBytes::new(vec![1]).unwrap(),
        }),
        BrokerResponse::Pty(pty_snapshot()),
    );
    assert_correlates(
        BrokerRequest::PtyResize(PtyResizeRequest {
            session_id: session("session-01"),
            pty_id: PtyId::new(0),
            rows: 24,
            cols: 80,
        }),
        BrokerResponse::Pty(pty_snapshot()),
    );
    assert_correlates(
        BrokerRequest::PtyStop(pty_request()),
        BrokerResponse::Pty(pty_snapshot()),
    );
    assert_correlates(
        BrokerRequest::PtyRelease(pty_request()),
        BrokerResponse::Acknowledged,
    );
    assert_correlates(
        BrokerRequest::CatalogPublicSnapshot,
        BrokerResponse::PublicCatalog(PublicCatalogSnapshot {
            schema_version: CATALOG_SCHEMA_V1,
            generation: generation(7),
            platform: platform(),
            records: vec![],
            revoked_tools: vec![],
        }),
    );
    assert_correlates(BrokerRequest::Ping, BrokerResponse::Pong);

    assert_correlates(
        BrokerRequest::ProcessDetail(process_request()),
        BrokerResponse::OperationError(
            OperationError::new(
                OperationErrorCode::ProcessNotFound,
                OperationSubject::Process(ProcessId::new(0)),
            )
            .unwrap(),
        ),
    );
    assert_correlates(
        BrokerRequest::PtyRelease(pty_request()),
        BrokerResponse::OperationError(
            OperationError::new(
                OperationErrorCode::PtyRunning,
                OperationSubject::Pty(PtyId::new(0)),
            )
            .unwrap(),
        ),
    );

    fn operation_request(kind: BrokerRequestKind) -> BrokerRequest {
        match kind {
            BrokerRequestKind::ProcessStart => BrokerRequest::ProcessStart(process_launch()),
            BrokerRequestKind::ProcessDetail => BrokerRequest::ProcessDetail(ProcessRequest {
                session_id: session("session-01"),
                process_id: ProcessId::new(0),
            }),
            BrokerRequestKind::ProcessRead => BrokerRequest::ProcessRead(ProcessReadRequest {
                session_id: session("session-01"),
                process_id: ProcessId::new(0),
                stdout_offset: 0,
                stderr_offset: 0,
                max_bytes_per_stream: 1,
            }),
            BrokerRequestKind::ProcessWrite => {
                BrokerRequest::ProcessWrite(ricochet_sandbox::ProcessWriteRequest {
                    session_id: session("session-01"),
                    process_id: ProcessId::new(0),
                    bytes: WireBytes::new(vec![1]).unwrap(),
                    close_stdin: false,
                })
            }
            BrokerRequestKind::ProcessCancel => BrokerRequest::ProcessCancel(ProcessRequest {
                session_id: session("session-01"),
                process_id: ProcessId::new(0),
            }),
            BrokerRequestKind::ProcessRelease => BrokerRequest::ProcessRelease(ProcessRequest {
                session_id: session("session-01"),
                process_id: ProcessId::new(0),
            }),
            BrokerRequestKind::PtyStart => BrokerRequest::PtyStart(pty_launch()),
            BrokerRequestKind::PtyDetail => BrokerRequest::PtyDetail(PtyRequest {
                session_id: session("session-01"),
                pty_id: PtyId::new(0),
            }),
            BrokerRequestKind::PtyRead => BrokerRequest::PtyRead(PtyReadRequest {
                session_id: session("session-01"),
                pty_id: PtyId::new(0),
                offset: 0,
                max_bytes: 1,
            }),
            BrokerRequestKind::PtyWrite => BrokerRequest::PtyWrite(PtyWriteRequest {
                session_id: session("session-01"),
                pty_id: PtyId::new(0),
                bytes: WireBytes::new(vec![1]).unwrap(),
            }),
            BrokerRequestKind::PtyResize => BrokerRequest::PtyResize(PtyResizeRequest {
                session_id: session("session-01"),
                pty_id: PtyId::new(0),
                rows: 24,
                cols: 80,
            }),
            BrokerRequestKind::PtyStop => BrokerRequest::PtyStop(PtyRequest {
                session_id: session("session-01"),
                pty_id: PtyId::new(0),
            }),
            BrokerRequestKind::PtyRelease => BrokerRequest::PtyRelease(PtyRequest {
                session_id: session("session-01"),
                pty_id: PtyId::new(0),
            }),
            _ => panic!("no operation errors for {kind:?}"),
        }
    }

    let allowed_operation_errors = [
        (
            BrokerRequestKind::ProcessStart,
            OperationErrorCode::ProcessError,
        ),
        (
            BrokerRequestKind::ProcessStart,
            OperationErrorCode::RegistryFull,
        ),
        (
            BrokerRequestKind::ProcessDetail,
            OperationErrorCode::ProcessNotFound,
        ),
        (
            BrokerRequestKind::ProcessRead,
            OperationErrorCode::ProcessNotFound,
        ),
        (
            BrokerRequestKind::ProcessWrite,
            OperationErrorCode::ProcessNotFound,
        ),
        (
            BrokerRequestKind::ProcessWrite,
            OperationErrorCode::ProcessNotRunning,
        ),
        (
            BrokerRequestKind::ProcessWrite,
            OperationErrorCode::ProcessStdinClosed,
        ),
        (
            BrokerRequestKind::ProcessWrite,
            OperationErrorCode::ProcessError,
        ),
        (
            BrokerRequestKind::ProcessCancel,
            OperationErrorCode::ProcessNotFound,
        ),
        (
            BrokerRequestKind::ProcessCancel,
            OperationErrorCode::ProcessError,
        ),
        (
            BrokerRequestKind::ProcessRelease,
            OperationErrorCode::ProcessNotFound,
        ),
        (
            BrokerRequestKind::ProcessRelease,
            OperationErrorCode::ProcessRunning,
        ),
        (BrokerRequestKind::PtyStart, OperationErrorCode::PtyError),
        (
            BrokerRequestKind::PtyStart,
            OperationErrorCode::RegistryFull,
        ),
        (
            BrokerRequestKind::PtyDetail,
            OperationErrorCode::PtyNotFound,
        ),
        (BrokerRequestKind::PtyRead, OperationErrorCode::PtyNotFound),
        (BrokerRequestKind::PtyWrite, OperationErrorCode::PtyNotFound),
        (BrokerRequestKind::PtyWrite, OperationErrorCode::PtyClosed),
        (BrokerRequestKind::PtyWrite, OperationErrorCode::PtyError),
        (
            BrokerRequestKind::PtyResize,
            OperationErrorCode::PtyNotFound,
        ),
        (BrokerRequestKind::PtyResize, OperationErrorCode::PtyClosed),
        (BrokerRequestKind::PtyResize, OperationErrorCode::PtyError),
        (BrokerRequestKind::PtyStop, OperationErrorCode::PtyNotFound),
        (BrokerRequestKind::PtyStop, OperationErrorCode::PtyError),
        (
            BrokerRequestKind::PtyRelease,
            OperationErrorCode::PtyNotFound,
        ),
        (
            BrokerRequestKind::PtyRelease,
            OperationErrorCode::PtyRunning,
        ),
    ];
    for (kind, code) in allowed_operation_errors {
        let subject = match code {
            OperationErrorCode::RegistryFull if kind == BrokerRequestKind::ProcessStart => {
                OperationSubject::Registry(ricochet_sandbox::ExecutionSurface::Process)
            }
            OperationErrorCode::RegistryFull => {
                OperationSubject::Registry(ricochet_sandbox::ExecutionSurface::Pty)
            }
            OperationErrorCode::ProcessError
            | OperationErrorCode::ProcessNotFound
            | OperationErrorCode::ProcessRunning
            | OperationErrorCode::ProcessNotRunning
            | OperationErrorCode::ProcessStdinClosed => {
                OperationSubject::Process(ProcessId::new(0))
            }
            OperationErrorCode::PtyError
            | OperationErrorCode::PtyNotFound
            | OperationErrorCode::PtyRunning
            | OperationErrorCode::PtyClosed => OperationSubject::Pty(PtyId::new(0)),
        };
        assert_correlates(
            operation_request(kind),
            BrokerResponse::OperationError(OperationError::new(code, subject).unwrap()),
        );
    }

    let mut correlation = ResponseCorrelation::default();
    correlation
        .record_request(
            RequestId::new(91),
            &BrokerRequest::ProcessStart(process_launch()),
        )
        .unwrap();
    assert!(correlation
        .validate_and_complete(
            RequestId::new(91),
            &BrokerResponse::OperationError(
                OperationError::new(
                    OperationErrorCode::RegistryFull,
                    OperationSubject::Registry(ricochet_sandbox::ExecutionSurface::Pty),
                )
                .unwrap(),
            ),
        )
        .is_err());
    correlation
        .validate_and_complete(
            RequestId::new(91),
            &BrokerResponse::OperationError(
                OperationError::new(
                    OperationErrorCode::RegistryFull,
                    OperationSubject::Registry(ricochet_sandbox::ExecutionSurface::Process),
                )
                .unwrap(),
            ),
        )
        .unwrap();

    let mut correlation = ResponseCorrelation::default();
    correlation
        .record_request(RequestId::new(1), &BrokerRequest::Ping)
        .unwrap();
    assert!(correlation
        .validate_and_complete(RequestId::new(1), &BrokerResponse::Pong)
        .is_ok());

    let mut correlation = ResponseCorrelation::default();
    correlation
        .record_request(
            RequestId::new(2),
            &BrokerRequest::ProcessDetail(process_request()),
        )
        .unwrap();
    assert!(correlation
        .validate_and_complete(RequestId::new(2), &BrokerResponse::Pong)
        .is_err());
    correlation
        .validate_and_complete(
            RequestId::new(2),
            &BrokerResponse::Process(process_snapshot()),
        )
        .unwrap();

    let mut correlation = ResponseCorrelation::default();
    correlation
        .record_request(RequestId::new(3), &BrokerRequest::Ping)
        .unwrap();
    correlation
        .validate_and_complete(
            RequestId::new(3),
            &BrokerResponse::Error(SandboxError::protocol(DiagnosticMetadata::empty())),
        )
        .unwrap();
}

#[test]
fn recursive_snapshot_role_and_event_session_validation_rejects_forgery() {
    let mut bad_process = process_snapshot();
    bad_process.argument_count = 0;
    assert!(
        ProtocolMessage::response(RequestId::new(1), BrokerResponse::Process(bad_process))
            .validate_for(ricochet_sandbox::EndpointRole::Broker)
            .is_err()
    );

    let mut bad_pty = pty_snapshot();
    bad_pty.status = PtyStatus::Stopped;
    assert!(bad_pty.validate().is_err());

    let mut contradictory = process_snapshot();
    contradictory.timed_out = true;
    assert!(contradictory.validate().is_err());

    let constrained = policy(ExecutionAccess::Read, true, true);
    let capabilities = ConfirmedExecutionCapabilities::new(
        session("session-01"),
        ScratchId::parse("scratch-01").unwrap(),
        BackendIdentity::new("windows-lpac", "1").unwrap(),
        PROTOCOL_V1,
        EnforcementState::Enforced,
        &constrained,
    )
    .unwrap();
    let mut forged_capabilities = serde_json::to_value(capabilities).unwrap();
    forged_capabilities["enforcement"] = json!("unenforced_full_access");
    assert!(serde_json::from_value::<ConfirmedExecutionCapabilities>(forged_capabilities).is_err());

    let mut forged_error: Value =
        serde_json::from_str(include_str!("fixtures/protocol/v1/error_response.json")).unwrap();
    forged_error["message"]["body"]["response"]["body"]["message"] = json!("forged native detail");
    assert!(serde_json::from_value::<ProtocolEnvelope>(forged_error).is_err());

    let request = ProtocolMessage::request(RequestId::new(1), BrokerRequest::Ping);
    assert!(request
        .validate_for(ricochet_sandbox::EndpointRole::Broker)
        .is_err());
    let response = ProtocolMessage::response(RequestId::new(1), BrokerResponse::Pong);
    assert!(response
        .validate_for(ricochet_sandbox::EndpointRole::Host)
        .is_err());

    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/protocol/v1/audit_event.json")).unwrap();
    let correct: ProtocolEnvelope = serde_json::from_value(fixture.clone()).unwrap();
    correct
        .message
        .validate_for(ricochet_sandbox::EndpointRole::Broker)
        .unwrap();

    let mut wrong = fixture;
    wrong["message"]["body"]["session_id"] = json!("session-02");
    let wrong: ProtocolEnvelope = serde_json::from_value(wrong).unwrap();
    assert!(wrong
        .message
        .validate_for(ricochet_sandbox::EndpointRole::Broker)
        .is_err());

    let mut wrong_version: Value =
        serde_json::from_str(include_str!("fixtures/protocol/v1/audit_event.json")).unwrap();
    wrong_version["message"]["body"]["event"]["body"]["context"]["broker_protocol"] = json!(2);
    let wrong_version: ProtocolEnvelope = serde_json::from_value(wrong_version).unwrap();
    assert!(wrong_version
        .message
        .validate_for(ricochet_sandbox::EndpointRole::Broker)
        .is_err());

    let termination = BrokerEvent::Terminated(TerminationNotice {
        reason: TerminationReason::CancelledByHost,
        process_tree_ids: vec![ProcessTreeId::new(0)],
        error: Some(SandboxError::terminated(
            TerminationReason::CancelledByHost,
            session("session-01"),
        )),
    });
    ProtocolMessage::event(session("session-01"), termination)
        .validate_for(ricochet_sandbox::EndpointRole::Broker)
        .unwrap();

    let forged_termination = BrokerEvent::Terminated(TerminationNotice {
        reason: TerminationReason::CancelledByHost,
        process_tree_ids: vec![ProcessTreeId::new(0)],
        error: Some(SandboxError::policy(
            FailedGuarantee::PolicyValidity,
            DiagnosticMetadata::empty(),
        )),
    });
    assert!(
        ProtocolMessage::event(session("session-01"), forged_termination)
            .validate_for(ricochet_sandbox::EndpointRole::Broker)
            .is_err()
    );
}

#[test]
fn termination_event_rejects_nested_error_for_a_different_session() {
    let decoded = decoded_termination_event(
        TerminationReason::CancelledByHost,
        Some(SandboxError::terminated(
            TerminationReason::CancelledByHost,
            session("session-02"),
        )),
    );

    assert!(decoded
        .message
        .validate_for(ricochet_sandbox::EndpointRole::Broker)
        .is_err());
}

#[test]
fn termination_event_rejects_nested_error_for_a_different_resource_limit() {
    let decoded = decoded_termination_event(
        TerminationReason::ResourceLimit(ResourceLimitKind::MemoryBytes),
        Some(SandboxError::terminated(
            TerminationReason::ResourceLimit(ResourceLimitKind::CpuTime),
            session("session-01"),
        )),
    );

    assert!(decoded
        .message
        .validate_for(ricochet_sandbox::EndpointRole::Broker)
        .is_err());
}

#[test]
fn non_resource_termination_event_rejects_nested_resource_metadata() {
    let decoded = decoded_termination_event(
        TerminationReason::CancelledByHost,
        Some(SandboxError::terminated(
            TerminationReason::ResourceLimit(ResourceLimitKind::CpuTime),
            session("session-01"),
        )),
    );

    assert!(decoded
        .message
        .validate_for(ricochet_sandbox::EndpointRole::Broker)
        .is_err());
}

#[test]
fn coherent_termination_events_round_trip_and_validate_recursively() {
    let cases = [
        decoded_termination_event(
            TerminationReason::ResourceLimit(ResourceLimitKind::MemoryBytes),
            Some(SandboxError::terminated(
                TerminationReason::ResourceLimit(ResourceLimitKind::MemoryBytes),
                session("session-01"),
            )),
        ),
        decoded_termination_event(
            TerminationReason::CancelledByHost,
            Some(SandboxError::terminated(
                TerminationReason::CancelledByHost,
                session("session-01"),
            )),
        ),
        decoded_termination_event(TerminationReason::BrokerShutdown, None),
    ];

    for case in cases {
        round_trip(&case);
        case.message
            .validate_for(ricochet_sandbox::EndpointRole::Broker)
            .unwrap();
    }
}

#[test]
fn raw_protocol_arguments_round_trip_while_debug_and_audit_are_redacted() {
    let request = BrokerRequest::ProcessStart(process_launch());
    let encoded = serde_json::to_value(&request).unwrap();
    assert_eq!(encoded["body"]["arguments"][0], "argument-secret");
    assert_eq!(
        encoded["body"]["environment"]["entries"][0]["value"],
        "environment-secret"
    );

    let snapshot = process_snapshot();
    assert_eq!(
        serde_json::to_value(&snapshot).unwrap()["arguments"][0],
        "snapshot-argument-secret"
    );

    let audit: Value =
        serde_json::from_str(include_str!("fixtures/protocol/v1/audit_event.json")).unwrap();
    assert_eq!(
        audit["message"]["body"]["event"]["body"]["event"]["body"]["argument_count"],
        2
    );
    assert!(audit.to_string().find("argument-secret").is_none());

    let debug_values: Vec<Box<dyn Debug>> = vec![
        Box::new(request),
        Box::new(BrokerRequest::PtyStart(pty_launch())),
        Box::new(snapshot),
        Box::new(PtyReadSnapshot {
            snapshot: pty_snapshot(),
            output: WireBytes::new(b"output-body-secret".to_vec()).unwrap(),
            offset: 0,
        }),
        Box::new(HandshakeRequest {
            supported_protocol_versions: vec![1],
            connection_nonce: nonce(0xaa),
            channel_binding: binding(0xbb),
        }),
    ];
    let joined = debug_values
        .iter()
        .map(|value| format!("{value:?}"))
        .collect::<Vec<_>>()
        .join("\n");
    for secret in [
        "argument-secret",
        "pty-argument-secret",
        "snapshot-argument-secret",
        "environment-secret",
        "output-body-secret",
        &"aa".repeat(32),
        &"bb".repeat(32),
    ] {
        assert!(!joined.contains(secret), "debug leaked {secret}");
    }

    let constrained = policy(ExecutionAccess::Read, true, true);
    let capabilities = ConfirmedExecutionCapabilities::new(
        session("session-01"),
        ScratchId::parse("scratch-01").unwrap(),
        BackendIdentity::new("windows-lpac", "1").unwrap(),
        PROTOCOL_V1,
        EnforcementState::Enforced,
        &constrained,
    )
    .unwrap();
    let capabilities_debug = format!("{capabilities:?}");
    assert!(!capabilities_debug.contains("C:/workspace"));
    assert!(!capabilities_debug.contains("github.com"));
}

#[test]
fn process_and_pty_requests_keep_environment_and_session_parity() {
    let process = process_launch();
    let pty = pty_launch();
    assert_eq!(process.session_id, pty.session_id);
    assert_eq!(
        serde_json::to_value(&process.environment).unwrap(),
        serde_json::to_value(&pty.environment).unwrap()
    );
    assert_eq!(process.arguments[0], "argument-secret");
    assert_eq!(pty.arguments[0], "pty-argument-secret");
}

#[test]
fn optional_fields_are_explicit_nulls() {
    let snapshot = process_snapshot();
    let value = serde_json::to_value(snapshot).unwrap();
    assert!(value.get("exit_code").is_some_and(Value::is_null));
    assert!(value.get("error").is_some_and(Value::is_null));

    let response = envelope(
        2,
        ProtocolMessage::response(
            RequestId::new(3),
            BrokerResponse::Error(SandboxError::policy(
                FailedGuarantee::PolicyValidity,
                DiagnosticMetadata::empty(),
            )),
        ),
    );
    assert!(
        serde_json::to_value(response).unwrap()["message"]["body"]["response"]["body"]
            .get("backend")
            .is_some_and(Value::is_null)
    );
}

#[test]
fn omitted_v1_optional_fields_are_rejected_at_every_wire_nesting() {
    let process_start = serde_json::to_value(envelope(
        4,
        ProtocolMessage::request(
            RequestId::new(4),
            BrokerRequest::ProcessStart(process_launch()),
        ),
    ))
    .unwrap();
    assert_omission_rejected::<ProtocolEnvelope>(&process_start, "process_start.cwd", |value| {
        value["message"]["body"]["request"]["body"]
            .as_object_mut()
            .unwrap()
            .remove("cwd");
    });

    let pty_start = serde_json::to_value(envelope(
        5,
        ProtocolMessage::request(RequestId::new(5), BrokerRequest::PtyStart(pty_launch())),
    ))
    .unwrap();
    assert_omission_rejected::<ProtocolEnvelope>(&pty_start, "pty_start.cwd", |value| {
        value["message"]["body"]["request"]["body"]
            .as_object_mut()
            .unwrap()
            .remove("cwd");
    });

    let process = serde_json::to_value(envelope(
        6,
        ProtocolMessage::response(
            RequestId::new(6),
            BrokerResponse::Process(process_snapshot()),
        ),
    ))
    .unwrap();
    for field in ["cwd", "exit_code", "error"] {
        assert_omission_rejected::<ProtocolEnvelope>(&process, field, |value| {
            value["message"]["body"]["response"]["body"]
                .as_object_mut()
                .unwrap()
                .remove(field);
        });
    }

    let pty = serde_json::to_value(envelope(
        7,
        ProtocolMessage::response(RequestId::new(7), BrokerResponse::Pty(pty_snapshot())),
    ))
    .unwrap();
    for field in ["cwd", "exit_code", "error", "native_process_id"] {
        assert_omission_rejected::<ProtocolEnvelope>(&pty, field, |value| {
            value["message"]["body"]["response"]["body"]
                .as_object_mut()
                .unwrap()
                .remove(field);
        });
    }

    let constrained = policy(ExecutionAccess::Read, true, true);
    let capabilities = ConfirmedExecutionCapabilities::new(
        session("session-01"),
        ScratchId::parse("scratch-01").unwrap(),
        BackendIdentity::new("windows-lpac", "1").unwrap(),
        PROTOCOL_V1,
        EnforcementState::Enforced,
        &constrained,
    )
    .unwrap();
    let capabilities = serde_json::to_value(envelope(
        8,
        ProtocolMessage::response(
            RequestId::new(8),
            BrokerResponse::SessionCreated(capabilities),
        ),
    ))
    .unwrap();
    for field in ["workspace", "resource_limits"] {
        assert_omission_rejected::<ProtocolEnvelope>(&capabilities, field, |value| {
            value["message"]["body"]["response"]["body"]
                .as_object_mut()
                .unwrap()
                .remove(field);
        });
    }

    let termination = serde_json::to_value(envelope(
        9,
        ProtocolMessage::event(
            session("session-01"),
            BrokerEvent::Terminated(TerminationNotice {
                reason: TerminationReason::CancelledByHost,
                process_tree_ids: vec![ProcessTreeId::new(0)],
                error: None,
            }),
        ),
    ))
    .unwrap();
    assert_omission_rejected::<ProtocolEnvelope>(&termination, "termination.error", |value| {
        value["message"]["body"]["event"]["body"]
            .as_object_mut()
            .unwrap()
            .remove("error");
    });

    let create_session = serde_json::to_value(envelope(
        10,
        ProtocolMessage::request(
            RequestId::new(10),
            BrokerRequest::CreateSession(CreateSessionRequest {
                session_id: session("session-01"),
                policy: policy_request(ExecutionAccess::Read, true, true),
            }),
        ),
    ))
    .unwrap();
    for field in ["workspace", "resource_limits"] {
        assert_omission_rejected::<ProtocolEnvelope>(&create_session, field, |value| {
            value["message"]["body"]["request"]["body"]["policy"]
                .as_object_mut()
                .unwrap()
                .remove(field);
        });
    }

    let public_catalog = serde_json::to_value(envelope(
        11,
        ProtocolMessage::response(
            RequestId::new(11),
            BrokerResponse::PublicCatalog(catalog().public_snapshot()),
        ),
    ))
    .unwrap();
    assert_omission_rejected::<ProtocolEnvelope>(
        &public_catalog,
        "public_tool.transport_adapter",
        |value| {
            value["message"]["body"]["response"]["body"]["records"][0]
                .as_object_mut()
                .unwrap()
                .remove("transport_adapter");
        },
    );

    let audit: Value =
        serde_json::from_str(include_str!("fixtures/protocol/v1/audit_event.json")).unwrap();
    for field in ["workspace", "resource_limits"] {
        assert_omission_rejected::<ProtocolEnvelope>(&audit, field, |value| {
            value["message"]["body"]["event"]["body"]["context"]
                .as_object_mut()
                .unwrap()
                .remove(field);
        });
    }
    assert_omission_rejected::<ProtocolEnvelope>(&audit, "audit.launch.tool_id", |value| {
        value["message"]["body"]["event"]["body"]["event"]["body"]
            .as_object_mut()
            .unwrap()
            .remove("tool_id");
    });

    let mut exited = audit.clone();
    exited["message"]["body"]["event"]["body"]["event"] = json!({
        "type": "exited",
        "body": {
            "execution": {
                "process_tree_id": 0,
                "instance": { "type": "process", "body": 0 }
            },
            "exit_code": null,
            "success": false
        }
    });
    assert_omission_rejected::<ProtocolEnvelope>(&exited, "audit.exited.exit_code", |value| {
        value["message"]["body"]["event"]["body"]["event"]["body"]
            .as_object_mut()
            .unwrap()
            .remove("exit_code");
    });

    let mut denied = audit.clone();
    denied["message"]["body"]["event"]["body"]["event"] = json!({
        "type": "denied",
        "body": {
            "code": "SandboxPolicyError",
            "guarantee": "policy_validity",
            "remediation": null
        }
    });
    assert_omission_rejected::<ProtocolEnvelope>(&denied, "audit.denied.remediation", |value| {
        value["message"]["body"]["event"]["body"]["event"]["body"]
            .as_object_mut()
            .unwrap()
            .remove("remediation");
    });

    let error: Value =
        serde_json::from_str(include_str!("fixtures/protocol/v1/error_response.json")).unwrap();
    for field in ["backend", "failed_guarantee", "remediation"] {
        assert_omission_rejected::<ProtocolEnvelope>(&error, field, |value| {
            value["message"]["body"]["response"]["body"]
                .as_object_mut()
                .unwrap()
                .remove(field);
        });
    }
    for field in [
        "tool_id",
        "destination",
        "resource_limit",
        "protocol_version",
        "session_id",
        "backend_feature",
    ] {
        assert_omission_rejected::<ProtocolEnvelope>(&error, field, |value| {
            value["message"]["body"]["response"]["body"]["metadata"]
                .as_object_mut()
                .unwrap()
                .remove(field);
        });
    }
}

#[test]
fn forged_public_catalog_rejects_revoked_helpers() {
    let helper = tool("helper");
    let snapshot = PublicCatalogSnapshot {
        schema_version: CATALOG_SCHEMA_V1,
        generation: generation(7),
        platform: platform(),
        records: vec![
            PublicToolRecord {
                tool_id: helper.clone(),
                executable_sha256: Sha256Digest::hash(b"helper"),
                helper_ids: Vec::new(),
                transport_adapter: None,
            },
            PublicToolRecord {
                tool_id: tool("main"),
                executable_sha256: Sha256Digest::hash(b"main"),
                helper_ids: vec![helper.clone()],
                transport_adapter: None,
            },
        ],
        revoked_tools: vec![helper],
    };
    assert!(
        ProtocolMessage::response(RequestId::new(12), BrokerResponse::PublicCatalog(snapshot),)
            .validate_for(ricochet_sandbox::EndpointRole::Broker)
            .is_err()
    );
}

#[test]
fn audit_and_capability_callers_reject_the_same_security_projection_forgery() {
    let constrained = policy(ExecutionAccess::Read, true, true);
    let capabilities = ConfirmedExecutionCapabilities::new(
        session("session-01"),
        ScratchId::parse("scratch-01").unwrap(),
        BackendIdentity::new("windows-lpac", "1").unwrap(),
        PROTOCOL_V1,
        EnforcementState::Enforced,
        &constrained,
    )
    .unwrap();
    let capabilities = serde_json::to_value(capabilities).unwrap();
    let audit: Value =
        serde_json::from_str(include_str!("fixtures/protocol/v1/audit_event.json")).unwrap();
    let context = &audit["message"]["body"]["event"]["body"]["context"];

    let mut bad_capabilities_matrix = capabilities.clone();
    bad_capabilities_matrix["enforcement"] = json!("unenforced_full_access");
    let mut bad_audit_matrix = context.clone();
    bad_audit_matrix["enforcement"] = json!("unenforced_full_access");
    assert!(
        serde_json::from_value::<ConfirmedExecutionCapabilities>(bad_capabilities_matrix).is_err()
    );
    assert!(serde_json::from_value::<AuditContext>(bad_audit_matrix).is_err());

    let mut bad_capabilities_tools = capabilities;
    bad_capabilities_tools["tools"][0]["helper_ids"] = json!(["missing-helper"]);
    let mut bad_audit_tools = context.clone();
    bad_audit_tools["tools"][0]["helper_ids"] = json!(["missing-helper"]);
    assert!(
        serde_json::from_value::<ConfirmedExecutionCapabilities>(bad_capabilities_tools).is_err()
    );
    assert!(serde_json::from_value::<AuditContext>(bad_audit_tools).is_err());
}
