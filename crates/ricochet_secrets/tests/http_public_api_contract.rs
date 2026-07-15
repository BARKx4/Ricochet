use std::collections::BTreeSet;

use ricochet_secrets::{
    EnvironmentCredentialPolicy, SecretHttpPolicySnapshot, SecretsHttpExecutor,
};

fn assert_send_sync<T: Send + Sync>() {}

#[test]
fn http_public_api_contract_exposes_only_opaque_send_types() {
    assert_send_sync::<SecretsHttpExecutor>();
    assert_send_sync::<SecretHttpPolicySnapshot>();
    assert_send_sync::<EnvironmentCredentialPolicy>();

    let environment = EnvironmentCredentialPolicy::new(
        true,
        Some(BTreeSet::from(["provider.api-key".to_string()])),
    );
    let policy = SecretHttpPolicySnapshot::new(
        true,
        Some(BTreeSet::from(["api.example.test".to_string()])),
        BTreeSet::new(),
        environment.clone(),
    );
    let executor = SecretsHttpExecutor::new();

    for rendered in [
        format!("{environment:?}"),
        format!("{policy:?}"),
        format!("{executor:?}"),
    ] {
        assert!(!rendered.contains("provider.api-key"));
        assert!(!rendered.contains("api.example.test"));
    }
}
