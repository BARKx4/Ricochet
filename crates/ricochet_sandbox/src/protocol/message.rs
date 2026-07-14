use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use serde::de::Error as _;
use serde::ser::{Error as _, SerializeStruct as _};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::audit::{AuditRecord, AuditWorkspace, EnforcementState};
use crate::catalog::{PublicCatalogSnapshot, PublicToolRecord};
use crate::destination::DestinationGrant;
use crate::error::{DiagnosticMetadata, SandboxError, SandboxErrorCode, TerminationReason};
use crate::identity::{
    BackendIdentity, CatalogGeneration, PolicyDigest, ProcessId, ProcessTreeId, PtyId, RequestId,
    ScratchId, SessionId, Sha256Digest, ToolId, UnixMillis,
};
use crate::lifecycle::SessionState;
use crate::policy::{
    ExecutionAccess, ExecutionPolicyRequest, ExecutionSurface, LaunchEnvironment, ResourceLimits,
    ValidatedExecutionPolicy,
};
use crate::version::{CATALOG_SCHEMA_V1, MAX_IO_CHUNK_BYTES, PROTOCOL_V1};

#[allow(clippy::result_large_err)]
fn protocol_error() -> SandboxError {
    SandboxError::protocol(DiagnosticMetadata::empty())
}

fn valid_wire_text(value: &str) -> bool {
    !value.is_empty() && !value.contains('\0')
}

pub struct ProtocolEnvelope {
    pub protocol_version: u16,
    pub sequence: u64,
    pub message: ProtocolMessage,
}

impl Serialize for ProtocolEnvelope {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if self.protocol_version != PROTOCOL_V1 {
            return Err(S::Error::custom(protocol_error()));
        }
        let mut state = serializer.serialize_struct("ProtocolEnvelope", 3)?;
        state.serialize_field("protocol_version", &self.protocol_version)?;
        state.serialize_field("sequence", &self.sequence)?;
        state.serialize_field("message", &self.message)?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for ProtocolEnvelope {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct WireEnvelope {
            protocol_version: u16,
            sequence: u64,
            message: ProtocolMessage,
        }

        let wire = WireEnvelope::deserialize(deserializer)?;
        if wire.protocol_version != PROTOCOL_V1 {
            return Err(D::Error::custom(protocol_error()));
        }
        Ok(Self {
            protocol_version: wire.protocol_version,
            sequence: wire.sequence,
            message: wire.message,
        })
    }
}

impl fmt::Debug for ProtocolEnvelope {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProtocolEnvelope")
            .field("protocol_version", &self.protocol_version)
            .field("sequence", &self.sequence)
            .field("message", &self.message)
            .finish()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EndpointRole {
    Host,
    Broker,
}

impl EndpointRole {
    pub fn peer(self) -> Self {
        match self {
            Self::Host => Self::Broker,
            Self::Broker => Self::Host,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(
    deny_unknown_fields,
    tag = "type",
    content = "body",
    rename_all = "snake_case"
)]
pub enum ProtocolMessage {
    Request {
        request_id: RequestId,
        request: BrokerRequest,
    },
    Response {
        request_id: RequestId,
        response: BrokerResponse,
    },
    Event {
        session_id: SessionId,
        event: BrokerEvent,
    },
}

impl ProtocolMessage {
    pub fn request(request_id: RequestId, request: BrokerRequest) -> Self {
        Self::Request {
            request_id,
            request,
        }
    }

    pub fn response(request_id: RequestId, response: BrokerResponse) -> Self {
        Self::Response {
            request_id,
            response,
        }
    }

    pub fn event(session_id: SessionId, event: BrokerEvent) -> Self {
        Self::Event { session_id, event }
    }

    #[allow(clippy::result_large_err)]
    pub fn validate_for(&self, sender: EndpointRole) -> Result<(), SandboxError> {
        match (sender, self) {
            (EndpointRole::Host, Self::Request { request, .. }) => request.validate_local(),
            (EndpointRole::Broker, Self::Response { response, .. }) => response.validate(),
            (EndpointRole::Broker, Self::Event { session_id, event }) => {
                event.validate_for_session(session_id)
            }
            _ => Err(protocol_error()),
        }
    }
}

impl fmt::Debug for ProtocolMessage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Request {
                request_id,
                request,
            } => formatter
                .debug_struct("Request")
                .field("request_id", request_id)
                .field("request", request)
                .finish(),
            Self::Response {
                request_id,
                response,
            } => formatter
                .debug_struct("Response")
                .field("request_id", request_id)
                .field("response", response)
                .finish(),
            Self::Event { session_id, event } => formatter
                .debug_struct("Event")
                .field("session_id", session_id)
                .field("event", event)
                .finish(),
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct ConnectionNonce([u8; 32]);

impl ConnectionNonce {
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    #[allow(clippy::result_large_err)]
    pub fn parse_hex(value: &str) -> Result<Self, SandboxError> {
        if value.len() != 64
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
        {
            return Err(protocol_error());
        }
        let mut bytes = [0_u8; 32];
        for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
            bytes[index] =
                u8::from_str_radix(std::str::from_utf8(pair).map_err(|_| protocol_error())?, 16)
                    .map_err(|_| protocol_error())?;
        }
        Ok(Self(bytes))
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    pub fn to_hex(&self) -> String {
        Sha256Digest::from_bytes(self.0).to_hex()
    }
}

impl fmt::Debug for ConnectionNonce {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ConnectionNonce([REDACTED])")
    }
}

impl Serialize for ConnectionNonce {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for ConnectionNonce {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse_hex(&value).map_err(D::Error::custom)
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct WireBytes(Vec<u8>);

impl WireBytes {
    #[allow(clippy::result_large_err)]
    pub fn new(bytes: Vec<u8>) -> Result<Self, SandboxError> {
        if bytes.len() > MAX_IO_CHUNK_BYTES {
            return Err(protocol_error());
        }
        Ok(Self(bytes))
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Debug for WireBytes {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("WireBytes")
            .field("byte_len", &self.0.len())
            .finish()
    }
}

impl Serialize for WireBytes {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&STANDARD.encode(&self.0))
    }
}

impl<'de> Deserialize<'de> for WireBytes {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let encoded = String::deserialize(deserializer)?;
        let bytes = STANDARD.decode(&encoded).map_err(D::Error::custom)?;
        if STANDARD.encode(&bytes) != encoded {
            return Err(D::Error::custom(protocol_error()));
        }
        Self::new(bytes).map_err(D::Error::custom)
    }
}

pub fn chunk_wire_bytes(bytes: &[u8]) -> Vec<WireBytes> {
    bytes
        .chunks(MAX_IO_CHUNK_BYTES)
        .map(|chunk| WireBytes(chunk.to_vec()))
        .collect()
}

#[derive(Clone, PartialEq, Eq)]
pub struct PeerContextId(String);

impl PeerContextId {
    #[allow(clippy::result_large_err)]
    pub fn parse(value: impl Into<String>) -> Result<Self, SandboxError> {
        let value = value.into();
        if !(1..=256).contains(&value.len())
            || value.chars().any(char::is_control)
            || value.contains('\0')
        {
            return Err(protocol_error());
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for PeerContextId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("PeerContextId")
            .field(&self.0)
            .finish()
    }
}

impl Serialize for PeerContextId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for PeerContextId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::parse(String::deserialize(deserializer)?).map_err(D::Error::custom)
    }
}

pub struct AuthenticatedChannelContext {
    peer_context_id: PeerContextId,
    channel_binding: Sha256Digest,
}

impl AuthenticatedChannelContext {
    pub fn from_native_acceptor(
        peer_context_id: PeerContextId,
        channel_binding: Sha256Digest,
    ) -> Self {
        Self {
            peer_context_id,
            channel_binding,
        }
    }
}

impl fmt::Debug for AuthenticatedChannelContext {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AuthenticatedChannelContext")
            .field("peer_context_id", &self.peer_context_id)
            .field("channel_binding", &"[REDACTED]")
            .finish()
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HandshakeRequest {
    pub supported_protocol_versions: Vec<u16>,
    pub connection_nonce: ConnectionNonce,
    pub channel_binding: Sha256Digest,
}

impl HandshakeRequest {
    #[allow(clippy::result_large_err)]
    fn validate_versions(&self) -> Result<(), SandboxError> {
        if self.supported_protocol_versions.as_slice() == [PROTOCOL_V1] {
            Ok(())
        } else {
            Err(protocol_error())
        }
    }

