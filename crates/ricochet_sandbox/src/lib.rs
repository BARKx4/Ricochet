//! Stable sandbox values can only be assembled through their typed constructors.
//!
//! ```compile_fail
//! use ricochet_sandbox::{DiagnosticMetadata, SandboxError};
//!
//! let _ = SandboxError {
//!     message: "arbitrary native failure".to_owned(),
//!     metadata: DiagnosticMetadata::default(),
//!     ..todo!()
//! };
//! ```
//!
//! ```compile_fail
//! use ricochet_sandbox::AuditContext;
//!
//! let _ = AuditContext {
//!     broker_protocol: 999,
//!     ..todo!()
//! };
//! ```
//!
//! ```compile_fail
//! use ricochet_sandbox::{AuditRecord, UnixMillis};
//!
//! let _ = AuditRecord {
//!     at: UnixMillis::new(0),
//!     ..todo!()
//! };
//! ```

mod audit;
mod backend;
mod catalog;
mod destination;
mod error;
mod exact_serde;
mod identity;
mod lifecycle;
mod mock;
mod policy;
mod protocol;
mod version;

pub use audit::{
    AuditContext, AuditEventKind, AuditRecord, AuditWorkspace, EnforcementState,
    ExecutionAuditIdentity, ExecutionInstanceId,
};
pub use backend::{
    BackendCapabilities, BackendSelfTest, BackendSelfTestFailure, SandboxBackend, SandboxSession,
    SessionCommand,
};
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
pub use lifecycle::{SessionLifecycle, SessionState};
pub use mock::{MockBackendConfig, MockFailurePoint, MockSandboxBackend};
pub use policy::{
    resolve_legacy_access, ArgumentAuditMode, AuditPolicy, EffectiveEnvironment, EnvironmentPolicy,
    EnvironmentVariable, ExecutionAccess, ExecutionGrant, ExecutionPolicyRequest, ExecutionSurface,
    GrantSet, LaunchEnvironment, ResourceLimits, ScratchDisposition, ValidatedExecutionPolicy,
    WorkspaceIdentity, WorkspaceIdentityResolver, WorkspaceRequest,
};
pub use protocol::{
    chunk_wire_bytes, AuthenticatedChannelContext, AuthenticatedCodec, BrokerEvent, BrokerRequest,
    BrokerRequestKind, BrokerResponse, CancelSessionRequest, ConfirmedExecutionCapabilities,
    ConnectionNonce, CreateSessionRequest, EndpointRole, ExecutableRef, HandshakeExpectation,
    HandshakeRequest, HandshakeResponse, OperationError, OperationErrorCode, OperationSubject,
    OutstandingRequest, PeerContextId, ProcessLaunchRequest, ProcessReadRequest,
    ProcessReadSnapshot, ProcessRequest, ProcessSnapshot, ProcessStatus, ProcessWriteRequest,
    ProtocolEnvelope, ProtocolKey, ProtocolMessage, PtyLaunchRequest, PtyReadRequest,
    PtyReadSnapshot, PtyRequest, PtyResizeRequest, PtySnapshot, PtyStatus, PtyWriteRequest,
    ResponseCorrelation, SessionRequest, TerminationNotice, WireBytes,
};
pub use version::{
    CATALOG_SCHEMA_V1, FRAME_MAC_BYTES, MAX_FRAME_BYTES, MAX_IO_CHUNK_BYTES, POLICY_SCHEMA_V1,
    PROTOCOL_V1,
};

pub fn crate_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
