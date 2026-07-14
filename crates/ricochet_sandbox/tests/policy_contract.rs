use std::cell::Cell;
use std::collections::BTreeSet;

use ricochet_sandbox::{
    resolve_legacy_access, ApprovalActor, Architecture, ArgumentAuditMode, ArtifactKind,
    AuditPolicy, CatalogGeneration, CatalogPathNormalizer, CatalogRecord, CatalogSnapshot,
    DestinationGrant, DiagnosticMetadata, EffectiveEnvironment, EnvironmentPolicy,
    EnvironmentVariable, ExecutionAccess, ExecutionGrant, ExecutionPolicyRequest, ExecutionSurface,
    FailedGuarantee, GrantSet, HashedArtifact, LaunchEnvironment, OperatingSystem, PlatformId,
    Remediation, ResourceLimits, SandboxError, ScratchDisposition, Sha256Digest, ToolId,
    ToolReference, UnixMillis, ValidatedCatalogSnapshot, WorkspaceIdentity,
    WorkspaceIdentityResolver, WorkspaceRequest, CATALOG_SCHEMA_V1, POLICY_SCHEMA_V1,
};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{json, Value};

fn tool_id(value: &str) -> ToolId {
    ToolId::parse(value).unwrap()
}

fn generation(value: u64) -> CatalogGeneration {
    CatalogGeneration::new(value).unwrap()
}

fn digest(value: &str) -> Sha256Digest {
    Sha256Digest::hash(value.as_bytes())
}

fn destination(value: &str) -> DestinationGrant {
    value.parse().unwrap()
}

fn platform() -> PlatformId {
    PlatformId {
        os: OperatingSystem::Windows,
        arch: Architecture::X86_64,
    }
}

struct FixturePathNormalizer;

impl CatalogPathNormalizer for FixturePathNormalizer {
    fn normalize(&self, _platform: &PlatformId, path: &str) -> Result<String, SandboxError> {
        Ok(path.replace('/', "\\").to_ascii_lowercase())
    }
}

fn artifact(base: &str, logical_name: &str, seed: &str, kind: ArtifactKind) -> HashedArtifact {
    HashedArtifact {
        logical_name: logical_name.to_owned(),
        managed_canonical_path: format!(r"{base}\{logical_name}.bin"),
        sha256: digest(seed),
        kind,
    }
}

fn catalog_snapshot(base: &str, executable_seed: &str) -> CatalogSnapshot {
    let helper_executable = artifact(
        base,
        "helper-executable",
        "helper-executable",
        ArtifactKind::Executable,
    );
    let helper = CatalogRecord {
        schema_version: CATALOG_SCHEMA_V1,
        generation: generation(7),
        tool_id: tool_id("helper"),
        platform: platform(),
        original_source_path: r"C:\source\helper.exe".to_owned(),
        executable: helper_executable.clone(),
        helpers: Vec::new(),
        non_system_libraries: Vec::new(),
        resources: Vec::new(),
        transport_adapter: None,
        approval_actor: ApprovalActor {
            display_name: "Sandbox Administrator".to_owned(),
            mechanism: "interactive-consent".to_owned(),
        },
        approved_at: UnixMillis::new(1_700_000_000_000),
        replaces: None,
    };
    let main = CatalogRecord {
        schema_version: CATALOG_SCHEMA_V1,
        generation: generation(7),
        tool_id: tool_id("main"),
        platform: platform(),
        original_source_path: r"C:\source\main.exe".to_owned(),
        executable: artifact(
            base,
            "main-executable",
            executable_seed,
            ArtifactKind::Executable,
        ),
        helpers: vec![ToolReference {
            tool_id: tool_id("helper"),
            sha256: helper_executable.sha256,
        }],
        non_system_libraries: vec![
            artifact(base, "z-library", "z-library", ArtifactKind::Library),
            artifact(base, "a-library", "a-library", ArtifactKind::Library),
        ],
        resources: vec![
            artifact(base, "z-resource", "z-resource", ArtifactKind::Resource),
            artifact(base, "a-resource", "a-resource", ArtifactKind::Resource),
        ],
        transport_adapter: None,
        approval_actor: ApprovalActor {
            display_name: "Sandbox Administrator".to_owned(),
            mechanism: "interactive-consent".to_owned(),
        },
        approved_at: UnixMillis::new(1_700_000_000_001),
        replaces: None,
    };

    CatalogSnapshot {
        schema_version: CATALOG_SCHEMA_V1,
        generation: generation(7),
        platform: platform(),
        records: vec![main, helper],
        revoked_tools: vec![tool_id("revoked")],
    }
}

fn validated_catalog() -> ValidatedCatalogSnapshot {
    catalog_snapshot(r"C:\broker-private", "main-executable")
        .validate(&FixturePathNormalizer)
        .unwrap()
}

