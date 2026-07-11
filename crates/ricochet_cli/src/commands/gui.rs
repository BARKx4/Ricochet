use crate::commands::package::{extract_embedded_mvc_bundle, MvcBundle};
use crate::*;

#[derive(Debug, Clone, PartialEq)]
struct WebviewDocument {
    title: String,
    body: String,
    html: String,
    width: u32,
    height: u32,
    state: Value,
    actions: Vec<WebviewAction>,
    menus: WebviewMenuBar,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct WebviewAction {
    action: String,
    callback: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct WebviewMenuBar {
    menus: Vec<WebviewMenu>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct WebviewMenu {
    label: String,
    items: Vec<WebviewMenuItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum WebviewMenuItem {
    Command {
        label: String,
        action: String,
        shortcut: Option<String>,
    },
    Separator,
}

impl Default for WebviewMenuBar {
    fn default() -> Self {
        Self {
            menus: vec![
                WebviewMenu {
                    label: "File".to_string(),
                    items: vec![WebviewMenuItem::Command {
                        label: "Quit".to_string(),
                        action: RICOCHET_QUIT_ACTION.to_string(),
                        shortcut: Some("Ctrl+Q".to_string()),
                    }],
                },
                WebviewMenu {
                    label: "Edit".to_string(),
                    items: vec![
                        WebviewMenuItem::Command {
                            label: "Copy".to_string(),
                            action: RICOCHET_COPY_ACTION.to_string(),
                            shortcut: Some("Ctrl+C".to_string()),
                        },
                        WebviewMenuItem::Command {
                            label: "Paste".to_string(),
                            action: RICOCHET_PASTE_ACTION.to_string(),
                            shortcut: Some("Ctrl+V".to_string()),
                        },
                    ],
                },
            ],
        }
    }
}

pub(crate) fn run_gui_file(
    path: &str,
    args: Vec<String>,
    capabilities: CapabilityOptions,
) -> Result<()> {
    let source_path = Path::new(path);
    let chunk = compile_source_file(source_path)?;
    run_gui_chunk(
        &chunk,
        args,
        capabilities,
        dynamic_import_parent_for_source(source_path)?,
    )
}

pub(crate) fn run_embedded_gui_app(chunk: &Chunk, args: Vec<String>) -> Result<()> {
    run_gui_chunk(
        chunk,
        args,
        CapabilityOptions::default(),
        current_dir_for_dynamic_imports()?,
    )
}

pub(crate) async fn run_embedded_mvc_gui_app(bundle: MvcBundle, _args: Vec<String>) -> Result<()> {
    let project_root = extract_embedded_mvc_bundle(&bundle)?;
    std::env::set_current_dir(&project_root).with_context(|| {
        format!(
            "failed to use embedded MVC project directory {}",
            project_root.display()
        )
    })?;

    let serve_options = embedded_mvc_serve_options(&project_root)?;
    let app = ricochet_web::build_served_app_from_dir(&project_root, false, false, &serve_options)
        .await?;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .context("failed to bind local MVC GUI server")?;
    let address = listener
        .local_addr()
        .context("failed to read local MVC GUI server address")?;
    let server = tokio::spawn(async move {
        if let Err(error) = ricochet_web::serve_app_on_listener(listener, app).await {
            eprintln!("Ricochet MVC GUI server stopped: {error:?}");
        }
    });
    let url = format!("http://{address}/");

    if let Ok(path) = std::env::var(GUI_EXPORT_HTML_ENV) {
        let export_request_path =
            std::env::var(GUI_EXPORT_PATH_ENV).unwrap_or_else(|_| "/".to_string());
        if !export_request_path.starts_with('/') {
            bail!("{GUI_EXPORT_PATH_ENV} must start with /");
        }
        let html = fetch_http_body(address, &export_request_path).await?;
        fs::write(&path, html).with_context(|| {
            format!("failed to write GUI HTML export requested by {GUI_EXPORT_HTML_ENV}={path}")
        })?;
        server.abort();
        return Ok(());
    }

    let result = open_native_webview_url(
        DEFAULT_MVC_GUI_TITLE,
        &url,
        DEFAULT_MVC_GUI_WIDTH,
        DEFAULT_MVC_GUI_HEIGHT,
    );
    server.abort();
    result
}

fn embedded_mvc_serve_options(project_root: &Path) -> Result<ricochet_web::ServeOptions> {
    let manifest = load_embedded_mvc_manifest(project_root)?;
    let capabilities = &manifest.web.capabilities;
    let process_root_requested =
        capabilities.allow_process || capabilities.allow_pty || capabilities.process_root.is_some();

    Ok(ricochet_web::ServeOptions {
        fs_root: Some(project_root.to_path_buf()),
        allow_env: capabilities.allow_env,
        env_allow: if capabilities.allow_env {
            Vec::new()
        } else {
            capabilities.env_allow.clone()
        },
        allow_process: capabilities.allow_process,
        process_root: process_root_requested.then(|| project_root.to_path_buf()),
        allow_pty: capabilities.allow_pty,
        http_allow_hosts: capabilities.http_allow_hosts.clone(),
        ..Default::default()
    })
}

fn load_embedded_mvc_manifest(project_root: &Path) -> Result<ricochet_web::Manifest> {
    let manifest_path = project_root.join("ricochet.toml");
    let manifest_source = fs::read_to_string(&manifest_path).with_context(|| {
        format!(
            "failed to read embedded MVC manifest {}",
            manifest_path.display()
        )
    })?;
    toml::from_str(&manifest_source).with_context(|| {
        format!(
            "failed to parse embedded MVC manifest {}",
            manifest_path.display()
        )
    })
}

async fn fetch_http_body(address: SocketAddr, path: &str) -> Result<String> {
    let mut last_error = None;
    for _ in 0..50 {
        match try_fetch_http_body(address, path).await {
            Ok(body) => return Ok(body),
            Err(error) => {
                last_error = Some(error);
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        }
    }

    Err(last_error
        .unwrap_or_else(|| anyhow::anyhow!("local MVC GUI server did not respond"))
        .context(format!("failed to fetch http://{address}{path}")))
}

async fn try_fetch_http_body(address: SocketAddr, path: &str) -> Result<String> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let mut stream = tokio::net::TcpStream::connect(address)
        .await
        .context("failed to connect to local MVC GUI server")?;
    let request = format!("GET {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n");
    stream
        .write_all(request.as_bytes())
        .await
        .context("failed to send MVC GUI export request")?;

    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .await
        .context("failed to read MVC GUI export response")?;
    let response = String::from_utf8_lossy(&response);
    let (headers, body) = response
        .split_once("\r\n\r\n")
        .context("MVC GUI export response was not valid HTTP")?;
    let status_line = headers.lines().next().unwrap_or_default();
    if !status_line.contains(" 200 ") {
        bail!("MVC GUI export request returned {status_line}");
    }
    Ok(body.to_string())
}

fn run_gui_chunk(
    chunk: &Chunk,
    args: Vec<String>,
    capabilities: CapabilityOptions,
    dynamic_import_parent: PathBuf,
) -> Result<()> {
    let mut session = WebviewSession::new(chunk, args, capabilities, dynamic_import_parent)?;
    session.dispatch_env_event_if_requested()?;
    if let Ok(path) = std::env::var(GUI_EXPORT_HTML_ENV) {
        fs::write(&path, &session.document.html).with_context(|| {
            format!("failed to write GUI HTML export requested by {GUI_EXPORT_HTML_ENV}={path}")
        })?;
        return Ok(());
    }
    open_native_webview(session)
}

struct WebviewSession {
    vm: Vm,
    document: WebviewDocument,
}

impl WebviewSession {
    fn new(
        chunk: &Chunk,
        args: Vec<String>,
        capabilities: CapabilityOptions,
        dynamic_import_parent: PathBuf,
    ) -> Result<Self> {
        let mut vm = cli_vm(args, &capabilities)?;
        install_dynamic_module_loader(&mut vm, dynamic_import_parent);
        let result = vm.run_chunk(chunk);
        print!("{}", vm.stdout());
        eprint!("{}", vm.stderr());
        if let Err(ricochet_vm::VmError::ExitRequested { code }) = result {
            std::process::exit(code);
        }
        if let Err(error) = result {
            bail!("{}", runtime_error_message(&vm, &error));
        }
        let document = webview_document_from_vm(&vm)?;
        Ok(Self { vm, document })
    }

    fn dispatch_env_event_if_requested(&mut self) -> Result<()> {
        let Ok(event_source) = std::env::var(GUI_EVENT_ENV) else {
            return Ok(());
        };
        let event_json: serde_json::Value = serde_json::from_str(&event_source)
            .with_context(|| format!("{GUI_EVENT_ENV} must be a JSON object"))?;
        self.dispatch_event_json(event_json)?;
        Ok(())
    }

    fn dispatch_event_json(&mut self, event_json: serde_json::Value) -> Result<&WebviewDocument> {
        let action_name = event_json
            .get("action")
            .and_then(|value| value.as_str())
            .context("GUI action event is missing string field `action`")?;
        let action = self
            .document
            .actions
            .iter()
            .find(|action| action.action == action_name)
            .with_context(|| format!("GUI document has no action named {action_name:?}"))?;
        let callback = action.callback.clone();

        self.vm.push_value(self.document.state.clone());
        self.vm.push_value(json_to_ricochet_value(event_json));
        let mut chunk = Chunk::new("<gui-event>");
        chunk.push(Op::CallWord(callback.clone()), gui_event_span());
        let result = self.vm.run_chunk(&chunk);
        print!("{}", self.vm.stdout());
        eprint!("{}", self.vm.stderr());
        if let Err(error) = result {
            bail!("{}", runtime_error_message(&self.vm, &error));
        }
        self.document = webview_document_from_vm(&self.vm).with_context(|| {
            format!(
                "GUI action callback {:?} must return a webview document",
                callback
            )
        })?;
        Ok(&self.document)
    }
}

fn webview_document_from_vm(vm: &Vm) -> Result<WebviewDocument> {
    for value in vm.stack().iter().rev() {
        if let Some(document) = webview_document_from_value(value)? {
            return Ok(document);
        }
    }

    if let Some(value) = vm.variable("document") {
        if let Some(document) = webview_document_from_value(value)? {
            return Ok(document);
        }
    }

    bail!(
        "GUI apps must leave a `webview_window` result on the stack or store it in a variable named `document`"
    )
}

fn webview_document_from_value(value: &Value) -> Result<Option<WebviewDocument>> {
    match value {
        Value::Result(RicochetResult::Ok(inner)) => webview_document_from_value(inner),
        Value::Result(RicochetResult::Err(error)) => {
            bail!(
                "GUI app returned an error result: {}: {}",
                error.kind,
                error.message
            )
        }
        Value::Map(map) => webview_document_from_map(map),
        _ => Ok(None),
    }
}

fn webview_document_from_map(map: &MapValue) -> Result<Option<WebviewDocument>> {
    if map.get("type") != Some(Value::String("webview".to_string())) {
        return Ok(None);
    }

    Ok(Some(WebviewDocument {
        title: required_document_string(map, "title")?,
        body: required_document_string(map, "body")?,
        html: required_document_string(map, "html")?,
        width: required_document_dimension(map, "width")?,
        height: required_document_dimension(map, "height")?,
        state: optional_document_value(map, "state")
            .unwrap_or_else(|| Value::Map(BTreeMap::new().into())),
        actions: optional_document_value(map, "actions")
            .map(|value| webview_actions_from_value(&value))
            .transpose()?
            .unwrap_or_default(),
        menus: optional_document_value(map, "menus")
            .map(|value| webview_menu_bar_from_value(&value))
            .transpose()?
            .unwrap_or_default(),
    }))
}

fn webview_actions_from_value(value: &Value) -> Result<Vec<WebviewAction>> {
    let values = match value {
        Value::Array(values) => values.snapshot(),
        Value::List(values) => values.snapshot(),
        value => bail!("webview document `actions` must be an array or list, got {value:?}"),
    };

    values
        .iter()
        .map(webview_action_from_value)
        .collect::<Result<Vec<_>>>()
}

fn webview_action_from_value(value: &Value) -> Result<WebviewAction> {
    let Value::Map(map) = value else {
        bail!("webview action entries must be maps, got {value:?}");
    };
    if let Some(Value::String(kind)) = map.get("type") {
        if kind != "action" {
            bail!("webview action `type` must be \"action\", got {kind:?}");
        }
    }
    Ok(WebviewAction {
        action: required_document_string(map, "action")?,
        callback: required_document_string(map, "callback")?,
    })
}

fn webview_menu_bar_from_value(value: &Value) -> Result<WebviewMenuBar> {
    let Value::Map(map) = value else {
        bail!("webview document `menus` must be a menu bar map, got {value:?}");
    };
    match map.get("type") {
        Some(Value::String(kind)) if kind == "menu_bar" => {}
        Some(Value::String(kind)) => {
            bail!("webview menu bar `type` must be \"menu_bar\", got {kind:?}")
        }
        Some(value) => bail!("webview menu bar `type` must be a string, got {value:?}"),
        None => bail!("webview menu bar is missing `type`"),
    }
    let menus = map
        .get("menus")
        .context("webview menu bar is missing `menus`")?;
    Ok(WebviewMenuBar {
        menus: webview_menu_list_from_value(&menus)?,
    })
}

fn webview_menu_list_from_value(value: &Value) -> Result<Vec<WebviewMenu>> {
    webview_value_list(value, "webview menu bar `menus`")?
        .iter()
        .map(webview_menu_from_value)
        .collect()
}

fn webview_menu_from_value(value: &Value) -> Result<WebviewMenu> {
    let Value::Map(map) = value else {
        bail!("webview menu entries must be maps, got {value:?}");
    };
    match map.get("type") {
        Some(Value::String(kind)) if kind == "menu" => {}
        Some(Value::String(kind)) => bail!("webview menu `type` must be \"menu\", got {kind:?}"),
        Some(value) => bail!("webview menu `type` must be a string, got {value:?}"),
        None => bail!("webview menu is missing `type`"),
    }
    let items = map
        .get("items")
        .context("webview menu is missing `items`")?;
    Ok(WebviewMenu {
        label: required_document_string(map, "label")?,
        items: webview_menu_items_from_value(&items)?,
    })
}

fn webview_menu_items_from_value(value: &Value) -> Result<Vec<WebviewMenuItem>> {
    webview_value_list(value, "webview menu `items`")?
        .iter()
        .map(webview_menu_item_from_value)
        .collect()
}

fn webview_menu_item_from_value(value: &Value) -> Result<WebviewMenuItem> {
    let Value::Map(map) = value else {
        bail!("webview menu item entries must be maps, got {value:?}");
    };
    let kind = match map.get("type") {
        Some(Value::String(kind)) => kind,
        Some(value) => bail!("webview menu item `type` must be a string, got {value:?}"),
        None => bail!("webview menu item is missing `type`"),
    };
    match kind.as_str() {
        "command" => {
            let shortcut = match map.get("shortcut") {
                Some(Value::String(value)) if value.trim().is_empty() => None,
                Some(Value::String(value)) => Some(value),
                Some(Value::Nil) | None => None,
                Some(value) => bail!("webview command `shortcut` must be a string, got {value:?}"),
            };
            Ok(WebviewMenuItem::Command {
                label: required_document_string(map, "label")?,
                action: required_document_string(map, "action")?,
                shortcut,
            })
        }
        "separator" => Ok(WebviewMenuItem::Separator),
        other => bail!("unsupported webview menu item type {other:?}"),
    }
}

fn webview_value_list(value: &Value, label: &str) -> Result<Vec<Value>> {
    match value {
        Value::Array(values) => Ok(values.snapshot()),
        Value::List(values) => Ok(values.snapshot()),
        value => bail!("{label} must be an array or list, got {value:?}"),
    }
}

fn optional_document_value(map: &MapValue, key: &str) -> Option<Value> {
    map.get(key)
}

fn required_document_string(map: &MapValue, key: &str) -> Result<String> {
    match map.get(key) {
        Some(Value::String(value)) => Ok(value),
        Some(value) => bail!("webview document `{key}` must be a string, got {value:?}"),
        None => bail!("webview document is missing `{key}`"),
    }
}

fn required_document_dimension(map: &MapValue, key: &str) -> Result<u32> {
    match map.get(key) {
        Some(Value::Number(value)) if value > 0 => u32::try_from(value)
            .with_context(|| format!("webview document `{key}` is too large: {value}")),
        Some(Value::Number(value)) => {
            bail!("webview document `{key}` must be positive, got {value}")
        }
        Some(value) => bail!("webview document `{key}` must be a number, got {value:?}"),
        None => bail!("webview document is missing `{key}`"),
    }
}

fn gui_event_span() -> SourceSpan {
    SourceSpan {
        file: "<gui-event>".to_string(),
        start: 0,
        end: 0,
        line: 1,
        column: 1,
    }
}

fn json_to_ricochet_value(value: serde_json::Value) -> Value {
    match value {
        serde_json::Value::Null => Value::Nil,
        serde_json::Value::Bool(value) => Value::Bool(value),
        serde_json::Value::Number(value) => {
            if let Some(value) = value.as_i64() {
                Value::Number(value)
            } else if let Some(value) = value.as_u64().and_then(|value| i64::try_from(value).ok()) {
                Value::Number(value)
            } else if let Some(value) = value.as_f64() {
                Value::Float(value)
            } else {
                Value::Nil
            }
        }
        serde_json::Value::String(value) => Value::String(value),
        serde_json::Value::Array(values) => Value::Array(
            values
                .into_iter()
                .map(json_to_ricochet_value)
                .collect::<Vec<_>>()
                .into(),
        ),
        serde_json::Value::Object(values) => Value::Map(
            values
                .into_iter()
                .map(|(key, value)| (key, json_to_ricochet_value(value)))
                .collect::<BTreeMap<_, _>>()
                .into(),
        ),
    }
}

fn webview_document_update_script(document: &WebviewDocument) -> Result<String> {
    let payload = json!({
        "title": document.title,
        "body": document.body,
        "state": ricochet_value_to_json(&document.state, "$.state")?,
        "actions": document
            .actions
            .iter()
            .map(|action| {
                json!({
                    "type": "action",
                    "action": action.action,
                    "callback": action.callback,
                })
            })
            .collect::<Vec<_>>(),
    });
    let json = serde_json::to_string(&payload).context("failed to encode GUI document update")?;
    Ok(format!(
        "window.__ricochetApplyDocument({});",
        js_json_literal(&json)
    ))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum WebviewJsonVisit {
    Array(usize),
    List(usize),
    Set(usize),
    Map(usize),
}

fn ricochet_value_to_json(value: &Value, root: &str) -> Result<serde_json::Value> {
    ricochet_value_to_json_inner(value, root, &mut Vec::new())
}

fn ricochet_value_to_json_inner(
    value: &Value,
    path: &str,
    visits: &mut Vec<WebviewJsonVisit>,
) -> Result<serde_json::Value> {
    Ok(match value {
        Value::Nil => serde_json::Value::Null,
        Value::Bool(value) => serde_json::Value::Bool(*value),
        Value::Number(value) => serde_json::Value::Number((*value).into()),
        Value::Float(value) => serde_json::Number::from_f64(*value)
            .map(serde_json::Value::Number)
            .context("webview state cannot encode non-finite floats")?,
        Value::String(value) => serde_json::Value::String(value.clone()),
        Value::Array(values) => with_webview_json_collection(
            visits,
            WebviewJsonVisit::Array(values.identity()),
            path,
            |visits| webview_json_sequence(values.snapshot(), path, visits),
        )?,
        Value::List(values) => with_webview_json_collection(
            visits,
            WebviewJsonVisit::List(values.identity()),
            path,
            |visits| webview_json_sequence(values.snapshot(), path, visits),
        )?,
        Value::Set(values) => with_webview_json_collection(
            visits,
            WebviewJsonVisit::Set(values.identity()),
            path,
            |visits| webview_json_sequence(values.snapshot(), path, visits),
        )?,
        Value::Map(values) => with_webview_json_collection(
            visits,
            WebviewJsonVisit::Map(values.identity()),
            path,
            |visits| {
                let mut object = serde_json::Map::new();
                for (key, value) in values.entries() {
                    object.insert(
                        key.clone(),
                        ricochet_value_to_json_inner(&value, &format!("{path}.{key}"), visits)?,
                    );
                }
                Ok(serde_json::Value::Object(object))
            },
        )?,
        value => bail!("webview state cannot encode {value:?} as JSON"),
    })
}

fn webview_json_sequence(
    values: Vec<Value>,
    path: &str,
    visits: &mut Vec<WebviewJsonVisit>,
) -> Result<serde_json::Value> {
    let mut output = Vec::with_capacity(values.len());
    for (index, value) in values.into_iter().enumerate() {
        output.push(ricochet_value_to_json_inner(
            &value,
            &format!("{path}[{index}]"),
            visits,
        )?);
    }
    Ok(serde_json::Value::Array(output))
}

fn with_webview_json_collection<T>(
    visits: &mut Vec<WebviewJsonVisit>,
    visit: WebviewJsonVisit,
    path: &str,
    serialize: impl FnOnce(&mut Vec<WebviewJsonVisit>) -> Result<T>,
) -> Result<T> {
    if visits.contains(&visit) {
        bail!("cannot encode cyclic collection as WebView JSON at {path}");
    }
    visits.push(visit);
    let result = serialize(visits);
    visits.pop();
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn webview_json_rejects_cycle_at_exact_state_path() {
        let state = MapValue::default();
        state.insert("self".to_string(), Value::Map(state.clone()));

        let error = ricochet_value_to_json(&Value::Map(state), "$.state")
            .expect_err("cyclic WebView state should be rejected");

        assert_eq!(
            error.to_string(),
            "cannot encode cyclic collection as WebView JSON at $.state.self"
        );
    }

    #[test]
    fn callback_webview_navigation_stays_on_its_trusted_document() {
        assert_eq!(
            callback_webview_navigation_decision("ricochet://localhost/"),
            NativeWebviewNavigationDecision::Allow
        );
        assert_eq!(
            callback_webview_navigation_decision("http://ricochet.localhost/#details"),
            NativeWebviewNavigationDecision::Allow
        );
        assert_eq!(
            callback_webview_navigation_decision("https://try.ricochet.today/docs"),
            NativeWebviewNavigationDecision::OpenExternal
        );
        assert_eq!(
            callback_webview_navigation_decision("http://ricochet.localhost.evil.example/"),
            NativeWebviewNavigationDecision::OpenExternal
        );

        for target in [
            "about:blank",
            "javascript:window.ipc.postMessage('{}')",
            "data:text/html,<script>window.ipc.postMessage('{}')</script>",
            "file:///etc/passwd",
            "ricochet://attacker/",
        ] {
            assert_eq!(
                callback_webview_navigation_decision(target),
                NativeWebviewNavigationDecision::Block,
                "callback webview navigation should block {target:?}"
            );
        }
    }

    #[test]
    fn callback_webview_ipc_requires_the_trusted_document_uri() {
        for trusted in ["ricochet://localhost/", "ricochet://localhost/#details"] {
            assert!(
                callback_webview_ipc_is_trusted(trusted),
                "callback IPC should accept {trusted:?}"
            );
        }

        #[cfg(windows)]
        for trusted in [
            "http://ricochet.localhost/",
            "http://ricochet.localhost/#details",
        ] {
            assert!(
                callback_webview_ipc_is_trusted(trusted),
                "Windows callback IPC should accept Wry's protocol URL {trusted:?}"
            );
        }

        for untrusted in [
            "about:blank",
            "https://example.com/",
            "ricochet://localhost/other",
            "ricochet://localhost/?document=other",
            "ricochet://user@localhost/",
            "http://ricochet.localhost:81/",
        ] {
            assert!(
                !callback_webview_ipc_is_trusted(untrusted),
                "callback IPC should reject {untrusted:?}"
            );
        }

        #[cfg(any(target_os = "linux", target_os = "macos"))]
        assert!(
            !callback_webview_ipc_is_trusted("http://ricochet.localhost/"),
            "non-Windows hosts must not trust a local HTTP origin"
        );
    }

    #[test]
    fn mvc_webview_navigation_keeps_local_routes_and_externalizes_web_links() {
        let app_url = "http://127.0.0.1:4312/";

        assert_eq!(
            mvc_webview_navigation_decision(app_url, "http://127.0.0.1:4312/accounts/1"),
            NativeWebviewNavigationDecision::Allow
        );
        assert_eq!(
            mvc_webview_navigation_decision(app_url, "https://example.com/help"),
            NativeWebviewNavigationDecision::OpenExternal
        );
        assert_eq!(
            mvc_webview_navigation_decision(app_url, "http://127.0.0.1:4313/"),
            NativeWebviewNavigationDecision::OpenExternal
        );

        for target in [
            "javascript:alert(1)",
            "data:text/html,untrusted",
            "file:///tmp/untrusted",
        ] {
            assert_eq!(
                mvc_webview_navigation_decision(app_url, target),
                NativeWebviewNavigationDecision::Block,
                "MVC webview navigation should block {target:?}"
            );
        }
    }
}

fn js_json_literal(json: &str) -> String {
    let mut escaped = String::with_capacity(json.len());
    for character in json.chars() {
        match character {
            '<' => escaped.push_str("\\u003c"),
            '>' => escaped.push_str("\\u003e"),
            '&' => escaped.push_str("\\u0026"),
            '\u{2028}' => escaped.push_str("\\u2028"),
            '\u{2029}' => escaped.push_str("\\u2029"),
            _ => escaped.push(character),
        }
    }
    escaped
}

#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
fn open_native_webview(session: WebviewSession) -> Result<()> {
    #[cfg(target_os = "linux")]
    if linux_external_browser_requested() {
        let path = write_linux_webview_document(&session.document)?;
        open_linux_gui_target(path.as_os_str())?;
        eprintln!(
            "Ricochet opened file {} through the Linux external-browser diagnostic fallback because {GUI_EXTERNAL_BROWSER_ENV} is set.",
            path.display()
        );
        return Ok(());
    }

    let document = session.document.clone();
    open_platform_webview(
        document.title,
        document.width,
        document.height,
        NativeWebviewSource::Html(document.html),
        Some(session),
        document.menus,
    )
}

#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
fn open_native_webview_url(title: &str, url: &str, width: u32, height: u32) -> Result<()> {
    #[cfg(target_os = "linux")]
    if linux_external_browser_requested() {
        open_linux_gui_target(OsStr::new(url))?;
        return wait_for_linux_browser_session(url);
    }

    open_platform_webview(
        title.to_string(),
        width,
        height,
        NativeWebviewSource::Url(url.to_string()),
        None,
        WebviewMenuBar::default(),
    )
}

#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
enum NativeWebviewSource {
    Html(String),
    Url(String),
}

#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
const RICOCHET_CALLBACK_WEBVIEW_SCHEME: &str = "ricochet";
#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
const RICOCHET_CALLBACK_WEBVIEW_URL: &str = "ricochet://localhost/";

#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NativeWebviewNavigationDecision {
    Allow,
    OpenExternal,
    Block,
}

#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
fn callback_webview_navigation_decision(target: &str) -> NativeWebviewNavigationDecision {
    if callback_webview_ipc_is_trusted(target) {
        NativeWebviewNavigationDecision::Allow
    } else if ricochet_vm::is_safe_external_web_url(target) {
        NativeWebviewNavigationDecision::OpenExternal
    } else {
        NativeWebviewNavigationDecision::Block
    }
}

#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
fn callback_webview_ipc_is_trusted(uri: &str) -> bool {
    let Ok(uri) = reqwest::Url::parse(uri) else {
        return false;
    };
    let trusted_origin =
        uri.scheme() == RICOCHET_CALLBACK_WEBVIEW_SCHEME && uri.host_str() == Some("localhost");
    #[cfg(windows)]
    let trusted_origin =
        trusted_origin || (uri.scheme() == "http" && uri.host_str() == Some("ricochet.localhost"));
    trusted_origin
        && uri.username().is_empty()
        && uri.password().is_none()
        && uri.port().is_none()
        && uri.path() == "/"
        && uri.query().is_none()
}

#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
fn mvc_webview_navigation_decision(app_url: &str, target: &str) -> NativeWebviewNavigationDecision {
    let same_origin = reqwest::Url::parse(app_url)
        .ok()
        .zip(reqwest::Url::parse(target).ok())
        .is_some_and(|(app_url, target)| {
            app_url.scheme() == target.scheme()
                && app_url.host_str() == target.host_str()
                && app_url.port_or_known_default() == target.port_or_known_default()
                && target.username().is_empty()
                && target.password().is_none()
        });
    if same_origin {
        NativeWebviewNavigationDecision::Allow
    } else if ricochet_vm::is_safe_external_web_url(target) {
        NativeWebviewNavigationDecision::OpenExternal
    } else {
        NativeWebviewNavigationDecision::Block
    }
}

#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
#[derive(Debug, Clone)]
enum NativeGuiEvent {
    Ipc(String),
    Menu(String),
    OpenExternal(String),
}

#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
fn open_platform_webview(
    title: String,
    width: u32,
    height: u32,
    source: NativeWebviewSource,
    session: Option<WebviewSession>,
    menus: WebviewMenuBar,
) -> Result<()> {
    use tao::{
        dpi::LogicalSize,
        event::{Event, WindowEvent},
        event_loop::{ControlFlow, EventLoopBuilder},
        window::WindowBuilder,
    };
    use wry::WebViewBuilder;

    let native_menu = build_native_menu(&menus)?;
    let mut event_loop_builder = EventLoopBuilder::<NativeGuiEvent>::with_user_event();
    #[cfg(windows)]
    {
        use tao::platform::windows::EventLoopBuilderExtWindows;
        use windows_sys::Win32::UI::WindowsAndMessaging::{TranslateAcceleratorW, MSG};

        let haccel = native_menu.haccel();
        event_loop_builder.with_msg_hook(move |message| {
            if haccel == 0 {
                return false;
            }
            let message = message as *const MSG;
            unsafe { TranslateAcceleratorW((*message).hwnd, haccel as _, message) == 1 }
        });
    }
    let event_loop = event_loop_builder.build();
    let proxy = event_loop.create_proxy();
    muda::MenuEvent::set_event_handler(Some(move |event: muda::MenuEvent| {
        let _ = proxy.send_event(NativeGuiEvent::Menu(event.id().as_ref().to_string()));
    }));

    let window = WindowBuilder::new()
        .with_title(title.clone())
        .with_inner_size(LogicalSize::new(f64::from(width), f64::from(height)))
        .build(&event_loop)
        .context("failed to create native GUI window")?;
    attach_native_menu(&native_menu, &window)?;

    let builder = match source {
        NativeWebviewSource::Html(html) => {
            let document = html.into_bytes();
            let navigation_proxy = event_loop.create_proxy();
            let ipc_proxy = event_loop.create_proxy();
            WebViewBuilder::new()
                .with_custom_protocol(
                    RICOCHET_CALLBACK_WEBVIEW_SCHEME.to_string(),
                    move |_webview_id, request| {
                        use std::borrow::Cow;
                        use wry::http::{header::CONTENT_TYPE, Method, Response, StatusCode};

                        let is_document = request.method() == Method::GET
                            && callback_webview_ipc_is_trusted(&request.uri().to_string());
                        let status = if is_document {
                            StatusCode::OK
                        } else {
                            StatusCode::NOT_FOUND
                        };
                        let body = if is_document {
                            document.clone()
                        } else {
                            Vec::new()
                        };
                        Response::builder()
                            .status(status)
                            .header(CONTENT_TYPE, "text/html; charset=utf-8")
                            .body(Cow::Owned(body))
                            .expect("static callback WebView response should be valid")
                    },
                )
                .with_url(RICOCHET_CALLBACK_WEBVIEW_URL)
                .with_navigation_handler(move |target| {
                    apply_native_navigation_decision(
                        &navigation_proxy,
                        &target,
                        callback_webview_navigation_decision(&target),
                    )
                })
                .with_ipc_handler(move |request| {
                    let uri = request.uri().to_string();
                    if callback_webview_ipc_is_trusted(&uri) {
                        let _ =
                            ipc_proxy.send_event(NativeGuiEvent::Ipc(request.body().to_string()));
                    } else {
                        eprintln!(
                            "Ricochet GUI rejected callback IPC from untrusted document {uri:?}"
                        );
                    }
                })
        }
        NativeWebviewSource::Url(url) => {
            let app_url = url.clone();
            let navigation_proxy = event_loop.create_proxy();
            WebViewBuilder::new()
                .with_url(url)
                .with_navigation_handler(move |target| {
                    apply_native_navigation_decision(
                        &navigation_proxy,
                        &target,
                        mvc_webview_navigation_decision(&app_url, &target),
                    )
                })
        }
    };

    let webview = build_platform_webview(builder, &window)?;
    let mut session = session;
    let _native_menu = native_menu;
    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => *control_flow = ControlFlow::Exit,
            Event::UserEvent(NativeGuiEvent::Ipc(message)) => {
                if let Err(error) =
                    dispatch_native_gui_ipc(&mut session, &webview, &window, &message)
                {
                    eprintln!("Ricochet GUI event failed: {error:#}");
                }
            }
            Event::UserEvent(NativeGuiEvent::Menu(action)) => {
                if action == RICOCHET_QUIT_ACTION {
                    *control_flow = ControlFlow::Exit;
                } else if action != RICOCHET_COPY_ACTION && action != RICOCHET_PASTE_ACTION {
                    if let Err(error) =
                        dispatch_native_gui_menu(&mut session, &webview, &window, &action)
                    {
                        eprintln!("Ricochet GUI menu action failed: {error:#}");
                    }
                }
            }
            Event::UserEvent(NativeGuiEvent::OpenExternal(url)) => {
                if let Err(error) = ricochet_vm::open_external_url(&url) {
                    eprintln!("Ricochet GUI could not open external URL {url:?}: {error}");
                }
            }
            _ => {}
        }
    });
}

#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
fn apply_native_navigation_decision(
    proxy: &tao::event_loop::EventLoopProxy<NativeGuiEvent>,
    target: &str,
    decision: NativeWebviewNavigationDecision,
) -> bool {
    match decision {
        NativeWebviewNavigationDecision::Allow => true,
        NativeWebviewNavigationDecision::OpenExternal => {
            let _ = proxy.send_event(NativeGuiEvent::OpenExternal(target.to_string()));
            false
        }
        NativeWebviewNavigationDecision::Block => {
            eprintln!("Ricochet GUI blocked unsafe WebView navigation to {target:?}");
            false
        }
    }
}

#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
fn dispatch_native_gui_ipc(
    session: &mut Option<WebviewSession>,
    webview: &wry::WebView,
    window: &tao::window::Window,
    message: &str,
) -> Result<()> {
    let event_json: serde_json::Value =
        serde_json::from_str(message).context("GUI IPC message must be JSON")?;
    dispatch_native_gui_event(session, webview, window, event_json)
}

#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
fn dispatch_native_gui_menu(
    session: &mut Option<WebviewSession>,
    webview: &wry::WebView,
    window: &tao::window::Window,
    action: &str,
) -> Result<()> {
    dispatch_native_gui_event(
        session,
        webview,
        window,
        json!({
            "type": "menu",
            "action": action,
        }),
    )
}

#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
fn dispatch_native_gui_event(
    session: &mut Option<WebviewSession>,
    webview: &wry::WebView,
    window: &tao::window::Window,
    event_json: serde_json::Value,
) -> Result<()> {
    let session = session
        .as_mut()
        .context("this GUI host does not accept Ricochet callback events")?;
    let document = session.dispatch_event_json(event_json)?;
    window.set_title(&document.title);
    let script = webview_document_update_script(document)?;
    webview
        .evaluate_script(&script)
        .context("failed to update native WebView document")?;
    Ok(())
}

#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
fn build_native_menu(menu_bar: &WebviewMenuBar) -> Result<muda::Menu> {
    use muda::{accelerator::Accelerator, Menu, MenuItem, PredefinedMenuItem, Submenu};

    let menu = Menu::new();
    for menu_def in &menu_bar.menus {
        let submenu = Submenu::new(&menu_def.label, true);
        for item in &menu_def.items {
            match item {
                WebviewMenuItem::Command {
                    label,
                    action,
                    shortcut,
                } if action == RICOCHET_COPY_ACTION => {
                    submenu.append(&PredefinedMenuItem::copy(Some(label)))?;
                }
                WebviewMenuItem::Command {
                    label,
                    action,
                    shortcut: _,
                } if action == RICOCHET_PASTE_ACTION => {
                    submenu.append(&PredefinedMenuItem::paste(Some(label)))?;
                }
                WebviewMenuItem::Command {
                    label,
                    action,
                    shortcut,
                } => {
                    let accelerator = shortcut
                        .as_deref()
                        .map(str::parse::<Accelerator>)
                        .transpose()
                        .with_context(|| {
                            format!("failed to parse menu shortcut for action {action:?}")
                        })?;
                    let item = MenuItem::with_id(action, label, true, accelerator);
                    submenu.append(&item)?;
                }
                WebviewMenuItem::Separator => {
                    submenu.append(&PredefinedMenuItem::separator())?;
                }
            }
        }
        menu.append(&submenu)?;
    }
    Ok(menu)
}

#[cfg(windows)]
fn attach_native_menu(menu: &muda::Menu, window: &tao::window::Window) -> Result<()> {
    use tao::platform::windows::WindowExtWindows;
    unsafe { menu.init_for_hwnd(window.hwnd()) }.context("failed to attach native Windows menu")?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn attach_native_menu(menu: &muda::Menu, _window: &tao::window::Window) -> Result<()> {
    menu.init_for_nsapp();
    Ok(())
}

#[cfg(target_os = "linux")]
fn attach_native_menu(menu: &muda::Menu, window: &tao::window::Window) -> Result<()> {
    use tao::platform::unix::WindowExtUnix;
    let vbox = window
        .default_vbox()
        .context("native Linux GUI window did not expose a GTK content box")?;
    menu.init_for_gtk_window(window.gtk_window(), Some(vbox))
        .context("failed to attach native Linux menu")?;
    Ok(())
}

#[cfg(any(windows, target_os = "macos"))]
fn build_platform_webview(
    builder: wry::WebViewBuilder,
    window: &tao::window::Window,
) -> Result<wry::WebView> {
    builder
        .build(window)
        .context("failed to create native WebView")
}

#[cfg(target_os = "linux")]
fn build_platform_webview(
    builder: wry::WebViewBuilder,
    window: &tao::window::Window,
) -> Result<wry::WebView> {
    use tao::platform::unix::WindowExtUnix;
    use wry::WebViewBuilderExtUnix;

    let vbox = window
        .default_vbox()
        .context("native Linux GUI window did not expose a GTK content box")?;
    builder
        .build_gtk(vbox)
        .context("failed to create embedded Linux WebView")
}

#[cfg(target_os = "linux")]
fn linux_external_browser_requested() -> bool {
    std::env::var(GUI_EXTERNAL_BROWSER_ENV)
        .ok()
        .is_some_and(|value| !matches!(value.as_str(), "" | "0" | "false" | "False" | "FALSE"))
}

#[cfg(target_os = "linux")]
fn write_linux_webview_document(document: &WebviewDocument) -> Result<PathBuf> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let file_name = format!("ricochet-gui-{}-{timestamp}.html", std::process::id());
    let path = std::env::temp_dir().join(file_name);
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&path)
        .with_context(|| format!("failed to create GUI HTML file {}", path.display()))?;
    file.write_all(document.html.as_bytes())
        .with_context(|| format!("failed to write GUI HTML file {}", path.display()))?;
    Ok(path)
}

#[cfg(target_os = "linux")]
fn open_linux_gui_target(target: &OsStr) -> Result<()> {
    let status = std::process::Command::new("xdg-open")
        .arg(target)
        .status()
        .context(
            "failed to launch `xdg-open`; install `xdg-utils` to use the Linux external-browser diagnostic fallback",
        )?;
    if !status.success() {
        bail!("`xdg-open` failed with status {status}");
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn wait_for_linux_browser_session(target_label: &str) -> Result<()> {
    eprintln!(
        "Ricochet opened {target_label} through the Linux external-browser diagnostic fallback. Press Ctrl+C in this terminal to stop the GUI host when finished."
    );
    loop {
        std::thread::park_timeout(Duration::from_secs(3600));
    }
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
fn open_native_webview(_session: WebviewSession) -> Result<()> {
    bail!("GUI hosting is currently implemented for Windows, Linux, and macOS builds")
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
fn open_native_webview_url(_title: &str, _url: &str, _width: u32, _height: u32) -> Result<()> {
    bail!("GUI hosting is currently implemented for Windows, Linux, and macOS builds")
}