    #[allow(clippy::result_large_err)]
    pub fn validate_channel(
        &self,
        context: &AuthenticatedChannelContext,
    ) -> Result<(), SandboxError> {
        self.validate_versions()?;
        if self.channel_binding == context.channel_binding {
            Ok(())
        } else {
            Err(protocol_error())
        }
    }
}

impl fmt::Debug for HandshakeRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HandshakeRequest")
            .field(
                "supported_protocol_versions",
                &self.supported_protocol_versions,
            )
            .field("connection_nonce", &"[REDACTED]")
            .field("channel_binding", &"[REDACTED]")
            .finish()
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HandshakeResponse {
    pub selected_protocol_version: u16,
    pub connection_nonce: ConnectionNonce,
    pub broker_nonce: ConnectionNonce,
    pub broker_identity: BackendIdentity,
    pub peer_context_id: PeerContextId,
    pub channel_binding: Sha256Digest,
}

impl HandshakeResponse {
    #[allow(clippy::result_large_err)]
    pub fn validate_against(&self, expected: &HandshakeExpectation) -> Result<(), SandboxError> {
        if self.selected_protocol_version == PROTOCOL_V1
            && expected
                .offered_protocol_versions
                .contains(&self.selected_protocol_version)
            && self.connection_nonce == expected.connection_nonce
            && self.peer_context_id == expected.expected_peer_context_id
            && self.channel_binding == expected.expected_channel_binding
        {
            Ok(())
        } else {
            Err(protocol_error())
        }
    }
}

impl fmt::Debug for HandshakeResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HandshakeResponse")
            .field("selected_protocol_version", &self.selected_protocol_version)
            .field("connection_nonce", &"[REDACTED]")
            .field("broker_nonce", &"[REDACTED]")
            .field("broker_identity", &self.broker_identity)
            .field("peer_context_id", &self.peer_context_id)
            .field("channel_binding", &"[REDACTED]")
            .finish()
    }
}

#[derive(Serialize, Deserialize)]
#[serde(
    deny_unknown_fields,
    tag = "type",
    content = "body",
    rename_all = "snake_case"
)]
pub enum ExecutableRef {
    ManagedTool(ToolId),
    HostCommand(String),
}

impl fmt::Debug for ExecutableRef {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ManagedTool(tool_id) => {
                formatter.debug_tuple("ManagedTool").field(tool_id).finish()
            }
            Self::HostCommand(_) => formatter.write_str("HostCommand([REDACTED])"),
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateSessionRequest {
    pub session_id: SessionId,
    pub policy: ExecutionPolicyRequest,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionRequest {
    pub session_id: SessionId,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CancelSessionRequest {
    pub session_id: SessionId,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessLaunchRequest {
    pub session_id: SessionId,
    pub executable: ExecutableRef,
    pub arguments: Vec<String>,
    pub cwd: Option<String>,
    pub stdin_open: bool,
    pub environment: LaunchEnvironment,
    pub timeout_ms: u64,
    pub stdout_max_bytes: u64,
    pub stderr_max_bytes: u64,
}

impl fmt::Debug for ProcessLaunchRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProcessLaunchRequest")
            .field("session_id", &self.session_id)
            .field("executable", &self.executable)
            .field("argument_count", &self.arguments.len())
            .field("cwd_present", &self.cwd.is_some())
            .field("stdin_open", &self.stdin_open)
            .field("environment_entry_count", &self.environment.entries.len())
            .field("timeout_ms", &self.timeout_ms)
            .field("stdout_max_bytes", &self.stdout_max_bytes)
            .field("stderr_max_bytes", &self.stderr_max_bytes)
            .finish()
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessRequest {
    pub session_id: SessionId,
    pub process_id: ProcessId,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessReadRequest {
    pub session_id: SessionId,
    pub process_id: ProcessId,
    pub stdout_offset: u64,
    pub stderr_offset: u64,
    pub max_bytes_per_stream: u32,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessWriteRequest {
    pub session_id: SessionId,
    pub process_id: ProcessId,
    pub bytes: WireBytes,
    pub close_stdin: bool,
}

impl fmt::Debug for ProcessWriteRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProcessWriteRequest")
            .field("session_id", &self.session_id)
            .field("process_id", &self.process_id)
            .field("byte_len", &self.bytes.as_slice().len())
            .field("close_stdin", &self.close_stdin)
            .finish()
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PtyLaunchRequest {
    pub session_id: SessionId,
    pub executable: ExecutableRef,
    pub arguments: Vec<String>,
    pub cwd: Option<String>,
    pub environment: LaunchEnvironment,
    pub rows: u16,
    pub cols: u16,
    pub output_max_bytes: u64,
}

impl fmt::Debug for PtyLaunchRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PtyLaunchRequest")
            .field("session_id", &self.session_id)
            .field("executable", &self.executable)
            .field("argument_count", &self.arguments.len())
            .field("cwd_present", &self.cwd.is_some())
            .field("environment_entry_count", &self.environment.entries.len())
            .field("rows", &self.rows)
            .field("cols", &self.cols)
            .field("output_max_bytes", &self.output_max_bytes)
            .finish()
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PtyRequest {
    pub session_id: SessionId,
    pub pty_id: PtyId,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PtyReadRequest {
    pub session_id: SessionId,
    pub pty_id: PtyId,
    pub offset: u64,
    pub max_bytes: u32,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PtyWriteRequest {
    pub session_id: SessionId,
    pub pty_id: PtyId,
    pub bytes: WireBytes,
}

impl fmt::Debug for PtyWriteRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PtyWriteRequest")
            .field("session_id", &self.session_id)
            .field("pty_id", &self.pty_id)
            .field("byte_len", &self.bytes.as_slice().len())
            .finish()
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PtyResizeRequest {
    pub session_id: SessionId,
    pub pty_id: PtyId,
    pub rows: u16,
    pub cols: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub enum ProcessStatus {
    Running,
    Exited,
    Failed,
    Cancelled,
    TimedOut,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub enum PtyStatus {
    Running,
    Exited,
    Failed,
    Stopped,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum OperationErrorCode {
    ProcessError,
    ProcessNotFound,
    ProcessRunning,
    ProcessNotRunning,
    ProcessStdinClosed,
    PtyError,
    PtyNotFound,
    PtyRunning,
    PtyClosed,
    RegistryFull,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    deny_unknown_fields,
    tag = "type",
    content = "body",
    rename_all = "snake_case"
)]
pub enum OperationSubject {
    Process(ProcessId),
    Pty(PtyId),
    Registry(ExecutionSurface),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OperationError {
    code: OperationErrorCode,
    subject: OperationSubject,
    message: String,
}

impl OperationError {
    #[allow(clippy::result_large_err)]
    pub fn new(code: OperationErrorCode, subject: OperationSubject) -> Result<Self, SandboxError> {
        let error = Self {
            code,
            subject,
            message: operation_message(code).to_owned(),
        };
        error.validate()?;
        Ok(error)
    }

    pub fn code(&self) -> OperationErrorCode {
        self.code
    }

    pub fn kind(&self) -> &'static str {
        operation_kind(self.code)
    }

    pub fn subject(&self) -> &OperationSubject {
        &self.subject
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    #[allow(clippy::result_large_err)]
    pub fn validate(&self) -> Result<(), SandboxError> {
        let subject_matches = match self.code {
            OperationErrorCode::ProcessError
            | OperationErrorCode::ProcessNotFound
            | OperationErrorCode::ProcessRunning
            | OperationErrorCode::ProcessNotRunning
            | OperationErrorCode::ProcessStdinClosed => {
                matches!(self.subject, OperationSubject::Process(_))
            }
            OperationErrorCode::PtyError
            | OperationErrorCode::PtyNotFound
            | OperationErrorCode::PtyRunning
            | OperationErrorCode::PtyClosed => {
                matches!(self.subject, OperationSubject::Pty(_))
            }
            OperationErrorCode::RegistryFull => {
                matches!(self.subject, OperationSubject::Registry(_))
            }
        };
        if subject_matches && self.message == operation_message(self.code) {
            Ok(())
        } else {
            Err(protocol_error())
        }
    }
}

impl<'de> Deserialize<'de> for OperationError {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct WireOperationError {
            code: OperationErrorCode,
            subject: OperationSubject,
            message: String,
        }

        let wire = WireOperationError::deserialize(deserializer)?;
        let error = Self {
            code: wire.code,
            subject: wire.subject,
            message: wire.message,
        };
        error.validate().map_err(D::Error::custom)?;
        Ok(error)
    }
}

const fn operation_kind(code: OperationErrorCode) -> &'static str {
    match code {
        OperationErrorCode::ProcessError => "ProcessError",
        OperationErrorCode::ProcessNotFound => "ProcessNotFound",
        OperationErrorCode::ProcessRunning => "ProcessRunning",
        OperationErrorCode::ProcessNotRunning => "ProcessNotRunning",
        OperationErrorCode::ProcessStdinClosed => "ProcessStdinClosed",
        OperationErrorCode::PtyError => "PtyError",
        OperationErrorCode::PtyNotFound => "PtyNotFound",
        OperationErrorCode::PtyRunning => "PtyRunning",
        OperationErrorCode::PtyClosed => "PtyClosed",
        OperationErrorCode::RegistryFull => "RegistryFull",
    }
}

const fn operation_message(code: OperationErrorCode) -> &'static str {
    match code {
        OperationErrorCode::ProcessError => "sandbox process operation failed",
        OperationErrorCode::ProcessNotFound => "sandbox process was not found",
        OperationErrorCode::ProcessRunning => "sandbox process is still running",
        OperationErrorCode::ProcessNotRunning => "sandbox process is not running",
        OperationErrorCode::ProcessStdinClosed => "sandbox process stdin is closed",
        OperationErrorCode::PtyError => "sandbox PTY operation failed",
        OperationErrorCode::PtyNotFound => "sandbox PTY was not found",
        OperationErrorCode::PtyRunning => "sandbox PTY is still running",
        OperationErrorCode::PtyClosed => "sandbox PTY is closed",
        OperationErrorCode::RegistryFull => "sandbox execution registry is full",
    }
}

#[derive(Clone, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessSnapshot {
    pub id: ProcessId,
    pub process_tree_id: ProcessTreeId,
    pub command_display: String,
    pub arguments: Vec<String>,
    pub argument_count: usize,
    pub cwd: Option<String>,
    pub started_at: UnixMillis,
    pub status: ProcessStatus,
    pub running: bool,
    pub success: bool,
    pub exit_code: Option<i64>,
    pub error: Option<SandboxError>,
    pub stdout_len: u64,
    pub stderr_len: u64,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
    pub stdin_open: bool,
    pub timed_out: bool,
    pub cancelled: bool,
}

impl ProcessSnapshot {
    #[allow(clippy::result_large_err)]
    pub fn validate(&self) -> Result<(), SandboxError> {
        if self.argument_count != self.arguments.len()
            || !valid_wire_text(&self.command_display)
            || self.arguments.iter().any(|value| value.contains('\0'))
            || self
                .cwd
                .as_ref()
                .is_some_and(|value| !valid_wire_text(value))
            || self
                .error
                .as_ref()
                .is_some_and(|error| error.validate().is_err())
        {
            return Err(protocol_error());
        }

        let status_valid = match self.status {
            ProcessStatus::Running => {
                self.running
                    && !self.success
                    && self.exit_code.is_none()
                    && self.error.is_none()
                    && !self.timed_out
                    && !self.cancelled
            }
            ProcessStatus::Exited => {
                !self.running
                    && self.error.is_none()
                    && !self.stdin_open
                    && !self.timed_out
                    && !self.cancelled
                    && self.exit_code.is_some()
                    && self.success == (self.exit_code == Some(0))
            }
            ProcessStatus::Failed => {
                !self.running
                    && !self.success
                    && !self.stdin_open
                    && !self.timed_out
                    && !self.cancelled
                    && self.error.is_some()
            }
            ProcessStatus::Cancelled => {
                !self.running
                    && !self.success
                    && !self.stdin_open
                    && !self.timed_out
                    && self.cancelled
            }
            ProcessStatus::TimedOut => {
                !self.running
                    && !self.success
                    && !self.stdin_open
                    && self.timed_out
                    && !self.cancelled
            }
        };
        if status_valid {
            Ok(())
        } else {
            Err(protocol_error())
        }
    }
}

impl<'de> Deserialize<'de> for ProcessSnapshot {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct WireProcessSnapshot {
            id: ProcessId,
            process_tree_id: ProcessTreeId,
            command_display: String,
            arguments: Vec<String>,
            argument_count: usize,
            cwd: Option<String>,
            started_at: UnixMillis,
            status: ProcessStatus,
            running: bool,
            success: bool,
            exit_code: Option<i64>,
            error: Option<SandboxError>,
            stdout_len: u64,
            stderr_len: u64,
            stdout_truncated: bool,
            stderr_truncated: bool,
            stdin_open: bool,
            timed_out: bool,
            cancelled: bool,
        }

        let wire = WireProcessSnapshot::deserialize(deserializer)?;
        let snapshot = Self {
            id: wire.id,
            process_tree_id: wire.process_tree_id,
            command_display: wire.command_display,
            arguments: wire.arguments,
            argument_count: wire.argument_count,
            cwd: wire.cwd,
            started_at: wire.started_at,
            status: wire.status,
            running: wire.running,
            success: wire.success,
            exit_code: wire.exit_code,
            error: wire.error,
            stdout_len: wire.stdout_len,
            stderr_len: wire.stderr_len,
            stdout_truncated: wire.stdout_truncated,
            stderr_truncated: wire.stderr_truncated,
            stdin_open: wire.stdin_open,
            timed_out: wire.timed_out,
            cancelled: wire.cancelled,
        };
        snapshot.validate().map_err(D::Error::custom)?;
        Ok(snapshot)
    }
}

impl fmt::Debug for ProcessSnapshot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProcessSnapshot")
            .field("id", &self.id)
            .field("process_tree_id", &self.process_tree_id)
            .field("argument_count", &self.argument_count)
            .field("cwd_present", &self.cwd.is_some())
            .field("started_at", &self.started_at)
            .field("status", &self.status)
            .field("running", &self.running)
            .field("success", &self.success)
            .field("exit_code", &self.exit_code)
            .field("error", &self.error)
            .field("stdout_len", &self.stdout_len)
            .field("stderr_len", &self.stderr_len)
            .field("stdout_truncated", &self.stdout_truncated)
            .field("stderr_truncated", &self.stderr_truncated)
            .field("stdin_open", &self.stdin_open)
            .field("timed_out", &self.timed_out)
            .field("cancelled", &self.cancelled)
            .finish()
    }
}

#[derive(Clone, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessReadSnapshot {
    pub snapshot: ProcessSnapshot,
    pub stdout: WireBytes,
    pub stderr: WireBytes,
    pub stdout_offset: u64,
    pub stderr_offset: u64,
}

impl ProcessReadSnapshot {
    #[allow(clippy::result_large_err)]
    pub fn validate(&self) -> Result<(), SandboxError> {
        self.snapshot.validate()?;
        let stdout_end = self
            .stdout_offset
            .checked_add(self.stdout.as_slice().len() as u64)
            .ok_or_else(protocol_error)?;
        let stderr_end = self
            .stderr_offset
            .checked_add(self.stderr.as_slice().len() as u64)
            .ok_or_else(protocol_error)?;
        if stdout_end <= self.snapshot.stdout_len && stderr_end <= self.snapshot.stderr_len {
            Ok(())
        } else {
            Err(protocol_error())
        }
    }
}

impl<'de> Deserialize<'de> for ProcessReadSnapshot {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct WireRead {
            snapshot: ProcessSnapshot,
            stdout: WireBytes,
            stderr: WireBytes,
            stdout_offset: u64,
            stderr_offset: u64,
        }
        let wire = WireRead::deserialize(deserializer)?;
        let snapshot = Self {
            snapshot: wire.snapshot,
            stdout: wire.stdout,
            stderr: wire.stderr,
            stdout_offset: wire.stdout_offset,
            stderr_offset: wire.stderr_offset,
        };
        snapshot.validate().map_err(D::Error::custom)?;
        Ok(snapshot)
    }
}

impl fmt::Debug for ProcessReadSnapshot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProcessReadSnapshot")
            .field("snapshot", &self.snapshot)
            .field("stdout_byte_len", &self.stdout.as_slice().len())
            .field("stderr_byte_len", &self.stderr.as_slice().len())
            .field("stdout_offset", &self.stdout_offset)
            .field("stderr_offset", &self.stderr_offset)
            .finish()
    }
}

