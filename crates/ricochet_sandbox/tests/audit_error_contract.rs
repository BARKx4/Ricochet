use std::cell::Cell;

use ricochet_sandbox::{
    ApprovalActor, Architecture, ArgumentAuditMode, ArtifactKind, AuditContext, AuditEventKind,
    AuditPolicy, AuditRecord, BackendFeatureId, BackendIdentity, CatalogGeneration,
    CatalogPathNormalizer, CatalogRecord, CatalogSnapshot, DestinationGrant, DiagnosticMetadata,
    EnforcementState, EnvironmentPolicy, EnvironmentVariable, ExecutionAccess,
    ExecutionAuditIdentity, ExecutionInstanceId, ExecutionPolicyRequest, ExecutionSurface,
    FailedGuarantee, HashedArtifact, OperatingSystem, PlatformId, ProcessId, ProcessTreeId, PtyId,
    Remediation, ResourceLimitKind, ResourceLimits, SandboxError, SandboxErrorCode, SandboxPhase,
    ScratchDisposition, ScratchId, SessionId, SessionLifecycle, SessionState, Sha256Digest, ToolId,
    ToolReference, UnixMillis, WorkspaceIdentity, WorkspaceIdentityResolver, WorkspaceRequest,
    CATALOG_SCHEMA_V1, POLICY_SCHEMA_V1, PROTOCOL_V1,
};
use serde_json::{json, Value};

const ENV_SECRET: &str = "env-secret-never-audit";
const RAW_ARGUMENT: &str = "raw-argument-never-audit";
const STDIN_SECRET: &str = "stdin-never-audit";
const OUTPUT_SECRET: &str = "output-never-audit";
const NATIVE_CAUSE: &str = "native-cause-never-audit";
const SOURCE_PATH_SECRET: &str = "source-secret-main";
const MANAGED_PATH_SECRET: &str = "managed-secret-main";

fn tool_id(value: &str) -> ToolId {
    ToolId::parse(value).unwrap()
}

fn session_id() -> SessionId {
    SessionId::parse("session-01").unwrap()
}

fn scratch_id() -> ScratchId {
    ScratchId::parse("scratch-01").unwrap()
}

fn backend(name: &str) -> BackendIdentity {
    BackendIdentity::new(name, "1").unwrap()
}

fn generation() -> CatalogGeneration {
    CatalogGeneration::new(7).unwrap()
}

fn digest(seed: &str) -> Sha256Digest {
    Sha256Digest::hash(seed.as_bytes())
}

fn destination() -> DestinationGrant {
    DestinationGrant::parse("audit.example:443").unwrap()
}

fn platform() -> PlatformId {
    PlatformId {
        os: OperatingSystem::Windows,
        arch: Architecture::X86_64,
    }
}

struct FixturePathNormalizer;

impl CatalogPathNormalizer for FixturePathNormalizer {
    fn normalize(&self, _platform: &PlatformId, path: &str) -> Result<String, SandboxError> {
        Ok(path.replace('/', "\\").to_ascii_lowercase())
    }
}

fn artifact(logical_name: &str, managed_name: &str, seed: &str) -> HashedArtifact {
    HashedArtifact {
        logical_name: logical_name.to_owned(),
        managed_canonical_path: format!(r"C:\broker-private\{managed_name}.exe"),
        sha256: digest(seed),
        kind: ArtifactKind::Executable,
    }
}

fn catalog_record(
    id: &str,
    source_name: &str,
    managed_name: &str,
    helpers: Vec<ToolReference>,
) -> CatalogRecord {
    CatalogRecord {
        schema_version: CATALOG_SCHEMA_V1,
        generation: generation(),
        tool_id: tool_id(id),
        platform: platform(),
        original_source_path: format!(r"C:\untrusted-source\{source_name}.exe"),
        executable: artifact(&format!("{id}-executable"), managed_name, id),
        helpers,
        non_system_libraries: Vec::new(),
        resources: Vec::new(),
        transport_adapter: None,
        approval_actor: ApprovalActor {
            display_name: "Sandbox Administrator".to_owned(),
            mechanism: "interactive-consent".to_owned(),
        },
        approved_at: UnixMillis::new(1_783_987_200_000),
        replaces: None,
    }
}

