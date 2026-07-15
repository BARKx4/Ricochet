use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use ricochet_application::{HostDisplayLabel, SecretName};
use ricochet_cli::secure_prompt::{
    HostPromptCoordinator, NativePromptControl, NativePromptDispatcher, NativePromptOutcome,
    NativePromptRequest,
};
use ricochet_compiler::compile_source;
use ricochet_sandbox::DestinationGrant;
use ricochet_secrets::test_host::{TestEnvironmentValue, TestSecretsHttpHost};
use ricochet_secrets::{
    DeferredHttpCredentials, DeferredSecretSource, EnvironmentCredentialPolicy, HostTokenSource,
    SecretHttpPolicySnapshot, SecretSession, SecurityDomainId,
};
use ricochet_vm::{ArrayValue, ListValue, MapValue, RicochetResult, SetValue, Value, Vm};
use zeroize::Zeroizing;

const AUDIT_PIPE_HANDLE: &str = "RICOCHET_DEFERRED_HTTP_AUDIT_PIPE_HANDLE";

fn unique_sentinel(prefix: &str) -> String {
    let mut random = [0_u8; 16];
    getrandom::fill(&mut random).expect("test sentinel entropy");
    format!(
        "{prefix}-{}",
        random
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    )
}

fn deferred_literal_request(sentinel: &str) -> Value {
    let source = format!(
        r#""POST" "https://api.openai.com/v1/responses" http_request_new value "{sentinel}" secret_literal http_bearer_auth value"#
    );
    let chunk = match compile_source("deferred-http-secret-no-leak.rco", &source) {
        Ok(chunk) => chunk,
        Err(_) => panic!("construction fixture failed to compile"),
    };
    let mut vm = Vm::default();
    if vm.run_chunk(&chunk).is_err() {
        panic!("construction fixture failed to execute");
    }
    assert_eq!(vm.stack().len(), 1);
    vm.stack()[0].clone()
}

fn run_word(values: impl IntoIterator<Item = Value>, word: &str) -> Result<Vm, String> {
    let chunk = compile_source("deferred-http-secret-operation.rco", word)
        .expect("operation fixture should compile");
    let mut vm = Vm::default();
    for value in values {
        vm.push_value(value);
    }
    vm.run_chunk(&chunk).map_err(|error| error.to_string())?;
    Ok(vm)
}

fn assert_sanitized_rejection(result: Result<Vm, String>, sentinel: &str, surface: &str) {
    let error = match result {
        Ok(_) => panic!("{surface}"),
        Err(error) => error,
    };
    assert!(
        error.contains("type error") || error.contains("cannot encode"),
        "{surface} should return a sanitized type/nonserializable error"
    );
    assert!(!error.contains(sentinel), "{surface} leaked the sentinel");
}

fn nested(value: Value) -> Value {
    Value::Map(MapValue::from(BTreeMap::from([(
        "request".to_string(),
        Value::Array(ArrayValue::from(vec![Value::List(ListValue::from(vec![
            value,
        ]))])),
    )])))
}

fn session_fixture() -> (
    SecurityDomainId,
    SecretSession,
    ricochet_secrets::SecretSessionGuard,
) {
    let tokens = HostTokenSource::system();
    let domain = SecurityDomainId::generate(&tokens).expect("security domain fixture");
    let (session, guard) =
        SecretSession::create(&tokens, domain.clone()).expect("secret session fixture");
    (domain, session, guard)
}

fn policy(
    host: &str,
    port: u16,
    environment: EnvironmentCredentialPolicy,
) -> SecretHttpPolicySnapshot {
    SecretHttpPolicySnapshot::new(
        true,
        Some(BTreeSet::from([host.to_string()])),
        BTreeSet::from([DestinationGrant::new(host, port).expect("exact destination fixture")]),
        environment,
    )
}

