use std::collections::BTreeSet;

use ricochet_sandbox::{
    ApprovalActor, Architecture, ArtifactKind, CatalogGeneration, CatalogPathNormalizer,
    CatalogRecord, CatalogSnapshot, HashedArtifact, OperatingSystem, PlatformId,
    PublicCatalogSnapshot, PublicToolRecord, ReplacementLineage, SandboxError, Sha256Digest,
    ToolId, ToolReference, TransportAdapter, UnixMillis, CATALOG_SCHEMA_V1,
};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{json, Value};

#[derive(Clone, Copy)]
struct FixturePathNormalizer;

impl CatalogPathNormalizer for FixturePathNormalizer {
    fn normalize(&self, platform: &PlatformId, path: &str) -> Result<String, SandboxError> {
        let separators_normalized = path.replace('/', "\\");
        Ok(match platform.os {
            OperatingSystem::Windows => separators_normalized.to_ascii_lowercase(),
            OperatingSystem::Linux => path.to_owned(),
            OperatingSystem::Macos => path.replace('\\', "/").to_ascii_lowercase(),
        })
    }
}

fn fixture_path_normalizer() -> FixturePathNormalizer {
    FixturePathNormalizer
}

fn generation(value: u64) -> CatalogGeneration {
    CatalogGeneration::new(value).unwrap()
}

fn tool_id(value: &str) -> ToolId {
    ToolId::parse(value).unwrap()
}

fn digest(value: &str) -> Sha256Digest {
    Sha256Digest::hash(value.as_bytes())
}

fn windows_platform() -> PlatformId {
    PlatformId {
        os: OperatingSystem::Windows,
        arch: Architecture::X86_64,
    }
}

fn artifact(tool: &str, suffix: &str, kind: ArtifactKind) -> HashedArtifact {
    HashedArtifact {
        logical_name: format!("{tool}-{suffix}"),
        managed_canonical_path: format!(r"C:\Managed\{tool}\{tool}-{suffix}.bin"),
        sha256: digest(&format!("{tool}-{suffix}")),
        kind,
    }
}

fn record(
    tool: &str,
    helpers: impl IntoIterator<Item = (&'static str, Sha256Digest)>,
) -> CatalogRecord {
    CatalogRecord {
        schema_version: CATALOG_SCHEMA_V1,
        generation: generation(7),
        tool_id: tool_id(tool),
        platform: windows_platform(),
        original_source_path: format!(r"C:\Approved\{tool}.exe"),
        executable: artifact(tool, "executable", ArtifactKind::Executable),
        helpers: helpers
            .into_iter()
            .map(|(helper, sha256)| ToolReference {
                tool_id: tool_id(helper),
                sha256,
            })
            .collect(),
        non_system_libraries: Vec::new(),
        resources: Vec::new(),
        transport_adapter: None,
        approval_actor: ApprovalActor {
            display_name: "Sandbox Administrator".to_owned(),
            mechanism: "interactive-consent-v1".to_owned(),
        },
        approved_at: UnixMillis::new(1_752_435_200_000),
        replaces: None,
    }
}

fn valid_catalog() -> CatalogSnapshot {
    let connect = record("connect-helper", []);
    let mut ssh = record("ssh", [("connect-helper", connect.executable.sha256)]);
    ssh.transport_adapter = Some(TransportAdapter::SshProxyCommand);
    let mut git = record("git", [("ssh", ssh.executable.sha256)]);
    git.non_system_libraries
        .push(artifact("git", "transport", ArtifactKind::Library));
    git.resources
        .push(artifact("git", "config", ArtifactKind::Resource));

    CatalogSnapshot {
        schema_version: CATALOG_SCHEMA_V1,
        generation: generation(7),
        platform: windows_platform(),
        records: vec![ssh, git, connect],
        revoked_tools: vec![tool_id("obsolete-tool")],
    }
}

fn find_record_mut<'a>(snapshot: &'a mut CatalogSnapshot, id: &str) -> &'a mut CatalogRecord {
    snapshot
        .records
        .iter_mut()
        .find(|record| record.tool_id.as_str() == id)
        .unwrap()
}