fn finite_limits() -> ResourceLimits {
    ResourceLimits {
        descendant_processes: 4,
        memory_bytes: 256 * 1024 * 1024,
        cpu_time_ms: 30_000,
        wall_time_ms: 60_000,
        open_descriptors_or_handles: 64,
        captured_output_bytes: 1024 * 1024,
    }
}

fn workspace_identity() -> WorkspaceIdentity {
    WorkspaceIdentity {
        requested_root: r"C:\untrusted\workspace".to_owned(),
        canonical_root: r"C:\resolved\workspace".to_owned(),
        native_object_identity: "volume-7:file-42".to_owned(),
    }
}

struct FakeResolver {
    calls: Cell<usize>,
    identity: WorkspaceIdentity,
    fail: bool,
}

impl FakeResolver {
    fn succeeding() -> Self {
        Self {
            calls: Cell::new(0),
            identity: workspace_identity(),
            fail: false,
        }
    }

    fn with_identity(identity: WorkspaceIdentity) -> Self {
        Self {
            calls: Cell::new(0),
            identity,
            fail: false,
        }
    }

    fn failing() -> Self {
        Self {
            calls: Cell::new(0),
            identity: workspace_identity(),
            fail: true,
        }
    }
}

impl WorkspaceIdentityResolver for FakeResolver {
    fn resolve(&self, _request: &WorkspaceRequest) -> Result<WorkspaceIdentity, SandboxError> {
        self.calls.set(self.calls.get() + 1);
        if self.fail {
            Err(SandboxError::unavailable(
                None,
                FailedGuarantee::BrokerAvailability,
                Remediation::InspectSandboxDoctor,
                DiagnosticMetadata::empty(),
            ))
        } else {
            Ok(self.identity.clone())
        }
    }
}

fn audit_policy() -> AuditPolicy {
    AuditPolicy {
        arguments: ArgumentAuditMode::CountOnly,
    }
}

fn environment(
    entries: impl IntoIterator<Item = (&'static str, &'static str)>,
) -> EnvironmentPolicy {
    EnvironmentPolicy {
        base: entries
            .into_iter()
            .map(|(name, value)| EnvironmentVariable {
                name: name.to_owned(),
                value: value.to_owned(),
            })
            .collect(),
    }
}

fn constrained_request(access: ExecutionAccess) -> ExecutionPolicyRequest {
    ExecutionPolicyRequest {
        schema_version: POLICY_SCHEMA_V1,
        access,
        allow_process: true,
        allow_pty: true,
        workspace: Some(WorkspaceRequest {
            requested_root: r"C:\untrusted\workspace".to_owned(),
        }),
        scratch_disposition: ScratchDisposition::DeleteOnCleanCloseRetainOtherwise,
        catalog_generation: generation(7),
        activated_tools: vec![tool_id("main")],
        destinations: vec![destination("z.example:443"), destination("a.example:443")],
        environment: environment([("LANG", "C"), ("TOKEN", "alpha")]),
        resource_limits: Some(finite_limits()),
        audit_policy: audit_policy(),
    }
}

fn full_request() -> ExecutionPolicyRequest {
    ExecutionPolicyRequest {
        schema_version: POLICY_SCHEMA_V1,
        access: ExecutionAccess::Full,
        allow_process: true,
        allow_pty: false,
        workspace: None,
        scratch_disposition: ScratchDisposition::DeleteOnCleanCloseRetainOtherwise,
        catalog_generation: generation(7),
        activated_tools: Vec::new(),
        destinations: Vec::new(),
        environment: environment([("PATH", r"C:\user-bin"), ("LANG", "C")]),
        resource_limits: None,
        audit_policy: audit_policy(),
    }
}

#[test]
fn legacy_grants_default_to_full_only_when_execution_is_enabled() {
    assert_eq!(
        resolve_legacy_access(None, true, false),
        Some(ExecutionAccess::Full)
    );
    assert_eq!(
        resolve_legacy_access(None, false, true),
        Some(ExecutionAccess::Full)
    );
    assert_eq!(resolve_legacy_access(None, false, false), None);
    assert_eq!(
        resolve_legacy_access(Some(ExecutionAccess::Read), true, true),
        Some(ExecutionAccess::Read),
    );
    assert_eq!(
        resolve_legacy_access(Some(ExecutionAccess::Workspace), false, false),
        Some(ExecutionAccess::Workspace),
    );
}

#[test]
fn execution_access_has_explicit_security_ordering() {
    assert!(ExecutionAccess::Read < ExecutionAccess::Workspace);
    assert!(ExecutionAccess::Workspace < ExecutionAccess::Full);
    let mut accesses = vec![
        ExecutionAccess::Full,
        ExecutionAccess::Read,
        ExecutionAccess::Workspace,
    ];
    accesses.sort();
    assert_eq!(
        accesses,
        vec![
            ExecutionAccess::Read,
            ExecutionAccess::Workspace,
            ExecutionAccess::Full,
        ]
    );
}