#[derive(Clone, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PtySnapshot {
    pub id: PtyId,
    pub process_tree_id: ProcessTreeId,
    pub command_display: String,
    pub arguments: Vec<String>,
    pub argument_count: usize,
    pub cwd: Option<String>,
    pub started_at: UnixMillis,
    pub status: PtyStatus,
    pub running: bool,
    pub success: bool,
    pub exit_code: Option<i64>,
    pub error: Option<SandboxError>,
    pub output_len: u64,
    pub output_truncated: bool,
    pub rows: u16,
    pub cols: u16,
    pub native_process_id: Option<u32>,
    pub stopped: bool,
}

impl PtySnapshot {
    #[allow(clippy::result_large_err)]
    pub fn validate(&self) -> Result<(), SandboxError> {
        if self.argument_count != self.arguments.len()
            || !valid_wire_text(&self.command_display)
            || self.arguments.iter().any(|value| value.contains('\0'))
            || self
                .cwd
                .as_ref()
                .is_some_and(|value| !valid_wire_text(value))
            || self.rows == 0
            || self.cols == 0
            || self
                .error
                .as_ref()
                .is_some_and(|error| error.validate().is_err())
        {
            return Err(protocol_error());
        }

        let status_valid = match self.status {
            PtyStatus::Running => {
                self.running
                    && !self.success
                    && self.exit_code.is_none()
                    && self.error.is_none()
                    && !self.stopped
            }
            PtyStatus::Exited => {
                !self.running
                    && self.error.is_none()
                    && !self.stopped
                    && self.exit_code.is_some()
                    && self.success == (self.exit_code == Some(0))
            }
            PtyStatus::Failed => {
                !self.running && !self.success && self.error.is_some() && !self.stopped
            }
            PtyStatus::Stopped => !self.running && !self.success && self.stopped,
        };
        if status_valid {
            Ok(())
        } else {
            Err(protocol_error())
        }
    }
}