#[test]
fn valid_catalog_builds_complete_transitive_helper_closure() {
    let snapshot = valid_catalog()
        .validate(&fixture_path_normalizer())
        .unwrap();
    let closure = snapshot.activate(&[tool_id("git")]).unwrap();

    assert_eq!(closure.generation(), generation(7));
    assert_eq!(closure.platform(), &windows_platform());
    assert_eq!(closure.roots(), &BTreeSet::from([tool_id("git")]));
    assert_eq!(
        closure
            .tools()
            .keys()
            .map(ToolId::as_str)
            .collect::<Vec<_>>(),
        vec!["connect-helper", "git", "ssh"]
    );
    assert_eq!(
        closure.tools()[&tool_id("git")]
            .helper_ids()
            .iter()
            .map(ToolId::as_str)
            .collect::<Vec<_>>(),
        vec!["ssh"]
    );
    assert_eq!(
        closure.tools()[&tool_id("ssh")]
            .helper_ids()
            .iter()
            .map(ToolId::as_str)
            .collect::<Vec<_>>(),
        vec!["connect-helper"]
    );
    assert_eq!(
        closure.tools()[&tool_id("git")]
            .executable()
            .managed_canonical_path,
        r"c:\managed\git\git-executable.bin"
    );
    assert_eq!(
        closure.tools()[&tool_id("git")]
            .non_system_libraries()
            .len(),
        1
    );
    assert_eq!(closure.tools()[&tool_id("git")].resources().len(), 1);
    assert_eq!(
        closure.tools()[&tool_id("ssh")].transport_adapter(),
        Some(TransportAdapter::SshProxyCommand)
    );
    assert_eq!(closure.tools()[&tool_id("git")].tool_id(), &tool_id("git"));
}

#[test]
fn missing_helper_record_is_rejected() {
    let mut snapshot = valid_catalog();
    snapshot
        .records
        .retain(|record| record.tool_id.as_str() != "connect-helper");

    assert!(snapshot.validate(&fixture_path_normalizer()).is_err());
}

#[test]
fn helper_fingerprint_must_match_its_executable() {
    let mut snapshot = valid_catalog();
    find_record_mut(&mut snapshot, "ssh").helpers[0].sha256 = digest("wrong-helper");

    let error = snapshot
        .validate(&fixture_path_normalizer())
        .err()
        .expect("fingerprint mismatch must fail validation");
    assert_eq!(error.kind(), "ToolFingerprintMismatch");
}

#[test]
fn helper_cycles_are_rejected() {
    let mut snapshot = valid_catalog();
    let git_sha256 = find_record_mut(&mut snapshot, "git").executable.sha256;
    find_record_mut(&mut snapshot, "connect-helper")
        .helpers
        .push(ToolReference {
            tool_id: tool_id("git"),
            sha256: git_sha256,
        });

    assert!(snapshot.validate(&fixture_path_normalizer()).is_err());
}

#[test]
fn deep_helper_chains_validate_and_activate_without_using_call_stack_depth() {
    const CHAIN_LENGTH: usize = 1_024;

    let mut records = Vec::with_capacity(CHAIN_LENGTH);
    for index in 0..CHAIN_LENGTH {
        let current = format!("deep-{index:04}");
        let mut current_record = record(&current, []);
        if index + 1 < CHAIN_LENGTH {
            let next = format!("deep-{:04}", index + 1);
            current_record.helpers.push(ToolReference {
                tool_id: tool_id(&next),
                sha256: digest(&format!("{next}-executable")),
            });
        }
        records.push(current_record);
    }
    records.reverse();

    let catalog = CatalogSnapshot {
        schema_version: CATALOG_SCHEMA_V1,
        generation: generation(7),
        platform: windows_platform(),
        records,
        revoked_tools: Vec::new(),
    };

    let tool_count = std::thread::Builder::new()
        .stack_size(128 * 1_024)
        .spawn(move || {
            let validated = catalog.validate(&fixture_path_normalizer()).unwrap();
            validated
                .activate(&[tool_id("deep-0000")])
                .unwrap()
                .tools()
                .len()
        })
        .unwrap()
        .join()
        .unwrap();

    assert_eq!(tool_count, CHAIN_LENGTH);
}

