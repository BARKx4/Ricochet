use std::collections::{BTreeMap, BTreeSet};
use std::net::SocketAddr;
use std::time::Duration;

use ricochet_application::SecretName;
use ricochet_sandbox::DestinationGrant;
use ricochet_secrets::test_host::TestSecretsHttpHost;
use ricochet_secrets::{
    DeferredHttpCredentials, DeferredSecretSource, EnvironmentCredentialPolicy, HostTokenSource,
    SecretHttpPolicySnapshot, SecretSession, SecretSessionErrorKind, SecurityDomainId,
};
use zeroize::Zeroizing;

fn fixture() -> (
    HostTokenSource,
    SecurityDomainId,
    SecretSession,
    ricochet_secrets::SecretSessionGuard,
) {
    let tokens = HostTokenSource::deterministic_for_test([0x41; 32]);
    let domain = SecurityDomainId::generate(&tokens).expect("test security domain");
    let (session, guard) =
        SecretSession::create(&tokens, domain.clone()).expect("test secret session");
    (tokens, domain, session, guard)
}

fn name(value: &str) -> SecretName {
    SecretName::parse(value).expect("fixture secret name")
}

fn policy(
    session: Option<&ricochet_secrets::SecretSessionContext>,
    domain: &SecurityDomainId,
    host: &str,
    port: u16,
) -> SecretHttpPolicySnapshot {
    let base = SecretHttpPolicySnapshot::new(
        true,
        Some(BTreeSet::from([host.to_string()])),
        BTreeSet::from([DestinationGrant::new(host, port).expect("fixture exact destination")]),
        EnvironmentCredentialPolicy::new(false, None),
    );
    match session {
        Some(session) => base.with_secret_session(session.clone(), domain.clone()),
        None => base.with_security_domain(domain.clone()),
    }
}

fn prepared_session_request(
    executor: &ricochet_secrets::SecretsHttpExecutor,
    reference: ricochet_secrets::SecretRef,
    policy: SecretHttpPolicySnapshot,
    host: &str,
    port: u16,
) -> Result<ricochet_secrets::PreparedSecretHttpRequest, ricochet_secrets::SecretHttpError> {
    executor.prepare(
        DeferredHttpCredentials::bearer(DeferredSecretSource::opaque(reference)),
        reqwest::Method::POST,
        format!("https://{host}:{port}/v1/responses"),
        reqwest::header::HeaderMap::new(),
        None,
        None,
        Duration::from_millis(50),
        1024,
        None,
        None,
        policy,
    )
}

#[test]
fn session_contract_binds_reacquires_and_resolves_only_at_native_send() {
    let (_tokens, domain, session, _guard) = fixture();
    let context = session.context();
    let slot = name("provider.openai");
    let reference = context
        .prompt(slot.clone())
        .expect("prebound prompt")
        .bind(Zeroizing::new("synthetic-session-secret".to_string()))
        .expect("valid synthetic secret");
    assert_eq!(format!("{reference:?}"), "<secret-ref>");
    assert!(context.present(&slot).expect("presence query"));
    let reacquired = context.reference(&slot).expect("opaque reacquisition");
    assert_eq!(format!("{reacquired}"), "<secret-ref>");

    let host = "session.example";
    let address = SocketAddr::from(([127, 0, 0, 1], 9));
    let http = TestSecretsHttpHost::new(host, address, BTreeMap::new());
    let prepared = prepared_session_request(
        &http.executor(),
        reacquired,
        policy(Some(&context), &domain, host, address.port()),
        host,
        address.port(),
    )
    .expect("authorization should finish before value resolution");
    assert_eq!(session.test_resolution_count(), 0);
    let error = http
        .executor()
        .execute(prepared)
        .expect_err("unused loopback fixture has no TLS server");
    assert_eq!(error.kind(), "HttpError");
    assert_eq!(session.test_resolution_count(), 1);
    assert_eq!(http.credential_resolution_count(), 1);
    assert_eq!(http.environment_source_access_count(), 0);
}