fn validated_catalog() -> ricochet_sandbox::ValidatedCatalogSnapshot {
    let helper = catalog_record("helper", "source-helper", "managed-helper", Vec::new());
    let main = catalog_record(
        "main",
        SOURCE_PATH_SECRET,
        MANAGED_PATH_SECRET,
        vec![ToolReference {
            tool_id: tool_id("helper"),
            sha256: helper.executable.sha256,
        }],
    );
    let unrelated = catalog_record(
        "unrelated",
        "source-secret-unrelated",
        "managed-secret-unrelated",
        Vec::new(),
    );

    CatalogSnapshot {
        schema_version: CATALOG_SCHEMA_V1,
        generation: generation(),
        platform: platform(),
        records: vec![unrelated, main, helper],
        revoked_tools: Vec::new(),
    }
    .validate(&FixturePathNormalizer)
    .unwrap()
}

fn finite_limits() -> ResourceLimits {
    ResourceLimits {
        descendant_processes: 4,
        memory_bytes: 64 * 1024 * 1024,
        cpu_time_ms: 10_000,
        wall_time_ms: 5_000,
        open_descriptors_or_handles: 64,
        captured_output_bytes: 4_096,
    }
}

fn workspace_identity() -> WorkspaceIdentity {
    WorkspaceIdentity {
        requested_root: r"C:\untrusted\workspace".to_owned(),
        canonical_root: "C:/workspace".to_owned(),
        native_object_identity: "volume-7:file-42".to_owned(),
    }
}

struct FixtureWorkspaceResolver {
    calls: Cell<usize>,
}

impl FixtureWorkspaceResolver {
    fn new() -> Self {
        Self {
            calls: Cell::new(0),
        }
    }
}

impl WorkspaceIdentityResolver for FixtureWorkspaceResolver {
    fn resolve(&self, _request: &WorkspaceRequest) -> Result<WorkspaceIdentity, SandboxError> {
        self.calls.set(self.calls.get() + 1);
        Ok(workspace_identity())
    }
}

fn validated_policy(access: ExecutionAccess) -> ricochet_sandbox::ValidatedExecutionPolicy {
    let constrained = matches!(access, ExecutionAccess::Read | ExecutionAccess::Workspace);
    ExecutionPolicyRequest {
        schema_version: POLICY_SCHEMA_V1,
        access,
        allow_process: true,
        allow_pty: true,
        workspace: constrained.then(|| WorkspaceRequest {
            requested_root: r"C:\untrusted\workspace".to_owned(),
        }),
        scratch_disposition: ScratchDisposition::DeleteOnCleanCloseRetainOtherwise,
        catalog_generation: generation(),
        activated_tools: if constrained {
            vec![tool_id("main")]
        } else {
            Vec::new()
        },
        destinations: if constrained {
            vec![destination()]
        } else {
            Vec::new()
        },
        environment: EnvironmentPolicy {
            base: vec![EnvironmentVariable {
                name: "TOKEN".to_owned(),
                value: ENV_SECRET.to_owned(),
            }],
        },
        resource_limits: constrained.then(finite_limits),
        audit_policy: AuditPolicy {
            arguments: ArgumentAuditMode::CountOnly,
        },
    }
    .validate(&validated_catalog(), &FixtureWorkspaceResolver::new())
    .unwrap()
}

#[allow(clippy::result_large_err)]
fn audit_context(
    access: ExecutionAccess,
    enforcement: EnforcementState,
    backend_name: &str,
) -> Result<AuditContext, SandboxError> {
    AuditContext::new(
        session_id(),
        scratch_id(),
        backend(backend_name),
        PROTOCOL_V1,
        enforcement,
        &validated_policy(access),
    )
}