#[test]
fn duplicate_tool_ids_are_rejected() {
    let mut snapshot = valid_catalog();
    snapshot.records.push(snapshot.records[0].clone());

    assert!(snapshot.validate(&fixture_path_normalizer()).is_err());
}

#[test]
fn duplicate_helper_references_are_rejected() {
    let mut snapshot = valid_catalog();
    let duplicate = find_record_mut(&mut snapshot, "git").helpers[0].clone();
    find_record_mut(&mut snapshot, "git")
        .helpers
        .push(duplicate);

    assert!(snapshot.validate(&fixture_path_normalizer()).is_err());
}

#[test]
fn duplicate_library_entries_are_rejected() {
    let mut snapshot = valid_catalog();
    let duplicate = find_record_mut(&mut snapshot, "git").non_system_libraries[0].clone();
    find_record_mut(&mut snapshot, "git")
        .non_system_libraries
        .push(duplicate);

    assert!(snapshot.validate(&fixture_path_normalizer()).is_err());
}

#[test]
fn duplicate_resource_entries_are_rejected() {
    let mut snapshot = valid_catalog();
    let duplicate = find_record_mut(&mut snapshot, "git").resources[0].clone();
    find_record_mut(&mut snapshot, "git")
        .resources
        .push(duplicate);

    assert!(snapshot.validate(&fixture_path_normalizer()).is_err());
}

#[test]
fn logical_names_are_unique_across_a_tool_artifact_set() {
    let mut snapshot = valid_catalog();
    let library_name = find_record_mut(&mut snapshot, "git").non_system_libraries[0]
        .logical_name
        .clone();
    find_record_mut(&mut snapshot, "git").resources[0].logical_name = library_name;

    assert!(snapshot.validate(&fixture_path_normalizer()).is_err());
}

#[test]
fn normalized_managed_path_aliases_are_rejected() {
    let mut snapshot = valid_catalog();
    let executable_path = find_record_mut(&mut snapshot, "git")
        .executable
        .managed_canonical_path
        .replace('\\', "/")
        .to_ascii_uppercase();
    find_record_mut(&mut snapshot, "git").non_system_libraries[0].managed_canonical_path =
        executable_path;

    assert!(snapshot.validate(&fixture_path_normalizer()).is_err());
}

#[test]
fn normalized_managed_path_aliases_are_rejected_across_tool_records() {
    let mut snapshot = valid_catalog();
    let git_path = find_record_mut(&mut snapshot, "git")
        .executable
        .managed_canonical_path
        .clone();
    find_record_mut(&mut snapshot, "ssh")
        .executable
        .managed_canonical_path = git_path.replace('\\', "/").to_ascii_uppercase();

    assert!(snapshot.validate(&fixture_path_normalizer()).is_err());
}

#[test]
fn path_normalization_is_platform_specific_and_deterministic() {
    let mut linux = valid_catalog();
    linux.platform.os = OperatingSystem::Linux;
    for record in &mut linux.records {
        record.platform.os = OperatingSystem::Linux;
    }
    let git = find_record_mut(&mut linux, "git");
    git.executable.managed_canonical_path = "/store/Git".to_owned();
    git.non_system_libraries[0].managed_canonical_path = "/store/git".to_owned();
    assert!(linux.validate(&fixture_path_normalizer()).is_ok());

    let mut macos = valid_catalog();
    macos.platform.os = OperatingSystem::Macos;
    for record in &mut macos.records {
        record.platform.os = OperatingSystem::Macos;
    }
    let git = find_record_mut(&mut macos, "git");
    git.executable.managed_canonical_path = "/Store/Git".to_owned();
    git.non_system_libraries[0].managed_canonical_path = "/store/git".to_owned();
    assert!(macos.validate(&fixture_path_normalizer()).is_err());
}