fn prepare_for_test_host(
    executor: &ricochet_secrets::SecretsHttpExecutor,
    credentials: DeferredHttpCredentials,
    policy: SecretHttpPolicySnapshot,
    host: &str,
    port: u16,
) -> ricochet_secrets::PreparedSecretHttpRequest {
    executor
        .prepare(
            credentials,
            reqwest::Method::POST,
            format!("https://{host}:{port}/v1/audit"),
            reqwest::header::HeaderMap::new(),
            None,
            None,
            Duration::from_millis(100),
            1024,
            None,
            None,
            policy,
        )
        .expect("authorized fixture should prepare before source access")
}

#[derive(Default)]
struct SyntheticNativeControl {
    value: Mutex<Option<Zeroizing<String>>>,
    prompt_count: Mutex<usize>,
}

impl SyntheticNativeControl {
    fn with_value(value: String) -> Self {
        Self {
            value: Mutex::new(Some(Zeroizing::new(value))),
            prompt_count: Mutex::new(0),
        }
    }
}

impl NativePromptControl for SyntheticNativeControl {
    fn prompt(
        &self,
        _request: &NativePromptRequest,
    ) -> Result<NativePromptOutcome, ricochet_cli::secure_prompt::NativePromptError> {
        *self.prompt_count.lock().expect("prompt count lock") += 1;
        let value = self
            .value
            .lock()
            .expect("native value lock")
            .take()
            .expect("one-use native value");
        Ok(NativePromptOutcome::Stored(value))
    }
}

#[test]
fn deep_container_operations_and_core_artifacts_reject_deferred_credentials() {
    let sentinel = unique_sentinel("ricochet-literal-sentinel");
    let request = deferred_literal_request(&sentinel);
    let nested = nested(request.clone());

    let rendered = format!("{nested:?}");
    assert!(rendered.contains("<http-credentials>"));
    assert!(!rendered.contains(&sentinel));
    assert!(nested.contains_deferred_http_credential());
    assert_eq!(rendered.matches("<http-credentials>").count(), 1);

    let set_child = ArrayValue::from(vec![Value::Number(1)]);
    let set =
        SetValue::try_from(vec![Value::Array(set_child.clone())]).expect("ordinary set fixture");
    set_child.push(nested.clone());
    let equality_containers = [
        Value::Array(ArrayValue::from(vec![nested.clone()])),
        Value::List(ListValue::from(vec![nested.clone()])),
        Value::Map(MapValue::from(BTreeMap::from([(
            "value".to_string(),
            nested.clone(),
        )]))),
        Value::Set(set),
        Value::Result(RicochetResult::Ok(Box::new(nested.clone()))),
    ];
    for container in equality_containers {
        for word in ["=", "not_equals?", "assert_equals"] {
            assert_sanitized_rejection(
                run_word([container.clone(), Value::Nil], word),
                &sentinel,
                word,
            );
        }
    }

    for word in ["has?", "contains?"] {
        assert_sanitized_rejection(
            run_word(
                [
                    Value::Array(ArrayValue::from(vec![nested.clone()])),
                    Value::String("absent".to_string()),
                ],
                word,
            ),
            &sentinel,
            &format!("stored-container {word}"),
        );
        assert_sanitized_rejection(
            run_word(
                [
                    Value::Array(ArrayValue::from(vec![Value::Number(1)])),
                    nested.clone(),
                ],
                word,
            ),
            &sentinel,
            &format!("candidate {word}"),
        );
    }

    for collection in [
        Value::Array(ArrayValue::from(vec![Value::Number(1)])),
        Value::List(ListValue::from(vec![Value::Number(1)])),
        Value::Set(SetValue::try_from(vec![Value::Number(1)]).expect("ordinary set fixture")),
    ] {
        assert_sanitized_rejection(
            run_word([collection, nested.clone()], "remove"),
            &sentinel,
            "collection removal candidate",
        );
    }

    for collection in [
        Value::Array(ArrayValue::from(vec![nested.clone()])),
        Value::List(ListValue::from(vec![nested.clone()])),
    ] {
        assert_sanitized_rejection(
            run_word([collection, Value::Nil], "remove"),
            &sentinel,
            "collection removal stored value",
        );
    }

    assert_sanitized_rejection(
        run_word([Value::Set(SetValue::default()), nested.clone()], "push"),
        &sentinel,
        "set insertion",
    );
    let stored_set_child = ArrayValue::from(vec![Value::Number(7)]);
    let stored_set = SetValue::try_from(vec![Value::Array(stored_set_child.clone())])
        .expect("ordinary stored set fixture");
    stored_set_child.push(nested.clone());
    for word in ["remove", "push"] {
        assert_sanitized_rejection(
            run_word([Value::Set(stored_set.clone()), Value::Nil], word),
            &sentinel,
            &format!("stored set {word}"),
        );
    }

    let cyclic = MapValue::default();
    cyclic.insert("request".to_string(), request.clone());
    cyclic.insert("self".to_string(), Value::Map(cyclic.clone()));
    assert_sanitized_rejection(
        run_word(
            [
                Value::Array(ArrayValue::from(vec![Value::Map(cyclic)])),
                Value::Nil,
            ],
            "contains?",
        ),
        &sentinel,
        "cycle-safe stored-container membership",
    );

    assert_sanitized_rejection(
        run_word([nested.clone()], "json_encode"),
        &sentinel,
        "core JSON",
    );
    let mut image_vm = Vm::default();
    image_vm.push_value(nested);
    let image_error = image_vm
        .to_image()
        .expect_err("VM image must reject nested deferred credentials")
        .to_string();
    assert!(image_error.contains("cannot serialize"));
    assert!(!image_error.contains(&sentinel));

    let Value::Map(request) = request else {
        panic!("request fixture must be a map");
    };
    if let Some(headers) = request.get("headers") {
        let rendered_headers = format!("{headers:?}");
        assert!(!rendered_headers
            .to_ascii_lowercase()
            .contains("authorization"));
        assert!(!rendered_headers.contains(&sentinel));
    }
}