#[test]
fn all_error_kinds_and_fixed_messages_are_stable() {
    let cases = vec![
        (
            SandboxError::unavailable(
                Some(backend("windows-lpac")),
                FailedGuarantee::BrokerAvailability,
                Remediation::StartOrInstallBroker,
                DiagnosticMetadata::default(),
            ),
            SandboxErrorCode::SandboxUnavailable,
            "SandboxUnavailable",
            SandboxPhase::Setup,
            "sandbox backend is unavailable",
        ),
        (
            SandboxError::policy(
                FailedGuarantee::PolicyValidity,
                DiagnosticMetadata::default(),
            ),
            SandboxErrorCode::SandboxPolicyError,
            "SandboxPolicyError",
            SandboxPhase::Setup,
            "requested execution policy is invalid",
        ),
        (
            SandboxError::tool_not_approved(tool_id("main")),
            SandboxErrorCode::ToolNotApproved,
            "ToolNotApproved",
            SandboxPhase::Launch,
            "tool is not approved",
        ),
        (
            SandboxError::tool_fingerprint_mismatch(tool_id("main")),
            SandboxErrorCode::ToolFingerprintMismatch,
            "ToolFingerprintMismatch",
            SandboxPhase::Launch,
            "tool fingerprint does not match the approved catalog",
        ),
        (
            SandboxError::network_denied(destination()),
            SandboxErrorCode::NetworkDenied,
            "NetworkDenied",
            SandboxPhase::Runtime,
            "network destination is not granted",
        ),
        (
            SandboxError::resource_limit(ResourceLimitKind::MemoryBytes),
            SandboxErrorCode::ResourceLimitExceeded,
            "ResourceLimitExceeded",
            SandboxPhase::Runtime,
            "sandbox resource limit was exceeded",
        ),
        (
            SandboxError::launch(backend("windows-lpac"), FailedGuarantee::NativeLaunch),
            SandboxErrorCode::SandboxLaunchError,
            "SandboxLaunchError",
            SandboxPhase::Launch,
            "native sandbox launch failed",
        ),
        (
            SandboxError::terminated(
                ricochet_sandbox::TerminationReason::ResourceLimit(ResourceLimitKind::WallTime),
                session_id(),
            ),
            SandboxErrorCode::SandboxTerminated,
            "SandboxTerminated",
            SandboxPhase::Shutdown,
            "sandbox session was terminated",
        ),
        (
            SandboxError::protocol(
                DiagnosticMetadata::default().with_protocol_version(PROTOCOL_V1),
            ),
            SandboxErrorCode::BrokerProtocolError,
            "BrokerProtocolError",
            SandboxPhase::Protocol,
            "broker protocol validation failed",
        ),
    ];

    for (error, code, kind, phase, message) in cases {
        error.validate().unwrap();
        assert_eq!(error.code(), code);
        assert_eq!(error.kind(), kind);
        assert_eq!(error.phase(), phase);
        assert_eq!(error.message(), message);
        assert_eq!(error.to_string(), message);

        let value = serde_json::to_value(&error).unwrap();
        assert_eq!(value["code"], kind);
        assert_eq!(value["message"], message);
        assert_eq!(
            value.get("native_cause"),
            None,
            "native cause became wire-visible for {kind}"
        );
    }
}

