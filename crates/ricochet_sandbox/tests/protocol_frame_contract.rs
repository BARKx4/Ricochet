use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use hmac::{Hmac, Mac};
use ricochet_sandbox::{
    Architecture, ArgumentAuditMode, AuditPolicy, AuthenticatedCodec, BrokerRequest,
    BrokerResponse, CatalogGeneration, CatalogPathNormalizer, CatalogSnapshot, DiagnosticMetadata,
    EndpointRole, EnvironmentPolicy, ExecutableRef, ExecutionAccess, ExecutionPolicyRequest,
    LaunchEnvironment, OperatingSystem, PlatformId, ProcessId, ProcessLaunchRequest,
    ProcessRequest, ProtocolKey, ProtocolMessage, RequestId, SandboxError, ScratchDisposition,
    SessionId, ValidatedExecutionPolicy, WorkspaceIdentity, WorkspaceIdentityResolver,
    WorkspaceRequest, CATALOG_SCHEMA_V1, MAX_FRAME_BYTES, MAX_IO_CHUNK_BYTES, POLICY_SCHEMA_V1,
    PROTOCOL_V1,
};
use serde_json::{json, Value};
use sha2::Sha256;

const HOST_TO_BROKER_KEY: [u8; 32] = [7; 32];
const BROKER_TO_HOST_KEY: [u8; 32] = [9; 32];

fn codecs() -> (AuthenticatedCodec, AuthenticatedCodec) {
    (
        AuthenticatedCodec::new(
            EndpointRole::Host,
            ProtocolKey::from_bytes(HOST_TO_BROKER_KEY),
            ProtocolKey::from_bytes(BROKER_TO_HOST_KEY),
        ),
        AuthenticatedCodec::new(
            EndpointRole::Broker,
            ProtocolKey::from_bytes(BROKER_TO_HOST_KEY),
            ProtocolKey::from_bytes(HOST_TO_BROKER_KEY),
        ),
    )
}

fn ping(request_id: u64) -> ProtocolMessage {
    ProtocolMessage::request(RequestId::new(request_id), BrokerRequest::Ping)
}

fn assert_protocol_error<T>(result: Result<T, SandboxError>) {
    let error = match result {
        Ok(_) => panic!("expected a broker protocol error"),
        Err(error) => error,
    };
    assert_eq!(error.kind(), "BrokerProtocolError");
}

fn signed_payload_frame(key: &[u8; 32], payload: &[u8]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(4 + payload.len() + 32);
    frame.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    frame.extend_from_slice(payload);
    let mut mac = Hmac::<Sha256>::new_from_slice(key).unwrap();
    mac.update(&frame);
    frame.extend_from_slice(&mac.finalize().into_bytes());
    frame
}

fn signed_json_frame(key: &[u8; 32], value: Value) -> Vec<u8> {
    signed_payload_frame(key, &serde_json::to_vec(&value).unwrap())
}

fn ping_envelope(sequence: u64, request_id: u64) -> Value {
    json!({
        "protocol_version": PROTOCOL_V1,
        "sequence": sequence,
        "message": {
            "type": "request",
            "body": {
                "request_id": request_id,
                "request": { "type": "ping" }
            }
        }
    })
}

fn assert_ping(message: ProtocolMessage, request_id: u64) {
    assert!(matches!(
        message,
        ProtocolMessage::Request {
            request_id: actual,
            request: BrokerRequest::Ping,
        } if actual == RequestId::new(request_id)
    ));
}

#[test]
fn authenticated_frames_round_trip_independently_in_both_directions() {
    let (mut host, mut broker) = codecs();

    let request = host.encode(ping(1)).unwrap();
    assert_ping(broker.decode(&request).unwrap(), 1);

    let response = broker
        .encode(ProtocolMessage::response(
            RequestId::new(1),
            BrokerResponse::Pong,
        ))
        .unwrap();
    assert!(matches!(
        host.decode(&response).unwrap(),
        ProtocolMessage::Response {
            request_id,
            response: BrokerResponse::Pong,
        } if request_id == RequestId::new(1)
    ));

    assert_ping(broker.decode(&host.encode(ping(2)).unwrap()).unwrap(), 2);
}

