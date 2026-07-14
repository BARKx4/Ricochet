use std::collections::VecDeque;

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize};

use crate::catalog::PublicToolRecord;
use crate::destination::DestinationGrant;
use crate::error::{
    typed_denial_shape_valid, DiagnosticMetadata, FailedGuarantee, Remediation, ResourceLimitKind,
    SandboxError, SandboxErrorCode, TerminationReason,
};
use crate::identity::{
    BackendIdentity, CatalogGeneration, PolicyDigest, ProcessId, ProcessTreeId, PtyId, ScratchId,
    SessionId, ToolId, UnixMillis,
};
use crate::lifecycle::{transition_allowed, SessionState};
use crate::policy::{
    ExecutionAccess, ExecutionSurface, ResourceLimits, ValidatedExecutionPolicy, WorkspaceIdentity,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub enum EnforcementState {
    Enforced,
    UnenforcedFullAccess,
    MockOnly,
}

impl EnforcementState {
    pub fn enforced(self) -> bool {
        matches!(self, Self::Enforced)
    }
}

#[derive(Clone, PartialEq, Eq, Serialize)]
pub struct AuditContext {
    session_id: SessionId,
    policy_digest: PolicyDigest,
    access: ExecutionAccess,
    enforcement: EnforcementState,
    backend: BackendIdentity,
    broker_protocol: u16,
    workspace: Option<AuditWorkspace>,
    scratch_id: ScratchId,
    catalog_generation: CatalogGeneration,
    tools: Vec<PublicToolRecord>,
    destinations: Vec<DestinationGrant>,
    resource_limits: Option<ResourceLimits>,
}

impl AuditContext {
    #[allow(clippy::result_large_err)]
    pub fn new(
        session_id: SessionId,
        scratch_id: ScratchId,
        backend: BackendIdentity,
        broker_protocol: u16,
        enforcement: EnforcementState,
        policy: &ValidatedExecutionPolicy,
    ) -> Result<Self, SandboxError> {
        let access = policy.access();
        if !compatible_access_enforcement(access, enforcement) {
            return Err(SandboxError::policy(
                FailedGuarantee::PolicyValidity,
                DiagnosticMetadata::empty(),
            ));
        }

        let context = Self {
            session_id,
            policy_digest: policy.audit_digest(),
            access,
            enforcement,
            backend,
            broker_protocol,
            workspace: policy.workspace_identity().map(|identity| {
                AuditWorkspace::from_identity(identity, access != ExecutionAccess::Read)
            }),
            scratch_id,
            catalog_generation: policy.prepared_catalog().generation(),
            tools: policy.prepared_catalog().public_records(),
            destinations: policy.destinations().to_vec(),
            resource_limits: policy.resource_limits().cloned(),
        };
        context.validate_against_policy(policy)?;
        Ok(context)
    }

    pub fn session_id(&self) -> &SessionId {
        &self.session_id
    }

    pub fn policy_digest(&self) -> &PolicyDigest {
        &self.policy_digest
    }

    pub fn access(&self) -> ExecutionAccess {
        self.access
    }

    pub fn enforcement(&self) -> EnforcementState {
        self.enforcement
    }

    pub fn backend(&self) -> &BackendIdentity {
        &self.backend
    }

    pub fn broker_protocol(&self) -> u16 {
        self.broker_protocol
    }

    pub fn workspace(&self) -> Option<&AuditWorkspace> {
        self.workspace.as_ref()
    }

    pub fn scratch_id(&self) -> &ScratchId {
        &self.scratch_id
    }

    pub fn catalog_generation(&self) -> CatalogGeneration {
        self.catalog_generation
    }

    pub fn tools(&self) -> &[PublicToolRecord] {
        &self.tools
    }

    pub fn destinations(&self) -> &[DestinationGrant] {
        &self.destinations
    }

    pub fn resource_limits(&self) -> Option<&ResourceLimits> {
        self.resource_limits.as_ref()
    }

    #[allow(clippy::result_large_err)]
    pub(crate) fn validate(&self) -> Result<(), SandboxError> {
        let workspace_valid = self.workspace.as_ref().is_none_or(AuditWorkspace::valid);
        let access_fields_valid = match self.access {
            ExecutionAccess::Read => {
                self.workspace.as_ref().is_some_and(|value| !value.writable)
                    && self.resource_limits.is_some()
            }
            ExecutionAccess::Workspace => {
                self.workspace.as_ref().is_some_and(|value| value.writable)
                    && self.resource_limits.is_some()
            }
            ExecutionAccess::Full => {
                self.workspace.as_ref().is_none_or(|value| value.writable)
                    && self.tools.is_empty()
                    && self.destinations.is_empty()
            }
        };
        let limits_valid = self
            .resource_limits
            .as_ref()
            .is_none_or(|limits| limits.validate().is_ok());
        let destinations_valid = self.destinations.windows(2).all(|pair| pair[0] < pair[1]);

        if compatible_access_enforcement(self.access, self.enforcement)
            && workspace_valid
            && access_fields_valid
            && limits_valid
            && public_tools_valid(&self.tools)
            && destinations_valid
        {
            return Ok(());
        }

        Err(audit_validation_error())
    }

    #[allow(clippy::result_large_err)]
    pub(crate) fn validate_against_policy(
        &self,
        policy: &ValidatedExecutionPolicy,
    ) -> Result<(), SandboxError> {
        self.validate()?;

        let expected_workspace = policy.workspace_identity().map(|identity| {
            AuditWorkspace::from_identity(identity, policy.access() != ExecutionAccess::Read)
        });
        if self.access == policy.access()
            && self.policy_digest == policy.audit_digest()
            && self.workspace == expected_workspace
            && self.catalog_generation == policy.prepared_catalog().generation()
            && self.tools == policy.prepared_catalog().public_records()
            && self.destinations.as_slice() == policy.destinations()
            && self.resource_limits.as_ref() == policy.resource_limits()
        {
            Ok(())
        } else {
            Err(audit_validation_error())
        }
    }
}

impl<'de> Deserialize<'de> for AuditContext {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct WireAuditContext {
            session_id: SessionId,
            policy_digest: PolicyDigest,
            access: ExecutionAccess,
            enforcement: EnforcementState,
            backend: BackendIdentity,
            broker_protocol: u16,
            workspace: Option<AuditWorkspace>,
            scratch_id: ScratchId,
            catalog_generation: CatalogGeneration,
            tools: Vec<PublicToolRecord>,
            destinations: Vec<DestinationGrant>,
            resource_limits: Option<ResourceLimits>,
        }

        let wire = WireAuditContext::deserialize(deserializer)?;
        let context = Self {
            session_id: wire.session_id,
            policy_digest: wire.policy_digest,
            access: wire.access,
            enforcement: wire.enforcement,
            backend: wire.backend,
            broker_protocol: wire.broker_protocol,
            workspace: wire.workspace,
            scratch_id: wire.scratch_id,
            catalog_generation: wire.catalog_generation,
            tools: wire.tools,
            destinations: wire.destinations,
            resource_limits: wire.resource_limits,
        };
        context.validate().map_err(D::Error::custom)?;
        Ok(context)
    }
}

