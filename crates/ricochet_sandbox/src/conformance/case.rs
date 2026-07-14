use crate::{
    AuditContext, ConfirmedExecutionCapabilities, ExecutionAccess, ExecutionSurface,
    OperatingSystem, ResourceLimitKind, ResourceLimits, SandboxErrorCode, TerminationReason,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ConformanceLevel {
    Model,
    RealOs,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PolicyProbe {
    VersionRejection,
    UnknownFieldRejection,
    InvalidAccessCombination,
    GrantBroadeningRejected,
    GrantNarrowingAllowed,
    CatalogClosure,
    FingerprintMismatch,
    Revocation,
    EnvironmentRedaction,
    ProcessPtyParity,
    LifecycleOrdering,
    CompleteTreeCancellation,
    MockHonesty,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FilesystemProbe {
    WorkspaceReadAllowed,
    WorkspaceWriteDeniedRead,
    WorkspaceWriteAllowed,
    ScratchReadWriteAllowed,
    OutsideRead,
    OutsideWrite,
    OutsideCreate,
    OutsideDelete,
    OutsideRename,
    OutsideLink,
    OutsideMetadata,
    OutsideExecute,
    SymlinkEscape,
    JunctionEscape,
    ReparseEscape,
    HardLinkEscape,
    MountEscape,
    DescriptorEscape,
    NamespaceEscape,
    RenameRace,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HostResourceProbe {
    UserProfile,
    Registry,
    Keychain,
    Proc,
    CredentialStore,
    Device,
    Clipboard,
    Ipc,
    InheritedHandle,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ExecutableProbe {
    ApprovedRootAllowed,
    ApprovedHelperAllowed,
    DirectChild,
    Grandchild,
    ShellHelper,
    Interpreter,
    FingerprintSubstitution,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NetworkProbe {
    GrantedHttpAdapterAllowed,
    GrantedSshAdapterAllowed,
    DirectSocket,
    AdapterBypass,
    DnsRebinding,
    Ipv4Literal,
    Ipv6Literal,
    SharedIp,
    PortSubstitution,
    Localhost,
    PrivateRange,
    Udp,
    Quic,
    Listener,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum IsolationProbe {
    CrossSessionScratch,
    CrossSessionCatalog,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LifecycleProbe {
    TimeoutTree,
    CancelTree,
    RevocationTree,
    BrokerShutdownTree,
    ResourceLimit(ResourceLimitKind),
    RegistryConsistency,
    CleanCloseScratchDeleted,
    CrashScratchRetained,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AvailabilityProbe {
    BrokerCrash,
    StaleProtocol,
    PartialInstallation,
    InactiveEnforcement,
    UnsupportedKernel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CompatibilityProbe {
    FullSourceCompatibility,
    FullAuditTruth,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ProbeKind {
    Policy(PolicyProbe),
    Filesystem(FilesystemProbe),
    HostResource(HostResourceProbe),
    Executable(ExecutableProbe),
    Network(NetworkProbe),
    Isolation(IsolationProbe),
    Lifecycle(LifecycleProbe),
    Availability(AvailabilityProbe),
    Compatibility(CompatibilityProbe),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExpectedOutcome {
    Allowed,
    DeniedBySandbox,
    Denied(SandboxErrorCode),
    TerminatesTree { reason: TerminationReason },
    Unavailable(SandboxErrorCode),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExpectedToolFingerprint {
    pub tool_id: &'static str,
    pub sha256_hex: &'static str,
    pub helper_ids: &'static [&'static str],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExpectedDestination {
    pub host: &'static str,
    pub port: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExpectedSessionAudit {
    pub catalog_generation: u64,
    pub tools: &'static [ExpectedToolFingerprint],
    pub destinations: &'static [ExpectedDestination],
    pub resource_limits: Option<ResourceLimits>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AuditExpectation {
    AbsentBeforeSession,
    Session(ExpectedSessionAudit),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ObservedOutcome {
    Allowed,
    DeniedBySandbox,
    Denied(SandboxErrorCode),
    TreeState {
        reason: TerminationReason,
        descendants_alive: u32,
    },
    Unavailable(SandboxErrorCode),
    Unexpected(SandboxErrorCode),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProbeStatus {
    Passed,
    Failed(String),
    NotRun(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConformanceCase {
    pub id: &'static str,
    pub level: ConformanceLevel,
    pub probe: ProbeKind,
    pub accesses: &'static [ExecutionAccess],
    pub platforms: &'static [OperatingSystem],
    pub surfaces: &'static [ExecutionSurface],
    pub expected: ExpectedOutcome,
    pub expected_audit: AuditExpectation,
}

#[derive(Clone)]
pub struct ProbeObservation {
    pub outcome: ObservedOutcome,
    pub capabilities: Option<ConfirmedExecutionCapabilities>,
    pub audit: Option<AuditContext>,
    pub audit_codes: Vec<SandboxErrorCode>,
}

#[allow(clippy::large_enum_variant)]
pub enum ProbeAttempt {
    Observed(ProbeObservation),
    NotRun(String),
}
