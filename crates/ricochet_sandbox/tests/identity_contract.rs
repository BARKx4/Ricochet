use ricochet_sandbox::{
    BackendFeatureId, BackendIdentity, CatalogGeneration, ProcessId, ProcessTreeId, PtyId,
    RequestId, ScratchId, SessionId, Sha256Digest, ToolId, UnixMillis,
};

#[test]
fn canonical_ids_and_hashes_round_trip() {
    assert_eq!(ToolId::parse("git").unwrap().as_str(), "git");
    assert_eq!(
        SessionId::parse("session-01").unwrap().as_str(),
        "session-01"
    );
    assert!(ToolId::parse("Git").is_err());
    assert!(ToolId::parse("-git").is_err());
    assert_eq!(ProcessId::new(0).get(), 0);
    assert_eq!(PtyId::new(0).get(), 0);
    assert!(CatalogGeneration::new(0).is_err());

    let hex = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    assert_eq!(Sha256Digest::parse_hex(hex).unwrap().to_hex(), hex);
    assert!(Sha256Digest::parse_hex(&hex.to_uppercase()).is_err());
}

#[test]
fn string_id_grammar_rejects_noncanonical_values() {
    for valid in ["a", "tool_01", "tool.name", "tool-name", "a1"] {
        assert_eq!(ScratchId::parse(valid).unwrap().as_str(), valid);
        assert_eq!(BackendFeatureId::parse(valid).unwrap().as_str(), valid);
    }

    for invalid in [
        "",
        ".tool",
        "tool.",
        "_tool",
        "tool_",
        "tool-",
        "Tool",
        "tool name",
        "tool\nname",
        "tool\0name",
    ] {
        assert!(ToolId::parse(invalid).is_err(), "accepted {invalid:?}");
    }

    assert!(ToolId::parse("a".repeat(65)).is_err());
}

#[test]
fn numeric_ids_preserve_zero_and_the_complete_u64_range() {
    assert_eq!(RequestId::new(0).get(), 0);
    assert_eq!(ProcessTreeId::new(0).get(), 0);
    assert_eq!(UnixMillis::new(0).get(), 0);
    assert_eq!(RequestId::new(u64::MAX).get(), u64::MAX);
    assert_eq!(ProcessId::new(u64::MAX).get(), u64::MAX);
    assert_eq!(PtyId::new(u64::MAX).get(), u64::MAX);
    assert_eq!(ProcessTreeId::new(u64::MAX).get(), u64::MAX);
    assert_eq!(CatalogGeneration::new(u64::MAX).unwrap().get(), u64::MAX);
}

#[test]
fn validated_values_round_trip_through_serde() {
    let tool_id = ToolId::parse("cargo-test").unwrap();
    let encoded = serde_json::to_string(&tool_id).unwrap();
    assert_eq!(encoded, "\"cargo-test\"");
    assert_eq!(
        serde_json::from_str::<ToolId>(&encoded).unwrap().as_str(),
        "cargo-test"
    );
    assert!(serde_json::from_str::<ToolId>("\"Cargo\"").is_err());

    let process_id = ProcessId::new(0);
    assert_eq!(serde_json::to_string(&process_id).unwrap(), "0");
    assert_eq!(serde_json::from_str::<ProcessId>("0").unwrap().get(), 0);

    let hex = "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff";
    let digest = Sha256Digest::parse_hex(hex).unwrap();
    assert_eq!(
        serde_json::to_string(&digest).unwrap(),
        format!("\"{hex}\"")
    );
    assert_eq!(
        serde_json::from_str::<Sha256Digest>(&format!("\"{hex}\""))
            .unwrap()
            .to_hex(),
        hex
    );
}

#[test]
fn backend_identity_validates_name_and_version() {
    let backend = BackendIdentity::new("linux-cgroup", "1.2.3+native").unwrap();
    assert_eq!(backend.name(), "linux-cgroup");
    assert_eq!(backend.version(), "1.2.3+native");
    assert!(BackendIdentity::new("Linux", "1.0").is_err());
    assert!(BackendIdentity::new("linux", "1 0").is_err());
    assert!(BackendIdentity::new("linux", "\u{7f}").is_err());
}

#[test]
fn identity_debug_output_is_intentionally_non_secret() {
    assert_eq!(
        format!("{:?}", ToolId::parse("git").unwrap()),
        "ToolId(\"git\")"
    );
    assert_eq!(format!("{:?}", ProcessId::new(7)), "ProcessId(7)");
    assert_eq!(
        format!(
            "{:?}",
            Sha256Digest::parse_hex(
                "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
            )
            .unwrap()
        ),
        "Sha256Digest(\"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\")"
    );
}
