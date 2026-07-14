use ricochet_sandbox::{
    run_shared_conformance, shared_cases, Architecture, AuditExpectation, BackendCapabilities,
    BackendFeatureId, BackendIdentity, BackendSelfTest, BackendSelfTestFailure, ConformanceLevel,
    ExecutionAccess, ExecutionSurface, ExpectedOutcome, FailedGuarantee, FilesystemProbe,
    LifecycleProbe, MockConformanceDriver, ObservedOutcome, OperatingSystem, PlatformId,
    ProbeAttempt, ProbeKind, ProbeStatus, Remediation, ResourceLimitKind, SandboxErrorCode,
    TerminationReason,
};
use ricochet_sandbox::{ConformanceAttestation, ConformanceCase, ConformanceDriver};
use serde_json::json;
use std::collections::HashSet;

const EXPECTED_CASE_IDS: [&str; 85] = [
    "policy.version_rejection",
    "policy.unknown_field_rejection",
    "policy.invalid_access_combination",
    "policy.grant_broadening_rejected",
    "policy.grant_narrowing_allowed",
    "policy.catalog_closure",
    "policy.fingerprint_mismatch",
    "policy.revocation",
    "policy.environment_redaction",
    "policy.process_pty_parity",
    "policy.lifecycle_ordering",
    "policy.complete_tree_cancellation",
    "policy.mock_honesty",
    "filesystem.workspace_read_allowed",
    "filesystem.workspace_write_denied_read",
    "filesystem.workspace_write_allowed",
    "filesystem.scratch_read_write_allowed",
    "filesystem.outside_read",
    "filesystem.outside_write",
    "filesystem.outside_create",
    "filesystem.outside_delete",
    "filesystem.outside_rename",
    "filesystem.outside_link",
    "filesystem.outside_metadata",
    "filesystem.outside_execute",
    "filesystem.symlink_escape",
    "filesystem.junction_escape",
    "filesystem.reparse_escape",
    "filesystem.hard_link_escape",
    "filesystem.mount_escape",
    "filesystem.descriptor_escape",
    "filesystem.namespace_escape",
    "filesystem.rename_race",
    "host_resource.user_profile",
    "host_resource.registry",
    "host_resource.keychain",
    "host_resource.proc",
    "host_resource.credential_store",
    "host_resource.device",
    "host_resource.clipboard",
    "host_resource.ipc",
    "host_resource.inherited_handle",
    "executable.approved_root_allowed",
    "executable.approved_helper_allowed",
    "executable.direct_child",
    "executable.grandchild",
    "executable.shell_helper",
    "executable.interpreter",
    "executable.fingerprint_substitution",
    "network.granted_http_adapter_allowed",
    "network.granted_ssh_adapter_allowed",
    "network.direct_socket",
    "network.adapter_bypass",
    "network.dns_rebinding",
    "network.ipv4_literal",
    "network.ipv6_literal",
    "network.shared_ip",
    "network.port_substitution",
    "network.localhost",
    "network.private_range",
    "network.udp",
    "network.quic",
    "network.listener",
    "isolation.cross_session_scratch",
    "isolation.cross_session_catalog",
    "lifecycle.timeout_tree",
    "lifecycle.cancel_tree",
    "lifecycle.revocation_tree",
    "lifecycle.broker_shutdown_tree",
    "lifecycle.resource_limit.descendant_processes",
    "lifecycle.resource_limit.memory_bytes",
    "lifecycle.resource_limit.cpu_time",
    "lifecycle.resource_limit.wall_time",
    "lifecycle.resource_limit.open_descriptors_or_handles",
    "lifecycle.resource_limit.captured_output",
    "lifecycle.registry_consistency",
    "lifecycle.clean_close_scratch_deleted",
    "lifecycle.crash_scratch_retained",
    "availability.broker_crash",
    "availability.stale_protocol",
    "availability.partial_installation",
    "availability.inactive_enforcement",
    "availability.unsupported_kernel",
    "compatibility.full_source_compatibility",
    "compatibility.full_audit_truth",
];

#[test]
fn shared_catalog_has_exact_unique_ids_and_typed_probe_coverage() {
    let cases = shared_cases();
    let ids = cases.iter().map(|case| case.id).collect::<Vec<_>>();
    let unique_ids = ids.iter().copied().collect::<HashSet<_>>();
    let unique_probes = cases.iter().map(|case| case.probe).collect::<HashSet<_>>();

    assert_eq!(ids, EXPECTED_CASE_IDS);
    assert_eq!(unique_ids.len(), EXPECTED_CASE_IDS.len());
    assert_eq!(unique_probes.len(), EXPECTED_CASE_IDS.len());
}

