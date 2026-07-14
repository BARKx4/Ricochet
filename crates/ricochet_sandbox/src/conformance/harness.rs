use super::{
    shared_cases, AuditExpectation, ConformanceCase, ConformanceLevel, ExpectedOutcome,
    ExpectedSessionAudit, ObservedOutcome, ProbeAttempt, ProbeObservation, ProbeStatus,
};
use crate::{
    AuditContext, BackendIdentity, BackendSelfTest, ConfirmedExecutionCapabilities,
    DiagnosticMetadata, EnforcementState, ExecutionAccess, ExecutionSurface, FailedGuarantee,
    MockBackendConfig, MockSandboxBackend, PlatformId, Remediation, SandboxBackend, SandboxError,
    SandboxErrorCode, PROTOCOL_V1,
};
use serde_json::json;

#[derive(Clone, Debug)]
pub struct ConformanceAttestation {
    pub level: ConformanceLevel,
    pub platform: Option<PlatformId>,
    pub backend: BackendIdentity,
    pub self_test: BackendSelfTest,
    pub runner_evidence_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProbeResult {
    pub case_id: &'static str,
    pub access: ExecutionAccess,
    pub surface: ExecutionSurface,
    pub status: ProbeStatus,
}

pub struct ConformanceReport {
    pub attestation: ConformanceAttestation,
    pub results: Vec<ProbeResult>,
}

pub trait ConformanceDriver {
    fn attestation(&self) -> ConformanceAttestation;

    fn observe(
        &mut self,
        case: &ConformanceCase,
        access: ExecutionAccess,
        surface: ExecutionSurface,
    ) -> ProbeAttempt;
}

pub struct MockConformanceDriver {
    failed_case: Option<&'static str>,
}

impl MockConformanceDriver {
    pub fn passing_model_cases() -> Self {
        Self { failed_case: None }
    }

    pub fn with_failed_case(case_id: &'static str) -> Self {
        Self {
            failed_case: Some(case_id),
        }
    }
}

impl ConformanceDriver for MockConformanceDriver {
    fn attestation(&self) -> ConformanceAttestation {
        let backend = MockSandboxBackend::new(MockBackendConfig::default());
        ConformanceAttestation {
            level: ConformanceLevel::Model,
            platform: None,
            backend: backend.identity(),
            self_test: backend.self_test(),
            runner_evidence_id: None,
        }
    }

    fn observe(
        &mut self,
        case: &ConformanceCase,
        access: ExecutionAccess,
        _surface: ExecutionSurface,
    ) -> ProbeAttempt {
        if self.failed_case == Some(case.id) {
            return ProbeAttempt::Observed(ProbeObservation {
                outcome: ObservedOutcome::Unexpected(SandboxErrorCode::SandboxLaunchError),
                capabilities: None,
                audit: None,
                audit_codes: Vec::new(),
            });
        }

        ProbeAttempt::Observed(mock_observation(case, access))
    }
}

pub fn run_shared_conformance(driver: &mut dyn ConformanceDriver) -> ConformanceReport {
    let attestation = driver.attestation();
    let mut results = Vec::new();

    for case in shared_cases() {
        let applicable = match attestation.level {
            ConformanceLevel::Model => true,
            ConformanceLevel::RealOs => attestation
                .platform
                .as_ref()
                .is_some_and(|platform| case.platforms.contains(&platform.os)),
        };
        if !applicable {
            continue;
        }
        for &access in case.accesses {
            for &surface in case.surfaces {
                let status = if attestation.level == ConformanceLevel::Model
                    && case.level == ConformanceLevel::RealOs
                {
                    ProbeStatus::NotRun("real OS runner required".to_owned())
                } else {
                    match driver.observe(case, access, surface) {
                        ProbeAttempt::Observed(observation) => {
                            compare_observation(&attestation, case, access, &observation)
                                .map_or_else(ProbeStatus::Failed, |()| ProbeStatus::Passed)
                        }
                        ProbeAttempt::NotRun(reason) => ProbeStatus::NotRun(reason),
                    }
                };
                results.push(ProbeResult {
                    case_id: case.id,
                    access,
                    surface,
                    status,
                });
            }
        }
    }

    ConformanceReport {
        attestation,
        results,
    }
}

impl ConformanceReport {
    #[allow(clippy::result_large_err)]
    pub fn assert_model_complete(&self) -> Result<(), SandboxError> {
        let platform = match self.attestation.level {
            ConformanceLevel::Model => None,
            ConformanceLevel::RealOs => Some(
                self.attestation
                    .platform
                    .as_ref()
                    .ok_or_else(|| gate_error(&self.attestation.backend))?
                    .os,
            ),
        };
        if self.results.len() != expected_result_count(platform) {
            return Err(gate_error(&self.attestation.backend));
        }

        let mut actual = self.results.iter();
        for case in shared_cases() {
            if platform.is_some_and(|os| !case.platforms.contains(&os)) {
                continue;
            }
            for &access in case.accesses {
                for &surface in case.surfaces {
                    let Some(result) = actual.next() else {
                        return Err(gate_error(&self.attestation.backend));
                    };
                    let status_valid = match self.attestation.level {
                        ConformanceLevel::Model => match (&result.status, case.level) {
                            (ProbeStatus::Passed, ConformanceLevel::Model) => true,
                            (ProbeStatus::NotRun(reason), ConformanceLevel::RealOs) => {
                                reason == "real OS runner required"
                            }
                            _ => false,
                        },
                        ConformanceLevel::RealOs => matches!(
                            (&result.status, case.level),
                            (ProbeStatus::Passed, _)
                                | (ProbeStatus::NotRun(_), ConformanceLevel::RealOs)
                        ),
                    };
                    if result.case_id != case.id
                        || result.access != access
                        || result.surface != surface
                        || !status_valid
                    {
                        return Err(gate_error(&self.attestation.backend));
                    }
                }
            }
        }
        if actual.next().is_some() {
            return Err(gate_error(&self.attestation.backend));
        }
        Ok(())
    }

