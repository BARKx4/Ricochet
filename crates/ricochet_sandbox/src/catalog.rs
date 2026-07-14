use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::error::{DiagnosticMetadata, FailedGuarantee, SandboxError};
use crate::identity::{CatalogGeneration, Sha256Digest, ToolId, UnixMillis};
use crate::version::CATALOG_SCHEMA_V1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub enum OperatingSystem {
    Windows,
    Linux,
    Macos,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub enum Architecture {
    X86_64,
    Aarch64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlatformId {
    pub os: OperatingSystem,
    pub arch: Architecture,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub enum TransportAdapter {
    HttpConnect,
    SshProxyCommand,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub enum ArtifactKind {
    Executable,
    Library,
    Resource,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HashedArtifact {
    pub logical_name: String,
    pub managed_canonical_path: String,
    pub sha256: Sha256Digest,
    pub kind: ArtifactKind,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolReference {
    pub tool_id: ToolId,
    pub sha256: Sha256Digest,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApprovalActor {
    pub display_name: String,
    pub mechanism: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplacementLineage {
    pub prior_generation: CatalogGeneration,
    pub prior_sha256: Sha256Digest,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogRecord {
    pub schema_version: u16,
    pub generation: CatalogGeneration,
    pub tool_id: ToolId,
    pub platform: PlatformId,
    pub original_source_path: String,
    pub executable: HashedArtifact,
    pub helpers: Vec<ToolReference>,
    pub non_system_libraries: Vec<HashedArtifact>,
    pub resources: Vec<HashedArtifact>,
    pub transport_adapter: Option<TransportAdapter>,
    pub approval_actor: ApprovalActor,
    pub approved_at: UnixMillis,
    pub replaces: Option<ReplacementLineage>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogSnapshot {
    pub schema_version: u16,
    pub generation: CatalogGeneration,
    pub platform: PlatformId,
    pub records: Vec<CatalogRecord>,
    pub revoked_tools: Vec<ToolId>,
}

pub trait CatalogPathNormalizer {
    #[allow(clippy::result_large_err)]
    fn normalize(&self, platform: &PlatformId, path: &str) -> Result<String, SandboxError>;
}

pub struct PreparedTool {
    tool_id: ToolId,
    executable: HashedArtifact,
    helper_ids: Vec<ToolId>,
    non_system_libraries: Vec<HashedArtifact>,
    resources: Vec<HashedArtifact>,
    transport_adapter: Option<TransportAdapter>,
}

pub struct PreparedCatalogClosure {
    generation: CatalogGeneration,
    platform: PlatformId,
    roots: BTreeSet<ToolId>,
    tools: BTreeMap<ToolId, PreparedTool>,
}

pub struct ValidatedCatalogSnapshot {
    schema_version: u16,
    generation: CatalogGeneration,
    platform: PlatformId,
    records: BTreeMap<ToolId, CatalogRecord>,
    revoked_tools: BTreeSet<ToolId>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicToolRecord {
    pub tool_id: ToolId,
    pub executable_sha256: Sha256Digest,
    pub helper_ids: Vec<ToolId>,
    pub transport_adapter: Option<TransportAdapter>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicCatalogSnapshot {
    pub schema_version: u16,
    pub generation: CatalogGeneration,
    pub platform: PlatformId,
    pub records: Vec<PublicToolRecord>,
    pub revoked_tools: Vec<ToolId>,
}

impl CatalogSnapshot {
    #[allow(clippy::result_large_err)]
    pub fn validate(
        self,
        path_normalizer: &dyn CatalogPathNormalizer,
    ) -> Result<ValidatedCatalogSnapshot, SandboxError> {
        if self.schema_version != CATALOG_SCHEMA_V1 {
            return Err(policy_error());
        }

        let mut records = BTreeMap::new();
        for record in self.records {
            if record.schema_version != CATALOG_SCHEMA_V1
                || record.generation != self.generation
                || record.platform != self.platform
                || record
                    .replaces
                    .is_some_and(|lineage| lineage.prior_generation >= self.generation)
            {
                return Err(policy_error());
            }

            let tool_id = record.tool_id.clone();
            if records.insert(tool_id, record).is_some() {
                return Err(policy_error());
            }
        }

        let revoked_tools = self.revoked_tools.into_iter().collect::<BTreeSet<_>>();
        let mut normalized_paths = BTreeSet::new();
        for record in records.values_mut() {
            validate_record(
                record,
                &self.platform,
                path_normalizer,
                &mut normalized_paths,
            )?;
        }
        validate_helper_graph(&records, &revoked_tools)?;

        Ok(ValidatedCatalogSnapshot {
            schema_version: self.schema_version,
            generation: self.generation,
            platform: self.platform,
            records,
            revoked_tools,
        })
    }
}

impl ValidatedCatalogSnapshot {
    pub fn generation(&self) -> CatalogGeneration {
        self.generation
    }

    #[allow(clippy::result_large_err)]
    pub fn activate(&self, requested: &[ToolId]) -> Result<PreparedCatalogClosure, SandboxError> {
        let roots = requested.iter().cloned().collect::<BTreeSet<_>>();
        let mut selected = BTreeSet::new();
        let mut pending = roots.iter().rev().cloned().collect::<Vec<_>>();
        while let Some(tool_id) = pending.pop() {
            if self.revoked_tools.contains(&tool_id) {
                return Err(SandboxError::tool_not_approved(tool_id));
            }
            let Some(record) = self.records.get(&tool_id) else {
                return Err(SandboxError::tool_not_approved(tool_id));
            };
            if !selected.insert(tool_id) {
                continue;
            }
            pending.extend(
                record
                    .helpers
                    .iter()
                    .rev()
                    .map(|reference| reference.tool_id.clone()),
            );
        }

        let tools = selected
            .into_iter()
            .map(|tool_id| {
                let record = &self.records[&tool_id];
                let prepared = PreparedTool {
                    tool_id: tool_id.clone(),
                    executable: record.executable.clone(),
                    helper_ids: record
                        .helpers
                        .iter()
                        .map(|reference| reference.tool_id.clone())
                        .collect(),
                    non_system_libraries: record.non_system_libraries.clone(),
                    resources: record.resources.clone(),
                    transport_adapter: record.transport_adapter,
                };
                (tool_id, prepared)
            })
            .collect();

        Ok(PreparedCatalogClosure {
            generation: self.generation,
            platform: self.platform,
            roots,
            tools,
        })
    }

    pub fn public_snapshot(&self) -> PublicCatalogSnapshot {
        PublicCatalogSnapshot {
            schema_version: self.schema_version,
            generation: self.generation,
            platform: self.platform,
            records: self.records.values().map(public_record).collect(),
            revoked_tools: self.revoked_tools.iter().cloned().collect(),
        }
    }
}

impl PreparedCatalogClosure {
    pub fn generation(&self) -> CatalogGeneration {
        self.generation
    }

    pub fn platform(&self) -> &PlatformId {
        &self.platform
    }

    pub fn roots(&self) -> &BTreeSet<ToolId> {
        &self.roots
    }

    pub fn tools(&self) -> &BTreeMap<ToolId, PreparedTool> {
        &self.tools
    }

    pub fn public_records(&self) -> Vec<PublicToolRecord> {
        self.tools.values().map(public_prepared_tool).collect()
    }
}

impl fmt::Debug for PreparedCatalogClosure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PreparedCatalogClosure")
            .field("generation", &self.generation)
            .field("platform", &self.platform)
            .field("roots", &self.roots)
            .field("tools", &RedactedPreparedTools(&self.tools))
            .finish()
    }
}

impl PreparedTool {
    pub fn tool_id(&self) -> &ToolId {
        &self.tool_id
    }

    pub fn executable(&self) -> &HashedArtifact {
        &self.executable
    }

    pub fn helper_ids(&self) -> &[ToolId] {
        &self.helper_ids
    }

    pub fn non_system_libraries(&self) -> &[HashedArtifact] {
        &self.non_system_libraries
    }

    pub fn resources(&self) -> &[HashedArtifact] {
        &self.resources
    }

    pub fn transport_adapter(&self) -> Option<TransportAdapter> {
        self.transport_adapter
    }
}

struct RedactedPreparedTools<'a>(&'a BTreeMap<ToolId, PreparedTool>);

impl fmt::Debug for RedactedPreparedTools<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut map = formatter.debug_map();
        for (tool_id, tool) in self.0 {
            map.entry(tool_id, &RedactedPreparedTool(tool));
        }
        map.finish()
    }
}

struct RedactedPreparedTool<'a>(&'a PreparedTool);

impl fmt::Debug for RedactedPreparedTool<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let library_hashes = self
            .0
            .non_system_libraries
            .iter()
            .map(|artifact| artifact.sha256)
            .collect::<Vec<_>>();
        let resource_hashes = self
            .0
            .resources
            .iter()
            .map(|artifact| artifact.sha256)
            .collect::<Vec<_>>();

        formatter
            .debug_struct("PreparedTool")
            .field("tool_id", &self.0.tool_id)
            .field("executable_sha256", &self.0.executable.sha256)
            .field("helper_ids", &self.0.helper_ids)
            .field("library_sha256", &library_hashes)
            .field("resource_sha256", &resource_hashes)
            .finish()
    }
}

#[allow(clippy::result_large_err)]
fn validate_record(
    record: &mut CatalogRecord,
    platform: &PlatformId,
    path_normalizer: &dyn CatalogPathNormalizer,
    normalized_paths: &mut BTreeSet<String>,
) -> Result<(), SandboxError> {
    if !valid_catalog_text(&record.original_source_path)
        || !valid_catalog_text(&record.approval_actor.display_name)
        || !valid_catalog_text(&record.approval_actor.mechanism)
    {
        return Err(policy_error());
    }

    let mut logical_names = BTreeSet::new();
    validate_artifact(
        &mut record.executable,
        ArtifactKind::Executable,
        platform,
        path_normalizer,
        &mut logical_names,
        normalized_paths,
    )?;
    for artifact in &mut record.non_system_libraries {
        validate_artifact(
            artifact,
            ArtifactKind::Library,
            platform,
            path_normalizer,
            &mut logical_names,
            normalized_paths,
        )?;
    }
    for artifact in &mut record.resources {
        validate_artifact(
            artifact,
            ArtifactKind::Resource,
            platform,
            path_normalizer,
            &mut logical_names,
            normalized_paths,
        )?;
    }

    let mut helper_ids = BTreeSet::new();
    for reference in &record.helpers {
        if !helper_ids.insert(reference.tool_id.clone()) {
            return Err(policy_error());
        }
    }

    record.helpers.sort();
    record.non_system_libraries.sort();
    record.resources.sort();
    Ok(())
}

#[allow(clippy::too_many_arguments, clippy::result_large_err)]
fn validate_artifact(
    artifact: &mut HashedArtifact,
    expected_kind: ArtifactKind,
    platform: &PlatformId,
    path_normalizer: &dyn CatalogPathNormalizer,
    logical_names: &mut BTreeSet<String>,
    normalized_paths: &mut BTreeSet<String>,
) -> Result<(), SandboxError> {
    if artifact.kind != expected_kind
        || !valid_catalog_text(&artifact.logical_name)
        || !logical_names.insert(artifact.logical_name.clone())
        || !valid_catalog_text(&artifact.managed_canonical_path)
    {
        return Err(policy_error());
    }

    let normalized = path_normalizer.normalize(platform, &artifact.managed_canonical_path)?;
    if !valid_catalog_text(&normalized) || !normalized_paths.insert(normalized.clone()) {
        return Err(policy_error());
    }
    artifact.managed_canonical_path = normalized;
    Ok(())
}

#[allow(clippy::result_large_err)]
fn validate_helper_graph(
    records: &BTreeMap<ToolId, CatalogRecord>,
    revoked_tools: &BTreeSet<ToolId>,
) -> Result<(), SandboxError> {
    for record in records.values() {
        for reference in &record.helpers {
            if revoked_tools.contains(&reference.tool_id) {
                return Err(SandboxError::tool_not_approved(reference.tool_id.clone()));
            }
            let Some(helper) = records.get(&reference.tool_id) else {
                return Err(SandboxError::tool_not_approved(reference.tool_id.clone()));
            };
            if reference.sha256 != helper.executable.sha256 {
                return Err(SandboxError::tool_fingerprint_mismatch(
                    reference.tool_id.clone(),
                ));
            }
        }
    }

    let mut states = BTreeMap::new();
    for root in records.keys() {
        if states.get(root) == Some(&VisitState::Complete) {
            continue;
        }

        states.insert(root.clone(), VisitState::Visiting);
        let mut stack = vec![VisitFrame {
            tool_id: root.clone(),
            next_helper_index: 0,
        }];
        while let Some(frame) = stack.last_mut() {
            let record = &records[&frame.tool_id];
            let Some(reference) = record.helpers.get(frame.next_helper_index) else {
                let completed = stack.pop().expect("helper traversal stack is nonempty");
                states.insert(completed.tool_id, VisitState::Complete);
                continue;
            };
            frame.next_helper_index += 1;

            match states.get(&reference.tool_id) {
                Some(VisitState::Visiting) => return Err(policy_error()),
                Some(VisitState::Complete) => {}
                None => {
                    states.insert(reference.tool_id.clone(), VisitState::Visiting);
                    stack.push(VisitFrame {
                        tool_id: reference.tool_id.clone(),
                        next_helper_index: 0,
                    });
                }
            }
        }
    }
    Ok(())
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum VisitState {
    Visiting,
    Complete,
}

struct VisitFrame {
    tool_id: ToolId,
    next_helper_index: usize,
}

fn public_record(record: &CatalogRecord) -> PublicToolRecord {
    PublicToolRecord {
        tool_id: record.tool_id.clone(),
        executable_sha256: record.executable.sha256,
        helper_ids: record
            .helpers
            .iter()
            .map(|reference| reference.tool_id.clone())
            .collect(),
        transport_adapter: record.transport_adapter,
    }
}

fn public_prepared_tool(tool: &PreparedTool) -> PublicToolRecord {
    PublicToolRecord {
        tool_id: tool.tool_id.clone(),
        executable_sha256: tool.executable.sha256,
        helper_ids: tool.helper_ids.clone(),
        transport_adapter: tool.transport_adapter,
    }
}

fn valid_catalog_text(value: &str) -> bool {
    !value.is_empty() && !value.contains('\0')
}

fn policy_error() -> SandboxError {
    SandboxError::policy(FailedGuarantee::PolicyValidity, DiagnosticMetadata::empty())
}