#[test]
fn artifact_kinds_must_match_their_roles() {
    let mut executable = valid_catalog();
    find_record_mut(&mut executable, "git").executable.kind = ArtifactKind::Library;
    assert!(executable.validate(&fixture_path_normalizer()).is_err());

    let mut library = valid_catalog();
    find_record_mut(&mut library, "git").non_system_libraries[0].kind = ArtifactKind::Resource;
    assert!(library.validate(&fixture_path_normalizer()).is_err());

    let mut resource = valid_catalog();
    find_record_mut(&mut resource, "git").resources[0].kind = ArtifactKind::Executable;
    assert!(resource.validate(&fixture_path_normalizer()).is_err());
}

#[test]
fn snapshot_and_record_schema_versions_must_be_v1() {
    let mut snapshot = valid_catalog();
    snapshot.schema_version = CATALOG_SCHEMA_V1 + 1;
    assert!(snapshot.validate(&fixture_path_normalizer()).is_err());

    let mut record = valid_catalog();
    find_record_mut(&mut record, "git").schema_version = CATALOG_SCHEMA_V1 + 1;
    assert!(record.validate(&fixture_path_normalizer()).is_err());
}

#[test]
fn every_record_must_match_snapshot_generation_and_platform() {
    let mut generation_mismatch = valid_catalog();
    find_record_mut(&mut generation_mismatch, "git").generation = generation(6);
    assert!(generation_mismatch
        .validate(&fixture_path_normalizer())
        .is_err());

    let mut platform_mismatch = valid_catalog();
    find_record_mut(&mut platform_mismatch, "git").platform.arch = Architecture::Aarch64;
    assert!(platform_mismatch
        .validate(&fixture_path_normalizer())
        .is_err());
}

#[test]
fn replacement_lineage_must_be_strictly_older() {
    for prior_generation in [generation(7), generation(8)] {
        let mut snapshot = valid_catalog();
        find_record_mut(&mut snapshot, "git").replaces = Some(ReplacementLineage {
            prior_generation,
            prior_sha256: digest("old-git"),
        });
        assert!(snapshot.validate(&fixture_path_normalizer()).is_err());
    }

    let mut older = valid_catalog();
    find_record_mut(&mut older, "git").replaces = Some(ReplacementLineage {
        prior_generation: generation(6),
        prior_sha256: digest("old-git"),
    });
    assert!(older.validate(&fixture_path_normalizer()).is_ok());
}

#[test]
fn provenance_and_managed_path_strings_are_nonempty_and_nul_free() {
    let mut empty_source = valid_catalog();
    find_record_mut(&mut empty_source, "git")
        .original_source_path
        .clear();
    assert!(empty_source.validate(&fixture_path_normalizer()).is_err());

    let mut nul_managed_path = valid_catalog();
    find_record_mut(&mut nul_managed_path, "git")
        .executable
        .managed_canonical_path
        .push('\0');
    assert!(nul_managed_path
        .validate(&fixture_path_normalizer())
        .is_err());

    let mut empty_actor = valid_catalog();
    find_record_mut(&mut empty_actor, "git")
        .approval_actor
        .display_name
        .clear();
    assert!(empty_actor.validate(&fixture_path_normalizer()).is_err());

    let mut nul_mechanism = valid_catalog();
    find_record_mut(&mut nul_mechanism, "git")
        .approval_actor
        .mechanism
        .push('\0');
    assert!(nul_mechanism.validate(&fixture_path_normalizer()).is_err());
}

