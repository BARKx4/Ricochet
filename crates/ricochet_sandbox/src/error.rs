use std::fmt;

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize};
use thiserror::Error;

use crate::destination::DestinationGrant;
use crate::identity::{BackendFeatureId, BackendIdentity, SessionId, ToolId};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
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
#[serde(rename_all = "snake_case")]
pub enum SandboxPhase {
    Setup,
    Launch,
    Runtime,
    Shutdown,
    Protocol,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
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
#[serde(rename_all = "snake_case")]
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
#[serde(rename_all = "snake_case")]
pub enum ResourceLimitKind {
    DescendantProcesses,
    MemoryBytes,
    CpuTime,
    WallTime,
    OpenDescriptorsOrHandles,
    CapturedOutput,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(
    deny_unknown_fields,
    tag = "type",
    content = "body",
    rename_all = "snake_case"
)]
pub enum TerminationReason {
    CancelledByHost,
    TimedOut,
    ToolRevoked,
    BrokerShutdown,
    PolicyEnforcement,
    ResourceLimit(ResourceLimitKind),
    SessionClosed,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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

    fn is_empty(&self) -> bool {
        self.tool_id.is_none()
            && self.destination.is_none()
            && self.resource_limit.is_none()
            && self.protocol_version.is_none()
            && self.session_id.is_none()
            && self.backend_feature.is_none()
    }

    fn has_only_tool_id(&self) -> bool {
        self.tool_id.is_some()
            && self.destination.is_none()
            && self.resource_limit.is_none()
            && self.protocol_version.is_none()
            && self.session_id.is_none()
            && self.backend_feature.is_none()
    }

    fn has_only_destination(&self) -> bool {
        self.tool_id.is_none()
            && self.destination.is_some()
            && self.resource_limit.is_none()
            && self.protocol_version.is_none()
            && self.session_id.is_none()
            && self.backend_feature.is_none()
    }

    fn has_only_resource_limit(&self) -> bool {
        self.tool_id.is_none()
            && self.destination.is_none()
            && self.resource_limit.is_some()
            && self.protocol_version.is_none()
            && self.session_id.is_none()
            && self.backend_feature.is_none()
    }

    fn is_termination(&self) -> bool {
        self.tool_id.is_none()
            && self.destination.is_none()
            && self.protocol_version.is_none()
            && self.session_id.is_some()
            && self.backend_feature.is_none()
    }
}

impl Default for DiagnosticMetadata {
    fn default() -> Self {
        Self::empty()
    }
}

impl fmt::Debug for DiagnosticMetadata {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DiagnosticMetadata")
            .field("tool_id", &self.tool_id)
            .field("destination_present", &self.destination.is_some())
            .field("resource_limit", &self.resource_limit)
            .field("protocol_version", &self.protocol_version)
            .field("session_id", &self.session_id)
            .field("backend_feature", &self.backend_feature)
            .finish()
    }
}

#[derive(Clone, Error, Serialize)]
#[error("{message}")]
pub struct SandboxError {
    code: SandboxErrorCode,
    phase: SandboxPhase,
    backend: Option<BackendIdentity>,
    failed_guarantee: Option<FailedGuarantee>,
    message: String,
    remediation: Option<Remediation>,
    metadata: DiagnosticMetadata,
    #[serde(skip)]
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
            Some(Remediation::RetryAfterBrokerRestart),
            metadata,
        )
    }

    pub fn with_native_cause(mut self, cause: impl Into<String>) -> Self {
        self.native_cause = Some(cause.into());
        self
    }

    pub fn code(&self) -> SandboxErrorCode {
        self.code
    }

    pub fn kind(&self) -> &'static str {
        kind_for(self.code)
    }

    pub fn phase(&self) -> SandboxPhase {
        self.phase
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn remediation(&self) -> Option<Remediation> {
        self.remediation
    }

    pub fn metadata(&self) -> &DiagnosticMetadata {
        &self.metadata
    }

    #[allow(clippy::result_large_err)]
    pub fn validate(&self) -> Result<(), SandboxError> {
        let fixed_fields_match = self.message == message_for(self.code)
            && typed_denial_shape_valid(self.code, self.failed_guarantee, self.remediation)
            && match self.code {
                SandboxErrorCode::SandboxUnavailable => self.phase == SandboxPhase::Setup,
                SandboxErrorCode::SandboxPolicyError => {
                    self.phase == SandboxPhase::Setup && self.backend.is_none()
                }
                SandboxErrorCode::ToolNotApproved => {
                    self.phase == SandboxPhase::Launch
                        && self.backend.is_none()
                        && self.metadata.has_only_tool_id()
                }
                SandboxErrorCode::ToolFingerprintMismatch => {
                    self.phase == SandboxPhase::Launch
                        && self.backend.is_none()
                        && self.metadata.has_only_tool_id()
                }
                SandboxErrorCode::NetworkDenied => {
                    self.phase == SandboxPhase::Runtime
                        && self.backend.is_none()
                        && self.metadata.has_only_destination()
                }
                SandboxErrorCode::ResourceLimitExceeded => {
                    self.phase == SandboxPhase::Runtime
                        && self.backend.is_none()
                        && self.metadata.has_only_resource_limit()
                }
                SandboxErrorCode::SandboxLaunchError => {
                    self.phase == SandboxPhase::Launch
                        && self.backend.is_some()
                        && self.metadata.is_empty()
                }
                SandboxErrorCode::SandboxTerminated => {
                    self.phase == SandboxPhase::Shutdown
                        && self.backend.is_none()
                        && self.metadata.is_termination()
                }
                SandboxErrorCode::BrokerProtocolError => {
                    self.phase == SandboxPhase::Protocol && self.backend.is_none()
                }
            };

        if fixed_fields_match {
            Ok(())
        } else {
            Err(Self::policy(
                FailedGuarantee::PolicyValidity,
                DiagnosticMetadata::empty(),
            ))
        }
    }

    fn new(
        code: SandboxErrorCode,
        phase: SandboxPhase,
        backend: Option<BackendIdentity>,
        failed_guarantee: Option<FailedGuarantee>,
        remediation: Option<Remediation>,
        metadata: DiagnosticMetadata,
    ) -> Self {
        Self {
            code,
            phase,
            backend,
            failed_guarantee,
            message: message_for(code).to_owned(),
            remediation,
            metadata,
            native_cause: None,
        }
    }
}