#[test]
fn static_catalog_mapping_and_native_applicability_are_exact() {
    let cases = shared_cases();
    let case = |id: &str| cases.iter().find(|case| case.id == id).unwrap();

    let version = case("policy.version_rejection");
    assert_eq!(version.level, ConformanceLevel::Model);
    assert_eq!(version.accesses, &[ExecutionAccess::Read]);
    assert_eq!(version.surfaces, &[ExecutionSurface::Process]);
    assert!(matches!(
        version.expected,
        ExpectedOutcome::Denied(SandboxErrorCode::BrokerProtocolError)
    ));
    assert!(matches!(
        version.expected_audit,
        AuditExpectation::AbsentBeforeSession
    ));

    let write_control = case("filesystem.workspace_write_denied_read");
    assert_eq!(
        write_control.probe,
        ProbeKind::Filesystem(FilesystemProbe::WorkspaceWriteDeniedRead)
    );
    assert_eq!(write_control.accesses, &[ExecutionAccess::Read]);
    assert!(matches!(
        write_control.expected,
        ExpectedOutcome::DeniedBySandbox
    ));

    for id in [
        "filesystem.junction_escape",
        "filesystem.reparse_escape",
        "host_resource.registry",
    ] {
        assert_eq!(case(id).platforms, &[OperatingSystem::Windows], "{id}");
    }
    assert_eq!(
        case("host_resource.keychain").platforms,
        &[OperatingSystem::Macos]
    );
    assert_eq!(
        case("host_resource.proc").platforms,
        &[OperatingSystem::Linux]
    );

    let resource_limits = cases
        .iter()
        .filter_map(|case| match case.probe {
            ProbeKind::Lifecycle(LifecycleProbe::ResourceLimit(kind)) => Some((case, kind)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(resource_limits.len(), 6);
    for (case, kind) in resource_limits {
        assert!(matches!(
            case.expected,
            ExpectedOutcome::TerminatesTree {
                reason: TerminationReason::ResourceLimit(actual)
            } if actual == kind
        ));
    }
    assert_eq!(
        case("lifecycle.resource_limit.descendant_processes").probe,
        ProbeKind::Lifecycle(LifecycleProbe::ResourceLimit(
            ResourceLimitKind::DescendantProcesses
        ))
    );
}

#[test]
fn declared_access_surface_cardinalities_are_exact() {
    let cases = shared_cases();
    let all_results = cases
        .iter()
        .map(|case| case.accesses.len() * case.surfaces.len())
        .sum::<usize>();
    let windows_results = cases
        .iter()
        .filter(|case| case.platforms.contains(&OperatingSystem::Windows))
        .map(|case| case.accesses.len() * case.surfaces.len())
        .sum::<usize>();
    let linux_results = cases
        .iter()
        .filter(|case| case.platforms.contains(&OperatingSystem::Linux))
        .map(|case| case.accesses.len() * case.surfaces.len())
        .sum::<usize>();
    let macos_results = cases
        .iter()
        .filter(|case| case.platforms.contains(&OperatingSystem::Macos))
        .map(|case| case.accesses.len() * case.surfaces.len())
        .sum::<usize>();

    assert_eq!(all_results, 297);
    assert_eq!(windows_results, 289);
    assert_eq!(linux_results, 281);
    assert_eq!(macos_results, 281);
}

enum ObservationMutation {
    AuditSessionIdentity,
    BackendIdentity,
    ModelEnforcement,
    ToolFingerprint,
    Destination,
    ResourceLimit,
    ExtraAuditCode,
}

struct MutatingModelDriver {
    inner: MockConformanceDriver,
    case_id: &'static str,
    mutation: ObservationMutation,
}

impl MutatingModelDriver {
    fn new(case_id: &'static str, mutation: ObservationMutation) -> Self {
        Self {
            inner: MockConformanceDriver::passing_model_cases(),
            case_id,
            mutation,
        }
    }
}

impl ConformanceDriver for MutatingModelDriver {
    fn attestation(&self) -> ConformanceAttestation {
        self.inner.attestation()
    }

    fn observe(
        &mut self,
        case: &ConformanceCase,
        access: ExecutionAccess,
        surface: ExecutionSurface,
    ) -> ProbeAttempt {
        let mut attempt = self.inner.observe(case, access, surface);
        if case.id != self.case_id {
            return attempt;
        }
        let ProbeAttempt::Observed(observation) = &mut attempt else {
            panic!("mock model case unexpectedly returned NotRun");
        };

        match self.mutation {
            ObservationMutation::AuditSessionIdentity => {
                let mut value = serde_json::to_value(observation.audit.as_ref().unwrap()).unwrap();
                value["session_id"] = json!("different-session");
                observation.audit = Some(serde_json::from_value(value).unwrap());
            }
            ObservationMutation::BackendIdentity => {
                let replacement = BackendIdentity::new("other-backend", "1").unwrap();
                let mut capabilities =
                    serde_json::to_value(observation.capabilities.as_ref().unwrap()).unwrap();
                capabilities["backend"] = serde_json::to_value(&replacement).unwrap();
                observation.capabilities = Some(serde_json::from_value(capabilities).unwrap());

                let mut audit = serde_json::to_value(observation.audit.as_ref().unwrap()).unwrap();
                audit["backend"] = serde_json::to_value(replacement).unwrap();
                observation.audit = Some(serde_json::from_value(audit).unwrap());
            }
            ObservationMutation::ModelEnforcement => {
                let mut capabilities =
                    serde_json::to_value(observation.capabilities.as_ref().unwrap()).unwrap();
                capabilities["enforcement"] = json!("enforced");
                observation.capabilities = Some(serde_json::from_value(capabilities).unwrap());

                let mut audit = serde_json::to_value(observation.audit.as_ref().unwrap()).unwrap();
                audit["enforcement"] = json!("enforced");
                observation.audit = Some(serde_json::from_value(audit).unwrap());
            }
            ObservationMutation::ToolFingerprint => {
                let replacement =
                    "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff";
                let mut capabilities =
                    serde_json::to_value(observation.capabilities.as_ref().unwrap()).unwrap();
                capabilities["tools"][0]["executable_sha256"] = json!(replacement);
                observation.capabilities = Some(serde_json::from_value(capabilities).unwrap());

                let mut audit = serde_json::to_value(observation.audit.as_ref().unwrap()).unwrap();
                audit["tools"][0]["executable_sha256"] = json!(replacement);
                observation.audit = Some(serde_json::from_value(audit).unwrap());
            }
            ObservationMutation::Destination => {
                let mut capabilities =
                    serde_json::to_value(observation.capabilities.as_ref().unwrap()).unwrap();
                capabilities["destinations"][0] = json!("other.example:443");
                observation.capabilities = Some(serde_json::from_value(capabilities).unwrap());

                let mut audit = serde_json::to_value(observation.audit.as_ref().unwrap()).unwrap();
                audit["destinations"][0] = json!("other.example:443");
                observation.audit = Some(serde_json::from_value(audit).unwrap());
            }
            ObservationMutation::ResourceLimit => {
                let mut capabilities =
                    serde_json::to_value(observation.capabilities.as_ref().unwrap()).unwrap();
                capabilities["resource_limits"]["memory_bytes"] = json!(67_108_865_u64);
                observation.capabilities = Some(serde_json::from_value(capabilities).unwrap());

                let mut audit = serde_json::to_value(observation.audit.as_ref().unwrap()).unwrap();
                audit["resource_limits"]["memory_bytes"] = json!(67_108_865_u64);
                observation.audit = Some(serde_json::from_value(audit).unwrap());
            }
            ObservationMutation::ExtraAuditCode => {
                observation
                    .audit_codes
                    .push(SandboxErrorCode::BrokerProtocolError);
            }
        }
        attempt
    }
}

#[test]
fn harness_rejects_capability_audit_session_identity_drift() {
    let mut driver = MutatingModelDriver::new(
        "policy.process_pty_parity",
        ObservationMutation::AuditSessionIdentity,
    );
    let report = run_shared_conformance(&mut driver);

    assert!(report.assert_model_complete().is_err());
    assert!(report.results.iter().any(|result| {
        result.case_id == "policy.process_pty_parity"
            && matches!(result.status, ProbeStatus::Failed(_))
    }));
}

#[test]
fn model_session_requires_mock_only_enforcement_truth() {
    let mut driver = MutatingModelDriver::new(
        "policy.process_pty_parity",
        ObservationMutation::ModelEnforcement,
    );
    let report = run_shared_conformance(&mut driver);

    assert!(report.assert_model_complete().is_err());
}

#[test]
fn harness_compares_complete_expected_tool_fingerprints() {
    let mut driver = MutatingModelDriver::new(
        "policy.process_pty_parity",
        ObservationMutation::ToolFingerprint,
    );
    let report = run_shared_conformance(&mut driver);

    assert!(report.assert_model_complete().is_err());
}

#[test]
fn harness_compares_backend_destination_and_resource_limit_truth() {
    for mutation in [
        ObservationMutation::BackendIdentity,
        ObservationMutation::Destination,
        ObservationMutation::ResourceLimit,
    ] {
        let mut driver = MutatingModelDriver::new("policy.process_pty_parity", mutation);
        let report = run_shared_conformance(&mut driver);
        assert!(report.assert_model_complete().is_err());
    }
}

#[test]
fn pre_session_denial_rejects_fabricated_structured_audit_code() {
    let mut driver = MutatingModelDriver::new(
        "policy.version_rejection",
        ObservationMutation::ExtraAuditCode,
    );
    let report = run_shared_conformance(&mut driver);

    assert!(report.assert_model_complete().is_err());
}

struct CountingModelDriver {
    inner: MockConformanceDriver,
    observed: Vec<ProbeKind>,
}

impl ConformanceDriver for CountingModelDriver {
    fn attestation(&self) -> ConformanceAttestation {
        self.inner.attestation()
    }

    fn observe(
        &mut self,
        case: &ConformanceCase,
        access: ExecutionAccess,
        surface: ExecutionSurface,
    ) -> ProbeAttempt {
        self.observed.push(case.probe);
        self.inner.observe(case, access, surface)
    }
}

#[test]
fn model_attestation_never_calls_observe_for_real_os_cases() {
    let mut driver = CountingModelDriver {
        inner: MockConformanceDriver::passing_model_cases(),
        observed: Vec::new(),
    };
    let report = run_shared_conformance(&mut driver);

    assert_eq!(driver.observed.len(), 26);
    assert!(driver
        .observed
        .iter()
        .all(|probe| matches!(probe, ProbeKind::Policy(_))));
    assert_eq!(report.results.len(), 297);
    assert_eq!(
        report
            .results
            .iter()
            .filter(|result| matches!(result.status, ProbeStatus::Passed))
            .count(),
        26
    );
    assert_eq!(
        report
            .results
            .iter()
            .filter(|result| matches!(result.status, ProbeStatus::NotRun(ref reason) if reason == "real OS runner required"))
            .count(),
        271
    );
}

struct RealOsObservationDriver {
    inner: MockConformanceDriver,
    attestation: ConformanceAttestation,
    not_run_case: Option<&'static str>,
    audit_codes_for_case: Option<(&'static str, Vec<SandboxErrorCode>)>,
    outcome_for_case: Option<(&'static str, ObservedOutcome)>,
    enforcement_for_case: Option<(&'static str, &'static str)>,
    observed_case_ids: Vec<&'static str>,
}

impl RealOsObservationDriver {
    fn passing(os: OperatingSystem) -> Self {
        let backend = BackendIdentity::new("test-real-os", "1").unwrap();
        Self {
            inner: MockConformanceDriver::passing_model_cases(),
            attestation: ConformanceAttestation {
                level: ConformanceLevel::RealOs,
                platform: Some(PlatformId {
                    os,
                    arch: Architecture::X86_64,
                }),
                backend: backend.clone(),
                self_test: BackendSelfTest {
                    identity: backend,
                    capabilities: complete_backend_capabilities(),
                    production_enforcement: true,
                    failures: Vec::new(),
                },
                runner_evidence_id: Some(format!("runner-evidence-{os:?}")),
            },
            not_run_case: None,
            audit_codes_for_case: None,
            outcome_for_case: None,
            enforcement_for_case: None,
            observed_case_ids: Vec::new(),
        }
    }

    fn without_platform() -> Self {
        let mut driver = Self::passing(OperatingSystem::Windows);
        driver.attestation.platform = None;
        driver
    }
}

impl ConformanceDriver for RealOsObservationDriver {
    fn attestation(&self) -> ConformanceAttestation {
        self.attestation.clone()
    }

    fn observe(
        &mut self,
        case: &ConformanceCase,
        access: ExecutionAccess,
        surface: ExecutionSurface,
    ) -> ProbeAttempt {
        self.observed_case_ids.push(case.id);
        if self.not_run_case == Some(case.id) {
            return ProbeAttempt::NotRun("fixture unavailable".to_owned());
        }

        let mut attempt = self.inner.observe(case, access, surface);
        let ProbeAttempt::Observed(observation) = &mut attempt else {
            return attempt;
        };
        if let Some((target, codes)) = &self.audit_codes_for_case {
            if *target == case.id {
                observation.audit_codes = codes.clone();
            }
        }
        if let Some((target, outcome)) = &self.outcome_for_case {
            if *target == case.id {
                observation.outcome = outcome.clone();
            }
        }
        let enforcement = self
            .enforcement_for_case
            .filter(|(target, _)| *target == case.id)
            .map_or_else(
                || match access {
                    ExecutionAccess::Read | ExecutionAccess::Workspace => "enforced",
                    ExecutionAccess::Full => "unenforced_full_access",
                },
                |(_, enforcement)| enforcement,
            );
        if let Some(capabilities) = observation.capabilities.take() {
            let mut value = serde_json::to_value(capabilities).unwrap();
            value["enforcement"] = json!(enforcement);
            value["backend"] = serde_json::to_value(&self.attestation.backend).unwrap();
            observation.capabilities = Some(serde_json::from_value(value).unwrap());
        }
        if let Some(audit) = observation.audit.take() {
            let mut value = serde_json::to_value(audit).unwrap();
            value["enforcement"] = json!(enforcement);
            value["backend"] = serde_json::to_value(&self.attestation.backend).unwrap();
            observation.audit = Some(serde_json::from_value(value).unwrap());
        }
        attempt
    }
}

fn complete_backend_capabilities() -> BackendCapabilities {
    BackendCapabilities {
        process: true,
        pty: true,
        filesystem_read: true,
        filesystem_write: true,
        executable_closure: true,
        descendant_confinement: true,
        destination_transport: true,
        resource_limits: true,
        scratch_isolation: true,
    }
}

#[test]
fn real_os_reports_run_only_exact_native_platform_cases() {
    for (os, expected_count, included, excluded) in [
        (
            OperatingSystem::Windows,
            289,
            "host_resource.registry",
            "host_resource.keychain",
        ),
        (
            OperatingSystem::Linux,
            281,
            "host_resource.proc",
            "filesystem.junction_escape",
        ),
        (
            OperatingSystem::Macos,
            281,
            "host_resource.keychain",
            "host_resource.proc",
        ),
    ] {
        let mut driver = RealOsObservationDriver::passing(os);
        let report = run_shared_conformance(&mut driver);

        assert_eq!(report.results.len(), expected_count, "{os:?}");
        assert!(driver.observed_case_ids.contains(&included), "{os:?}");
        assert!(!driver.observed_case_ids.contains(&excluded), "{os:?}");
        assert!(report
            .results
            .iter()
            .all(|result| matches!(result.status, ProbeStatus::Passed)));
        report.assert_model_complete().unwrap();
        report.assert_real_os_complete().unwrap();
    }
}

#[test]
fn real_os_gate_requires_concrete_platform_and_nonempty_evidence() {
    let mut no_platform = RealOsObservationDriver::without_platform();
    let report = run_shared_conformance(&mut no_platform);
    assert!(no_platform.observed_case_ids.is_empty());
    assert!(report.assert_real_os_complete().is_err());

    for evidence in [None, Some(String::new()), Some("  ".to_owned())] {
        let mut driver = RealOsObservationDriver::passing(OperatingSystem::Windows);
        driver.attestation.runner_evidence_id = evidence;
        let report = run_shared_conformance(&mut driver);
        assert!(report.assert_real_os_complete().is_err());
    }
}

#[test]
fn real_os_gate_requires_honest_complete_production_self_test() {
    let mut mismatched_identity = RealOsObservationDriver::passing(OperatingSystem::Windows);
    mismatched_identity.attestation.self_test.identity =
        BackendIdentity::new("different-backend", "1").unwrap();

    let mut nonproduction = RealOsObservationDriver::passing(OperatingSystem::Windows);
    nonproduction.attestation.self_test.production_enforcement = false;

    let mut incomplete = RealOsObservationDriver::passing(OperatingSystem::Windows);
    incomplete
        .attestation
        .self_test
        .capabilities
        .destination_transport = false;

    let mut self_test_failure = RealOsObservationDriver::passing(OperatingSystem::Windows);
    self_test_failure
        .attestation
        .self_test
        .failures
        .push(BackendSelfTestFailure {
            feature: BackendFeatureId::parse("test-feature").unwrap(),
            guarantee: FailedGuarantee::BrokerAvailability,
            remediation: Some(Remediation::EnableBackendPrerequisite),
        });

    for mut driver in [
        mismatched_identity,
        nonproduction,
        incomplete,
        self_test_failure,
    ] {
        let report = run_shared_conformance(&mut driver);
        assert!(report.assert_real_os_complete().is_err());
    }
}

#[test]
fn real_os_gate_rejects_failed_or_not_run_applicable_probe() {
    let mut failed = RealOsObservationDriver::passing(OperatingSystem::Windows);
    failed.inner = MockConformanceDriver::with_failed_case("policy.process_pty_parity");
    let failed_report = run_shared_conformance(&mut failed);
    assert!(failed_report.assert_real_os_complete().is_err());

    let mut not_run = RealOsObservationDriver::passing(OperatingSystem::Windows);
    not_run.not_run_case = Some("filesystem.workspace_read_allowed");
    let not_run_report = run_shared_conformance(&mut not_run);
    assert!(not_run_report.assert_real_os_complete().is_err());
}

#[test]
fn session_denials_require_one_exact_structured_audit_code() {
    for codes in [
        Vec::new(),
        vec![SandboxErrorCode::NetworkDenied],
        vec![
            SandboxErrorCode::ToolNotApproved,
            SandboxErrorCode::ToolNotApproved,
        ],
    ] {
        let mut driver = RealOsObservationDriver::passing(OperatingSystem::Windows);
        driver.audit_codes_for_case = Some(("executable.direct_child", codes));
        let report = run_shared_conformance(&mut driver);
        assert!(report.assert_real_os_complete().is_err());
    }

    let mut denied_by_sandbox = RealOsObservationDriver::passing(OperatingSystem::Windows);
    denied_by_sandbox.audit_codes_for_case = Some((
        "filesystem.outside_read",
        vec![SandboxErrorCode::NetworkDenied],
    ));
    let report = run_shared_conformance(&mut denied_by_sandbox);
    assert!(report.assert_real_os_complete().is_err());
}

#[test]
fn tree_termination_requires_zero_live_descendants() {
    let mut driver = RealOsObservationDriver::passing(OperatingSystem::Windows);
    driver.outcome_for_case = Some((
        "lifecycle.timeout_tree",
        ObservedOutcome::TreeState {
            reason: TerminationReason::TimedOut,
            descendants_alive: 1,
        },
    ));
    let report = run_shared_conformance(&mut driver);

    assert!(report.assert_real_os_complete().is_err());
}

#[test]
fn real_os_enforcement_truth_is_exact_for_constrained_and_full_access() {
    for (case_id, enforcement) in [
        ("filesystem.workspace_read_allowed", "mock_only"),
        ("compatibility.full_audit_truth", "mock_only"),
    ] {
        let mut driver = RealOsObservationDriver::passing(OperatingSystem::Windows);
        driver.enforcement_for_case = Some((case_id, enforcement));
        let report = run_shared_conformance(&mut driver);
        assert!(report.assert_real_os_complete().is_err());
    }
}

#[test]
fn real_os_gate_rejects_result_cardinality_or_identity_tampering() {
    let mut driver = RealOsObservationDriver::passing(OperatingSystem::Windows);
    let mut report = run_shared_conformance(&mut driver);
    report.results.pop();
    assert!(report.assert_real_os_complete().is_err());

    let mut driver = RealOsObservationDriver::passing(OperatingSystem::Windows);
    let mut report = run_shared_conformance(&mut driver);
    report.results[0].case_id = "policy.unknown_field_rejection";
    assert!(report.assert_real_os_complete().is_err());
}

#[test]
fn mock_proves_model_contract_but_cannot_satisfy_real_os_gate() {
    let mut driver = MockConformanceDriver::passing_model_cases();
    let report = run_shared_conformance(&mut driver);

    report.assert_model_complete().unwrap();
    assert_eq!(
        report.assert_real_os_complete().unwrap_err().kind(),
        "SandboxUnavailable"
    );
}

#[test]
fn one_bad_observation_fails_the_model_gate() {
    let mut driver = MockConformanceDriver::with_failed_case("policy.process_pty_parity");
    let report = run_shared_conformance(&mut driver);

    assert!(report.assert_model_complete().is_err());
}