#[test]
fn typed_diagnostics_round_trip_and_forged_messages_are_rejected() {
    let error = SandboxError::protocol(
        DiagnosticMetadata::default()
            .with_tool_id(tool_id("main"))
            .with_destination(destination())
            .with_resource_limit(ResourceLimitKind::CapturedOutput)
            .with_protocol_version(PROTOCOL_V1)
            .with_session_id(session_id())
            .with_backend_feature(BackendFeatureId::parse("lpac.profile").unwrap()),
    );
    let value = serde_json::to_value(&error).unwrap();

    assert_eq!(value["phase"], "protocol");
    assert_eq!(value["failed_guarantee"], "protocol_authenticity");
    assert_eq!(value["remediation"], "retry_after_broker_restart");
    assert_eq!(value["metadata"]["tool_id"], "main");
    assert_eq!(value["metadata"]["destination"], "audit.example:443");
    assert_eq!(value["metadata"]["resource_limit"], "captured_output");
    assert_eq!(value["metadata"]["protocol_version"], PROTOCOL_V1);
    assert_eq!(value["metadata"]["session_id"], "session-01");
    assert_eq!(value["metadata"]["backend_feature"], "lpac.profile");

    let decoded: SandboxError = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(serde_json::to_value(decoded).unwrap(), value);

    let mut forged_message = value.clone();
    forged_message["message"] = json!("raw os error text");
    assert!(serde_json::from_value::<SandboxError>(forged_message).is_err());

    let mut forged_shape = value;
    forged_shape["code"] = json!("SandboxPolicyError");
    assert!(serde_json::from_value::<SandboxError>(forged_shape).is_err());
}

#[test]
fn native_causes_are_private_and_redacted_everywhere() {
    let error = SandboxError::launch(backend("windows-lpac"), FailedGuarantee::NativeLaunch)
        .with_native_cause(NATIVE_CAUSE);

    let debug = format!("{error:?}");
    let display = error.to_string();
    let json = serde_json::to_string(&error).unwrap();

    assert!(!debug.contains(NATIVE_CAUSE));
    assert!(!display.contains(NATIVE_CAUSE));
    assert!(!json.contains(NATIVE_CAUSE));
    assert!(!json.contains("native_cause"));
}

fn lifecycle_at(state: SessionState) -> SessionLifecycle {
    let mut lifecycle = SessionLifecycle::new();
    match state {
        SessionState::Preparing => {}
        SessionState::Ready => lifecycle.transition(SessionState::Ready).unwrap(),
        SessionState::Running => {
            lifecycle.transition(SessionState::Ready).unwrap();
            lifecycle.transition(SessionState::Running).unwrap();
        }
        SessionState::Stopping => {
            lifecycle.transition(SessionState::Ready).unwrap();
            lifecycle.transition(SessionState::Stopping).unwrap();
        }
        SessionState::Closed => {
            lifecycle.transition(SessionState::Ready).unwrap();
            lifecycle.transition(SessionState::Stopping).unwrap();
            lifecycle.transition(SessionState::Closed).unwrap();
        }
        SessionState::Failed => lifecycle.transition(SessionState::Failed).unwrap(),
    }
    lifecycle
}

fn transition_allowed(from: SessionState, to: SessionState) -> bool {
    matches!(
        (from, to),
        (
            SessionState::Preparing,
            SessionState::Ready | SessionState::Failed
        ) | (
            SessionState::Ready,
            SessionState::Running | SessionState::Stopping | SessionState::Failed
        ) | (
            SessionState::Running,
            SessionState::Stopping | SessionState::Failed
        ) | (
            SessionState::Stopping,
            SessionState::Closed | SessionState::Failed
        )
    )
}

#[test]
fn lifecycle_accepts_only_the_approved_transition_table() {
    let states = [
        SessionState::Preparing,
        SessionState::Ready,
        SessionState::Running,
        SessionState::Stopping,
        SessionState::Closed,
        SessionState::Failed,
    ];

    for from in states {
        for to in states {
            let mut lifecycle = lifecycle_at(from);
            let result = lifecycle.transition(to);
            assert_eq!(
                result.is_ok(),
                transition_allowed(from, to),
                "unexpected transition result for {from:?} -> {to:?}"
            );
            assert_eq!(
                lifecycle.state(),
                if result.is_ok() { to } else { from },
                "a rejected transition mutated lifecycle state"
            );
        }
    }
}