#[test]
fn grant_intersection_never_broadens_access_surfaces_tools_or_destinations() {
    let server = ExecutionGrant {
        access: Some(ExecutionAccess::Full),
        allow_process: true,
        allow_pty: false,
        tools: GrantSet::Only(BTreeSet::from([tool_id("main"), tool_id("helper")])),
        destinations: GrantSet::Only(BTreeSet::from([
            destination("a.example:443"),
            destination("b.example:443"),
        ])),
    };
    let requested = ExecutionGrant {
        access: Some(ExecutionAccess::Workspace),
        allow_process: true,
        allow_pty: true,
        tools: GrantSet::Only(BTreeSet::from([tool_id("helper"), tool_id("third")])),
        destinations: GrantSet::Unrestricted,
    };

    let effective = server.intersect(&requested).unwrap();

    assert_eq!(effective.access, Some(ExecutionAccess::Workspace));
    assert!(effective.allow_process);
    assert!(!effective.allow_pty);
    assert_eq!(
        effective.tools,
        GrantSet::Only(BTreeSet::from([tool_id("helper")]))
    );
    let GrantSet::Only(destinations) = &effective.destinations else {
        panic!("intersection must retain the finite destination grant set");
    };
    assert_eq!(
        destinations
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        vec!["a.example:443", "b.example:443"]
    );
}

#[test]
fn unrestricted_is_the_identity_for_grant_set_intersection() {
    let only = GrantSet::Only(BTreeSet::from([tool_id("main")]));
    for (left, right) in [
        (GrantSet::Unrestricted, only.clone()),
        (only.clone(), GrantSet::Unrestricted),
    ] {
        let effective = ExecutionGrant {
            access: None,
            allow_process: true,
            allow_pty: true,
            tools: left,
            destinations: GrantSet::Unrestricted,
        }
        .intersect(&ExecutionGrant {
            access: Some(ExecutionAccess::Read),
            allow_process: true,
            allow_pty: true,
            tools: right,
            destinations: GrantSet::Unrestricted,
        })
        .unwrap();
        assert_eq!(effective.access, Some(ExecutionAccess::Read));
        assert_eq!(effective.tools, only);
        assert!(matches!(effective.destinations, GrantSet::Unrestricted));
    }

    let unspecified = ExecutionGrant {
        access: None,
        allow_process: true,
        allow_pty: false,
        tools: GrantSet::Unrestricted,
        destinations: GrantSet::Unrestricted,
    }
    .intersect(&ExecutionGrant {
        access: None,
        allow_process: false,
        allow_pty: true,
        tools: GrantSet::Unrestricted,
        destinations: GrantSet::Unrestricted,
    })
    .unwrap();
    assert_eq!(unspecified.access, None);
    assert!(!unspecified.allow_process);
    assert!(!unspecified.allow_pty);
}

#[test]
fn finite_resource_limits_are_positive_and_cannot_exceed_a_ceiling() {
    let limits = finite_limits();
    limits.validate().unwrap();
    limits.ensure_not_above(&limits).unwrap();

    let mut invalid = finite_limits();
    invalid.descendant_processes = 0;
    assert!(invalid.validate().is_err());
    let mut invalid = finite_limits();
    invalid.memory_bytes = 0;
    assert!(invalid.validate().is_err());
    let mut invalid = finite_limits();
    invalid.cpu_time_ms = 0;
    assert!(invalid.validate().is_err());
    let mut invalid = finite_limits();
    invalid.wall_time_ms = 0;
    assert!(invalid.validate().is_err());
    let mut invalid = finite_limits();
    invalid.open_descriptors_or_handles = 0;
    assert!(invalid.validate().is_err());
    let mut invalid = finite_limits();
    invalid.captured_output_bytes = 0;
    assert!(invalid.validate().is_err());

    let ceiling = finite_limits();
    let mut above = finite_limits();
    above.descendant_processes += 1;
    assert!(above.ensure_not_above(&ceiling).is_err());
    let mut above = finite_limits();
    above.memory_bytes += 1;
    assert!(above.ensure_not_above(&ceiling).is_err());
    let mut above = finite_limits();
    above.cpu_time_ms += 1;
    assert!(above.ensure_not_above(&ceiling).is_err());
    let mut above = finite_limits();
    above.wall_time_ms += 1;
    assert!(above.ensure_not_above(&ceiling).is_err());
    let mut above = finite_limits();
    above.open_descriptors_or_handles += 1;
    assert!(above.ensure_not_above(&ceiling).is_err());
    let mut above = finite_limits();
    above.captured_output_bytes += 1;
    assert!(above.ensure_not_above(&ceiling).is_err());
}