#[test]
fn wrong_receive_key_is_rejected_without_consuming_sequence() {
    let (mut host, _) = codecs();
    let frame = host.encode(ping(1)).unwrap();
    let mut wrong_receiver = AuthenticatedCodec::new(
        EndpointRole::Broker,
        ProtocolKey::from_bytes([6; 32]),
        ProtocolKey::from_bytes([8; 32]),
    );

    assert_protocol_error(wrong_receiver.decode(&frame));

    let mut matching_sender = AuthenticatedCodec::new(
        EndpointRole::Host,
        ProtocolKey::from_bytes([8; 32]),
        ProtocolKey::from_bytes([6; 32]),
    );
    assert_ping(
        wrong_receiver
            .decode(&matching_sender.encode(ping(2)).unwrap())
            .unwrap(),
        2,
    );
}

#[test]
fn host_to_broker_frame_cannot_be_reflected_to_its_sender() {
    let (mut host, mut broker) = codecs();
    let reflected = host.encode(ping(1)).unwrap();

    assert_protocol_error(host.decode(&reflected));

    let legitimate = broker
        .encode(ProtocolMessage::response(
            RequestId::new(1),
            BrokerResponse::Pong,
        ))
        .unwrap();
    assert!(matches!(
        host.decode(&legitimate).unwrap(),
        ProtocolMessage::Response {
            response: BrokerResponse::Pong,
            ..
        }
    ));
}

#[test]
fn one_bit_payload_tamper_is_rejected_before_json_and_does_not_consume() {
    let (mut host, mut broker) = codecs();
    let frame = host.encode(ping(1)).unwrap();
    let mut tampered = frame.clone();
    let payload_len = u32::from_be_bytes(tampered[..4].try_into().unwrap()) as usize;
    tampered[4 + payload_len / 2] ^= 1;

    assert_protocol_error(broker.decode(&tampered));
    assert_ping(broker.decode(&frame).unwrap(), 1);
}

#[test]
fn one_bit_mac_tamper_is_rejected_and_does_not_consume() {
    let (mut host, mut broker) = codecs();
    let frame = host.encode(ping(1)).unwrap();
    let mut tampered = frame.clone();
    *tampered.last_mut().unwrap() ^= 1;

    assert_protocol_error(broker.decode(&tampered));
    assert_ping(broker.decode(&frame).unwrap(), 1);
}

#[test]
fn accepted_frame_cannot_be_replayed() {
    let (mut host, mut broker) = codecs();
    let frame = host.encode(ping(1)).unwrap();

    assert_ping(broker.decode(&frame).unwrap(), 1);
    assert_protocol_error(broker.decode(&frame));

    assert_ping(broker.decode(&host.encode(ping(2)).unwrap()).unwrap(), 2);
}

#[test]
fn skipped_sequence_is_rejected_without_consuming_expected_sequence() {
    let (mut host, mut broker) = codecs();
    let skipped = signed_json_frame(&HOST_TO_BROKER_KEY, ping_envelope(1, 2));

    assert_protocol_error(broker.decode(&skipped));
    assert_ping(broker.decode(&host.encode(ping(1)).unwrap()).unwrap(), 1);
}

#[test]
fn empty_payload_is_rejected_without_consuming_sequence() {
    let (mut host, mut broker) = codecs();
    let empty = signed_payload_frame(&HOST_TO_BROKER_KEY, &[]);

    assert_protocol_error(broker.decode(&empty));
    assert_ping(broker.decode(&host.encode(ping(1)).unwrap()).unwrap(), 1);
}

#[test]
fn oversized_declaration_is_rejected_from_prefix_without_allocation() {
    let (mut host, mut broker) = codecs();
    let oversized = ((MAX_FRAME_BYTES + 1) as u32).to_be_bytes();

    assert_protocol_error(broker.decode(&oversized));
    assert_ping(broker.decode(&host.encode(ping(1)).unwrap()).unwrap(), 1);
}

#[test]
fn every_length_payload_and_mac_truncation_boundary_is_rejected() {
    let (mut host, mut broker) = codecs();
    let frame = host.encode(ping(1)).unwrap();
    let payload_len = u32::from_be_bytes(frame[..4].try_into().unwrap()) as usize;
    let payload_end = 4 + payload_len;

    for cut in 0..4 {
        assert_protocol_error(broker.decode(&frame[..cut]));
    }
    for cut in 4..payload_end {
        assert_protocol_error(broker.decode(&frame[..cut]));
    }
    for cut in payload_end..frame.len() {
        assert_protocol_error(broker.decode(&frame[..cut]));
    }

    assert_ping(broker.decode(&frame).unwrap(), 1);
}

#[test]
fn trailing_bytes_are_rejected_without_consuming_sequence() {
    let (mut host, mut broker) = codecs();
    let frame = host.encode(ping(1)).unwrap();
    let mut trailing = frame.clone();
    trailing.push(0);

    assert_protocol_error(broker.decode(&trailing));
    assert_ping(broker.decode(&frame).unwrap(), 1);
}