#[test]
fn revoked_helpers_are_rejected() {
    let mut snapshot = valid_catalog();
    snapshot.revoked_tools.push(tool_id("ssh"));

    assert!(snapshot.validate(&fixture_path_normalizer()).is_err());
}

#[test]
fn missing_or_revoked_requested_roots_are_rejected() {
    let snapshot = valid_catalog()
        .validate(&fixture_path_normalizer())
        .unwrap();

    assert_eq!(
        snapshot
            .activate(&[tool_id("missing-tool")])
            .unwrap_err()
            .kind(),
        "ToolNotApproved"
    );
    assert_eq!(
        snapshot
            .activate(&[tool_id("obsolete-tool")])
            .unwrap_err()
            .kind(),
        "ToolNotApproved"
    );
}

fn assert_rejects_unknown_field<T>(value: &T)
where
    T: Serialize,
    T: DeserializeOwned,
{
    let mut value = serde_json::to_value(value).unwrap();
    value
        .as_object_mut()
        .unwrap()
        .insert("gremlin_field".to_owned(), Value::Bool(true));
    assert!(serde_json::from_value::<T>(value).is_err());
}

#[test]
fn every_serialized_catalog_struct_denies_unknown_fields() {
    let catalog = valid_catalog();
    let record = &catalog.records[0];
    assert_rejects_unknown_field(&catalog.platform);
    assert_rejects_unknown_field(&record.executable);
    assert_rejects_unknown_field(&record.helpers[0]);
    assert_rejects_unknown_field(&record.approval_actor);
    assert_rejects_unknown_field(&ReplacementLineage {
        prior_generation: generation(6),
        prior_sha256: digest("prior"),
    });
    assert_rejects_unknown_field(record);
    assert_rejects_unknown_field(&catalog);

    let public = catalog
        .validate(&fixture_path_normalizer())
        .unwrap()
        .public_snapshot();
    assert_rejects_unknown_field(&public.records[0]);
    assert_rejects_unknown_field(&public);

    assert!(serde_json::from_value::<OperatingSystem>(json!("Plan9")).is_err());
    assert!(serde_json::from_value::<Architecture>(json!("RiscV64")).is_err());
    assert!(serde_json::from_value::<ArtifactKind>(json!("HelperExecutable")).is_err());
    assert!(serde_json::from_value::<TransportAdapter>(json!("RawSocket")).is_err());
}

#[test]
fn identity_values_are_revalidated_during_catalog_deserialization() {
    let mut value = serde_json::to_value(valid_catalog()).unwrap();
    value["records"][0]["tool_id"] = json!("Uppercase-Is-Invalid");
    assert!(serde_json::from_value::<CatalogSnapshot>(value).is_err());

    let mut value = serde_json::to_value(valid_catalog()).unwrap();
    value["generation"] = json!(0);
    assert!(serde_json::from_value::<CatalogSnapshot>(value).is_err());

    let mut value = serde_json::to_value(valid_catalog()).unwrap();
    value["records"][0]["executable"]["sha256"] = json!("not-a-digest");
    assert!(serde_json::from_value::<CatalogSnapshot>(value).is_err());
}

#[test]
fn activation_and_public_outputs_are_deterministically_sorted() {
    let mut catalog = valid_catalog();
    catalog.revoked_tools = vec![tool_id("z-tool"), tool_id("a-tool")];
    let snapshot = catalog.validate(&fixture_path_normalizer()).unwrap();
    let closure = snapshot
        .activate(&[tool_id("ssh"), tool_id("git"), tool_id("git")])
        .unwrap();

    assert_eq!(
        closure
            .roots()
            .iter()
            .map(ToolId::as_str)
            .collect::<Vec<_>>(),
        vec!["git", "ssh"]
    );
    assert_eq!(
        closure
            .public_records()
            .iter()
            .map(|record| record.tool_id.as_str())
            .collect::<Vec<_>>(),
        vec!["connect-helper", "git", "ssh"]
    );
    assert_eq!(
        snapshot
            .public_snapshot()
            .records
            .iter()
            .map(|record| record.tool_id.as_str())
            .collect::<Vec<_>>(),
        vec!["connect-helper", "git", "ssh"]
    );
    assert_eq!(
        snapshot
            .public_snapshot()
            .revoked_tools
            .iter()
            .map(ToolId::as_str)
            .collect::<Vec<_>>(),
        vec!["a-tool", "z-tool"]
    );
}