#[test]
fn injected_environment_source_resolves_once_and_emits_only_sanitized_failure() {
    let sentinel = unique_sentinel("ricochet-environment-sentinel");
    let source_name = SecretName::parse("audit.environment").expect("source name fixture");
    let host = "audit-env.example";
    let address = SocketAddr::from(([127, 0, 0, 1], 49141));
    let http = TestSecretsHttpHost::new(
        host,
        address,
        BTreeMap::from([(
            "audit.environment".to_string(),
            TestEnvironmentValue::unicode(sentinel.clone()),
        )]),
    );
    let prepared = prepare_for_test_host(
        &http.executor(),
        DeferredHttpCredentials::bearer(DeferredSecretSource::environment(source_name)),
        policy(
            host,
            address.port(),
            EnvironmentCredentialPolicy::new(
                true,
                Some(BTreeSet::from(["audit.environment".to_string()])),
            ),
        ),
        host,
        address.port(),
    );
    assert_eq!(http.environment_source_access_count(), 0);
    let error = http
        .executor()
        .execute(prepared)
        .expect_err("closed TLS fixture should fail after the authorized source boundary")
        .to_string();
    assert_eq!(http.environment_source_access_count(), 1);
    assert_eq!(http.credential_resolution_count(), 1);
    assert!(!error.contains(&sentinel));
}

