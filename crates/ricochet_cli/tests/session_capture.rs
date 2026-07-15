use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use ricochet_cli::gui_test_host::{CallbackGuiSnapshot, CallbackGuiTestHost};
use ricochet_cli::secure_prompt::{
    NativePromptControl, NativePromptDispatcher, NativePromptOutcome, NativePromptRequest,
};
use ricochet_compiler::compile_source;
use ricochet_sandbox::DestinationGrant;
use ricochet_secrets::test_host::TestSecretsHttpHost;
use zeroize::Zeroizing;

const TEST_HOST: &str = "phase0.test";

#[derive(Default)]
struct SyntheticNativePrompt {
    value: Mutex<Option<Zeroizing<String>>>,
    requests: Mutex<Vec<(String, String)>>,
}

impl SyntheticNativePrompt {
    fn with_value(value: String) -> Self {
        Self {
            value: Mutex::new(Some(Zeroizing::new(value))),
            requests: Mutex::new(Vec::new()),
        }
    }

    fn requests(&self) -> Vec<(String, String)> {
        self.requests.lock().expect("prompt request lock").clone()
    }
}

impl NativePromptControl for SyntheticNativePrompt {
    fn prompt(
        &self,
        request: &NativePromptRequest,
    ) -> Result<NativePromptOutcome, ricochet_cli::secure_prompt::NativePromptError> {
        self.requests.lock().expect("prompt request lock").push((
            request.label().as_str().to_string(),
            request.canonical_path().to_string(),
        ));
        Ok(NativePromptOutcome::Stored(
            self.value
                .lock()
                .expect("native prompt value lock")
                .take()
                .expect("native prompt is one-use"),
        ))
    }
}

struct LocalTlsCapture {
    address: SocketAddr,
    requests: Arc<Mutex<Vec<String>>>,
    shutdown: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}

impl LocalTlsCapture {
    fn new() -> Self {
        let certified = rcgen::generate_simple_self_signed(vec![TEST_HOST.to_string()])
            .expect("test TLS certificate");
        let certificate = certified.cert.der().clone();
        let private_key =
            rustls::pki_types::PrivatePkcs8KeyDer::from(certified.key_pair.serialize_der());
        let tls = Arc::new(
            rustls::ServerConfig::builder()
                .with_no_client_auth()
                .with_single_cert(vec![certificate], private_key.into())
                .expect("test TLS server config"),
        );
        let listener = TcpListener::bind("127.0.0.1:0").expect("local TLS listener");
        listener
            .set_nonblocking(true)
            .expect("local TLS listener nonblocking mode");
        let address = listener.local_addr().expect("local TLS address");
        let requests = Arc::new(Mutex::new(Vec::new()));
        let captured = Arc::clone(&requests);
        let shutdown = Arc::new(AtomicBool::new(false));
        let stop = Arc::clone(&shutdown);
        let worker = thread::spawn(move || {
            while !stop.load(Ordering::Acquire) {
                let (stream, _) = match listener.accept() {
                    Ok(accepted) => accepted,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                        continue;
                    }
                    Err(_) => break,
                };
                stream
                    .set_nonblocking(false)
                    .expect("accepted TLS stream blocking mode");
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .expect("accepted TLS read timeout");
                stream
                    .set_write_timeout(Some(Duration::from_secs(2)))
                    .expect("accepted TLS write timeout");
                let connection = rustls::ServerConnection::new(Arc::clone(&tls))
                    .expect("test TLS server connection");
                let mut stream = rustls::StreamOwned::new(connection, stream);
                let mut request = Vec::new();
                let mut buffer = [0_u8; 1024];
                while !request.windows(4).any(|window| window == b"\r\n\r\n") {
                    match stream.read(&mut buffer) {
                        Ok(0) => break,
                        Ok(count) => request.extend_from_slice(&buffer[..count]),
                        Err(_) => break,
                    }
                    assert!(request.len() <= 64 * 1024, "bounded test request");
                }
                captured
                    .lock()
                    .expect("captured TLS request lock")
                    .push(String::from_utf8_lossy(&request).into_owned());
                stream
                    .write_all(
                        b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok",
                    )
                    .expect("test TLS response");
                stream.flush().expect("test TLS response flush");
            }
        });
        Self {
            address,
            requests,
            shutdown,
            worker: Some(worker),
        }
    }

    fn address(&self) -> SocketAddr {
        self.address
    }

    fn wait_for_requests(&self, expected: usize) -> Vec<String> {
        for _ in 0..200 {
            let requests = self.requests.lock().expect("TLS request lock").clone();
            if requests.len() >= expected {
                return requests;
            }
            thread::sleep(Duration::from_millis(10));
        }
        self.requests.lock().expect("TLS request lock").clone()
    }
}

impl Drop for LocalTlsCapture {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            worker.join().expect("local TLS worker shutdown");
        }
    }
}

fn unique_sentinel(variant: &str) -> String {
    let mut random = [0_u8; 16];
    getrandom::fill(&mut random).expect("sentinel entropy");
    format!(
        "phase0-{variant}-{}",
        random
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    )
}