#[test]
fn audit_context_accepts_exactly_the_access_enforcement_matrix() {
    let cases = [
        (ExecutionAccess::Read, EnforcementState::Enforced, true),
        (ExecutionAccess::Read, EnforcementState::MockOnly, true),
        (
            ExecutionAccess::Read,
            EnforcementState::UnenforcedFullAccess,
            false,
        ),
        (ExecutionAccess::Workspace, EnforcementState::Enforced, true),
        (ExecutionAccess::Workspace, EnforcementState::MockOnly, true),
        (
            ExecutionAccess::Workspace,
            EnforcementState::UnenforcedFullAccess,
            false,
        ),
        (ExecutionAccess::Full, EnforcementState::Enforced, false),
        (
            ExecutionAccess::Full,
            EnforcementState::UnenforcedFullAccess,
            true,
        ),
        (ExecutionAccess::Full, EnforcementState::MockOnly, true),
    ];

    for (access, enforcement, allowed) in cases {
        let backend_name = if enforcement == EnforcementState::Enforced {
            "mock"
        } else {
            "production"
        };
        let result = audit_context(access, enforcement, backend_name);
        assert_eq!(
            result.is_ok(),
            allowed,
            "unexpected compatibility for {access:?}/{enforcement:?}"
        );
    }

    assert!(EnforcementState::Enforced.enforced());
    assert!(!EnforcementState::UnenforcedFullAccess.enforced());
    assert!(!EnforcementState::MockOnly.enforced());
}

#[test]
fn audit_context_uses_only_the_activated_catalog_closure() {
    let read = audit_context(ExecutionAccess::Read, EnforcementState::Enforced, "mock").unwrap();
    let tool_ids = read
        .tools()
        .iter()
        .map(|record| record.tool_id.as_str())
        .collect::<Vec<_>>();

    assert_eq!(tool_ids, ["helper", "main"]);
    assert_eq!(read.session_id().as_str(), "session-01");
    assert_eq!(read.scratch_id().as_str(), "scratch-01");
    assert_eq!(read.policy_digest().to_hex().len(), 64);
    assert_eq!(read.access(), ExecutionAccess::Read);
    assert_eq!(read.enforcement(), EnforcementState::Enforced);
    assert_eq!(read.backend().name(), "mock");
    assert_eq!(read.broker_protocol(), PROTOCOL_V1);
    assert_eq!(read.catalog_generation(), generation());
    assert_eq!(read.destinations().len(), 1);
    assert_eq!(read.destinations()[0].host(), destination().host());
    assert_eq!(read.destinations()[0].port(), destination().port());
    assert_eq!(read.resource_limits(), Some(&finite_limits()));
    assert_eq!(
        read.workspace().unwrap().canonical_root(),
        workspace_identity().canonical_root
    );
    assert!(!read.workspace().unwrap().writable());

    let workspace = audit_context(
        ExecutionAccess::Workspace,
        EnforcementState::Enforced,
        "native",
    )
    .unwrap();
    assert!(workspace.workspace().unwrap().writable());

    let full = audit_context(
        ExecutionAccess::Full,
        EnforcementState::UnenforcedFullAccess,
        "native",
    )
    .unwrap();
    assert!(full.workspace().is_none());
    assert!(full.tools().is_empty());
    assert!(full.destinations().is_empty());
    assert!(full.resource_limits().is_none());
}

fn execution_from_terminal(event: &AuditEventKind) -> &ExecutionAuditIdentity {
    match event {
        AuditEventKind::Exited { execution, .. }
        | AuditEventKind::Cancelled { execution, .. }
        | AuditEventKind::TimedOut { execution }
        | AuditEventKind::ResourceLimit { execution, .. } => execution,
        _ => panic!("event is not a terminal execution event"),
    }
}