#[test]
fn constrained_policies_require_workspace_and_finite_limits() {
    let catalog = validated_catalog();
    for access in [ExecutionAccess::Read, ExecutionAccess::Workspace] {
        let resolver = FakeResolver::succeeding();
        let mut missing_workspace = constrained_request(access);
        missing_workspace.workspace = None;
        assert!(missing_workspace.validate(&catalog, &resolver).is_err());
        assert_eq!(resolver.calls.get(), 0);

        let resolver = FakeResolver::succeeding();
        let mut missing_limits = constrained_request(access);
        missing_limits.resource_limits = None;
        assert!(missing_limits.validate(&catalog, &resolver).is_err());
        assert_eq!(resolver.calls.get(), 0);

        let resolver = FakeResolver::succeeding();
        let mut zero_limits = constrained_request(access);
        zero_limits.resource_limits.as_mut().unwrap().wall_time_ms = 0;
        assert!(zero_limits.validate(&catalog, &resolver).is_err());
        assert_eq!(resolver.calls.get(), 0);
    }
}

#[test]
fn schema_and_execution_surface_are_validated_before_resolution() {
    let catalog = validated_catalog();
    let resolver = FakeResolver::succeeding();
    let mut wrong_schema = constrained_request(ExecutionAccess::Read);
    wrong_schema.schema_version += 1;
    assert!(wrong_schema.validate(&catalog, &resolver).is_err());
    assert_eq!(resolver.calls.get(), 0);

    let resolver = FakeResolver::succeeding();
    let mut no_surface = constrained_request(ExecutionAccess::Read);
    no_surface.allow_process = false;
    no_surface.allow_pty = false;
    assert!(no_surface.validate(&catalog, &resolver).is_err());
    assert_eq!(resolver.calls.get(), 0);

    let resolver = FakeResolver::succeeding();
    let mut no_surface = full_request();
    no_surface.allow_process = false;
    assert!(no_surface.validate(&catalog, &resolver).is_err());
    assert_eq!(resolver.calls.get(), 0);
}

#[test]
fn resolver_output_is_recomputed_once_and_is_the_only_workspace_identity_stored() {
    let catalog = validated_catalog();
    let trusted = WorkspaceIdentity {
        requested_root: "resolver-recomputed-request".to_owned(),
        canonical_root: "resolver-canonical-root".to_owned(),
        native_object_identity: "resolver-native-object".to_owned(),
    };
    let resolver = FakeResolver::with_identity(trusted.clone());
    let mut request = constrained_request(ExecutionAccess::Workspace);
    request.workspace.as_mut().unwrap().requested_root = "client-path-intent-only".to_owned();

    let policy = request.validate(&catalog, &resolver).unwrap();

    assert_eq!(resolver.calls.get(), 1);
    assert_eq!(policy.workspace_identity(), Some(&trusted));
}

#[test]
fn full_may_resolve_workspace_identity_and_use_optional_positive_limits() {
    let catalog = validated_catalog();
    let trusted = WorkspaceIdentity {
        requested_root: "full-resolver-request".to_owned(),
        canonical_root: "full-resolver-canonical".to_owned(),
        native_object_identity: "full-resolver-object".to_owned(),
    };
    let resolver = FakeResolver::with_identity(trusted.clone());
    let mut request = full_request();
    request.workspace = Some(WorkspaceRequest {
        requested_root: "full-client-intent".to_owned(),
    });
    request.resource_limits = Some(finite_limits());

    let policy = request.validate(&catalog, &resolver).unwrap();

    assert_eq!(resolver.calls.get(), 1);
    assert_eq!(policy.workspace_identity(), Some(&trusted));
    assert_eq!(policy.resource_limits(), Some(&finite_limits()));

    let resolver = FakeResolver::succeeding();
    let mut invalid = full_request();
    invalid.resource_limits = Some(finite_limits());
    invalid.resource_limits.as_mut().unwrap().cpu_time_ms = 0;
    assert!(invalid.validate(&catalog, &resolver).is_err());
    assert_eq!(resolver.calls.get(), 0);
}

#[test]
fn resolver_failure_precedes_catalog_preparation() {
    let catalog = validated_catalog();
    let resolver = FakeResolver::failing();
    let mut request = constrained_request(ExecutionAccess::Read);
    request.activated_tools = vec![tool_id("not-approved")];

    let error = request
        .validate(&catalog, &resolver)
        .err()
        .expect("resolver failure must reject validation");

    assert_eq!(resolver.calls.get(), 1);
    assert_eq!(error.kind(), "SandboxUnavailable");
}