    #[allow(clippy::result_large_err)]
    pub fn assert_real_os_complete(&self) -> Result<(), SandboxError> {
        if self.attestation.level != ConformanceLevel::RealOs
            || self
                .attestation
                .runner_evidence_id
                .as_deref()
                .is_none_or(|evidence| evidence.trim().is_empty())
            || self.attestation.backend != self.attestation.self_test.identity
            || !self.attestation.self_test.production_enforcement
            || !self
                .attestation
                .self_test
                .capabilities
                .supports_complete_contract()
            || !self.attestation.self_test.failures.is_empty()
        {
            return Err(gate_error(&self.attestation.backend));
        }
        let platform = self
            .attestation
            .platform
            .as_ref()
            .ok_or_else(|| gate_error(&self.attestation.backend))?
            .os;
        if self.results.len() != expected_result_count(Some(platform)) {
            return Err(gate_error(&self.attestation.backend));
        }

        let mut actual = self.results.iter();
        for case in shared_cases() {
            if !case.platforms.contains(&platform) {
                continue;
            }
            for &access in case.accesses {
                for &surface in case.surfaces {
                    let Some(result) = actual.next() else {
                        return Err(gate_error(&self.attestation.backend));
                    };
                    if result.case_id != case.id
                        || result.access != access
                        || result.surface != surface
                        || !matches!(result.status, ProbeStatus::Passed)
                    {
                        return Err(gate_error(&self.attestation.backend));
                    }
                }
            }
        }
        if actual.next().is_some() {
            return Err(gate_error(&self.attestation.backend));
        }
        Ok(())
    }
}

fn expected_result_count(platform: Option<crate::OperatingSystem>) -> usize {
    shared_cases()
        .iter()
        .filter(|case| platform.is_none_or(|os| case.platforms.contains(&os)))
        .map(|case| case.accesses.len() * case.surfaces.len())
        .sum()
}

fn mock_observation(case: &ConformanceCase, access: ExecutionAccess) -> ProbeObservation {
    let outcome = match case.expected {
        ExpectedOutcome::Allowed => ObservedOutcome::Allowed,
        ExpectedOutcome::DeniedBySandbox => ObservedOutcome::DeniedBySandbox,
        ExpectedOutcome::Denied(code) => ObservedOutcome::Denied(code),
        ExpectedOutcome::TerminatesTree { reason } => ObservedOutcome::TreeState {
            reason,
            descendants_alive: 0,
        },
        ExpectedOutcome::Unavailable(code) => ObservedOutcome::Unavailable(code),
    };

    match &case.expected_audit {
        AuditExpectation::AbsentBeforeSession => ProbeObservation {
            outcome,
            capabilities: None,
            audit: None,
            audit_codes: Vec::new(),
        },
        AuditExpectation::Session(expected) => {
            let (capabilities, audit) = mock_session_evidence(access, expected)
                .map_or((None, None), |(capabilities, audit)| {
                    (Some(capabilities), Some(audit))
                });
            let audit_codes = match case.expected {
                ExpectedOutcome::Denied(code) => vec![code],
                _ => Vec::new(),
            };
            ProbeObservation {
                outcome,
                capabilities,
                audit,
                audit_codes,
            }
        }
    }
}

fn mock_session_evidence(
    access: ExecutionAccess,
    expected: &ExpectedSessionAudit,
) -> Option<(ConfirmedExecutionCapabilities, AuditContext)> {
    let backend = BackendIdentity::new("mock", "1").ok()?;
    let workspace = match access {
        ExecutionAccess::Read => Some(json!({
            "canonical_root": "C:/conformance/workspace",
            "writable": false
        })),
        ExecutionAccess::Workspace => Some(json!({
            "canonical_root": "C:/conformance/workspace",
            "writable": true
        })),
        ExecutionAccess::Full => None,
    };
    let tools = expected
        .tools
        .iter()
        .map(|tool| {
            json!({
                "tool_id": tool.tool_id,
                "executable_sha256": tool.sha256_hex,
                "helper_ids": tool.helper_ids,
                "transport_adapter": null
            })
        })
        .collect::<Vec<_>>();
    let destinations = expected
        .destinations
        .iter()
        .map(|destination| format!("{}:{}", destination.host, destination.port))
        .collect::<Vec<_>>();
    let value = json!({
        "access": access,
        "enforcement": EnforcementState::MockOnly,
        "backend": backend,
        "broker_protocol": PROTOCOL_V1,
        "session_id": "conformance-session",
        "policy_digest": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "workspace": workspace,
        "scratch_id": "conformance-scratch",
        "catalog_generation": expected.catalog_generation,
        "tools": tools,
        "destinations": destinations,
        "resource_limits": expected.resource_limits
    });
    let capabilities = serde_json::from_value(value.clone()).ok()?;
    let audit = serde_json::from_value(value).ok()?;
    Some((capabilities, audit))
}

fn compare_observation(
    attestation: &ConformanceAttestation,
    case: &ConformanceCase,
    access: ExecutionAccess,
    observation: &ProbeObservation,
) -> Result<(), String> {
    compare_outcome(case.expected, &observation.outcome)?;
    compare_audit_codes(case, &observation.audit_codes)?;
    match &case.expected_audit {
        AuditExpectation::AbsentBeforeSession => require(
            observation.capabilities.is_none() && observation.audit.is_none(),
            "pre-session observation included session evidence",
        ),
        AuditExpectation::Session(expected) => {
            let capabilities = observation
                .capabilities
                .as_ref()
                .ok_or_else(|| "session observation omitted capabilities".to_owned())?;
            let audit = observation
                .audit
                .as_ref()
                .ok_or_else(|| "session observation omitted audit context".to_owned())?;
            compare_session_evidence(attestation, access, expected, capabilities, audit)
        }
    }
}

fn compare_outcome(expected: ExpectedOutcome, observed: &ObservedOutcome) -> Result<(), String> {
    let matches = match (expected, observed) {
        (ExpectedOutcome::Allowed, ObservedOutcome::Allowed)
        | (ExpectedOutcome::DeniedBySandbox, ObservedOutcome::DeniedBySandbox) => true,
        (ExpectedOutcome::Denied(expected), ObservedOutcome::Denied(observed))
        | (ExpectedOutcome::Unavailable(expected), ObservedOutcome::Unavailable(observed)) => {
            expected == *observed
        }
        (
            ExpectedOutcome::TerminatesTree { reason: expected },
            ObservedOutcome::TreeState {
                reason: observed,
                descendants_alive,
            },
        ) => expected == *observed && *descendants_alive == 0,
        _ => false,
    };
    require(matches, "observed outcome did not match expected outcome")
}

fn compare_audit_codes(case: &ConformanceCase, actual: &[SandboxErrorCode]) -> Result<(), String> {
    match (&case.expected_audit, case.expected) {
        (AuditExpectation::Session(_), ExpectedOutcome::Denied(code)) => require(
            actual == [code],
            "session structured denial audit code mismatch",
        ),
        _ => require(actual.is_empty(), "unexpected structured denial audit code"),
    }
}

fn compare_session_evidence(
    attestation: &ConformanceAttestation,
    access: ExecutionAccess,
    expected: &ExpectedSessionAudit,
    capabilities: &ConfirmedExecutionCapabilities,
    audit: &AuditContext,
) -> Result<(), String> {
    capabilities
        .validate()
        .map_err(|_| "capabilities failed validation".to_owned())?;
    let expected_enforcement = match (attestation.level, access) {
        (ConformanceLevel::Model, _) => EnforcementState::MockOnly,
        (ConformanceLevel::RealOs, ExecutionAccess::Read | ExecutionAccess::Workspace) => {
            EnforcementState::Enforced
        }
        (ConformanceLevel::RealOs, ExecutionAccess::Full) => EnforcementState::UnenforcedFullAccess,
    };
    require(
        capabilities.access() == access && audit.access() == access,
        "session access mismatch",
    )?;
    require(
        capabilities.enforcement() == expected_enforcement
            && audit.enforcement() == expected_enforcement,
        "session enforcement truth mismatch",
    )?;
    require(
        capabilities.backend() == &attestation.backend && audit.backend() == &attestation.backend,
        "session backend attestation mismatch",
    )?;
    require(
        capabilities.broker_protocol() == PROTOCOL_V1 && audit.broker_protocol() == PROTOCOL_V1,
        "session protocol mismatch",
    )?;
    require(
        capabilities.session_id() == audit.session_id()
            && capabilities.scratch_id() == audit.scratch_id()
            && capabilities.policy_digest() == audit.policy_digest()
            && capabilities.workspace() == audit.workspace()
            && capabilities.catalog_generation() == audit.catalog_generation()
            && capabilities.tools() == audit.tools()
            && capabilities.destinations() == audit.destinations()
            && capabilities.resource_limits() == audit.resource_limits(),
        "capability and audit context identity mismatch",
    )?;

    let workspace_truth = match access {
        ExecutionAccess::Read => capabilities
            .workspace()
            .is_some_and(|workspace| !workspace.writable()),
        ExecutionAccess::Workspace => capabilities
            .workspace()
            .is_some_and(crate::AuditWorkspace::writable),
        ExecutionAccess::Full => capabilities
            .workspace()
            .is_none_or(crate::AuditWorkspace::writable),
    };
    require(workspace_truth, "workspace writable truth mismatch")?;
    require(
        capabilities.catalog_generation().get() == expected.catalog_generation,
        "catalog generation mismatch",
    )?;
    require(
        capabilities.resource_limits() == expected.resource_limits.as_ref(),
        "resource limit audit mismatch",
    )?;
    require(
        capabilities.tools().len() == expected.tools.len()
            && capabilities
                .tools()
                .iter()
                .zip(expected.tools)
                .all(|(actual, expected)| {
                    actual.tool_id.as_str() == expected.tool_id
                        && actual.executable_sha256.to_hex() == expected.sha256_hex
                        && actual
                            .helper_ids
                            .iter()
                            .map(crate::ToolId::as_str)
                            .eq(expected.helper_ids.iter().copied())
                }),
        "tool fingerprint audit mismatch",
    )?;
    require(
        capabilities.destinations().len() == expected.destinations.len()
            && capabilities
                .destinations()
                .iter()
                .zip(expected.destinations)
                .all(|(actual, expected)| {
                    actual.host() == expected.host && actual.port() == expected.port
                }),
        "destination audit mismatch",
    )
}

fn require(condition: bool, message: &str) -> Result<(), String> {
    if condition {
        Ok(())
    } else {
        Err(message.to_owned())
    }
}

fn gate_error(backend: &BackendIdentity) -> SandboxError {
    SandboxError::unavailable(
        Some(backend.clone()),
        FailedGuarantee::BrokerAvailability,
        Remediation::InspectSandboxDoctor,
        DiagnosticMetadata::empty(),
    )
}