impl<'de> Deserialize<'de> for PtySnapshot {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct WirePtySnapshot {
            id: PtyId,
            process_tree_id: ProcessTreeId,
            command_display: String,
            arguments: Vec<String>,
            argument_count: usize,
            cwd: Option<String>,
            started_at: UnixMillis,
            status: PtyStatus,
            running: bool,
            success: bool,
            exit_code: Option<i64>,
            error: Option<SandboxError>,
            output_len: u64,
            output_truncated: bool,
            rows: u16,
            cols: u16,
            native_process_id: Option<u32>,
            stopped: bool,
        }
        let wire = WirePtySnapshot::deserialize(deserializer)?;
        let snapshot = Self {
            id: wire.id,
            process_tree_id: wire.process_tree_id,
            command_display: wire.command_display,
            arguments: wire.arguments,
            argument_count: wire.argument_count,
            cwd: wire.cwd,
            started_at: wire.started_at,
            status: wire.status,
            running: wire.running,
            success: wire.success,
            exit_code: wire.exit_code,
            error: wire.error,
            output_len: wire.output_len,
            output_truncated: wire.output_truncated,
            rows: wire.rows,
            cols: wire.cols,
            native_process_id: wire.native_process_id,
            stopped: wire.stopped,
        };
        snapshot.validate().map_err(D::Error::custom)?;
        Ok(snapshot)
    }
}

impl fmt::Debug for PtySnapshot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PtySnapshot")
            .field("id", &self.id)
            .field("process_tree_id", &self.process_tree_id)
            .field("argument_count", &self.argument_count)
            .field("cwd_present", &self.cwd.is_some())
            .field("started_at", &self.started_at)
            .field("status", &self.status)
            .field("running", &self.running)
            .field("success", &self.success)
            .field("exit_code", &self.exit_code)
            .field("error", &self.error)
            .field("output_len", &self.output_len)
            .field("output_truncated", &self.output_truncated)
            .field("rows", &self.rows)
            .field("cols", &self.cols)
            .field("native_process_id", &self.native_process_id)
            .field("stopped", &self.stopped)
            .finish()
    }
}

#[derive(Clone, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PtyReadSnapshot {
    pub snapshot: PtySnapshot,
    pub output: WireBytes,
    pub offset: u64,
}

impl PtyReadSnapshot {
    #[allow(clippy::result_large_err)]
    pub fn validate(&self) -> Result<(), SandboxError> {
        self.snapshot.validate()?;
        let end = self
            .offset
            .checked_add(self.output.as_slice().len() as u64)
            .ok_or_else(protocol_error)?;
        if end <= self.snapshot.output_len {
            Ok(())
        } else {
            Err(protocol_error())
        }
    }
}

impl<'de> Deserialize<'de> for PtyReadSnapshot {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct WireRead {
            snapshot: PtySnapshot,
            output: WireBytes,
            offset: u64,
        }
        let wire = WireRead::deserialize(deserializer)?;
        let snapshot = Self {
            snapshot: wire.snapshot,
            output: wire.output,
            offset: wire.offset,
        };
        snapshot.validate().map_err(D::Error::custom)?;
        Ok(snapshot)
    }
}

impl fmt::Debug for PtyReadSnapshot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PtyReadSnapshot")
            .field("snapshot", &self.snapshot)
            .field("output_byte_len", &self.output.as_slice().len())
            .field("offset", &self.offset)
            .finish()
    }
}

#[derive(Clone, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConfirmedExecutionCapabilities {
    access: ExecutionAccess,
    enforcement: EnforcementState,
    backend: BackendIdentity,
    broker_protocol: u16,
    session_id: SessionId,
    policy_digest: PolicyDigest,
    workspace: Option<AuditWorkspace>,
    scratch_id: ScratchId,
    catalog_generation: CatalogGeneration,
    tools: Vec<PublicToolRecord>,
    destinations: Vec<DestinationGrant>,
    resource_limits: Option<ResourceLimits>,
}

impl ConfirmedExecutionCapabilities {
    #[allow(clippy::too_many_arguments, clippy::result_large_err)]
    pub fn new(
        session_id: SessionId,
        scratch_id: ScratchId,
        backend: BackendIdentity,
        broker_protocol: u16,
        enforcement: EnforcementState,
        policy: &ValidatedExecutionPolicy,
    ) -> Result<Self, SandboxError> {
        let value = Self {
            access: policy.access(),
            enforcement,
            backend,
            broker_protocol,
            session_id,
            policy_digest: policy.audit_digest(),
            workspace: policy.workspace_identity().map(|identity| {
                AuditWorkspace::from_identity(identity, policy.access() != ExecutionAccess::Read)
            }),
            scratch_id,
            catalog_generation: policy.prepared_catalog().generation(),
            tools: policy.prepared_catalog().public_records(),
            destinations: policy.destinations().to_vec(),
            resource_limits: policy.resource_limits().cloned(),
        };
        value.validate_against_policy(policy)?;
        Ok(value)
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

    pub fn session_id(&self) -> &SessionId {
        &self.session_id
    }

    pub fn policy_digest(&self) -> &PolicyDigest {
        &self.policy_digest
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
    pub fn validate(&self) -> Result<(), SandboxError> {
        let access_fields_valid = match self.access {
            ExecutionAccess::Read => {
                self.workspace
                    .as_ref()
                    .is_some_and(|workspace| !workspace.writable())
                    && self.resource_limits.is_some()
            }
            ExecutionAccess::Workspace => {
                self.workspace
                    .as_ref()
                    .is_some_and(AuditWorkspace::writable)
                    && self.resource_limits.is_some()
            }
            ExecutionAccess::Full => {
                self.workspace.as_ref().is_none_or(AuditWorkspace::writable)
                    && self.tools.is_empty()
                    && self.destinations.is_empty()
            }
        };
        let enforcement_valid = matches!(
            (self.access, self.enforcement),
            (
                ExecutionAccess::Read | ExecutionAccess::Workspace,
                EnforcementState::Enforced | EnforcementState::MockOnly
            ) | (
                ExecutionAccess::Full,
                EnforcementState::UnenforcedFullAccess | EnforcementState::MockOnly
            )
        );
        let workspace_valid = self
            .workspace
            .as_ref()
            .is_none_or(|workspace| valid_wire_text(workspace.canonical_root()));
        let tools_valid = public_tools_valid(&self.tools);
        let destinations_valid = self.destinations.windows(2).all(|pair| pair[0] < pair[1]);
        let limits_valid = self
            .resource_limits
            .as_ref()
            .is_none_or(|limits| limits.validate().is_ok());
        if self.broker_protocol == PROTOCOL_V1
            && access_fields_valid
            && enforcement_valid
            && workspace_valid
            && tools_valid
            && destinations_valid
            && limits_valid
        {
            Ok(())
        } else {
            Err(protocol_error())
        }
    }

    #[allow(clippy::result_large_err)]
    fn validate_against_policy(
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
            Err(protocol_error())
        }
    }
}

impl<'de> Deserialize<'de> for ConfirmedExecutionCapabilities {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct WireCapabilities {
            access: ExecutionAccess,
            enforcement: EnforcementState,
            backend: BackendIdentity,
            broker_protocol: u16,
            session_id: SessionId,
            policy_digest: PolicyDigest,
            workspace: Option<AuditWorkspace>,
            scratch_id: ScratchId,
            catalog_generation: CatalogGeneration,
            tools: Vec<PublicToolRecord>,
            destinations: Vec<DestinationGrant>,
            resource_limits: Option<ResourceLimits>,
        }
        let wire = WireCapabilities::deserialize(deserializer)?;
        let value = Self {
            access: wire.access,
            enforcement: wire.enforcement,
            backend: wire.backend,
            broker_protocol: wire.broker_protocol,
            session_id: wire.session_id,
            policy_digest: wire.policy_digest,
            workspace: wire.workspace,
            scratch_id: wire.scratch_id,
            catalog_generation: wire.catalog_generation,
            tools: wire.tools,
            destinations: wire.destinations,
            resource_limits: wire.resource_limits,
        };
        value.validate().map_err(D::Error::custom)?;
        Ok(value)
    }
}

impl fmt::Debug for ConfirmedExecutionCapabilities {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ConfirmedExecutionCapabilities")
            .field("access", &self.access)
            .field("enforcement", &self.enforcement)
            .field("backend", &self.backend)
            .field("broker_protocol", &self.broker_protocol)
            .field("session_id", &self.session_id)
            .field("policy_digest", &self.policy_digest)
            .field("workspace_present", &self.workspace.is_some())
            .field("scratch_id", &self.scratch_id)
            .field("catalog_generation", &self.catalog_generation)
            .field("tool_count", &self.tools.len())
            .field("destination_count", &self.destinations.len())
            .field("resource_limits_present", &self.resource_limits.is_some())
            .finish()
    }
}

fn public_tools_valid(tools: &[PublicToolRecord]) -> bool {
    if !tools
        .windows(2)
        .all(|pair| pair[0].tool_id < pair[1].tool_id)
    {
        return false;
    }
    let mut incoming = vec![0_usize; tools.len()];
    for tool in tools {
        if !tool.helper_ids.windows(2).all(|pair| pair[0] < pair[1]) {
            return false;
        }
        for helper in &tool.helper_ids {
            let Ok(index) = tools.binary_search_by(|candidate| candidate.tool_id.cmp(helper))
            else {
                return false;
            };
            incoming[index] += 1;
        }
    }
    let mut pending = incoming
        .iter()
        .enumerate()
        .filter_map(|(index, count)| (*count == 0).then_some(index))
        .collect::<VecDeque<_>>();
    let mut visited = 0;
    while let Some(index) = pending.pop_front() {
        visited += 1;
        for helper in &tools[index].helper_ids {
            let helper_index = tools
                .binary_search_by(|candidate| candidate.tool_id.cmp(helper))
                .expect("helper existence validated above");
            incoming[helper_index] -= 1;
            if incoming[helper_index] == 0 {
                pending.push_back(helper_index);
            }
        }
    }
    visited == tools.len()
}