pub(crate) fn typed_denial_shape_valid(
    code: SandboxErrorCode,
    guarantee: Option<FailedGuarantee>,
    remediation: Option<Remediation>,
) -> bool {
    match code {
        SandboxErrorCode::SandboxUnavailable => guarantee.is_some() && remediation.is_some(),
        SandboxErrorCode::SandboxPolicyError => guarantee.is_some() && remediation.is_none(),
        SandboxErrorCode::ToolNotApproved => {
            guarantee == Some(FailedGuarantee::ToolApproval)
                && remediation == Some(Remediation::ApproveTool)
        }
        SandboxErrorCode::ToolFingerprintMismatch => {
            guarantee == Some(FailedGuarantee::ToolFingerprint)
                && remediation == Some(Remediation::RefreshToolFingerprint)
        }
        SandboxErrorCode::NetworkDenied => {
            guarantee == Some(FailedGuarantee::DestinationGrant)
                && remediation == Some(Remediation::AddDestinationGrant)
        }
        SandboxErrorCode::ResourceLimitExceeded => {
            guarantee == Some(FailedGuarantee::ResourceCeiling)
                && remediation == Some(Remediation::LowerRequestedLimit)
        }
        SandboxErrorCode::SandboxLaunchError => {
            guarantee.is_some() && remediation == Some(Remediation::EnableBackendPrerequisite)
        }
        SandboxErrorCode::SandboxTerminated => guarantee.is_none() && remediation.is_none(),
        SandboxErrorCode::BrokerProtocolError => {
            guarantee == Some(FailedGuarantee::ProtocolAuthenticity)
                && remediation == Some(Remediation::RetryAfterBrokerRestart)
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

impl<'de> Deserialize<'de> for SandboxError {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct WireSandboxError {
            code: SandboxErrorCode,
            phase: SandboxPhase,
            backend: Option<BackendIdentity>,
            failed_guarantee: Option<FailedGuarantee>,
            message: String,
            remediation: Option<Remediation>,
            metadata: DiagnosticMetadata,
        }

        let wire = WireSandboxError::deserialize(deserializer)?;
        let error = Self {
            code: wire.code,
            phase: wire.phase,
            backend: wire.backend,
            failed_guarantee: wire.failed_guarantee,
            message: wire.message,
            remediation: wire.remediation,
            metadata: wire.metadata,
            native_cause: None,
        };
        error.validate().map_err(D::Error::custom)?;
        Ok(error)
    }
}

const fn kind_for(code: SandboxErrorCode) -> &'static str {
    match code {
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

const fn message_for(code: SandboxErrorCode) -> &'static str {
    match code {
        SandboxErrorCode::SandboxUnavailable => "sandbox backend is unavailable",
        SandboxErrorCode::SandboxPolicyError => "requested execution policy is invalid",
        SandboxErrorCode::ToolNotApproved => "tool is not approved",
        SandboxErrorCode::ToolFingerprintMismatch => {
            "tool fingerprint does not match the approved catalog"
        }
        SandboxErrorCode::NetworkDenied => "network destination is not granted",
        SandboxErrorCode::ResourceLimitExceeded => "sandbox resource limit was exceeded",
        SandboxErrorCode::SandboxLaunchError => "native sandbox launch failed",
        SandboxErrorCode::SandboxTerminated => "sandbox session was terminated",
        SandboxErrorCode::BrokerProtocolError => "broker protocol validation failed",
    }
}