#[test]
fn authenticated_unsupported_version_is_rejected_without_consuming() {
    let (mut host, mut broker) = codecs();
    let mut invalid = ping_envelope(0, 99);
    invalid["protocol_version"] = json!(PROTOCOL_V1 + 1);

    assert_protocol_error(broker.decode(&signed_json_frame(&HOST_TO_BROKER_KEY, invalid)));
    assert_ping(broker.decode(&host.encode(ping(1)).unwrap()).unwrap(), 1);
}

#[test]
fn authenticated_unknown_envelope_field_is_rejected_without_consuming() {
    let (mut host, mut broker) = codecs();
    let mut invalid = ping_envelope(0, 99);
    invalid
        .as_object_mut()
        .unwrap()
        .insert("goblin".to_owned(), json!(true));

    assert_protocol_error(broker.decode(&signed_json_frame(&HOST_TO_BROKER_KEY, invalid)));
    assert_ping(broker.decode(&host.encode(ping(1)).unwrap()).unwrap(), 1);
}

#[test]
fn authenticated_invalid_bounded_dto_is_rejected_without_consuming() {
    let (mut host, mut broker) = codecs();
    let invalid = json!({
        "protocol_version": PROTOCOL_V1,
        "sequence": 0,
        "message": {
            "type": "request",
            "body": {
                "request_id": 99,
                "request": {
                    "type": "process_write",
                    "body": {
                        "session_id": "session-01",
                        "process_id": 0,
                        "bytes": STANDARD.encode(vec![0; MAX_IO_CHUNK_BYTES + 1]),
                        "close_stdin": false
                    }
                }
            }
        }
    });

    assert_protocol_error(broker.decode(&signed_json_frame(&HOST_TO_BROKER_KEY, invalid)));
    assert_ping(broker.decode(&host.encode(ping(1)).unwrap()).unwrap(), 1);
}

#[test]
fn protocol_key_debug_is_exactly_redacted() {
    let key = ProtocolKey::from_bytes([0xab; 32]);
    assert_eq!(format!("{key:?}"), "ProtocolKey([REDACTED])");
    assert!(!format!("{key:?}").contains("ab"));
}

#[test]
fn local_role_encode_failure_does_not_consume_send_sequence() {
    let (mut host, mut broker) = codecs();
    let invalid_for_host = ProtocolMessage::response(RequestId::new(1), BrokerResponse::Pong);

    assert_protocol_error(host.encode(invalid_for_host));
    assert_ping(broker.decode(&host.encode(ping(1)).unwrap()).unwrap(), 1);
}

#[test]
fn oversized_encode_failure_does_not_consume_send_sequence() {
    let (mut host, mut broker) = codecs();
    let oversized = ProtocolMessage::request(
        RequestId::new(99),
        BrokerRequest::ProcessStart(ProcessLaunchRequest {
            session_id: SessionId::parse("session-01").unwrap(),
            executable: ExecutableRef::HostCommand("oversized-command".to_owned()),
            arguments: vec!["x".repeat(MAX_FRAME_BYTES)],
            cwd: None,
            stdin_open: false,
            environment: LaunchEnvironment {
                clear_environment: true,
                entries: Vec::new(),
            },
            timeout_ms: 1,
            stdout_max_bytes: 0,
            stderr_max_bytes: 0,
        }),
    );

    assert_protocol_error(host.encode(oversized));
    assert_ping(broker.decode(&host.encode(ping(1)).unwrap()).unwrap(), 1);
}

struct TestNormalizer;

