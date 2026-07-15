use ricochet_application::SecretName;

#[test]
fn secret_name_accepts_the_exact_shared_grammar() {
    for valid in [
        "a".to_string(),
        "0".to_string(),
        "api.openai_key-2".to_string(),
        "a".repeat(128),
    ] {
        let parsed = SecretName::parse(&valid)
            .unwrap_or_else(|error| panic!("valid secret name should parse: {error}"));
        assert_eq!(
            serde_json::to_string(&parsed).expect("secret name should serialize"),
            serde_json::to_string(&valid).expect("fixture should serialize")
        );
    }
}

#[test]
fn secret_name_rejects_every_value_outside_the_exact_shared_grammar() {
    for invalid in [
        "".to_string(),
        "a".repeat(129),
        "UPPER".to_string(),
        "-leading".to_string(),
        ".leading".to_string(),
        "_leading".to_string(),
        "has space".to_string(),
        "nonascii-é".to_string(),
    ] {
        assert!(
            SecretName::parse(&invalid).is_err(),
            "invalid secret name should be rejected"
        );
    }
}

#[test]
fn secret_name_string_deserialization_uses_the_same_parser() {
    let parsed: SecretName =
        serde_json::from_str("\"api.openai_key-2\"").expect("valid name should deserialize");
    assert_eq!(
        serde_json::to_string(&parsed).expect("parsed name should serialize"),
        "\"api.openai_key-2\""
    );

    assert!(serde_json::from_str::<SecretName>("\"UPPER\"").is_err());
    assert!(serde_json::from_str::<SecretName>("7").is_err());
}

#[test]
fn secret_name_formatting_does_not_reveal_the_name() {
    let name = SecretName::parse("sensitive.secret-name").expect("fixture should parse");

    assert!(!format!("{name}").contains("sensitive"));
    assert!(!format!("{name:?}").contains("sensitive"));
}