#[derive(Serialize, Deserialize)]
#[serde(
    deny_unknown_fields,
    tag = "type",
    content = "body",
    rename_all = "snake_case"
)]
pub enum BrokerRequest {
    Handshake(HandshakeRequest),
    CreateSession(CreateSessionRequest),
    CloseSession(SessionRequest),
    CancelSession(CancelSessionRequest),
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
    CatalogPublicSnapshot,
    Ping,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BrokerRequestKind {
    Handshake,
    CreateSession,
    CloseSession,
    CancelSession,
    ProcessStart,
    ProcessList,
    ProcessDetail,
    ProcessRead,
    ProcessWrite,
    ProcessCancel,
    ProcessRelease,
    PtyStart,
    PtyList,
    PtyDetail,
    PtyRead,
    PtyWrite,
    PtyResize,
    PtyStop,
    PtyRelease,
    CatalogPublicSnapshot,
    Ping,
}

impl BrokerRequest {
    pub fn kind(&self) -> BrokerRequestKind {
        match self {
            Self::Handshake(_) => BrokerRequestKind::Handshake,
            Self::CreateSession(_) => BrokerRequestKind::CreateSession,
            Self::CloseSession(_) => BrokerRequestKind::CloseSession,
            Self::CancelSession(_) => BrokerRequestKind::CancelSession,
            Self::ProcessStart(_) => BrokerRequestKind::ProcessStart,
            Self::ProcessList(_) => BrokerRequestKind::ProcessList,
            Self::ProcessDetail(_) => BrokerRequestKind::ProcessDetail,
            Self::ProcessRead(_) => BrokerRequestKind::ProcessRead,
            Self::ProcessWrite(_) => BrokerRequestKind::ProcessWrite,
            Self::ProcessCancel(_) => BrokerRequestKind::ProcessCancel,
            Self::ProcessRelease(_) => BrokerRequestKind::ProcessRelease,
            Self::PtyStart(_) => BrokerRequestKind::PtyStart,
            Self::PtyList(_) => BrokerRequestKind::PtyList,
            Self::PtyDetail(_) => BrokerRequestKind::PtyDetail,
            Self::PtyRead(_) => BrokerRequestKind::PtyRead,
            Self::PtyWrite(_) => BrokerRequestKind::PtyWrite,
            Self::PtyResize(_) => BrokerRequestKind::PtyResize,
            Self::PtyStop(_) => BrokerRequestKind::PtyStop,
            Self::PtyRelease(_) => BrokerRequestKind::PtyRelease,
            Self::CatalogPublicSnapshot => BrokerRequestKind::CatalogPublicSnapshot,
            Self::Ping => BrokerRequestKind::Ping,
        }
    }

    #[allow(clippy::result_large_err)]
    fn validate_local(&self) -> Result<(), SandboxError> {
        match self {
            Self::Handshake(request) => request.validate_versions(),
            Self::CreateSession(request) => {
                if request.policy.schema_version == crate::version::POLICY_SCHEMA_V1 {
                    Ok(())
                } else {
                    Err(protocol_error())
                }
            }
            Self::ProcessStart(request) => validate_process_launch_local(request),
            Self::ProcessRead(request) => validate_chunk_bound(request.max_bytes_per_stream),
            Self::ProcessWrite(request) => {
                if !request.bytes.as_slice().is_empty() || request.close_stdin {
                    Ok(())
                } else {
                    Err(protocol_error())
                }
            }
            Self::PtyStart(request) => validate_pty_launch_local(request),
            Self::PtyRead(request) => validate_chunk_bound(request.max_bytes),
            Self::PtyWrite(request) => {
                if request.bytes.as_slice().is_empty() {
                    Err(protocol_error())
                } else {
                    Ok(())
                }
            }
            Self::PtyResize(request) => validate_dimensions(request.rows, request.cols),
            Self::CloseSession(_)
            | Self::CancelSession(_)
            | Self::ProcessList(_)
            | Self::ProcessDetail(_)
            | Self::ProcessCancel(_)
            | Self::ProcessRelease(_)
            | Self::PtyList(_)
            | Self::PtyDetail(_)
            | Self::PtyStop(_)
            | Self::PtyRelease(_)
            | Self::CatalogPublicSnapshot
            | Self::Ping => Ok(()),
        }
    }

