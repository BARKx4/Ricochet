mod catalog;
mod destination;
mod error;
mod identity;
mod policy;
mod version;

pub use catalog::{
    ApprovalActor, Architecture, ArtifactKind, CatalogPathNormalizer, CatalogRecord,
    CatalogSnapshot, HashedArtifact, OperatingSystem, PlatformId, PreparedCatalogClosure,
    PreparedTool, PublicCatalogSnapshot, PublicToolRecord, ReplacementLineage, ToolReference,
    TransportAdapter, ValidatedCatalogSnapshot,
};
pub use destination::DestinationGrant;
pub use error::{
    DiagnosticMetadata, FailedGuarantee, Remediation, ResourceLimitKind, SandboxError,
    SandboxErrorCode, SandboxPhase, TerminationReason,
};
pub use identity::{
    BackendFeatureId, BackendIdentity, CatalogGeneration, PolicyDigest, ProcessId, ProcessTreeId,
    PtyId, RequestId, ScratchId, SessionId, Sha256Digest, ToolId, UnixMillis,
};
pub use policy::{
    resolve_legacy_access, ArgumentAuditMode, AuditPolicy, EffectiveEnvironment, EnvironmentPolicy,
    EnvironmentVariable, ExecutionAccess, ExecutionGrant, ExecutionPolicyRequest, ExecutionSurface,
    GrantSet, LaunchEnvironment, ResourceLimits, ScratchDisposition, ValidatedExecutionPolicy,
    WorkspaceIdentity, WorkspaceIdentityResolver, WorkspaceRequest,
};
pub use version::{
    CATALOG_SCHEMA_V1, FRAME_MAC_BYTES, MAX_FRAME_BYTES, MAX_IO_CHUNK_BYTES, POLICY_SCHEMA_V1,
    PROTOCOL_V1,
};

pub fn crate_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
