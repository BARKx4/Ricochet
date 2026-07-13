use std::fmt;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::destination::DestinationGrant;
use crate::identity::{BackendFeatureId, BackendIdentity, SessionId, ToolId};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SandboxErrorCode {
    SandboxUnavailable,
    SandboxPolicyError,
    ToolNotApproved,
    ToolFingerprintMismatch,
    NetworkDenied,
    ResourceLimitExceeded,
    SandboxLaunchError,
    SandboxTerminated,
    BrokerProtocolError,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SandboxPhase {
    Setup,
    Launch,
    Runtime,
    Shutdown,
    Protocol,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FailedGuarantee {
    BrokerAvailability,
    PolicyValidity,
    ToolApproval,
    ToolFingerprint,
    DestinationGrant,
    ResourceCeiling,
    NativeLaunch,
    SessionOwnership,
    ProtocolAuthenticity,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Remediation {
    StartOrInstallBroker,
    ApproveTool,
    RefreshToolFingerprint,
    AddDestinationGrant,
    LowerRequestedLimit,
    EnableBackendPrerequisite,
    RetryAfterBrokerRestart,
    InspectSandboxDoctor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ResourceLimitKind {
    DescendantProcesses,
    MemoryBytes,
    CpuTime,
    WallTime,
    OpenDescriptorsOrHandles,
    CapturedOutput,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TerminationReason {
    CancelledByHost,
    TimedOut,
    ToolRevoked,
    BrokerShutdown,
    PolicyEnforcement,
    ResourceLimit(ResourceLimitKind),
    SessionClosed,
}

#[derive(Debug)]
pub struct DiagnosticMetadata {
    tool_id: Option<ToolId>,
    destination: Option<DestinationGrant>,
    resource_limit: Option<ResourceLimitKind>,
    protocol_version: Option<u16>,
    session_id: Option<SessionId>,
    backend_feature: Option<BackendFeatureId>,
}

impl DiagnosticMetadata {
    pub fn empty() -> Self {
        Self {
            tool_id: None,
            destination: None,
            resource_limit: None,
            protocol_version: None,
            session_id: None,
            backend_feature: None,
        }
    }

    pub fn with_tool_id(mut self, value: ToolId) -> Self {
        self.tool_id = Some(value);
        self
    }

    pub fn with_destination(mut self, value: DestinationGrant) -> Self {
        self.destination = Some(value);
        self
    }

    pub fn with_resource_limit(mut self, value: ResourceLimitKind) -> Self {
        self.resource_limit = Some(value);
        self
    }

    pub fn with_protocol_version(mut self, value: u16) -> Self {
        self.protocol_version = Some(value);
        self
    }

    pub fn with_session_id(mut self, value: SessionId) -> Self {
        self.session_id = Some(value);
        self
    }

    pub fn with_backend_feature(mut self, value: BackendFeatureId) -> Self {
        self.backend_feature = Some(value);
        self
    }
}

impl Default for DiagnosticMetadata {
    fn default() -> Self {
        Self::empty()
    }
}

#[derive(Error)]
#[error("{message}")]
pub struct SandboxError {
    code: SandboxErrorCode,
    phase: SandboxPhase,
    backend: Option<BackendIdentity>,
    failed_guarantee: Option<FailedGuarantee>,
    message: String,
    remediation: Option<Remediation>,
    metadata: DiagnosticMetadata,
    native_cause: Option<String>,
}

impl SandboxError {
    pub fn unavailable(
        backend: Option<BackendIdentity>,
        guarantee: FailedGuarantee,
        remediation: Remediation,
        metadata: DiagnosticMetadata,
    ) -> Self {
        Self::new(
            SandboxErrorCode::SandboxUnavailable,
            SandboxPhase::Setup,
            backend,
            Some(guarantee),
            "sandbox backend is unavailable",
            Some(remediation),
            metadata,
        )
    }

    pub fn policy(guarantee: FailedGuarantee, metadata: DiagnosticMetadata) -> Self {
        Self::new(
            SandboxErrorCode::SandboxPolicyError,
            SandboxPhase::Setup,
            None,
            Some(guarantee),
            "sandbox policy is invalid",
            None,
            metadata,
        )
    }

    pub fn tool_not_approved(tool_id: ToolId) -> Self {
        Self::new(
            SandboxErrorCode::ToolNotApproved,
            SandboxPhase::Launch,
            None,
            Some(FailedGuarantee::ToolApproval),
            "tool is not approved",
            Some(Remediation::ApproveTool),
            DiagnosticMetadata::empty().with_tool_id(tool_id),
        )
    }

    pub fn tool_fingerprint_mismatch(tool_id: ToolId) -> Self {
        Self::new(
            SandboxErrorCode::ToolFingerprintMismatch,
            SandboxPhase::Launch,
            None,
            Some(FailedGuarantee::ToolFingerprint),
            "tool fingerprint does not match the approved catalog",
            Some(Remediation::RefreshToolFingerprint),
            DiagnosticMetadata::empty().with_tool_id(tool_id),
        )
    }

    pub fn network_denied(destination: DestinationGrant) -> Self {
        Self::new(
            SandboxErrorCode::NetworkDenied,
            SandboxPhase::Runtime,
            None,
            Some(FailedGuarantee::DestinationGrant),
            "network destination is not granted",
            Some(Remediation::AddDestinationGrant),
            DiagnosticMetadata::empty().with_destination(destination),
        )
    }

    pub fn resource_limit(limit: ResourceLimitKind) -> Self {
        Self::new(
            SandboxErrorCode::ResourceLimitExceeded,
            SandboxPhase::Runtime,
            None,
            Some(FailedGuarantee::ResourceCeiling),
            "sandbox resource limit was exceeded",
            Some(Remediation::LowerRequestedLimit),
            DiagnosticMetadata::empty().with_resource_limit(limit),
        )
    }

    pub fn launch(backend: BackendIdentity, guarantee: FailedGuarantee) -> Self {
        Self::new(
            SandboxErrorCode::SandboxLaunchError,
            SandboxPhase::Launch,
            Some(backend),
            Some(guarantee),
            "native sandbox launch failed",
            Some(Remediation::EnableBackendPrerequisite),
            DiagnosticMetadata::empty(),
        )
    }

    pub fn terminated(reason: TerminationReason, session_id: SessionId) -> Self {
        let metadata = match reason {
            TerminationReason::ResourceLimit(limit) => DiagnosticMetadata::empty()
                .with_session_id(session_id)
                .with_resource_limit(limit),
            _ => DiagnosticMetadata::empty().with_session_id(session_id),
        };
        Self::new(
            SandboxErrorCode::SandboxTerminated,
            SandboxPhase::Shutdown,
            None,
            None,
            "sandbox session was terminated",
            None,
            metadata,
        )
    }

    pub fn protocol(metadata: DiagnosticMetadata) -> Self {
        Self::new(
            SandboxErrorCode::BrokerProtocolError,
            SandboxPhase::Protocol,
            None,
            Some(FailedGuarantee::ProtocolAuthenticity),
            "broker protocol validation failed",
            Some(Remediation::RetryAfterBrokerRestart),
            metadata,
        )
    }

    pub fn with_native_cause(mut self, cause: impl Into<String>) -> Self {
        self.native_cause = Some(cause.into());
        self
    }

    pub fn kind(&self) -> &'static str {
        match self.code {
            SandboxErrorCode::SandboxUnavailable => "SandboxUnavailable",
            SandboxErrorCode::SandboxPolicyError => "SandboxPolicyError",
            SandboxErrorCode::ToolNotApproved => "ToolNotApproved",
            SandboxErrorCode::ToolFingerprintMismatch => "ToolFingerprintMismatch",
            SandboxErrorCode::NetworkDenied => "NetworkDenied",
            SandboxErrorCode::ResourceLimitExceeded => "ResourceLimitExceeded",
            SandboxErrorCode::SandboxLaunchError => "SandboxLaunchError",
            SandboxErrorCode::SandboxTerminated => "SandboxTerminated",
            SandboxErrorCode::BrokerProtocolError => "BrokerProtocolError",
        }
    }

    fn new(
        code: SandboxErrorCode,
        phase: SandboxPhase,
        backend: Option<BackendIdentity>,
        failed_guarantee: Option<FailedGuarantee>,
        message: impl Into<String>,
        remediation: Option<Remediation>,
        metadata: DiagnosticMetadata,
    ) -> Self {
        Self {
            code,
            phase,
            backend,
            failed_guarantee,
            message: message.into(),
            remediation,
            metadata,
            native_cause: None,
        }
    }
}

impl fmt::Debug for SandboxError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SandboxError")
            .field("code", &self.code)
            .field("phase", &self.phase)
            .field("backend", &self.backend)
            .field("failed_guarantee", &self.failed_guarantee)
            .field("message", &self.message)
            .field("remediation", &self.remediation)
            .field("metadata", &self.metadata)
            .finish_non_exhaustive()
    }
}