    #[allow(clippy::result_large_err)]
    pub fn validate_against(
        &self,
        expected_session_id: &SessionId,
        policy: &ValidatedExecutionPolicy,
    ) -> Result<(), SandboxError> {
        self.validate_local()?;
        match self {
            Self::Handshake(_) | Self::CatalogPublicSnapshot | Self::Ping => Ok(()),
            Self::CreateSession(request) => {
                ensure_session(&request.session_id, expected_session_id)
            }
            Self::CloseSession(request) => ensure_session(&request.session_id, expected_session_id),
            Self::CancelSession(request) => {
                ensure_session(&request.session_id, expected_session_id)
            }
            Self::ProcessStart(request) => {
                ensure_surface_session(
                    &request.session_id,
                    expected_session_id,
                    ExecutionSurface::Process,
                    policy,
                )?;
                validate_executable(&request.executable, policy)?;
                policy.resolve_launch_environment(&request.environment)?;
                let total = request
                    .stdout_max_bytes
                    .checked_add(request.stderr_max_bytes)
                    .ok_or_else(protocol_error)?;
                if let Some(limits) = policy.resource_limits() {
                    if request.timeout_ms > limits.wall_time_ms
                        || total > limits.captured_output_bytes
                    {
                        return Err(protocol_error());
                    }
                }
                Ok(())
            }
            Self::ProcessDetail(request)
            | Self::ProcessCancel(request)
            | Self::ProcessRelease(request) => ensure_surface_session(
                &request.session_id,
                expected_session_id,
                ExecutionSurface::Process,
                policy,
            ),
            Self::ProcessRead(request) => ensure_surface_session(
                &request.session_id,
                expected_session_id,
                ExecutionSurface::Process,
                policy,
            ),
            Self::ProcessWrite(request) => ensure_surface_session(
                &request.session_id,
                expected_session_id,
                ExecutionSurface::Process,
                policy,
            ),
            Self::ProcessList(request) => ensure_surface_session(
                &request.session_id,
                expected_session_id,
                ExecutionSurface::Process,
                policy,
            ),
            Self::PtyStart(request) => {
                ensure_surface_session(
                    &request.session_id,
                    expected_session_id,
                    ExecutionSurface::Pty,
                    policy,
                )?;
                validate_executable(&request.executable, policy)?;
                policy.resolve_launch_environment(&request.environment)?;
                if policy
                    .resource_limits()
                    .is_some_and(|limits| request.output_max_bytes > limits.captured_output_bytes)
                {
                    return Err(protocol_error());
                }
                Ok(())
            }
            Self::PtyDetail(request) | Self::PtyStop(request) | Self::PtyRelease(request) => {
                ensure_surface_session(
                    &request.session_id,
                    expected_session_id,
                    ExecutionSurface::Pty,
                    policy,
                )
            }
            Self::PtyRead(request) => ensure_surface_session(
                &request.session_id,
                expected_session_id,
                ExecutionSurface::Pty,
                policy,
            ),
            Self::PtyWrite(request) => ensure_surface_session(
                &request.session_id,
                expected_session_id,
                ExecutionSurface::Pty,
                policy,
            ),
            Self::PtyResize(request) => ensure_surface_session(
                &request.session_id,
                expected_session_id,
                ExecutionSurface::Pty,
                policy,
            ),
            Self::PtyList(request) => ensure_surface_session(
                &request.session_id,
                expected_session_id,
                ExecutionSurface::Pty,
                policy,
            ),
        }
    }
}

#[allow(clippy::result_large_err)]
fn validate_process_launch_local(request: &ProcessLaunchRequest) -> Result<(), SandboxError> {
    validate_launch_text(
        &request.executable,
        &request.arguments,
        request.cwd.as_deref(),
    )?;
    if request.timeout_ms == 0 {
        Err(protocol_error())
    } else {
        Ok(())
    }
}

#[allow(clippy::result_large_err)]
fn validate_pty_launch_local(request: &PtyLaunchRequest) -> Result<(), SandboxError> {
    validate_launch_text(
        &request.executable,
        &request.arguments,
        request.cwd.as_deref(),
    )?;
    validate_dimensions(request.rows, request.cols)
}

#[allow(clippy::result_large_err)]
fn validate_launch_text(
    executable: &ExecutableRef,
    arguments: &[String],
    cwd: Option<&str>,
) -> Result<(), SandboxError> {
    let executable_valid = match executable {
        ExecutableRef::ManagedTool(_) => true,
        ExecutableRef::HostCommand(command) => valid_wire_text(command),
    };
    if executable_valid
        && arguments.iter().all(|argument| !argument.contains('\0'))
        && cwd.is_none_or(valid_wire_text)
    {
        Ok(())
    } else {
        Err(protocol_error())
    }
}

#[allow(clippy::result_large_err)]
fn validate_chunk_bound(value: u32) -> Result<(), SandboxError> {
    if (1..=MAX_IO_CHUNK_BYTES as u32).contains(&value) {
        Ok(())
    } else {
        Err(protocol_error())
    }
}

#[allow(clippy::result_large_err)]
fn validate_dimensions(rows: u16, cols: u16) -> Result<(), SandboxError> {
    if rows > 0 && cols > 0 {
        Ok(())
    } else {
        Err(protocol_error())
    }
}

#[allow(clippy::result_large_err)]
fn validate_executable(
    executable: &ExecutableRef,
    policy: &ValidatedExecutionPolicy,
) -> Result<(), SandboxError> {
    let valid = match executable {
        ExecutableRef::HostCommand(_) => policy.access() == ExecutionAccess::Full,
        ExecutableRef::ManagedTool(tool_id) => {
            policy.access() != ExecutionAccess::Full
                && policy.prepared_catalog().roots().contains(tool_id)
        }
    };
    if valid {
        Ok(())
    } else {
        Err(protocol_error())
    }
}

#[allow(clippy::result_large_err)]
fn ensure_session(actual: &SessionId, expected: &SessionId) -> Result<(), SandboxError> {
    if actual == expected {
        Ok(())
    } else {
        Err(protocol_error())
    }
}

#[allow(clippy::result_large_err)]
fn ensure_surface_session(
    actual: &SessionId,
    expected: &SessionId,
    surface: ExecutionSurface,
    policy: &ValidatedExecutionPolicy,
) -> Result<(), SandboxError> {
    ensure_session(actual, expected)?;
    if policy.allows(surface) {
        Ok(())
    } else {
        Err(protocol_error())
    }
}

impl fmt::Debug for BrokerRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Handshake(value) => formatter.debug_tuple("Handshake").field(value).finish(),
            Self::CreateSession(value) => formatter
                .debug_struct("CreateSession")
                .field("session_id", &value.session_id)
                .finish(),
            Self::CloseSession(value) => formatter
                .debug_tuple("CloseSession")
                .field(&value.session_id)
                .finish(),
            Self::CancelSession(value) => formatter
                .debug_tuple("CancelSession")
                .field(&value.session_id)
                .finish(),
            Self::ProcessStart(value) => {
                formatter.debug_tuple("ProcessStart").field(value).finish()
            }
            Self::ProcessList(value) => formatter
                .debug_tuple("ProcessList")
                .field(&value.session_id)
                .finish(),
            Self::ProcessDetail(value) => formatter
                .debug_tuple("ProcessDetail")
                .field(&(&value.session_id, value.process_id))
                .finish(),
            Self::ProcessRead(value) => formatter
                .debug_struct("ProcessRead")
                .field("session_id", &value.session_id)
                .field("process_id", &value.process_id)
                .field("stdout_offset", &value.stdout_offset)
                .field("stderr_offset", &value.stderr_offset)
                .field("max_bytes_per_stream", &value.max_bytes_per_stream)
                .finish(),
            Self::ProcessWrite(value) => {
                formatter.debug_tuple("ProcessWrite").field(value).finish()
            }
            Self::ProcessCancel(value) => formatter
                .debug_tuple("ProcessCancel")
                .field(&(&value.session_id, value.process_id))
                .finish(),
            Self::ProcessRelease(value) => formatter
                .debug_tuple("ProcessRelease")
                .field(&(&value.session_id, value.process_id))
                .finish(),
            Self::PtyStart(value) => formatter.debug_tuple("PtyStart").field(value).finish(),
            Self::PtyList(value) => formatter
                .debug_tuple("PtyList")
                .field(&value.session_id)
                .finish(),
            Self::PtyDetail(value) => formatter
                .debug_tuple("PtyDetail")
                .field(&(&value.session_id, value.pty_id))
                .finish(),
            Self::PtyRead(value) => formatter
                .debug_struct("PtyRead")
                .field("session_id", &value.session_id)
                .field("pty_id", &value.pty_id)
                .field("offset", &value.offset)
                .field("max_bytes", &value.max_bytes)
                .finish(),
            Self::PtyWrite(value) => formatter.debug_tuple("PtyWrite").field(value).finish(),
            Self::PtyResize(value) => formatter
                .debug_struct("PtyResize")
                .field("session_id", &value.session_id)
                .field("pty_id", &value.pty_id)
                .field("rows", &value.rows)
                .field("cols", &value.cols)
                .finish(),
            Self::PtyStop(value) => formatter
                .debug_tuple("PtyStop")
                .field(&(&value.session_id, value.pty_id))
                .finish(),
            Self::PtyRelease(value) => formatter
                .debug_tuple("PtyRelease")
                .field(&(&value.session_id, value.pty_id))
                .finish(),
            Self::CatalogPublicSnapshot => formatter.write_str("CatalogPublicSnapshot"),
            Self::Ping => formatter.write_str("Ping"),
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(
    deny_unknown_fields,
    tag = "type",
    content = "body",
    rename_all = "snake_case"
)]
pub enum BrokerResponse {
    Handshake(HandshakeResponse),
    SessionCreated(ConfirmedExecutionCapabilities),
    Acknowledged,
    Process(ProcessSnapshot),
    Processes(Vec<ProcessSnapshot>),
    ProcessRead(ProcessReadSnapshot),
    Pty(PtySnapshot),
    Ptys(Vec<PtySnapshot>),
    PtyRead(PtyReadSnapshot),
    PublicCatalog(PublicCatalogSnapshot),
    Pong,
    OperationError(OperationError),
    Error(SandboxError),
}

impl BrokerResponse {
    #[allow(clippy::result_large_err)]
    fn validate(&self) -> Result<(), SandboxError> {
        match self {
            Self::Handshake(response) => {
                if response.selected_protocol_version == PROTOCOL_V1 {
                    Ok(())
                } else {
                    Err(protocol_error())
                }
            }
            Self::SessionCreated(capabilities) => capabilities.validate(),
            Self::Process(snapshot) => snapshot.validate(),
            Self::Processes(snapshots) => snapshots.iter().try_for_each(ProcessSnapshot::validate),
            Self::ProcessRead(snapshot) => snapshot.validate(),
            Self::Pty(snapshot) => snapshot.validate(),
            Self::Ptys(snapshots) => snapshots.iter().try_for_each(PtySnapshot::validate),
            Self::PtyRead(snapshot) => snapshot.validate(),
            Self::PublicCatalog(snapshot) => validate_public_catalog(snapshot),
            Self::OperationError(error) => error.validate(),
            Self::Error(error) => error.validate(),
            Self::Acknowledged | Self::Pong => Ok(()),
        }
    }
}

#[allow(clippy::result_large_err)]
fn validate_public_catalog(snapshot: &PublicCatalogSnapshot) -> Result<(), SandboxError> {
    let records_sorted = snapshot
        .records
        .windows(2)
        .all(|pair| pair[0].tool_id < pair[1].tool_id);
    let revoked_sorted = snapshot
        .revoked_tools
        .windows(2)
        .all(|pair| pair[0] < pair[1]);
    if snapshot.schema_version == CATALOG_SCHEMA_V1
        && records_sorted
        && revoked_sorted
        && public_tools_valid(&snapshot.records)
    {
        Ok(())
    } else {
        Err(protocol_error())
    }
}

impl fmt::Debug for BrokerResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Handshake(value) => formatter.debug_tuple("Handshake").field(value).finish(),
            Self::SessionCreated(value) => formatter
                .debug_tuple("SessionCreated")
                .field(value)
                .finish(),
            Self::Acknowledged => formatter.write_str("Acknowledged"),
            Self::Process(value) => formatter.debug_tuple("Process").field(value).finish(),
            Self::Processes(values) => formatter
                .debug_struct("Processes")
                .field("count", &values.len())
                .finish(),
            Self::ProcessRead(value) => formatter.debug_tuple("ProcessRead").field(value).finish(),
            Self::Pty(value) => formatter.debug_tuple("Pty").field(value).finish(),
            Self::Ptys(values) => formatter
                .debug_struct("Ptys")
                .field("count", &values.len())
                .finish(),
            Self::PtyRead(value) => formatter.debug_tuple("PtyRead").field(value).finish(),
            Self::PublicCatalog(value) => formatter
                .debug_struct("PublicCatalog")
                .field("generation", &value.generation)
                .field("record_count", &value.records.len())
                .field("revoked_count", &value.revoked_tools.len())
                .finish(),
            Self::Pong => formatter.write_str("Pong"),
            Self::OperationError(value) => formatter
                .debug_tuple("OperationError")
                .field(value)
                .finish(),
            Self::Error(value) => formatter.debug_tuple("Error").field(value).finish(),
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TerminationNotice {
    pub reason: TerminationReason,
    pub process_tree_ids: Vec<ProcessTreeId>,
    pub error: Option<SandboxError>,
}

impl TerminationNotice {
    #[allow(clippy::result_large_err)]
    fn validate(&self) -> Result<(), SandboxError> {
        let unique = self
            .process_tree_ids
            .iter()
            .copied()
            .collect::<BTreeSet<_>>()
            .len()
            == self.process_tree_ids.len();
        if unique
            && self.error.as_ref().is_none_or(|error| {
                error.code() == SandboxErrorCode::SandboxTerminated && error.validate().is_ok()
            })
        {
            Ok(())
        } else {
            Err(protocol_error())
        }
    }
}