fn compatible_access_enforcement(access: ExecutionAccess, enforcement: EnforcementState) -> bool {
    matches!(
        (access, enforcement),
        (
            ExecutionAccess::Read | ExecutionAccess::Workspace,
            EnforcementState::Enforced | EnforcementState::MockOnly
        ) | (
            ExecutionAccess::Full,
            EnforcementState::UnenforcedFullAccess | EnforcementState::MockOnly
        )
    )
}

fn public_tools_valid(tools: &[PublicToolRecord]) -> bool {
    if !tools
        .windows(2)
        .all(|pair| pair[0].tool_id < pair[1].tool_id)
    {
        return false;
    }

    let mut incoming_edges = vec![0_usize; tools.len()];
    for tool in tools {
        if !tool.helper_ids.windows(2).all(|pair| pair[0] < pair[1]) {
            return false;
        }
        for helper_id in &tool.helper_ids {
            let Ok(index) = tools.binary_search_by(|candidate| candidate.tool_id.cmp(helper_id))
            else {
                return false;
            };
            incoming_edges[index] += 1;
        }
    }

    let mut pending = incoming_edges
        .iter()
        .enumerate()
        .filter_map(|(index, incoming)| (*incoming == 0).then_some(index))
        .collect::<VecDeque<_>>();
    let mut visited = 0_usize;
    while let Some(index) = pending.pop_front() {
        visited += 1;
        for helper_id in &tools[index].helper_ids {
            let helper_index = tools
                .binary_search_by(|candidate| candidate.tool_id.cmp(helper_id))
                .expect("helper existence was validated before cycle detection");
            incoming_edges[helper_index] -= 1;
            if incoming_edges[helper_index] == 0 {
                pending.push_back(helper_index);
            }
        }
    }

    visited == tools.len()
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuditWorkspace {
    canonical_root: String,
    writable: bool,
}

impl AuditWorkspace {
    pub fn from_identity(identity: &WorkspaceIdentity, writable: bool) -> Self {
        Self {
            canonical_root: identity.canonical_root.clone(),
            writable,
        }
    }

    pub fn canonical_root(&self) -> &str {
        &self.canonical_root
    }

    pub fn writable(&self) -> bool {
        self.writable
    }

    fn valid(&self) -> bool {
        !self.canonical_root.is_empty() && !self.canonical_root.contains('\0')
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(
    deny_unknown_fields,
    tag = "type",
    content = "body",
    rename_all = "snake_case"
)]
pub enum ExecutionInstanceId {
    Process(ProcessId),
    Pty(PtyId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionAuditIdentity {
    process_tree_id: ProcessTreeId,
    instance: ExecutionInstanceId,
}

impl ExecutionAuditIdentity {
    pub fn process(process_tree_id: ProcessTreeId, process_id: ProcessId) -> Self {
        Self {
            process_tree_id,
            instance: ExecutionInstanceId::Process(process_id),
        }
    }

    pub fn pty(process_tree_id: ProcessTreeId, pty_id: PtyId) -> Self {
        Self {
            process_tree_id,
            instance: ExecutionInstanceId::Pty(pty_id),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(
    deny_unknown_fields,
    tag = "type",
    content = "body",
    rename_all = "snake_case"
)]
pub enum AuditEventKind {
    StateTransition {
        from: SessionState,
        to: SessionState,
    },
    LaunchRequested {
        surface: ExecutionSurface,
        tool_id: Option<ToolId>,
        argument_count: usize,
    },
    ProcessTreeStarted {
        process_tree_id: ProcessTreeId,
    },
    Exited {
        execution: ExecutionAuditIdentity,
        exit_code: Option<i64>,
        success: bool,
    },
    Cancelled {
        execution: ExecutionAuditIdentity,
        reason: TerminationReason,
    },
    Revoked {
        tool_id: ToolId,
        affected_process_trees: Vec<ProcessTreeId>,
    },
    TimedOut {
        execution: ExecutionAuditIdentity,
    },
    ResourceLimit {
        execution: ExecutionAuditIdentity,
        limit: ResourceLimitKind,
    },
    Denied {
        code: SandboxErrorCode,
        guarantee: FailedGuarantee,
        remediation: Option<Remediation>,
    },
}

impl AuditEventKind {
    #[allow(clippy::result_large_err)]
    pub(crate) fn validate(&self) -> Result<(), SandboxError> {
        let valid = match self {
            Self::StateTransition { from, to } => transition_allowed(*from, *to),
            Self::Denied {
                code,
                guarantee,
                remediation,
            } => typed_denial_shape_valid(*code, Some(*guarantee), *remediation),
            _ => true,
        };
        if valid {
            Ok(())
        } else {
            Err(audit_validation_error())
        }
    }
}

#[derive(Deserialize)]
#[serde(
    deny_unknown_fields,
    tag = "type",
    content = "body",
    rename_all = "snake_case"
)]
enum WireAuditEventKind {
    StateTransition {
        from: SessionState,
        to: SessionState,
    },
    LaunchRequested {
        surface: ExecutionSurface,
        tool_id: Option<ToolId>,
        argument_count: usize,
    },
    ProcessTreeStarted {
        process_tree_id: ProcessTreeId,
    },
    Exited {
        execution: ExecutionAuditIdentity,
        exit_code: Option<i64>,
        success: bool,
    },
    Cancelled {
        execution: ExecutionAuditIdentity,
        reason: TerminationReason,
    },
    Revoked {
        tool_id: ToolId,
        affected_process_trees: Vec<ProcessTreeId>,
    },
    TimedOut {
        execution: ExecutionAuditIdentity,
    },
    ResourceLimit {
        execution: ExecutionAuditIdentity,
        limit: ResourceLimitKind,
    },
    Denied {
        code: SandboxErrorCode,
        guarantee: FailedGuarantee,
        remediation: Option<Remediation>,
    },
}

impl From<WireAuditEventKind> for AuditEventKind {
    fn from(value: WireAuditEventKind) -> Self {
        match value {
            WireAuditEventKind::StateTransition { from, to } => Self::StateTransition { from, to },
            WireAuditEventKind::LaunchRequested {
                surface,
                tool_id,
                argument_count,
            } => Self::LaunchRequested {
                surface,
                tool_id,
                argument_count,
            },
            WireAuditEventKind::ProcessTreeStarted { process_tree_id } => {
                Self::ProcessTreeStarted { process_tree_id }
            }
            WireAuditEventKind::Exited {
                execution,
                exit_code,
                success,
            } => Self::Exited {
                execution,
                exit_code,
                success,
            },
            WireAuditEventKind::Cancelled { execution, reason } => {
                Self::Cancelled { execution, reason }
            }
            WireAuditEventKind::Revoked {
                tool_id,
                affected_process_trees,
            } => Self::Revoked {
                tool_id,
                affected_process_trees,
            },
            WireAuditEventKind::TimedOut { execution } => Self::TimedOut { execution },
            WireAuditEventKind::ResourceLimit { execution, limit } => {
                Self::ResourceLimit { execution, limit }
            }
            WireAuditEventKind::Denied {
                code,
                guarantee,
                remediation,
            } => Self::Denied {
                code,
                guarantee,
                remediation,
            },
        }
    }
}

impl<'de> Deserialize<'de> for AuditEventKind {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let event = Self::from(WireAuditEventKind::deserialize(deserializer)?);
        event.validate().map_err(D::Error::custom)?;
        Ok(event)
    }
}

#[derive(Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AuditRecord {
    at: UnixMillis,
    context: AuditContext,
    event: AuditEventKind,
}

fn audit_validation_error() -> SandboxError {
    SandboxError::policy(FailedGuarantee::PolicyValidity, DiagnosticMetadata::empty())
}

impl AuditRecord {
    pub fn new(at: UnixMillis, context: AuditContext, event: AuditEventKind) -> Self {
        Self { at, context, event }
    }

    pub fn context(&self) -> &AuditContext {
        &self.context
    }

    pub fn event(&self) -> &AuditEventKind {
        &self.event
    }

    #[allow(clippy::result_large_err)]
    pub(crate) fn validate(&self) -> Result<(), SandboxError> {
        self.context.validate()?;
        self.event.validate()
    }
}

impl<'de> Deserialize<'de> for AuditRecord {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct WireAuditRecord {
            at: UnixMillis,
            context: AuditContext,
            event: AuditEventKind,
        }

        let wire = WireAuditRecord::deserialize(deserializer)?;
        let record = Self {
            at: wire.at,
            context: wire.context,
            event: wire.event,
        };
        record.validate().map_err(D::Error::custom)?;
        Ok(record)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{
        ApprovalActor, Architecture, ArtifactKind, CatalogPathNormalizer, CatalogRecord,
        CatalogSnapshot, HashedArtifact, OperatingSystem, PlatformId,
    };
    use crate::identity::Sha256Digest;
    use crate::policy::{
        ArgumentAuditMode, AuditPolicy, EnvironmentPolicy, ExecutionPolicyRequest,
        ScratchDisposition, WorkspaceIdentityResolver, WorkspaceRequest,
    };
    use crate::version::{CATALOG_SCHEMA_V1, POLICY_SCHEMA_V1, PROTOCOL_V1};

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

    fn generation(value: u64) -> CatalogGeneration {
        CatalogGeneration::new(value).unwrap()
    }

    fn tool_id(value: &str) -> ToolId {
        ToolId::parse(value).unwrap()
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

    fn policy() -> ValidatedExecutionPolicy {
        let platform = PlatformId {
            os: OperatingSystem::Windows,
            arch: Architecture::X86_64,
        };
        let catalog = CatalogSnapshot {
            schema_version: CATALOG_SCHEMA_V1,
            generation: generation(7),
            platform,
            records: vec![CatalogRecord {
                schema_version: CATALOG_SCHEMA_V1,
                generation: generation(7),
                tool_id: tool_id("main"),
                platform,
                original_source_path: r"C:\source\main.exe".to_owned(),
                executable: HashedArtifact {
                    logical_name: "main-executable".to_owned(),
                    managed_canonical_path: r"C:\broker-private\main.exe".to_owned(),
                    sha256: Sha256Digest::hash(b"main"),
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
        .unwrap();

        ExecutionPolicyRequest {
            schema_version: POLICY_SCHEMA_V1,
            access: ExecutionAccess::Read,
            allow_process: true,
            allow_pty: true,
            workspace: Some(WorkspaceRequest {
                requested_root: r"C:\untrusted\workspace".to_owned(),
            }),
            scratch_disposition: ScratchDisposition::DeleteOnCleanCloseRetainOtherwise,
            catalog_generation: generation(7),
            activated_tools: vec![tool_id("main")],
            destinations: vec![DestinationGrant::parse("audit.example:443").unwrap()],
            environment: EnvironmentPolicy { base: Vec::new() },
            resource_limits: Some(limits()),
            audit_policy: AuditPolicy {
                arguments: ArgumentAuditMode::CountOnly,
            },
        }
        .validate(&catalog, &FixtureWorkspaceResolver)
        .unwrap()
    }

    fn context(policy: &ValidatedExecutionPolicy) -> AuditContext {
        AuditContext::new(
            SessionId::parse("session-01").unwrap(),
            ScratchId::parse("scratch-01").unwrap(),
            BackendIdentity::new("windows-lpac", "1").unwrap(),
            PROTOCOL_V1,
            EnforcementState::Enforced,
            policy,
        )
        .unwrap()
    }

    #[test]
    fn policy_aware_context_validation_compares_every_derived_policy_field() {
        let policy = policy();
        let context = context(&policy);
        context.validate_against_policy(&policy).unwrap();

        let mut mismatches = Vec::new();

        let mut access = context.clone();
        access.access = ExecutionAccess::Workspace;
        access.workspace.as_mut().unwrap().writable = true;
        mismatches.push(("access", access));

        let mut digest = context.clone();
        digest.policy_digest = PolicyDigest::from_sha256(Sha256Digest::hash(b"different-policy"));
        mismatches.push(("policy digest", digest));

        let mut workspace = context.clone();
        workspace.workspace.as_mut().unwrap().canonical_root = "C:/other".to_owned();
        mismatches.push(("workspace", workspace));

        let mut catalog_generation = context.clone();
        catalog_generation.catalog_generation = generation(8);
        mismatches.push(("catalog generation", catalog_generation));

        let mut tools = context.clone();
        tools.tools.clear();
        mismatches.push(("tools", tools));

        let mut destinations = context.clone();
        destinations.destinations.clear();
        mismatches.push(("destinations", destinations));

        let mut resource_limits = context;
        resource_limits
            .resource_limits
            .as_mut()
            .unwrap()
            .memory_bytes += 1;
        mismatches.push(("resource limits", resource_limits));

        for (field, mismatched) in mismatches {
            assert!(
                mismatched.validate_against_policy(&policy).is_err(),
                "accepted mismatched {field}"
            );
        }
    }
}
