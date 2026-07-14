use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize};

use crate::catalog::PublicToolRecord;
use crate::destination::DestinationGrant;
use crate::error::{
    DiagnosticMetadata, FailedGuarantee, Remediation, ResourceLimitKind, SandboxError,
    SandboxErrorCode, TerminationReason,
};
use crate::identity::{
    BackendIdentity, CatalogGeneration, PolicyDigest, ProcessId, ProcessTreeId, PtyId, ScratchId,
    SessionId, ToolId, UnixMillis,
};
use crate::lifecycle::SessionState;
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

        Ok(Self {
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
        })
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
        if compatible_access_enforcement(self.access, self.enforcement) {
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
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
}