#[test]
fn session_and_injected_native_prompt_values_remain_opaque() {
    let sentinel = unique_sentinel("ricochet-session-sentinel");
    let (domain, session, _guard) = session_fixture();
    let context = session.context();
    let control = Arc::new(SyntheticNativeControl::with_value(sentinel.clone()));
    let dispatcher = NativePromptDispatcher::from_control(control.clone());
    let result = HostPromptCoordinator::new()
        .prompt(
            &dispatcher,
            NativePromptRequest::new(
                1,
                HostDisplayLabel::parse("Audit session credential").expect("prompt label"),
                "callback-gui/audit.session",
            ),
        )
        .expect("injected native prompt");
    let outcome = result.into_outcome();
    let NativePromptOutcome::Stored(value) = outcome else {
        panic!("injected native prompt must return a stored value");
    };
    let slot = SecretName::parse("audit.session").expect("session slot fixture");
    let reference = context
        .prompt(slot)
        .expect("prebound session prompt")
        .bind(value)
        .expect("native value should bind directly into the session");
    assert_eq!(*control.prompt_count.lock().expect("prompt count lock"), 1);

    let value = nested(Value::DeferredHttpCredentials(
        DeferredHttpCredentials::bearer(DeferredSecretSource::opaque(reference)),
    ));
    assert_sanitized_rejection(
        run_word([value.clone(), Value::Nil], "="),
        &sentinel,
        "session-backed equality",
    );
    let rendered = format!("{value:?}");
    assert!(rendered.contains("<http-credentials>"));
    assert!(!rendered.contains(&sentinel));

    let host = "audit-session.example";
    let address = SocketAddr::from(([127, 0, 0, 1], 49142));
    let http = TestSecretsHttpHost::new(host, address, BTreeMap::new());
    let Value::Map(map) = value else {
        panic!("nested fixture must be a map");
    };
    let Value::Array(array) = map.get("request").expect("nested request") else {
        panic!("nested request must be an array");
    };
    let Value::List(list) = array.get(0).expect("nested list") else {
        panic!("nested request must contain a list");
    };
    let Value::DeferredHttpCredentials(credentials) = list.get(0).expect("opaque credentials")
    else {
        panic!("nested request must contain deferred credentials");
    };
    let prepared = prepare_for_test_host(
        &http.executor(),
        credentials,
        policy(
            host,
            address.port(),
            EnvironmentCredentialPolicy::new(false, None),
        )
        .with_secret_session(context, domain),
        host,
        address.port(),
    );
    assert_eq!(session.test_resolution_count(), 0);
    let error = http
        .executor()
        .execute(prepared)
        .expect_err("closed TLS fixture should fail after session resolution")
        .to_string();
    assert_eq!(session.test_resolution_count(), 1);
    assert_eq!(http.credential_resolution_count(), 1);
    assert!(!error.contains(&sentinel));
}

#[test]
#[ignore = "invoked only by the retained evidence scanner with an inherited anonymous pipe"]
fn scanner_child_writes_secret_only_to_inherited_anonymous_pipe() {
    let sentinel = unique_sentinel("ricochet-scanner-child-sentinel");
    let request = deferred_literal_request(&sentinel);
    let rendered = format!("{:?}", nested(request));
    assert!(
        !rendered.contains(&sentinel),
        "debug surface leaked audited bytes"
    );

    let handle = std::env::var(AUDIT_PIPE_HANDLE).expect("audit pipe handle is required");
    let bytes = sentinel.as_bytes();
    let length = u32::try_from(bytes.len()).expect("bounded sentinel length");

    #[cfg(windows)]
    let mut pipe = {
        use std::os::windows::io::FromRawHandle;
        let raw = handle.parse::<isize>().expect("decimal audit pipe handle");
        // SAFETY: The PowerShell scanner transfers sole client-handle ownership to this
        // test child. The handle is used once and closed when `pipe` drops.
        unsafe { std::fs::File::from_raw_handle(raw as *mut std::ffi::c_void) }
    };
    #[cfg(unix)]
    let mut pipe = {
        use std::os::fd::FromRawFd;
        let raw = handle
            .parse::<i32>()
            .expect("decimal audit pipe descriptor");
        // SAFETY: The scanner transfers sole client-descriptor ownership to this child.
        unsafe { std::fs::File::from_raw_fd(raw) }
    };
    #[cfg(not(any(windows, unix)))]
    compile_error!("anonymous-pipe security audit is supported only on Windows and Unix");

    pipe.write_all(&length.to_le_bytes())
        .expect("write audit length prefix");
    pipe.write_all(bytes).expect("write audit bytes");
    pipe.flush().expect("flush audit pipe");
    println!("audit-boundaries=literal:1;ordinary-output-secret-bytes:0");
}