#[test]
fn public_catalog_exposes_generation_platform_records_and_revocation_state() {
    let public = valid_catalog()
        .validate(&fixture_path_normalizer())
        .unwrap()
        .public_snapshot();

    assert_eq!(public.schema_version, CATALOG_SCHEMA_V1);
    assert_eq!(public.generation, generation(7));
    assert_eq!(public.platform, windows_platform());
    assert_eq!(public.revoked_tools, vec![tool_id("obsolete-tool")]);
    let git = public
        .records
        .iter()
        .find(|record| record.tool_id.as_str() == "git")
        .unwrap();
    assert_eq!(git.executable_sha256, digest("git-executable"));
    assert_eq!(git.helper_ids, vec![tool_id("ssh")]);
}

#[test]
fn public_catalog_metadata_never_exposes_managed_paths() {
    let snapshot = valid_catalog()
        .validate(&fixture_path_normalizer())
        .unwrap();
    let json = serde_json::to_string(&snapshot.public_snapshot()).unwrap();
    assert!(!json.contains("original_source_path"));
    assert!(!json.contains("managed_canonical_path"));
    assert!(!json.contains("approval_actor"));
    assert!(!json.contains("replaces"));
    assert!(!json.contains("C:\\"));
    assert!(!json.contains("/var/"));
}

#[test]
fn public_closure_records_never_expose_managed_paths() {
    let closure = valid_catalog()
        .validate(&fixture_path_normalizer())
        .unwrap()
        .activate(&[tool_id("git")])
        .unwrap();
    let json = serde_json::to_string(&closure.public_records()).unwrap();
    assert!(!json.contains("original_source_path"));
    assert!(!json.contains("managed_canonical_path"));
    assert!(!json.contains("approval_actor"));
    assert!(!json.contains("C:\\"));
}

#[test]
fn prepared_closure_debug_includes_only_public_identity_and_hash_material() {
    let closure = valid_catalog()
        .validate(&fixture_path_normalizer())
        .unwrap()
        .activate(&[tool_id("git")])
        .unwrap();
    let debug = format!("{closure:?}");

    assert!(debug.contains("CatalogGeneration(7)"));
    assert!(debug.contains("Windows"));
    assert!(debug.contains("git"));
    assert!(debug.contains(&digest("git-executable").to_hex()));
    assert!(debug.contains(&digest("git-transport").to_hex()));
    assert!(debug.contains(&digest("git-config").to_hex()));
    assert!(!debug.contains("managed"));
    assert!(!debug.contains("approved"));
    assert!(!debug.contains("Sandbox Administrator"));
    assert!(!debug.contains("interactive-consent"));
    assert!(!debug.contains("original_source_path"));
    assert!(!debug.contains("managed_canonical_path"));
    assert!(!debug.contains("C:\\"));
}

#[test]
fn public_catalog_types_round_trip_canonically() {
    let snapshot = valid_catalog()
        .validate(&fixture_path_normalizer())
        .unwrap();
    let public = snapshot.public_snapshot();
    let json = serde_json::to_string(&public).unwrap();
    let decoded = serde_json::from_str::<PublicCatalogSnapshot>(&json).unwrap();
    assert_eq!(decoded, public);

    let record_json = serde_json::to_string(&public.records[0]).unwrap();
    let decoded_record = serde_json::from_str::<PublicToolRecord>(&record_json).unwrap();
    assert_eq!(decoded_record, public.records[0]);
}