#[test]
fn empty_resolver_produced_workspace_identity_fields_are_rejected() {
    let catalog = validated_catalog();
    for identity in [
        WorkspaceIdentity {
            requested_root: String::new(),
            ..workspace_identity()
        },
        WorkspaceIdentity {
            canonical_root: String::new(),
            ..workspace_identity()
        },
        WorkspaceIdentity {
            native_object_identity: String::new(),
            ..workspace_identity()
        },
    ] {
        let resolver = FakeResolver::with_identity(identity);
        assert!(constrained_request(ExecutionAccess::Read)
            .validate(&catalog, &resolver)
            .is_err());
        assert_eq!(resolver.calls.get(), 1);
    }
}

#[test]
fn catalog_generation_tools_and_destinations_are_pinned_deduplicated_and_sorted() {
    let catalog = validated_catalog();

    let resolver = FakeResolver::succeeding();
    let mut wrong_generation = constrained_request(ExecutionAccess::Read);
    wrong_generation.catalog_generation = generation(6);
    assert!(wrong_generation.validate(&catalog, &resolver).is_err());

    let resolver = FakeResolver::succeeding();
    let mut duplicate_tools = constrained_request(ExecutionAccess::Read);
    duplicate_tools.activated_tools.push(tool_id("main"));
    assert!(duplicate_tools.validate(&catalog, &resolver).is_err());

    let resolver = FakeResolver::succeeding();
    let mut duplicate_destinations = constrained_request(ExecutionAccess::Read);
    duplicate_destinations
        .destinations
        .push(destination("a.example:443"));
    assert!(duplicate_destinations
        .validate(&catalog, &resolver)
        .is_err());

    for denied in ["missing", "revoked"] {
        let resolver = FakeResolver::succeeding();
        let mut request = constrained_request(ExecutionAccess::Read);
        request.activated_tools = vec![tool_id(denied)];
        assert_eq!(
            request
                .validate(&catalog, &resolver)
                .err()
                .expect("unapproved tool must reject validation")
                .kind(),
            "ToolNotApproved"
        );
    }

    let resolver = FakeResolver::succeeding();
    let policy = constrained_request(ExecutionAccess::Read)
        .validate(&catalog, &resolver)
        .unwrap();
    assert_eq!(
        policy
            .destinations()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        vec!["a.example:443", "z.example:443"]
    );
    assert_eq!(
        policy
            .prepared_catalog()
            .tools()
            .keys()
            .map(ToolId::as_str)
            .collect::<Vec<_>>(),
        vec!["helper", "main"]
    );
}

#[test]
fn empty_constrained_tool_set_is_a_valid_deny_all_policy() {
    let catalog = validated_catalog();
    let resolver = FakeResolver::succeeding();
    let mut request = constrained_request(ExecutionAccess::Workspace);
    request.activated_tools.clear();

    let policy = request.validate(&catalog, &resolver).unwrap();

    assert!(policy.prepared_catalog().roots().is_empty());
    assert!(policy.prepared_catalog().tools().is_empty());
    assert!(policy.allows(ExecutionSurface::Process));
    assert!(policy.allows(ExecutionSurface::Pty));
}

#[test]
fn full_rejects_pseudo_restricted_tool_and_destination_lists() {
    let catalog = validated_catalog();
    let resolver = FakeResolver::succeeding();
    let mut tools = full_request();
    tools.activated_tools.push(tool_id("main"));
    assert!(tools.validate(&catalog, &resolver).is_err());

    let resolver = FakeResolver::succeeding();
    let mut destinations = full_request();
    destinations.destinations.push(destination("a.example:443"));
    assert!(destinations.validate(&catalog, &resolver).is_err());
}

#[test]
fn constrained_environment_rejects_invalid_duplicate_and_reserved_names() {
    let catalog = validated_catalog();
    for entries in [
        vec![("", "value")],
        vec![("A=B", "value")],
        vec![("NUL\0NAME", "value")],
        vec![("TOKEN", "one"), ("token", "two")],
        vec![("path", "value")],
        vec![("Home", "value")],
        vec![("userprofile", "value")],
        vec![("Temp", "value")],
        vec![("tmp", "value")],
        vec![("TmpDir", "value")],
        vec![("ricochet_broker_endpoint", "value")],
        vec![("Ricochet_Sandbox_Session", "value")],
        vec![("RICOCHET_SANDBOX_GREMLIN", "value")],
    ] {
        let resolver = FakeResolver::succeeding();
        let mut request = constrained_request(ExecutionAccess::Read);
        request.environment = EnvironmentPolicy {
            base: entries
                .into_iter()
                .map(|(name, value)| EnvironmentVariable {
                    name: name.to_owned(),
                    value: value.to_owned(),
                })
                .collect(),
        };
        assert!(request.validate(&catalog, &resolver).is_err());
    }
}

