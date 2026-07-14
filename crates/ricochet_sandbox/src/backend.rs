#![allow(clippy::result_large_err)]

use crate::error::{FailedGuarantee, Remediation, SandboxError, TerminationReason};
use crate::identity::{BackendFeatureId, BackendIdentity, ScratchId, SessionId, ToolId};
use crate::lifecycle::SessionState;
use crate::policy::ValidatedExecutionPolicy;
use crate::protocol::{
    BrokerEvent, BrokerRequest, BrokerResponse, CancelSessionRequest,
    ConfirmedExecutionCapabilities, ProcessLaunchRequest, ProcessReadRequest, ProcessRequest,
    ProcessWriteRequest, PtyLaunchRequest, PtyReadRequest, PtyRequest, PtyResizeRequest,
    PtyWriteRequest, SessionRequest,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackendCapabilities {
    pub process: bool,
    pub pty: bool,
    pub filesystem_read: bool,
    pub filesystem_write: bool,
    pub executable_closure: bool,
    pub descendant_confinement: bool,
    pub destination_transport: bool,
    pub resource_limits: bool,
    pub scratch_isolation: bool,
}

impl BackendCapabilities {
    pub fn supports_complete_contract(&self) -> bool {
        self.process
            && self.pty
            && self.filesystem_read
            && self.filesystem_write
            && self.executable_closure
            && self.descendant_confinement
            && self.destination_transport
            && self.resource_limits
            && self.scratch_isolation
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackendSelfTest {
    pub identity: BackendIdentity,
    pub capabilities: BackendCapabilities,
    pub production_enforcement: bool,
    pub failures: Vec<BackendSelfTestFailure>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackendSelfTestFailure {
    pub feature: BackendFeatureId,
    pub guarantee: FailedGuarantee,
    pub remediation: Option<Remediation>,
}

pub enum SessionCommand {
    Close(SessionRequest),
    Cancel(CancelSessionRequest),
    ProcessStart(ProcessLaunchRequest),
    ProcessList(SessionRequest),
    ProcessDetail(ProcessRequest),
    ProcessRead(ProcessReadRequest),
    ProcessWrite(ProcessWriteRequest),
    ProcessCancel(ProcessRequest),
    ProcessRelease(ProcessRequest),
    PtyStart(PtyLaunchRequest),
    PtyList(SessionRequest),
    PtyDetail(PtyRequest),
    PtyRead(PtyReadRequest),
    PtyWrite(PtyWriteRequest),
    PtyResize(PtyResizeRequest),
    PtyStop(PtyRequest),
    PtyRelease(PtyRequest),
}

impl SessionCommand {
    pub(crate) fn session_id(&self) -> &SessionId {
        match self {
            Self::Close(request) | Self::ProcessList(request) | Self::PtyList(request) => {
                &request.session_id
            }
            Self::Cancel(request) => &request.session_id,
            Self::ProcessStart(request) => &request.session_id,
            Self::ProcessDetail(request)
            | Self::ProcessCancel(request)
            | Self::ProcessRelease(request) => &request.session_id,
            Self::ProcessRead(request) => &request.session_id,
            Self::ProcessWrite(request) => &request.session_id,
            Self::PtyStart(request) => &request.session_id,
            Self::PtyDetail(request) | Self::PtyStop(request) | Self::PtyRelease(request) => {
                &request.session_id
            }
            Self::PtyRead(request) => &request.session_id,
            Self::PtyWrite(request) => &request.session_id,
            Self::PtyResize(request) => &request.session_id,
        }
    }
}

impl TryFrom<BrokerRequest> for SessionCommand {
    type Error = SandboxError;

    fn try_from(request: BrokerRequest) -> Result<Self, Self::Error> {
        match request {
            BrokerRequest::CloseSession(request) => Ok(Self::Close(request)),
            BrokerRequest::CancelSession(request) => Ok(Self::Cancel(request)),
            BrokerRequest::ProcessStart(request) => Ok(Self::ProcessStart(request)),
            BrokerRequest::ProcessList(request) => Ok(Self::ProcessList(request)),
            BrokerRequest::ProcessDetail(request) => Ok(Self::ProcessDetail(request)),
            BrokerRequest::ProcessRead(request) => Ok(Self::ProcessRead(request)),
            BrokerRequest::ProcessWrite(request) => Ok(Self::ProcessWrite(request)),
            BrokerRequest::ProcessCancel(request) => Ok(Self::ProcessCancel(request)),
            BrokerRequest::ProcessRelease(request) => Ok(Self::ProcessRelease(request)),
            BrokerRequest::PtyStart(request) => Ok(Self::PtyStart(request)),
            BrokerRequest::PtyList(request) => Ok(Self::PtyList(request)),
            BrokerRequest::PtyDetail(request) => Ok(Self::PtyDetail(request)),
            BrokerRequest::PtyRead(request) => Ok(Self::PtyRead(request)),
            BrokerRequest::PtyWrite(request) => Ok(Self::PtyWrite(request)),
            BrokerRequest::PtyResize(request) => Ok(Self::PtyResize(request)),
            BrokerRequest::PtyStop(request) => Ok(Self::PtyStop(request)),
            BrokerRequest::PtyRelease(request) => Ok(Self::PtyRelease(request)),
            BrokerRequest::Handshake(_)
            | BrokerRequest::CreateSession(_)
            | BrokerRequest::CatalogPublicSnapshot
            | BrokerRequest::Ping => Err(SandboxError::protocol(Default::default())),
        }
    }
}

pub trait SandboxBackend: Send + Sync {
    fn identity(&self) -> BackendIdentity;
    fn self_test(&self) -> BackendSelfTest;
    fn prepare(
        &self,
        session_id: SessionId,
        scratch_id: ScratchId,
        policy: ValidatedExecutionPolicy,
    ) -> Result<Box<dyn SandboxSession>, SandboxError>;
}

pub trait SandboxSession: Send {
    fn confirmed_capabilities(&self) -> &ConfirmedExecutionCapabilities;
    fn state(&self) -> SessionState;
    fn handle(&mut self, command: SessionCommand) -> Result<BrokerResponse, SandboxError>;
    fn terminate(&mut self, reason: TerminationReason) -> Result<(), SandboxError>;
    fn revoke(&mut self, tool_id: &ToolId) -> Result<(), SandboxError>;
    fn drain_events(&mut self) -> Vec<BrokerEvent>;
}