impl fmt::Debug for TerminationNotice {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TerminationNotice")
            .field("reason", &self.reason)
            .field("process_tree_ids", &self.process_tree_ids)
            .field("error", &self.error)
            .finish()
    }
}

#[derive(Serialize, Deserialize)]
#[serde(
    deny_unknown_fields,
    tag = "type",
    content = "body",
    rename_all = "snake_case"
)]
pub enum BrokerEvent {
    Audit(AuditRecord),
    SessionState(SessionState),
    Terminated(TerminationNotice),
}

impl BrokerEvent {
    #[allow(clippy::result_large_err)]
    fn validate_for_session(&self, session_id: &SessionId) -> Result<(), SandboxError> {
        match self {
            Self::Audit(record) => {
                record.validate()?;
                ensure_session(record.context().session_id(), session_id)
            }
            Self::SessionState(_) => Ok(()),
            Self::Terminated(notice) => notice.validate(),
        }
    }

    #[allow(clippy::result_large_err)]
    #[allow(dead_code)]
    pub(crate) fn validate_against_policy(
        &self,
        session_id: &SessionId,
        policy: &ValidatedExecutionPolicy,
    ) -> Result<(), SandboxError> {
        self.validate_for_session(session_id)?;
        if let Self::Audit(record) = self {
            record.context().validate_against_policy(policy)?;
        }
        Ok(())
    }
}

impl fmt::Debug for BrokerEvent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Audit(record) => formatter
                .debug_struct("Audit")
                .field("session_id", record.context().session_id())
                .finish(),
            Self::SessionState(state) => {
                formatter.debug_tuple("SessionState").field(state).finish()
            }
            Self::Terminated(notice) => formatter.debug_tuple("Terminated").field(notice).finish(),
        }
    }
}

#[derive(Default)]
pub struct ResponseCorrelation {
    outstanding: BTreeMap<RequestId, OutstandingRequest>,
}

pub enum OutstandingRequest {
    Handshake(HandshakeExpectation),
    Ordinary(BrokerRequestKind),
}

pub struct HandshakeExpectation {
    offered_protocol_versions: Vec<u16>,
    connection_nonce: ConnectionNonce,
    expected_peer_context_id: PeerContextId,
    expected_channel_binding: Sha256Digest,
}

impl fmt::Debug for HandshakeExpectation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HandshakeExpectation")
            .field("offered_protocol_versions", &self.offered_protocol_versions)
            .field("connection_nonce", &"[REDACTED]")
            .field("expected_peer_context_id", &self.expected_peer_context_id)
            .field("expected_channel_binding", &"[REDACTED]")
            .finish()
    }
}

impl ResponseCorrelation {
    #[allow(clippy::result_large_err)]
    pub fn record_handshake(
        &mut self,
        request_id: RequestId,
        request: &HandshakeRequest,
        channel: &AuthenticatedChannelContext,
    ) -> Result<(), SandboxError> {
        request.validate_channel(channel)?;
        self.insert(
            request_id,
            OutstandingRequest::Handshake(HandshakeExpectation {
                offered_protocol_versions: request.supported_protocol_versions.clone(),
                connection_nonce: request.connection_nonce.clone(),
                expected_peer_context_id: channel.peer_context_id.clone(),
                expected_channel_binding: channel.channel_binding,
            }),
        )
    }

    #[allow(clippy::result_large_err)]
    pub fn record_request(
        &mut self,
        request_id: RequestId,
        request: &BrokerRequest,
    ) -> Result<(), SandboxError> {
        if matches!(request, BrokerRequest::Handshake(_)) {
            return Err(protocol_error());
        }
        request.validate_local()?;
        self.insert(request_id, OutstandingRequest::Ordinary(request.kind()))
    }

    #[allow(clippy::result_large_err)]
    fn insert(
        &mut self,
        request_id: RequestId,
        outstanding: OutstandingRequest,
    ) -> Result<(), SandboxError> {
        if self.outstanding.contains_key(&request_id) {
            return Err(protocol_error());
        }
        self.outstanding.insert(request_id, outstanding);
        Ok(())
    }

    #[allow(clippy::result_large_err)]
    pub fn validate_and_complete(
        &mut self,
        request_id: RequestId,
        response: &BrokerResponse,
    ) -> Result<(), SandboxError> {
        response.validate()?;
        let outstanding = self
            .outstanding
            .get(&request_id)
            .ok_or_else(protocol_error)?;
        let valid = match outstanding {
            OutstandingRequest::Handshake(expected) => match response {
                BrokerResponse::Handshake(response) => response.validate_against(expected).is_ok(),
                BrokerResponse::Error(_) => true,
                _ => false,
            },
            OutstandingRequest::Ordinary(kind) => response_allowed(*kind, response),
        };
        if !valid {
            return Err(protocol_error());
        }
        self.outstanding.remove(&request_id);
        Ok(())
    }
}

fn response_allowed(kind: BrokerRequestKind, response: &BrokerResponse) -> bool {
    if matches!(response, BrokerResponse::Error(_)) {
        return true;
    }
    match response {
        BrokerResponse::Handshake(_) => kind == BrokerRequestKind::Handshake,
        BrokerResponse::SessionCreated(_) => kind == BrokerRequestKind::CreateSession,
        BrokerResponse::Acknowledged => matches!(
            kind,
            BrokerRequestKind::CloseSession
                | BrokerRequestKind::CancelSession
                | BrokerRequestKind::ProcessRelease
                | BrokerRequestKind::PtyRelease
        ),
        BrokerResponse::Process(_) => matches!(
            kind,
            BrokerRequestKind::ProcessStart
                | BrokerRequestKind::ProcessDetail
                | BrokerRequestKind::ProcessWrite
                | BrokerRequestKind::ProcessCancel
        ),
        BrokerResponse::Processes(_) => kind == BrokerRequestKind::ProcessList,
        BrokerResponse::ProcessRead(_) => kind == BrokerRequestKind::ProcessRead,
        BrokerResponse::Pty(_) => matches!(
            kind,
            BrokerRequestKind::PtyStart
                | BrokerRequestKind::PtyDetail
                | BrokerRequestKind::PtyWrite
                | BrokerRequestKind::PtyResize
                | BrokerRequestKind::PtyStop
        ),
        BrokerResponse::Ptys(_) => kind == BrokerRequestKind::PtyList,
        BrokerResponse::PtyRead(_) => kind == BrokerRequestKind::PtyRead,
        BrokerResponse::PublicCatalog(_) => kind == BrokerRequestKind::CatalogPublicSnapshot,
        BrokerResponse::Pong => kind == BrokerRequestKind::Ping,
        BrokerResponse::OperationError(error) => operation_error_allowed(kind, error),
        BrokerResponse::Error(_) => true,
    }
}

fn operation_error_allowed(kind: BrokerRequestKind, error: &OperationError) -> bool {
    let code = error.code();
    let registry_subject_matches = match (kind, error.subject()) {
        (
            BrokerRequestKind::ProcessStart,
            OperationSubject::Registry(ExecutionSurface::Process),
        )
        | (BrokerRequestKind::PtyStart, OperationSubject::Registry(ExecutionSurface::Pty)) => true,
        (_, OperationSubject::Registry(_)) => false,
        _ => true,
    };
    if !registry_subject_matches {
        return false;
    }
    match kind {
        BrokerRequestKind::ProcessStart => matches!(
            code,
            OperationErrorCode::ProcessError | OperationErrorCode::RegistryFull
        ),
        BrokerRequestKind::ProcessDetail | BrokerRequestKind::ProcessRead => {
            code == OperationErrorCode::ProcessNotFound
        }
        BrokerRequestKind::ProcessWrite => matches!(
            code,
            OperationErrorCode::ProcessNotFound
                | OperationErrorCode::ProcessNotRunning
                | OperationErrorCode::ProcessStdinClosed
                | OperationErrorCode::ProcessError
        ),
        BrokerRequestKind::ProcessCancel => matches!(
            code,
            OperationErrorCode::ProcessNotFound | OperationErrorCode::ProcessError
        ),
        BrokerRequestKind::ProcessRelease => matches!(
            code,
            OperationErrorCode::ProcessNotFound | OperationErrorCode::ProcessRunning
        ),
        BrokerRequestKind::PtyStart => {
            matches!(
                code,
                OperationErrorCode::PtyError | OperationErrorCode::RegistryFull
            )
        }
        BrokerRequestKind::PtyDetail | BrokerRequestKind::PtyRead => {
            code == OperationErrorCode::PtyNotFound
        }
        BrokerRequestKind::PtyWrite | BrokerRequestKind::PtyResize => matches!(
            code,
            OperationErrorCode::PtyNotFound
                | OperationErrorCode::PtyClosed
                | OperationErrorCode::PtyError
        ),
        BrokerRequestKind::PtyStop => matches!(
            code,
            OperationErrorCode::PtyNotFound | OperationErrorCode::PtyError
        ),
        BrokerRequestKind::PtyRelease => matches!(
            code,
            OperationErrorCode::PtyNotFound | OperationErrorCode::PtyRunning
        ),
        BrokerRequestKind::Handshake
        | BrokerRequestKind::CreateSession
        | BrokerRequestKind::CloseSession
        | BrokerRequestKind::CancelSession
        | BrokerRequestKind::ProcessList
        | BrokerRequestKind::PtyList
        | BrokerRequestKind::CatalogPublicSnapshot
        | BrokerRequestKind::Ping => false,
    }
}