fn source(port: u16, dispatch: &str) -> String {
    format!(
        r#"
( state -> Map ) render_capture function
  state var
  actions array
  actions get "Store session key" "provider.openai" "OpenAI session key" "after_secret" webview_secure_session_action push drop
  "Session capture" "Native prompt only" state get actions get webview_window_state value
end

( state event -> Map ) after_secret function
  event var
  state var
  state get "callback" event get put drop
  "provider.openai" secret_session_get value credential var
  "GET" "https://{TEST_HOST}:{port}/capture" http_request_new value request var
  request get credential get http_bearer_auth value request set
  {dispatch}
  state get "status" response get "status" at put drop
  state get render_capture
end

state map
state get render_capture document var
"#
    )
}

fn assert_secret_absent(snapshot: &CallbackGuiSnapshot, sentinel: &str, surface: &str) {
    let state = snapshot.state.to_string();
    for (name, value) in [
        ("DOM", snapshot.dom.as_str()),
        ("state", state.as_str()),
        ("stdout", snapshot.stdout.as_str()),
        ("stderr", snapshot.stderr.as_str()),
        ("Debug", snapshot.debug.as_str()),
        ("image", snapshot.image.as_str()),
        ("DAP", snapshot.dap.as_str()),
    ] {
        assert!(
            !value.contains(sentinel),
            "{surface} {name} surface leaked the sentinel"
        );
    }
}

fn assert_exact_authorization(request: &str, sentinel: &str) {
    let authorization = request
        .lines()
        .filter_map(|line| line.split_once(':'))
        .filter(|(name, _)| name.eq_ignore_ascii_case("authorization"))
        .map(|(_, value)| value.trim())
        .collect::<Vec<_>>();
    assert_eq!(authorization.len(), 1, "exactly one Authorization header");
    let expected = format!("Bearer {sentinel}");
    assert!(
        authorization[0] == expected,
        "captured bearer value did not match the native prompt value"
    );
}

#[test]
fn callback_gui_session_capture_is_opaque_one_use_and_succeeds_for_sync_spawn_and_stream() {
    let variants = [
        ("sync", "request get http_request value response var"),
        (
            "spawn",
            r#"[ request get http_request ] spawn requestTask var
  requestTask get await value response var
  requestTask get release_task drop"#,
        ),
        (
            "stream",
            r#"request get http_stream_start value response var
  response get "id" at streamId var
  streamId get http_stream value streamState var
  streamState get "running" at while
    5 sleep
    streamId get http_stream value streamState set
  end
  response get "status" streamState get "status_code" at put drop
  streamId get http_stream_release value drop"#,
        ),
    ];

    for (variant, dispatch) in variants {
        let sentinel = unique_sentinel(variant);
        let capture = LocalTlsCapture::new();
        let address = capture.address();
        let http = TestSecretsHttpHost::new(TEST_HOST, address, Default::default());
        let destination =
            DestinationGrant::new(TEST_HOST, address.port()).expect("exact local test destination");
        let chunk = compile_source(
            &format!("session-capture-{variant}.rco"),
            &source(address.port(), dispatch),
        )
        .expect("session capture source");
        let mut gui = CallbackGuiTestHost::new(&chunk, http.executor(), destination)
            .expect("callback GUI test host");

        let initial = gui.snapshot().expect("initial callback GUI snapshot");
        assert_eq!(initial.secure_action_ids.len(), 1);
        let action_id = initial.secure_action_ids[0].clone();
        assert_eq!(action_id.len(), 64);
        assert!(action_id.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert!(initial.dom.contains(&action_id));
        assert!(initial.dom.contains("Store session key"));
        for forbidden in [
            "provider.openai",
            "OpenAI session key",
            "after_secret",
            "prompt_label",
            sentinel.as_str(),
        ] {
            assert!(
                !initial.dom.contains(forbidden),
                "{variant} initial DOM exposed host binding {forbidden:?}"
            );
        }
        assert_secret_absent(&initial, &sentinel, variant);
        assert_eq!(gui.session_resolution_count(), 0);
        assert_eq!(http.credential_resolution_count(), 0);
        assert!(capture.wait_for_requests(0).is_empty());

        let control = Arc::new(SyntheticNativePrompt::with_value(sentinel.clone()));
        let dispatcher = NativePromptDispatcher::from_control(control.clone());
        let updated = gui
            .dispatch_secure_action(&action_id, &dispatcher)
            .expect("opaque secure action dispatch");

        assert_eq!(
            control.requests(),
            vec![(
                "OpenAI session key".to_string(),
                "Unverified ephemeral session\ncallback-gui/provider.openai".to_string(),
            )]
        );
        assert_eq!(
            updated.state,
            serde_json::json!({"callback": "stored", "status": 200})
        );
        assert_secret_absent(&updated, &sentinel, variant);
        assert_eq!(gui.session_resolution_count(), 1);
        assert_eq!(http.credential_resolution_count(), 1);
        assert_eq!(http.environment_source_access_count(), 0);
        let requests = capture.wait_for_requests(1);
        assert_eq!(requests.len(), 1, "{variant} request count");
        assert_exact_authorization(&requests[0], &sentinel);

        let replay = gui
            .dispatch_secure_action(&action_id, &dispatcher)
            .expect_err("consumed opaque action ID must not replay")
            .to_string();
        assert!(replay.contains("secure session action is unavailable"));
        assert!(!replay.contains(&sentinel));
        assert_eq!(control.requests().len(), 1);
        assert_eq!(gui.session_resolution_count(), 1);
        assert_eq!(http.credential_resolution_count(), 1);
        thread::sleep(Duration::from_millis(50));
        assert_eq!(capture.wait_for_requests(1).len(), 1);
    }
}