#[test]
fn every_terminal_execution_event_carries_tree_and_process_or_pty_identity() {
    let process = ExecutionAuditIdentity::process(ProcessTreeId::new(11), ProcessId::new(12));
    let pty = ExecutionAuditIdentity::pty(ProcessTreeId::new(21), PtyId::new(22));
    let events = [
        AuditEventKind::Exited {
            execution: process,
            exit_code: Some(0),
            success: true,
        },
        AuditEventKind::Cancelled {
            execution: pty,
            reason: ricochet_sandbox::TerminationReason::CancelledByHost,
        },
        AuditEventKind::TimedOut {
            execution: ExecutionAuditIdentity::process(ProcessTreeId::new(31), ProcessId::new(32)),
        },
        AuditEventKind::ResourceLimit {
            execution: ExecutionAuditIdentity::pty(ProcessTreeId::new(41), PtyId::new(42)),
            limit: ResourceLimitKind::MemoryBytes,
        },
    ];

    for event in events {
        let value = serde_json::to_value(execution_from_terminal(&event)).unwrap();
        assert!(value.get("process_tree_id").is_some());
        assert!(value.get("instance").is_some());
    }

    let process_value = serde_json::to_value(ExecutionAuditIdentity::process(
        ProcessTreeId::new(51),
        ProcessId::new(52),
    ))
    .unwrap();
    assert_eq!(process_value["process_tree_id"], 51);
    assert_eq!(process_value["instance"]["type"], "process");
    assert_eq!(process_value["instance"]["body"], 52);

    let pty_value = serde_json::to_value(ExecutionAuditIdentity::pty(
        ProcessTreeId::new(61),
        PtyId::new(62),
    ))
    .unwrap();
    assert_eq!(pty_value["process_tree_id"], 61);
    assert_eq!(pty_value["instance"]["type"], "pty");
    assert_eq!(pty_value["instance"]["body"], 62);
}

#[test]
fn serialized_audit_is_count_only_and_secret_safe() {
    let arguments = ["--token", RAW_ARGUMENT];
    let context = audit_context(
        ExecutionAccess::Read,
        EnforcementState::Enforced,
        "windows-lpac",
    )
    .unwrap();
    let record = AuditRecord::new(
        UnixMillis::new(1_783_987_200_000),
        context,
        AuditEventKind::LaunchRequested {
            surface: ExecutionSurface::Process,
            tool_id: Some(tool_id("main")),
            argument_count: arguments.len(),
        },
    );
    let value = serde_json::to_value(&record).unwrap();
    let serialized = serde_json::to_string(&value).unwrap();

    assert_eq!(record.context().session_id().as_str(), "session-01");
    assert!(matches!(
        record.event(),
        AuditEventKind::LaunchRequested {
            argument_count: 2,
            ..
        }
    ));
    assert_eq!(value["event"]["type"], "launch_requested");
    assert_eq!(value["event"]["body"]["surface"], "process");
    assert_eq!(value["event"]["body"]["argument_count"], 2);

    for forbidden in [
        ENV_SECRET,
        RAW_ARGUMENT,
        STDIN_SECRET,
        OUTPUT_SECRET,
        SOURCE_PATH_SECRET,
        MANAGED_PATH_SECRET,
        NATIVE_CAUSE,
    ] {
        assert!(
            !serialized.contains(forbidden),
            "serialized audit leaked forbidden value {forbidden}: {serialized}"
        );
    }
    for forbidden_field in [
        "\"arguments\":",
        "\"stdin\":",
        "\"output\":",
        "\"environment\":",
        "\"original_source_path\":",
        "\"managed_canonical_path\":",
        "\"native_cause\":",
    ] {
        assert!(
            !serialized.contains(forbidden_field),
            "serialized audit exposed forbidden field {forbidden_field}: {serialized}"
        );
    }
}