#[allow(dead_code)]
pub(crate) fn event_permitted_for(kind: BrokerRequestKind, event: &BrokerEvent) -> bool {
    match kind {
        BrokerRequestKind::CreateSession => {
            matches!(event, BrokerEvent::SessionState(_) | BrokerEvent::Audit(_))
        }
        BrokerRequestKind::CloseSession | BrokerRequestKind::CancelSession => true,
        BrokerRequestKind::ProcessStart
        | BrokerRequestKind::ProcessWrite
        | BrokerRequestKind::ProcessCancel
        | BrokerRequestKind::PtyStart
        | BrokerRequestKind::PtyWrite
        | BrokerRequestKind::PtyStop => {
            matches!(event, BrokerEvent::Audit(_) | BrokerEvent::Terminated(_))
        }
        BrokerRequestKind::PtyResize => matches!(event, BrokerEvent::Audit(_)),
        BrokerRequestKind::Handshake
        | BrokerRequestKind::ProcessList
        | BrokerRequestKind::ProcessDetail
        | BrokerRequestKind::ProcessRead
        | BrokerRequestKind::ProcessRelease
        | BrokerRequestKind::PtyList
        | BrokerRequestKind::PtyDetail
        | BrokerRequestKind::PtyRead
        | BrokerRequestKind::PtyRelease
        | BrokerRequestKind::CatalogPublicSnapshot
        | BrokerRequestKind::Ping => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::{AuditContext, AuditEventKind};
    use crate::catalog::{
        ApprovalActor, Architecture, ArtifactKind, CatalogPathNormalizer, CatalogRecord,
        CatalogSnapshot, HashedArtifact, OperatingSystem, PlatformId,
    };
    use crate::policy::{
        ArgumentAuditMode, AuditPolicy, EnvironmentPolicy, ScratchDisposition, WorkspaceIdentity,
        WorkspaceIdentityResolver, WorkspaceRequest,
    };
    use crate::version::{CATALOG_SCHEMA_V1, POLICY_SCHEMA_V1};

    struct TestNormalizer;

    impl CatalogPathNormalizer for TestNormalizer {
        fn normalize(&self, _platform: &PlatformId, path: &str) -> Result<String, SandboxError> {
            Ok(path.replace('/', "\\").to_ascii_lowercase())
        }
    }

    struct TestWorkspaceResolver;

    impl WorkspaceIdentityResolver for TestWorkspaceResolver {
        fn resolve(&self, request: &WorkspaceRequest) -> Result<WorkspaceIdentity, SandboxError> {
            Ok(WorkspaceIdentity {
                requested_root: request.requested_root.clone(),
                canonical_root: "C:/workspace".to_owned(),
                native_object_identity: "volume-7:file-42".to_owned(),
            })
        }
    }

    fn test_policy(destination: &str) -> ValidatedExecutionPolicy {
        let generation = CatalogGeneration::new(7).unwrap();
        let platform = PlatformId {
            os: OperatingSystem::Windows,
            arch: Architecture::X86_64,
        };
        let tool_id = ToolId::parse("git").unwrap();
        let catalog = CatalogSnapshot {
            schema_version: CATALOG_SCHEMA_V1,
            generation,
            platform,
            records: vec![CatalogRecord {
                schema_version: CATALOG_SCHEMA_V1,
                generation,
                tool_id: tool_id.clone(),
                platform,
                original_source_path: "C:/source/git.exe".to_owned(),
                executable: HashedArtifact {
                    logical_name: "git-executable".to_owned(),
                    managed_canonical_path: "C:/managed/git.exe".to_owned(),
                    sha256: Sha256Digest::hash(b"git"),
                    kind: ArtifactKind::Executable,
                },
                helpers: Vec::new(),
                non_system_libraries: Vec::new(),
                resources: Vec::new(),
                transport_adapter: None,
                approval_actor: ApprovalActor {
                    display_name: "Administrator".to_owned(),
                    mechanism: "test".to_owned(),
                },
                approved_at: UnixMillis::new(1),
                replaces: None,
            }],
            revoked_tools: Vec::new(),
        }
        .validate(&TestNormalizer)
        .unwrap();
        ExecutionPolicyRequest {
            schema_version: POLICY_SCHEMA_V1,
            access: ExecutionAccess::Read,
            allow_process: true,
            allow_pty: true,
            workspace: Some(WorkspaceRequest {
                requested_root: "C:/workspace".to_owned(),
            }),
            scratch_disposition: ScratchDisposition::DeleteOnCleanCloseRetainOtherwise,
            catalog_generation: generation,
            activated_tools: vec![tool_id],
            destinations: vec![DestinationGrant::parse(destination).unwrap()],
            environment: EnvironmentPolicy { base: Vec::new() },
            resource_limits: Some(ResourceLimits {
                descendant_processes: 4,
                memory_bytes: 64 * 1024 * 1024,
                cpu_time_ms: 10_000,
                wall_time_ms: 5_000,
                open_descriptors_or_handles: 64,
                captured_output_bytes: 4_096,
            }),
            audit_policy: AuditPolicy {
                arguments: ArgumentAuditMode::CountOnly,
            },
        }
        .validate(&catalog, &TestWorkspaceResolver)
        .unwrap()
    }

    fn audit_event(policy: &ValidatedExecutionPolicy) -> BrokerEvent {
        let context = AuditContext::new(
            SessionId::parse("session-01").unwrap(),
            ScratchId::parse("scratch-01").unwrap(),
            BackendIdentity::new("windows-lpac", "1").unwrap(),
            PROTOCOL_V1,
            EnforcementState::Enforced,
            policy,
        )
        .unwrap();
        BrokerEvent::Audit(AuditRecord::new(
            UnixMillis::new(1),
            context,
            AuditEventKind::LaunchRequested {
                surface: ExecutionSurface::Process,
                tool_id: Some(ToolId::parse("git").unwrap()),
                argument_count: 2,
            },
        ))
    }

    #[test]
    fn policy_aware_audit_validation_uses_active_policy_and_outer_session() {
        let active = test_policy("github.com:443");
        let different = test_policy("example.com:443");
        let event = audit_event(&active);
        let session_id = SessionId::parse("session-01").unwrap();

        event.validate_against_policy(&session_id, &active).unwrap();
        assert!(event
            .validate_against_policy(&session_id, &different)
            .is_err());
        assert!(event
            .validate_against_policy(&SessionId::parse("session-02").unwrap(), &active)
            .is_err());
    }

    #[test]
    fn permitted_event_table_is_exact_for_every_request_kind() {
        let policy = test_policy("github.com:443");
        let audit = audit_event(&policy);
        let state = BrokerEvent::SessionState(SessionState::Ready);
        let terminated = BrokerEvent::Terminated(TerminationNotice {
            reason: TerminationReason::CancelledByHost,
            process_tree_ids: vec![ProcessTreeId::new(0)],
            error: None,
        });
        let cases = [
            (BrokerRequestKind::Handshake, [false, false, false]),
            (BrokerRequestKind::CreateSession, [true, true, false]),
            (BrokerRequestKind::CloseSession, [true, true, true]),
            (BrokerRequestKind::CancelSession, [true, true, true]),
            (BrokerRequestKind::ProcessStart, [true, false, true]),
            (BrokerRequestKind::ProcessList, [false, false, false]),
            (BrokerRequestKind::ProcessDetail, [false, false, false]),
            (BrokerRequestKind::ProcessRead, [false, false, false]),
            (BrokerRequestKind::ProcessWrite, [true, false, true]),
            (BrokerRequestKind::ProcessCancel, [true, false, true]),
            (BrokerRequestKind::ProcessRelease, [false, false, false]),
            (BrokerRequestKind::PtyStart, [true, false, true]),
            (BrokerRequestKind::PtyList, [false, false, false]),
            (BrokerRequestKind::PtyDetail, [false, false, false]),
            (BrokerRequestKind::PtyRead, [false, false, false]),
            (BrokerRequestKind::PtyWrite, [true, false, true]),
            (BrokerRequestKind::PtyResize, [true, false, false]),
            (BrokerRequestKind::PtyStop, [true, false, true]),
            (BrokerRequestKind::PtyRelease, [false, false, false]),
            (
                BrokerRequestKind::CatalogPublicSnapshot,
                [false, false, false],
            ),
            (BrokerRequestKind::Ping, [false, false, false]),
        ];

        for (kind, expected) in cases {
            assert_eq!(
                event_permitted_for(kind, &audit),
                expected[0],
                "{kind:?} audit"
            );
            assert_eq!(
                event_permitted_for(kind, &state),
                expected[1],
                "{kind:?} state"
            );
            assert_eq!(
                event_permitted_for(kind, &terminated),
                expected[2],
                "{kind:?} terminated"
            );
        }
    }
}