#[test]
fn environment_values_and_debug_output_never_expose_secrets() {
    let variable = EnvironmentVariable {
        name: "TOKEN".to_owned(),
        value: "super-secret-value".to_owned(),
    };
    let debug = format!("{variable:?}");
    assert!(debug.contains("TOKEN"));
    assert!(debug.contains("[REDACTED]"));
    assert!(!debug.contains("super-secret-value"));

    let catalog = validated_catalog();
    let resolver = FakeResolver::succeeding();
    let mut nul_value = constrained_request(ExecutionAccess::Read);
    nul_value.environment = environment([("TOKEN", "secret\0suffix")]);
    assert!(nul_value.validate(&catalog, &resolver).is_err());
}

#[test]
fn constrained_launch_environment_never_inherits_ambient_and_overlays_base() {
    let catalog = validated_catalog();
    let resolver = FakeResolver::succeeding();
    let policy = constrained_request(ExecutionAccess::Read)
        .validate(&catalog, &resolver)
        .unwrap();

    for clear_environment in [false, true] {
        let effective = policy
            .resolve_launch_environment(&LaunchEnvironment {
                clear_environment,
                entries: vec![
                    EnvironmentVariable {
                        name: "TOKEN".to_owned(),
                        value: "launch-secret".to_owned(),
                    },
                    EnvironmentVariable {
                        name: "LC_ALL".to_owned(),
                        value: "C.UTF-8".to_owned(),
                    },
                ],
            })
            .unwrap();
        assert!(!effective.inherit_ambient);
        assert_eq!(
            effective.entries,
            vec![
                EnvironmentVariable {
                    name: "LANG".to_owned(),
                    value: "C".to_owned(),
                },
                EnvironmentVariable {
                    name: "LC_ALL".to_owned(),
                    value: "C.UTF-8".to_owned(),
                },
                EnvironmentVariable {
                    name: "TOKEN".to_owned(),
                    value: "launch-secret".to_owned(),
                },
            ]
        );
    }
}

#[test]
fn constrained_launch_entries_cannot_replace_reserved_broker_values() {
    let catalog = validated_catalog();
    let resolver = FakeResolver::succeeding();
    let policy = constrained_request(ExecutionAccess::Workspace)
        .validate(&catalog, &resolver)
        .unwrap();

    for reserved in [
        "Path",
        "RICOCHET_SANDBOX_SESSION",
        "ricochet_sandbox_goblin",
    ] {
        let launch = LaunchEnvironment {
            clear_environment: false,
            entries: vec![EnvironmentVariable {
                name: reserved.to_owned(),
                value: "source-controlled".to_owned(),
            }],
        };
        assert!(policy.resolve_launch_environment(&launch).is_err());
    }
}

#[test]
fn full_preserves_clear_and_inherit_compatibility_and_allows_profile_overrides() {
    let catalog = validated_catalog();
    let resolver = FakeResolver::succeeding();
    let policy = full_request().validate(&catalog, &resolver).unwrap();
    assert_eq!(resolver.calls.get(), 0);

    for (clear_environment, inherit_ambient) in [(false, true), (true, false)] {
        let effective = policy
            .resolve_launch_environment(&LaunchEnvironment {
                clear_environment,
                entries: vec![
                    EnvironmentVariable {
                        name: "path".to_owned(),
                        value: r"C:\launch-bin".to_owned(),
                    },
                    EnvironmentVariable {
                        name: "USERPROFILE".to_owned(),
                        value: r"C:\profile".to_owned(),
                    },
                ],
            })
            .unwrap();
        assert_eq!(effective.inherit_ambient, inherit_ambient);
        assert_eq!(
            effective.entries,
            vec![
                EnvironmentVariable {
                    name: "LANG".to_owned(),
                    value: "C".to_owned(),
                },
                EnvironmentVariable {
                    name: "path".to_owned(),
                    value: r"C:\launch-bin".to_owned(),
                },
                EnvironmentVariable {
                    name: "USERPROFILE".to_owned(),
                    value: r"C:\profile".to_owned(),
                },
            ]
        );
    }
}

#[test]
fn duplicate_or_invalid_launch_environment_entries_fail() {
    let catalog = validated_catalog();
    let resolver = FakeResolver::succeeding();
    let policy = full_request().validate(&catalog, &resolver).unwrap();

    for entries in [
        vec![("TOKEN", "one"), ("token", "two")],
        vec![("", "value")],
        vec![("A=B", "value")],
        vec![("TOKEN", "value\0suffix")],
    ] {
        let launch = LaunchEnvironment {
            clear_environment: false,
            entries: entries
                .into_iter()
                .map(|(name, value)| EnvironmentVariable {
                    name: name.to_owned(),
                    value: value.to_owned(),
                })
                .collect(),
        };
        assert!(policy.resolve_launch_environment(&launch).is_err());
    }
}

