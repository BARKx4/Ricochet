use std::collections::BTreeSet;
use std::path::PathBuf;

use ricochet_sandbox::DestinationGrant;

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
    pub(crate) http_allowed_destinations: BTreeSet<DestinationGrant>,
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
