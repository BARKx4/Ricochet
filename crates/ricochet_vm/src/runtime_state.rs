use std::collections::BTreeSet;
use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;

use ricochet_application::{HostDisplayLabel, SecretName};
use ricochet_sandbox::DestinationGrant;
use ricochet_secrets::{SecretSessionContext, SecretsHttpExecutor};

use crate::{
    ApprovalRegistry, HttpStreamRegistry, ProcessRegistry, PtyRegistry, TcpListenerRegistry,
    TcpSocketRegistry, UploadStreamRegistry, WebSocketListenerRegistry, WebSocketRegistry,
    WorkspaceWriteRegistry,
};

#[derive(Clone)]
pub(crate) struct HostRuntimeState {
    pub(crate) filesystem_enabled: bool,
    pub(crate) filesystem_root: Option<PathBuf>,
    pub(crate) filesystem_writes_enabled: bool,
    pub(crate) http_enabled: bool,
    pub(crate) http_allowed_hosts: Option<BTreeSet<String>>,
    pub(crate) socket_enabled: bool,
    pub(crate) socket_allowed_hosts: Option<BTreeSet<String>>,
    pub(crate) process_enabled: bool,
    pub(crate) process_root: Option<PathBuf>,
    pub(crate) pty_enabled: bool,
    pub(crate) terminal_enabled: bool,
    pub(crate) webview_enabled: bool,
    pub(crate) environment_enabled: bool,
    pub(crate) environment_allowed_names: Option<BTreeSet<String>>,
    pub(crate) sleep_enabled: bool,
}

#[derive(Clone)]
pub(crate) struct SharedRuntimeState {
    pub(crate) security_domain_id: ricochet_secrets::SecurityDomainId,
    pub(crate) secret_session_bridge: Option<Arc<dyn HostSecureSessionBridge>>,
    pub(crate) http_allowed_destinations: BTreeSet<DestinationGrant>,
    pub(crate) secrets_http_executor: SecretsHttpExecutor,
    pub(crate) http_stream_registry: HttpStreamRegistry,
    pub(crate) upload_stream_registry: UploadStreamRegistry,
    pub(crate) tcp_socket_registry: TcpSocketRegistry,
    pub(crate) tcp_listener_registry: TcpListenerRegistry,
    pub(crate) websocket_registry: WebSocketRegistry,
    pub(crate) websocket_listener_registry: WebSocketListenerRegistry,
    pub(crate) process_registry: ProcessRegistry,
    pub(crate) pty_registry: PtyRegistry,
    pub(crate) approval_registry: ApprovalRegistry,
    pub(crate) workspace_write_registry: WorkspaceWriteRegistry,
}

pub trait HostSecureSessionBridge: Send + Sync {
    fn session_context(&self) -> SecretSessionContext;

    fn issue_action(
        &self,
        request: SecureSessionActionRequest,
    ) -> Result<SecureSessionActionDescriptor, SecretSessionBridgeError>;
}

#[derive(Clone)]
pub struct SecureSessionActionRequest {
    button_label: HostDisplayLabel,
    slot_name: SecretName,
    prompt_label: HostDisplayLabel,
    callback_word: String,
}

#[derive(Clone, PartialEq, Eq)]
pub struct SecureSessionActionDescriptor {
    action_id: String,
    button_label: HostDisplayLabel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SecretSessionBridgeErrorKind {
    InvalidActionId,
    Capacity,
    Closed,
    Host,
}

#[derive(Clone, PartialEq, Eq)]
pub struct SecretSessionBridgeError {
    kind: SecretSessionBridgeErrorKind,
}

impl SecureSessionActionRequest {
    pub(crate) fn new(
        button_label: HostDisplayLabel,
        slot_name: SecretName,
        prompt_label: HostDisplayLabel,
        callback_word: String,
    ) -> Self {
        Self {
            button_label,
            slot_name,
            prompt_label,
            callback_word,
        }
    }

    pub fn button_label(&self) -> &HostDisplayLabel {
        &self.button_label
    }

    pub fn slot_name(&self) -> &SecretName {
        &self.slot_name
    }

    pub fn prompt_label(&self) -> &HostDisplayLabel {
        &self.prompt_label
    }

    pub fn callback_word(&self) -> &str {
        &self.callback_word
    }
}

impl SecureSessionActionDescriptor {
    pub fn from_host(
        action_id: String,
        button_label: HostDisplayLabel,
    ) -> Result<Self, SecretSessionBridgeError> {
        if action_id.len() != 64
            || !action_id
                .bytes()
                .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
        {
            return Err(SecretSessionBridgeError::new(
                SecretSessionBridgeErrorKind::InvalidActionId,
            ));
        }
        Ok(Self {
            action_id,
            button_label,
        })
    }

    pub fn action_id(&self) -> &str {
        &self.action_id
    }

    pub fn button_label(&self) -> &HostDisplayLabel {
        &self.button_label
    }
}

impl SecretSessionBridgeError {
    pub fn new(kind: SecretSessionBridgeErrorKind) -> Self {
        Self { kind }
    }

    pub fn kind(&self) -> SecretSessionBridgeErrorKind {
        self.kind
    }
}

impl fmt::Debug for SecureSessionActionRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("<secure-session-action-request>")
    }
}

impl fmt::Debug for SecureSessionActionDescriptor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("<secure-session-action>")
    }
}

impl fmt::Debug for SecretSessionBridgeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SecretSessionBridgeError")
            .field("kind", &self.kind)
            .finish()
    }
}

impl fmt::Display for SecretSessionBridgeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("secure session host action failed")
    }
}

impl std::error::Error for SecretSessionBridgeError {}
