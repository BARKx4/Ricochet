use ricochet_cli::secure_action::{SecretActionErrorKind, SecretActionRegistry};
use ricochet_secrets::HostTokenSource;

#[test]
fn secure_session_action_is_generation_bound_and_one_use_in_public_host_api() {
    let tokens = HostTokenSource::system();
    let registry = SecretActionRegistry::new(tokens);
    let id = registry
        .issue(7, "binding")
        .expect("first action should issue");

    assert_eq!(format!("{id:?}"), "<secret-action-id>");
    assert_eq!(id.as_str().len(), 64);
    let wrong = registry
        .take(&id, 8)
        .expect_err("wrong generation must fail without consuming");
    assert_eq!(wrong.kind(), SecretActionErrorKind::WrongGeneration);
    assert_eq!(
        registry.take(&id, 7).expect("correct generation"),
        "binding"
    );
    assert_eq!(
        registry.take(&id, 7).expect_err("replay must fail").kind(),
        SecretActionErrorKind::Missing
    );
}

#[test]
fn secure_session_action_caps_each_document_and_invalidates_navigation_generation() {
    let tokens = HostTokenSource::system();
    let registry = SecretActionRegistry::new(tokens);
    let mut ids = Vec::new();
    for index in 0..32 {
        ids.push(registry.issue(11, index).expect("bounded document action"));
    }
    assert_eq!(
        registry
            .issue(11, 33)
            .expect_err("33rd live action must fail")
            .kind(),
        SecretActionErrorKind::Capacity
    );
    registry.invalidate_generation(11);
    for id in ids {
        assert_eq!(
            registry
                .take(&id, 11)
                .expect_err("navigation invalidates")
                .kind(),
            SecretActionErrorKind::Missing
        );
    }
}