impl CatalogPathNormalizer for TestNormalizer {
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

fn full_access_policy() -> ValidatedExecutionPolicy {
    let generation = CatalogGeneration::new(1).unwrap();
    let platform = PlatformId {
        os: OperatingSystem::Windows,
        arch: Architecture::X86_64,
    };
    let catalog = CatalogSnapshot {
        schema_version: CATALOG_SCHEMA_V1,
        generation,
        platform,
        records: Vec::new(),
        revoked_tools: Vec::new(),
    }
    .validate(&TestNormalizer)
    .unwrap();
    ExecutionPolicyRequest {
        schema_version: POLICY_SCHEMA_V1,
        access: ExecutionAccess::Full,
        allow_process: true,
        allow_pty: true,
        workspace: None,
        scratch_disposition: ScratchDisposition::DeleteOnCleanCloseRetainOtherwise,
        catalog_generation: generation,
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

#[test]
fn authenticated_message_is_consumed_before_later_session_policy_rejection() {
    let (mut host, mut broker) = codecs();
    let request = ProtocolMessage::request(
        RequestId::new(1),
        BrokerRequest::ProcessDetail(ProcessRequest {
            session_id: SessionId::parse("session-wrong").unwrap(),
            process_id: ProcessId::new(0),
        }),
    );

    let decoded = broker.decode(&host.encode(request).unwrap()).unwrap();
    let ProtocolMessage::Request { request, .. } = decoded else {
        panic!("expected request");
    };
    assert!(request
        .validate_against(
            &SessionId::parse("session-expected").unwrap(),
            &full_access_policy(),
        )
        .is_err());

    assert_ping(broker.decode(&host.encode(ping(2)).unwrap()).unwrap(), 2);
}

#[test]
fn authenticated_payload_preserves_v1_tagging_and_explicit_nulls() {
    let frame = signed_json_frame(
        &HOST_TO_BROKER_KEY,
        json!({
            "protocol_version": PROTOCOL_V1,
            "sequence": 0,
            "message": {
                "type": "request",
                "body": {
                    "request_id": 1,
                    "request": {
                        "type": "process_start",
                        "body": {
                            "session_id": "session-01",
                            "executable": {
                                "type": "host_command",
                                "body": "cmd.exe"
                            },
                            "arguments": [],
                            "cwd": null,
                            "stdin_open": false,
                            "environment": {
                                "clear_environment": true,
                                "entries": []
                            },
                            "timeout_ms": 1,
                            "stdout_max_bytes": 0,
                            "stderr_max_bytes": 0
                        }
                    }
                }
            }
        }),
    );
    let (_, mut broker) = codecs();

    assert!(matches!(
        broker.decode(&frame).unwrap(),
        ProtocolMessage::Request {
            request: BrokerRequest::ProcessStart(ProcessLaunchRequest { cwd: None, .. }),
            ..
        }
    ));
}

#[test]
fn omitted_v1_optional_field_is_rejected_without_consuming_sequence() {
    let (mut host, mut broker) = codecs();
    let invalid = json!({
        "protocol_version": PROTOCOL_V1,
        "sequence": 0,
        "message": {
            "type": "request",
            "body": {
                "request_id": 99,
                "request": {
                    "type": "process_start",
                    "body": {
                        "session_id": "session-01",
                        "executable": {
                            "type": "host_command",
                            "body": "cmd.exe"
                        },
                        "arguments": [],
                        "stdin_open": false,
                        "environment": {
                            "clear_environment": true,
                            "entries": []
                        },
                        "timeout_ms": 1,
                        "stdout_max_bytes": 0,
                        "stderr_max_bytes": 0
                    }
                }
            }
        }
    });

    assert_protocol_error(broker.decode(&signed_json_frame(&HOST_TO_BROKER_KEY, invalid)));
    assert_ping(broker.decode(&host.encode(ping(1)).unwrap()).unwrap(), 1);
}

#[test]
fn protocol_errors_do_not_expose_key_material() {
    let (mut host, mut broker) = codecs();
    let frame = host.encode(ping(1)).unwrap();
    let mut tampered = frame;
    tampered[4] ^= 1;

    let error = broker.decode(&tampered).unwrap_err();
    let debug = format!("{error:?}");
    assert!(!debug.contains(&"07".repeat(32)));
    assert!(!debug.contains(&"09".repeat(32)));
}

#[test]
fn frame_declaration_is_four_byte_big_endian_json_length_plus_sha256_mac() {
    let (mut host, _) = codecs();
    let frame = host.encode(ping(1)).unwrap();
    let declared = u32::from_be_bytes(frame[..4].try_into().unwrap()) as usize;

    assert!(declared > 0);
    assert!(declared <= MAX_FRAME_BYTES);
    assert_eq!(frame.len(), 4 + declared + 32);
    assert!(serde_json::from_slice::<Value>(&frame[4..4 + declared]).is_ok());

    let mut mac = Hmac::<Sha256>::new_from_slice(&HOST_TO_BROKER_KEY).unwrap();
    mac.update(&frame[..4 + declared]);
    mac.verify_slice(&frame[4 + declared..]).unwrap();
}

#[test]
fn malformed_frame_errors_use_empty_protocol_metadata() {
    let (_, mut broker) = codecs();
    let error = broker.decode(&[0, 0, 0]).unwrap_err();
    assert_eq!(
        serde_json::to_value(error.metadata()).unwrap(),
        serde_json::to_value(DiagnosticMetadata::empty()).unwrap()
    );
}