#[test]
fn session_contract_rejects_stale_cross_domain_and_cross_session_before_value_access() {
    let (tokens, domain, session, _guard) = fixture();
    let context = session.context();
    let slot = name("provider.openai");
    let stale = context
        .prompt(slot.clone())
        .expect("first prompt")
        .bind(Zeroizing::new("first-synthetic-value".to_string()))
        .expect("first bind");
    context
        .prompt(slot.clone())
        .expect("replacement prompt")
        .bind(Zeroizing::new("replacement-synthetic-value".to_string()))
        .expect("replacement bind");

    let sibling_domain = SecurityDomainId::generate(&tokens).expect("sibling domain");
    let (other_session, _other_guard) =
        SecretSession::create(&tokens, domain.clone()).expect("other session");
    let other_context = other_session.context();
    let host = "session.example";
    let address = SocketAddr::from(([127, 0, 0, 1], 9));
    let http = TestSecretsHttpHost::new(host, address, BTreeMap::new());

    for denied_policy in [
        policy(Some(&context), &domain, host, address.port()),
        policy(Some(&context), &sibling_domain, host, address.port()),
        policy(Some(&other_context), &domain, host, address.port()),
        policy(None, &domain, host, address.port()),
    ] {
        let error = prepared_session_request(
            &http.executor(),
            stale.clone(),
            denied_policy,
            host,
            address.port(),
        )
        .expect_err("stale/cross-domain/cross-session use must fail");
        assert_eq!(error.kind(), "SecretReferenceError");
    }
    assert_eq!(session.test_resolution_count(), 0);
    assert_eq!(other_session.test_resolution_count(), 0);
    assert_eq!(http.credential_resolution_count(), 0);
}

#[test]
fn session_contract_enforces_bounds_capacity_and_absence_without_mutation() {
    let (_tokens, _domain, session, _guard) = fixture();
    let context = session.context();
    let first = name("slot.0");
    for invalid in [String::new(), "x".repeat(2049)] {
        let error = context
            .prompt(first.clone())
            .expect("prebound prompt")
            .bind(Zeroizing::new(invalid))
            .expect_err("invalid value length must fail");
        assert_eq!(error.kind(), SecretSessionErrorKind::InvalidValue);
        assert!(!context.present(&first).expect("presence query"));
    }

    for index in 0..32 {
        let slot = name(&format!("slot.{index}"));
        context
            .prompt(slot)
            .expect("prebound prompt")
            .bind(Zeroizing::new(format!("synthetic-{index}")))
            .expect("bounded slot bind");
    }
    assert_eq!(session.test_slot_count(), 32);
    let overflow = context
        .prompt(name("slot.overflow"))
        .expect("prebound overflow prompt")
        .bind(Zeroizing::new("synthetic-overflow".to_string()))
        .expect_err("33rd live slot must fail");
    assert_eq!(overflow.kind(), SecretSessionErrorKind::Capacity);
    let missing = context
        .reference(&name("slot.missing"))
        .expect_err("absent slot must not produce a ref");
    assert_eq!(missing.kind(), SecretSessionErrorKind::Missing);
    assert_eq!(session.test_slot_count(), 32);
}

#[test]
fn session_contract_close_is_idempotent_and_invalidates_worker_clones() {
    let (_tokens, domain, session, guard) = fixture();
    let context = session.context();
    let slot = name("provider.openai");
    let reference = context
        .prompt(slot)
        .expect("prebound prompt")
        .bind(Zeroizing::new("synthetic-session-secret".to_string()))
        .expect("valid bind");
    let worker_ref = reference.clone();
    guard.close();
    guard.close();
    assert_eq!(session.test_slot_count(), 0);

    let host = "session.example";
    let address = SocketAddr::from(([127, 0, 0, 1], 9));
    let http = TestSecretsHttpHost::new(host, address, BTreeMap::new());
    let error = prepared_session_request(
        &http.executor(),
        worker_ref,
        policy(Some(&context), &domain, host, address.port()),
        host,
        address.port(),
    )
    .expect_err("closed session must invalidate outstanding refs");
    assert_eq!(error.kind(), "SecretReferenceError");
    assert_eq!(session.test_resolution_count(), 0);
    assert_eq!(http.credential_resolution_count(), 0);
}