#[test]
fn process_and_pty_share_the_same_immutable_policy_and_environment_resolver() {
    let catalog = validated_catalog();
    let resolver = FakeResolver::succeeding();
    let policy = constrained_request(ExecutionAccess::Workspace)
        .validate(&catalog, &resolver)
        .unwrap();
    let launch = LaunchEnvironment {
        clear_environment: false,
        entries: vec![EnvironmentVariable {
            name: "TOKEN".to_owned(),
            value: "per-launch".to_owned(),
        }],
    };

    assert!(policy.allows(ExecutionSurface::Process));
    assert!(policy.allows(ExecutionSurface::Pty));
    let process_environment = policy.resolve_launch_environment(&launch).unwrap();
    let pty_environment = policy.resolve_launch_environment(&launch).unwrap();
    assert_eq!(process_environment, pty_environment);
    assert_eq!(policy.access(), ExecutionAccess::Workspace);
    assert_eq!(
        policy.scratch_disposition(),
        ScratchDisposition::DeleteOnCleanCloseRetainOtherwise
    );
    assert_eq!(policy.audit_policy(), &audit_policy());
    assert_eq!(policy.resource_limits(), Some(&finite_limits()));
    assert_eq!(
        policy.environment_policy(),
        &environment([("LANG", "C"), ("TOKEN", "alpha")])
    );
}

fn validated_policy_with_env(
    entries: impl IntoIterator<Item = (&'static str, &'static str)>,
) -> ricochet_sandbox::ValidatedExecutionPolicy {
    let catalog = validated_catalog();
    let resolver = FakeResolver::succeeding();
    let mut request = constrained_request(ExecutionAccess::Read);
    request.environment = environment(entries);
    request.validate(&catalog, &resolver).unwrap()
}

#[test]
fn audit_digest_is_order_stable_and_secret_value_blind() {
    let first = validated_policy_with_env([("TOKEN", "alpha"), ("LANG", "C")]);
    let reordered = validated_policy_with_env([("LANG", "C"), ("TOKEN", "alpha")]);
    let changed_secret = validated_policy_with_env([("TOKEN", "beta"), ("LANG", "C")]);
    assert_eq!(first.audit_digest(), reordered.audit_digest());
    assert_eq!(first.audit_digest(), changed_secret.audit_digest());
    assert_ne!(
        first.audit_digest().to_hex(),
        Sha256Digest::hash(b"alpha").to_hex()
    );
}

#[test]
fn audit_digest_is_order_stable_across_tools_destinations_and_environment_names() {
    let catalog = validated_catalog();
    let first_resolver = FakeResolver::succeeding();
    let second_resolver = FakeResolver::succeeding();
    let mut first = constrained_request(ExecutionAccess::Read);
    first.activated_tools = vec![tool_id("main"), tool_id("helper")];
    first.destinations = vec![destination("z.example:443"), destination("a.example:443")];
    first.environment = environment([("TOKEN", "alpha"), ("LANG", "C")]);
    let mut reordered = constrained_request(ExecutionAccess::Read);
    reordered.activated_tools = vec![tool_id("helper"), tool_id("main")];
    reordered.destinations = vec![destination("a.example:443"), destination("z.example:443")];
    reordered.environment = environment([("LANG", "C"), ("TOKEN", "alpha")]);

    let first = first.validate(&catalog, &first_resolver).unwrap();
    let reordered = reordered.validate(&catalog, &second_resolver).unwrap();

    assert_eq!(first.audit_digest(), reordered.audit_digest());
}

#[test]
fn audit_digest_changes_with_public_security_facts() {
    let catalog = validated_catalog();
    let resolver = FakeResolver::succeeding();
    let baseline = constrained_request(ExecutionAccess::Read)
        .validate(&catalog, &resolver)
        .unwrap()
        .audit_digest();

    let resolver = FakeResolver::succeeding();
    let access = constrained_request(ExecutionAccess::Workspace)
        .validate(&catalog, &resolver)
        .unwrap()
        .audit_digest();
    assert_ne!(baseline, access);

    let resolver = FakeResolver::with_identity(WorkspaceIdentity {
        native_object_identity: "volume-7:file-99".to_owned(),
        ..workspace_identity()
    });
    let workspace = constrained_request(ExecutionAccess::Read)
        .validate(&catalog, &resolver)
        .unwrap()
        .audit_digest();
    assert_ne!(baseline, workspace);

    let resolver = FakeResolver::succeeding();
    let mut changed_destination = constrained_request(ExecutionAccess::Read);
    changed_destination.destinations = vec![destination("other.example:443")];
    let changed_destination = changed_destination
        .validate(&catalog, &resolver)
        .unwrap()
        .audit_digest();
    assert_ne!(baseline, changed_destination);

    let resolver = FakeResolver::succeeding();
    let mut changed_tools = constrained_request(ExecutionAccess::Read);
    changed_tools.activated_tools = vec![tool_id("helper")];
    let changed_tools = changed_tools
        .validate(&catalog, &resolver)
        .unwrap()
        .audit_digest();
    assert_ne!(baseline, changed_tools);

    let resolver = FakeResolver::succeeding();
    let mut changed_name = constrained_request(ExecutionAccess::Read);
    changed_name.environment = environment([("LANG", "C"), ("OTHER_TOKEN", "alpha")]);
    let changed_name = changed_name
        .validate(&catalog, &resolver)
        .unwrap()
        .audit_digest();
    assert_ne!(baseline, changed_name);

    let resolver = FakeResolver::succeeding();
    let mut changed_limits = constrained_request(ExecutionAccess::Read);
    changed_limits
        .resource_limits
        .as_mut()
        .unwrap()
        .memory_bytes += 1;
    let changed_limits = changed_limits
        .validate(&catalog, &resolver)
        .unwrap()
        .audit_digest();
    assert_ne!(baseline, changed_limits);

    let changed_catalog = catalog_snapshot(r"C:\broker-private", "main-executable-v2")
        .validate(&FixturePathNormalizer)
        .unwrap();
    let resolver = FakeResolver::succeeding();
    let artifact = constrained_request(ExecutionAccess::Read)
        .validate(&changed_catalog, &resolver)
        .unwrap()
        .audit_digest();
    assert_ne!(baseline, artifact);
}