#[test]
fn denied_audit_records_contain_only_typed_denial_fields() {
    let native_error = SandboxError::network_denied(destination()).with_native_cause(NATIVE_CAUSE);
    let record = AuditRecord::new(
        UnixMillis::new(1_783_987_200_001),
        audit_context(
            ExecutionAccess::Read,
            EnforcementState::MockOnly,
            "production",
        )
        .unwrap(),
        AuditEventKind::Denied {
            code: native_error.code(),
            guarantee: FailedGuarantee::DestinationGrant,
            remediation: native_error.remediation(),
        },
    );
    let value = serde_json::to_value(record).unwrap();
    let body = &value["event"]["body"];

    assert_eq!(value["event"]["type"], "denied");
    assert_eq!(
        body,
        &json!({
            "code": "NetworkDenied",
            "guarantee": "destination_grant",
            "remediation": "add_destination_grant"
        })
    );
    assert_eq!(body.get("message"), None);
    assert_eq!(body.get("metadata"), None);
    assert_eq!(body.get("native_cause"), None);
}

#[test]
fn audit_wire_shape_matches_the_v1_fixture_conventions() {
    let value = serde_json::to_value(AuditEventKind::StateTransition {
        from: SessionState::Preparing,
        to: SessionState::Ready,
    })
    .unwrap();

    assert_eq!(
        value,
        json!({
            "type": "state_transition",
            "body": { "from": "preparing", "to": "ready" }
        })
    );
    assert_eq!(
        serde_json::to_value(ExecutionInstanceId::Process(ProcessId::new(0))).unwrap(),
        json!({ "type": "process", "body": 0 })
    );
}

#[test]
fn error_and_audit_structs_reject_unknown_fields() {
    let error = SandboxError::policy(
        FailedGuarantee::PolicyValidity,
        DiagnosticMetadata::default(),
    );
    let mut error_value = serde_json::to_value(error).unwrap();
    error_value["unexpected"] = Value::Bool(true);
    assert!(serde_json::from_value::<SandboxError>(error_value).is_err());
}

#[test]
fn audit_records_round_trip_and_reject_forged_or_unknown_fields() {
    let record = AuditRecord::new(
        UnixMillis::new(1_783_987_200_002),
        audit_context(
            ExecutionAccess::Read,
            EnforcementState::Enforced,
            "windows-lpac",
        )
        .unwrap(),
        AuditEventKind::LaunchRequested {
            surface: ExecutionSurface::Process,
            tool_id: Some(tool_id("main")),
            argument_count: 2,
        },
    );
    let value = serde_json::to_value(record).unwrap();
    let decoded: AuditRecord = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(serde_json::to_value(decoded).unwrap(), value);

    for path in ["record", "context", "workspace", "event_body"] {
        let mut forged = value.clone();
        match path {
            "record" => forged["unexpected"] = json!(true),
            "context" => forged["context"]["unexpected"] = json!(true),
            "workspace" => forged["context"]["workspace"]["unexpected"] = json!(true),
            "event_body" => forged["event"]["body"]["arguments"] = json!([RAW_ARGUMENT]),
            _ => unreachable!(),
        }
        assert!(
            serde_json::from_value::<AuditRecord>(forged).is_err(),
            "accepted unknown {path} field"
        );
    }

    let mut incompatible = value;
    incompatible["context"]["enforcement"] = json!("unenforced_full_access");
    assert!(serde_json::from_value::<AuditRecord>(incompatible).is_err());

    let execution = ExecutionAuditIdentity::process(ProcessTreeId::new(71), ProcessId::new(72));
    let execution_value = serde_json::to_value(execution).unwrap();
    let decoded: ExecutionAuditIdentity = serde_json::from_value(execution_value.clone()).unwrap();
    assert_eq!(serde_json::to_value(decoded).unwrap(), execution_value);

    let mut unknown_execution = execution_value;
    unknown_execution["native_process_id"] = json!(72);
    assert!(serde_json::from_value::<ExecutionAuditIdentity>(unknown_execution).is_err());
}
