use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::catalog::{
    ArtifactKind, PlatformId, PreparedCatalogClosure, PreparedTool, TransportAdapter,
    ValidatedCatalogSnapshot,
};
use crate::destination::DestinationGrant;
use crate::error::{DiagnosticMetadata, FailedGuarantee, ResourceLimitKind, SandboxError};
use crate::identity::{CatalogGeneration, PolicyDigest, Sha256Digest, ToolId};
use crate::version::POLICY_SCHEMA_V1;

const AUDIT_DIGEST_PROJECTION_V1: &str = "ricochet.sandbox.policy.audit.v1";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub enum ExecutionAccess {
    Read,
    Workspace,
    Full,
}

impl ExecutionAccess {
    const fn security_rank(self) -> u8 {
        match self {
            Self::Read => 0,
            Self::Workspace => 1,
            Self::Full => 2,
        }
    }

    const fn is_constrained(self) -> bool {
        matches!(self, Self::Read | Self::Workspace)
    }
}

impl PartialOrd for ExecutionAccess {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ExecutionAccess {
    fn cmp(&self, other: &Self) -> Ordering {
        self.security_rank().cmp(&other.security_rank())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub enum ExecutionSurface {
    Process,
    Pty,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub enum ScratchDisposition {
    DeleteOnCleanCloseRetainOtherwise,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub enum ArgumentAuditMode {
    CountOnly,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuditPolicy {
    pub arguments: ArgumentAuditMode,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceRequest {
    pub requested_root: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceIdentity {
    pub requested_root: String,
    pub canonical_root: String,
    pub native_object_identity: String,
}

pub trait WorkspaceIdentityResolver {
    #[allow(clippy::result_large_err)]
    fn resolve(&self, request: &WorkspaceRequest) -> Result<WorkspaceIdentity, SandboxError>;
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnvironmentVariable {
    pub name: String,
    pub value: String,
}

impl fmt::Debug for EnvironmentVariable {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EnvironmentVariable")
            .field("name", &self.name)
            .field("value", &"[REDACTED]")
            .finish()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnvironmentPolicy {
    pub base: Vec<EnvironmentVariable>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LaunchEnvironment {
    pub clear_environment: bool,
    pub entries: Vec<EnvironmentVariable>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EffectiveEnvironment {
    pub inherit_ambient: bool,
    pub entries: Vec<EnvironmentVariable>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceLimits {
    pub descendant_processes: u32,
    pub memory_bytes: u64,
    pub cpu_time_ms: u64,
    pub wall_time_ms: u64,
    pub open_descriptors_or_handles: u32,
    pub captured_output_bytes: u64,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionPolicyRequest {
    pub schema_version: u16,
    pub access: ExecutionAccess,
    pub allow_process: bool,
    pub allow_pty: bool,
    pub workspace: Option<WorkspaceRequest>,
    pub scratch_disposition: ScratchDisposition,
    pub catalog_generation: CatalogGeneration,
    pub activated_tools: Vec<ToolId>,
    pub destinations: Vec<DestinationGrant>,
    pub environment: EnvironmentPolicy,
    pub resource_limits: Option<ResourceLimits>,
    pub audit_policy: AuditPolicy,
}

pub struct ValidatedExecutionPolicy {
    schema_version: u16,
    access: ExecutionAccess,
    allow_process: bool,
    allow_pty: bool,
    workspace_identity: Option<WorkspaceIdentity>,
    scratch_disposition: ScratchDisposition,
    prepared_catalog: PreparedCatalogClosure,
    destinations: Vec<DestinationGrant>,
    environment: EnvironmentPolicy,
    resource_limits: Option<ResourceLimits>,
    audit_policy: AuditPolicy,
    audit_digest: PolicyDigest,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    deny_unknown_fields,
    bound(serialize = "T: Serialize", deserialize = "T: Ord + Deserialize<'de>")
)]
pub enum GrantSet<T> {
    Unrestricted,
    Only(BTreeSet<T>),
}

#[derive(PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionGrant {
    pub access: Option<ExecutionAccess>,
    pub allow_process: bool,
    pub allow_pty: bool,
    pub tools: GrantSet<ToolId>,
    pub destinations: GrantSet<DestinationGrant>,
}

pub fn resolve_legacy_access(
    explicit: Option<ExecutionAccess>,
    allow_process: bool,
    allow_pty: bool,
) -> Option<ExecutionAccess> {
    explicit.or_else(|| (allow_process || allow_pty).then_some(ExecutionAccess::Full))
}

impl ExecutionGrant {
    #[allow(clippy::result_large_err)]
    pub fn intersect(&self, requested: &Self) -> Result<Self, SandboxError> {
        Ok(Self {
            access: narrower_access(self.access, requested.access),
            allow_process: self.allow_process && requested.allow_process,
            allow_pty: self.allow_pty && requested.allow_pty,
            tools: intersect_grant_set(&self.tools, &requested.tools),
            destinations: intersect_destination_grant_set(
                &self.destinations,
                &requested.destinations,
            )?,
        })
    }
}

impl ResourceLimits {
    #[allow(clippy::result_large_err)]
    pub fn validate(&self) -> Result<(), SandboxError> {
        if self.descendant_processes == 0 {
            return Err(resource_policy_error(
                ResourceLimitKind::DescendantProcesses,
            ));
        }
        if self.memory_bytes == 0 {
            return Err(resource_policy_error(ResourceLimitKind::MemoryBytes));
        }
        if self.cpu_time_ms == 0 {
            return Err(resource_policy_error(ResourceLimitKind::CpuTime));
        }
        if self.wall_time_ms == 0 {
            return Err(resource_policy_error(ResourceLimitKind::WallTime));
        }
        if self.open_descriptors_or_handles == 0 {
            return Err(resource_policy_error(
                ResourceLimitKind::OpenDescriptorsOrHandles,
            ));
        }
        if self.captured_output_bytes == 0 {
            return Err(resource_policy_error(ResourceLimitKind::CapturedOutput));
        }
        Ok(())
    }

    #[allow(clippy::result_large_err)]
    pub fn ensure_not_above(&self, ceiling: &Self) -> Result<(), SandboxError> {
        if self.descendant_processes > ceiling.descendant_processes {
            return Err(resource_policy_error(
                ResourceLimitKind::DescendantProcesses,
            ));
        }
        if self.memory_bytes > ceiling.memory_bytes {
            return Err(resource_policy_error(ResourceLimitKind::MemoryBytes));
        }
        if self.cpu_time_ms > ceiling.cpu_time_ms {
            return Err(resource_policy_error(ResourceLimitKind::CpuTime));
        }
        if self.wall_time_ms > ceiling.wall_time_ms {
            return Err(resource_policy_error(ResourceLimitKind::WallTime));
        }
        if self.open_descriptors_or_handles > ceiling.open_descriptors_or_handles {
            return Err(resource_policy_error(
                ResourceLimitKind::OpenDescriptorsOrHandles,
            ));
        }
        if self.captured_output_bytes > ceiling.captured_output_bytes {
            return Err(resource_policy_error(ResourceLimitKind::CapturedOutput));
        }
        Ok(())
    }
}

impl ExecutionPolicyRequest {
    #[allow(clippy::result_large_err)]
    pub fn validate(
        self,
        catalog: &ValidatedCatalogSnapshot,
        workspace_resolver: &dyn WorkspaceIdentityResolver,
    ) -> Result<ValidatedExecutionPolicy, SandboxError> {
        if self.schema_version != POLICY_SCHEMA_V1
            || (!self.allow_process && !self.allow_pty)
            || self.catalog_generation != catalog.generation()
        {
            return Err(policy_error());
        }

        let constrained = self.access.is_constrained();
        if constrained && (self.workspace.is_none() || self.resource_limits.is_none()) {
            return Err(policy_error());
        }
        if !constrained && (!self.activated_tools.is_empty() || !self.destinations.is_empty()) {
            return Err(policy_error());
        }

        if let Some(limits) = &self.resource_limits {
            limits.validate()?;
        }

        let activated_tools = collect_unique(self.activated_tools)?;
        let destinations = collect_unique(self.destinations)?;
        let environment = validate_environment_policy(self.environment, constrained)?;

        let workspace_identity = match self.workspace {
            Some(request) => {
                let identity = workspace_resolver.resolve(&request)?;
                validate_workspace_identity(&identity)?;
                Some(identity)
            }
            None => None,
        };

        let prepared_catalog =
            catalog.activate(&activated_tools.into_iter().collect::<Vec<_>>())?;
        let destinations = destinations.into_iter().collect::<Vec<_>>();

        let audit_digest = compute_audit_digest(AuditDigestInput {
            projection_version: AUDIT_DIGEST_PROJECTION_V1,
            policy_schema_version: self.schema_version,
            access: self.access,
            allow_process: self.allow_process,
            allow_pty: self.allow_pty,
            workspace_identity: workspace_identity.as_ref(),
            scratch_disposition: self.scratch_disposition,
            catalog: audit_catalog(&prepared_catalog),
            destinations: &destinations,
            environment: AuditEnvironment {
                count: environment.base.len(),
                names: environment
                    .base
                    .iter()
                    .map(|entry| entry.name.as_str())
                    .collect(),
            },
            resource_limits: self.resource_limits.as_ref(),
            audit_policy: &self.audit_policy,
        });

        Ok(ValidatedExecutionPolicy {
            schema_version: self.schema_version,
            access: self.access,
            allow_process: self.allow_process,
            allow_pty: self.allow_pty,
            workspace_identity,
            scratch_disposition: self.scratch_disposition,
            prepared_catalog,
            destinations,
            environment,
            resource_limits: self.resource_limits,
            audit_policy: self.audit_policy,
            audit_digest,
        })
    }
}

impl ValidatedExecutionPolicy {
    pub fn access(&self) -> ExecutionAccess {
        self.access
    }

    pub fn allows(&self, surface: ExecutionSurface) -> bool {
        match surface {
            ExecutionSurface::Process => self.allow_process,
            ExecutionSurface::Pty => self.allow_pty,
        }
    }

    pub fn workspace_identity(&self) -> Option<&WorkspaceIdentity> {
        self.workspace_identity.as_ref()
    }

    pub fn prepared_catalog(&self) -> &PreparedCatalogClosure {
        &self.prepared_catalog
    }

    pub fn destinations(&self) -> &[DestinationGrant] {
        &self.destinations
    }

    pub fn environment_policy(&self) -> &EnvironmentPolicy {
        &self.environment
    }

    pub fn resource_limits(&self) -> Option<&ResourceLimits> {
        self.resource_limits.as_ref()
    }

    pub fn scratch_disposition(&self) -> ScratchDisposition {
        self.scratch_disposition
    }

    pub fn audit_policy(&self) -> &AuditPolicy {
        &self.audit_policy
    }

    #[allow(clippy::result_large_err)]
    pub fn resolve_launch_environment(
        &self,
        launch: &LaunchEnvironment,
    ) -> Result<EffectiveEnvironment, SandboxError> {
        let constrained = self.access.is_constrained();
        let launch_entries = validate_environment_entries(launch.entries.clone(), constrained)?;
        let mut entries = self
            .environment
            .base
            .iter()
            .cloned()
            .map(|entry| (normalized_environment_name(&entry.name), entry))
            .collect::<BTreeMap<_, _>>();
        entries.extend(launch_entries);

        Ok(EffectiveEnvironment {
            inherit_ambient: !constrained && !launch.clear_environment,
            entries: entries.into_values().collect(),
        })
    }

    pub fn audit_digest(&self) -> PolicyDigest {
        debug_assert_eq!(self.schema_version, POLICY_SCHEMA_V1);
        self.audit_digest
    }
}

fn narrower_access(
    first: Option<ExecutionAccess>,
    second: Option<ExecutionAccess>,
) -> Option<ExecutionAccess> {
    match (first, second) {
        (Some(first), Some(second)) => Some(first.min(second)),
        (Some(access), None) | (None, Some(access)) => Some(access),
        (None, None) => None,
    }
}

fn intersect_grant_set<T: Ord + Clone>(first: &GrantSet<T>, second: &GrantSet<T>) -> GrantSet<T> {
    match (first, second) {
        (GrantSet::Unrestricted, other) | (other, GrantSet::Unrestricted) => other.clone(),
        (GrantSet::Only(first), GrantSet::Only(second)) => {
            GrantSet::Only(first.intersection(second).cloned().collect())
        }
    }
}

#[allow(clippy::result_large_err)]
fn intersect_destination_grant_set(
    first: &GrantSet<DestinationGrant>,
    second: &GrantSet<DestinationGrant>,
) -> Result<GrantSet<DestinationGrant>, SandboxError> {
    let selected = match (first, second) {
        (GrantSet::Unrestricted, other) | (other, GrantSet::Unrestricted) => match other {
            GrantSet::Unrestricted => return Ok(GrantSet::Unrestricted),
            GrantSet::Only(values) => values.iter().collect::<Vec<_>>(),
        },
        (GrantSet::Only(first), GrantSet::Only(second)) => {
            first.intersection(second).collect::<Vec<_>>()
        }
    };
    Ok(GrantSet::Only(
        selected
            .into_iter()
            .map(|grant| DestinationGrant::new(grant.host(), grant.port()))
            .collect::<Result<_, _>>()?,
    ))
}

#[allow(clippy::result_large_err)]
fn collect_unique<T: Ord>(values: Vec<T>) -> Result<BTreeSet<T>, SandboxError> {
    let value_count = values.len();
    let values = values.into_iter().collect::<BTreeSet<_>>();
    if values.len() != value_count {
        return Err(policy_error());
    }
    Ok(values)
}

#[allow(clippy::result_large_err)]
fn validate_workspace_identity(identity: &WorkspaceIdentity) -> Result<(), SandboxError> {
    if [
        &identity.requested_root,
        &identity.canonical_root,
        &identity.native_object_identity,
    ]
    .into_iter()
    .any(|value| value.is_empty() || value.contains('\0'))
    {
        return Err(policy_error());
    }
    Ok(())
}

#[allow(clippy::result_large_err)]
fn validate_environment_policy(
    policy: EnvironmentPolicy,
    constrained: bool,
) -> Result<EnvironmentPolicy, SandboxError> {
    Ok(EnvironmentPolicy {
        base: validate_environment_entries(policy.base, constrained)?
            .into_values()
            .collect(),
    })
}

#[allow(clippy::result_large_err)]
fn validate_environment_entries(
    entries: Vec<EnvironmentVariable>,
    constrained: bool,
) -> Result<BTreeMap<String, EnvironmentVariable>, SandboxError> {
    let mut validated = BTreeMap::new();
    for entry in entries {
        if !valid_environment_name(&entry.name)
            || entry.value.contains('\0')
            || (constrained && reserved_environment_name(&entry.name))
        {
            return Err(policy_error());
        }
        let normalized_name = normalized_environment_name(&entry.name);
        if validated.insert(normalized_name, entry).is_some() {
            return Err(policy_error());
        }
    }
    Ok(validated)
}

fn valid_environment_name(name: &str) -> bool {
    !name.is_empty() && !name.contains(['=', '\0'])
}

fn normalized_environment_name(name: &str) -> String {
    name.to_ascii_uppercase()
}

fn reserved_environment_name(name: &str) -> bool {
    let name = normalized_environment_name(name);
    matches!(
        name.as_str(),
        "PATH"
            | "HOME"
            | "USERPROFILE"
            | "TEMP"
            | "TMP"
            | "TMPDIR"
            | "RICOCHET_BROKER_ENDPOINT"
            | "RICOCHET_SANDBOX_SESSION"
    ) || name.starts_with("RICOCHET_SANDBOX_")
}

#[derive(Serialize)]
struct AuditDigestInput<'a> {
    projection_version: &'static str,
    policy_schema_version: u16,
    access: ExecutionAccess,
    allow_process: bool,
    allow_pty: bool,
    workspace_identity: Option<&'a WorkspaceIdentity>,
    scratch_disposition: ScratchDisposition,
    catalog: AuditCatalog,
    destinations: &'a [DestinationGrant],
    environment: AuditEnvironment<'a>,
    resource_limits: Option<&'a ResourceLimits>,
    audit_policy: &'a AuditPolicy,
}

#[derive(Serialize)]
struct AuditCatalog {
    generation: CatalogGeneration,
    platform: PlatformId,
    roots: Vec<ToolId>,
    tools: Vec<AuditTool>,
}

#[derive(Serialize)]
struct AuditTool {
    tool_id: ToolId,
    executable: AuditArtifact,
    helpers: Vec<AuditHelper>,
    non_system_libraries: Vec<AuditArtifact>,
    resources: Vec<AuditArtifact>,
    transport_adapter: Option<TransportAdapter>,
}

#[derive(Serialize)]
struct AuditHelper {
    tool_id: ToolId,
    executable_sha256: Sha256Digest,
}

#[derive(Serialize)]
struct AuditArtifact {
    logical_name: String,
    sha256: Sha256Digest,
    kind: ArtifactKind,
}

#[derive(Serialize)]
struct AuditEnvironment<'a> {
    count: usize,
    names: Vec<&'a str>,
}

fn compute_audit_digest(input: AuditDigestInput<'_>) -> PolicyDigest {
    let canonical_json =
        serde_json::to_vec(&input).expect("policy audit projection serialization is infallible");
    PolicyDigest::from_sha256(Sha256Digest::hash(&canonical_json))
}

fn audit_catalog(catalog: &PreparedCatalogClosure) -> AuditCatalog {
    AuditCatalog {
        generation: catalog.generation(),
        platform: *catalog.platform(),
        roots: catalog.roots().iter().cloned().collect(),
        tools: catalog
            .tools()
            .values()
            .map(|tool| audit_tool(catalog, tool))
            .collect(),
    }
}

fn audit_tool(catalog: &PreparedCatalogClosure, tool: &PreparedTool) -> AuditTool {
    let mut helpers = tool
        .helper_ids()
        .iter()
        .map(|helper_id| {
            let helper = &catalog.tools()[helper_id];
            AuditHelper {
                tool_id: helper_id.clone(),
                executable_sha256: helper.executable().sha256,
            }
        })
        .collect::<Vec<_>>();
    helpers.sort_by(|first, second| first.tool_id.cmp(&second.tool_id));

    AuditTool {
        tool_id: tool.tool_id().clone(),
        executable: audit_artifact(tool.executable()),
        helpers,
        non_system_libraries: sorted_audit_artifacts(tool.non_system_libraries()),
        resources: sorted_audit_artifacts(tool.resources()),
        transport_adapter: tool.transport_adapter(),
    }
}

fn sorted_audit_artifacts(artifacts: &[crate::catalog::HashedArtifact]) -> Vec<AuditArtifact> {
    let mut artifacts = artifacts.iter().map(audit_artifact).collect::<Vec<_>>();
    artifacts.sort_by(|first, second| {
        (&first.logical_name, first.kind, first.sha256).cmp(&(
            &second.logical_name,
            second.kind,
            second.sha256,
        ))
    });
    artifacts
}

fn audit_artifact(artifact: &crate::catalog::HashedArtifact) -> AuditArtifact {
    AuditArtifact {
        logical_name: artifact.logical_name.clone(),
        sha256: artifact.sha256,
        kind: artifact.kind,
    }
}

fn policy_error() -> SandboxError {
    SandboxError::policy(FailedGuarantee::PolicyValidity, DiagnosticMetadata::empty())
}

fn resource_policy_error(limit: ResourceLimitKind) -> SandboxError {
    SandboxError::policy(
        FailedGuarantee::ResourceCeiling,
        DiagnosticMetadata::empty().with_resource_limit(limit),
    )
}