#[test]
fn audit_digest_excludes_broker_private_catalog_paths() {
    let first_catalog = catalog_snapshot(r"C:\broker-private-a", "main-executable")
        .validate(&FixturePathNormalizer)
        .unwrap();
    let second_catalog = catalog_snapshot(r"D:\hidden-store-b", "main-executable")
        .validate(&FixturePathNormalizer)
        .unwrap();
    let first_resolver = FakeResolver::succeeding();
    let second_resolver = FakeResolver::succeeding();

    let first = constrained_request(ExecutionAccess::Read)
        .validate(&first_catalog, &first_resolver)
        .unwrap();
    let second = constrained_request(ExecutionAccess::Read)
        .validate(&second_catalog, &second_resolver)
        .unwrap();

    assert_eq!(first.audit_digest(), second.audit_digest());
    let debug = format!("{:?}", first.prepared_catalog());
    assert!(!debug.contains("broker-private"));
}

fn assert_rejects_unknown_field<T>(value: &T)
where
    T: Serialize + DeserializeOwned,
{
    let mut value = serde_json::to_value(value).unwrap();
    value
        .as_object_mut()
        .unwrap()
        .insert("gremlin_field".to_owned(), Value::Bool(true));
    assert!(serde_json::from_value::<T>(value).is_err());
}

#[test]
fn every_serialized_policy_dto_denies_unknown_fields() {
    let workspace_request = WorkspaceRequest {
        requested_root: "workspace".to_owned(),
    };
    let workspace_identity = workspace_identity();
    let variable = EnvironmentVariable {
        name: "LANG".to_owned(),
        value: "C".to_owned(),
    };
    let environment_policy = EnvironmentPolicy {
        base: vec![variable.clone()],
    };
    let launch_environment = LaunchEnvironment {
        clear_environment: true,
        entries: vec![variable.clone()],
    };
    let effective_environment = EffectiveEnvironment {
        inherit_ambient: false,
        entries: vec![variable],
    };
    let limits = finite_limits();
    let audit = audit_policy();
    let request = constrained_request(ExecutionAccess::Read);
    let grant = ExecutionGrant {
        access: Some(ExecutionAccess::Read),
        allow_process: true,
        allow_pty: false,
        tools: GrantSet::Only(BTreeSet::from([tool_id("main")])),
        destinations: GrantSet::Only(BTreeSet::from([destination("a.example:443")])),
    };

    assert_rejects_unknown_field(&workspace_request);
    assert_rejects_unknown_field(&workspace_identity);
    assert_rejects_unknown_field(&EnvironmentVariable {
        name: "LANG".to_owned(),
        value: "C".to_owned(),
    });
    assert_rejects_unknown_field(&environment_policy);
    assert_rejects_unknown_field(&launch_environment);
    assert_rejects_unknown_field(&effective_environment);
    assert_rejects_unknown_field(&limits);
    assert_rejects_unknown_field(&audit);
    assert_rejects_unknown_field(&request);
    assert_rejects_unknown_field(&grant);

    assert!(serde_json::from_value::<ExecutionAccess>(json!("Admin")).is_err());
    assert!(serde_json::from_value::<ExecutionSurface>(json!("Shell")).is_err());
    assert!(serde_json::from_value::<ScratchDisposition>(json!("NeverDelete")).is_err());
    assert!(serde_json::from_value::<ArgumentAuditMode>(json!("FullArguments")).is_err());
    assert!(serde_json::from_value::<GrantSet<ToolId>>(json!({"Only": ["UPPERCASE"]})).is_err());
}
