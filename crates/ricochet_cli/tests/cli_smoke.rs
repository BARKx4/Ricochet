use std::fs;
use std::io::Write;
use std::net::{Shutdown, TcpListener};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[test]
fn new_creates_mvc_project_skeleton() {
    let source_path = temp_source_path();
    let project_path = source_path
        .parent()
        .expect("source path has parent")
        .join("hello_app");

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("new")
        .arg(&project_path)
        .output()
        .expect("rco new should launch");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "rco new failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("created"),
        "stdout should mention created project, got:\n{stdout}"
    );

    let manifest =
        fs::read_to_string(project_path.join("ricochet.toml")).expect("manifest should exist");
    let routes =
        fs::read_to_string(project_path.join("config/routes.rco")).expect("routes should exist");
    let controller = fs::read_to_string(
        project_path
            .join("app")
            .join("Controllers")
            .join("HomeController.rco"),
    )
    .expect("controller should exist");
    let view = fs::read_to_string(
        project_path
            .join("app")
            .join("Views")
            .join("home")
            .join("index.html"),
    )
    .expect("view should exist");
    let stylesheet =
        fs::read_to_string(project_path.join("public").join("app.css")).expect("css should exist");
    let model = fs::read_to_string(project_path.join("app").join("Models").join("User.rco"))
        .expect("model should exist");
    let user_controller = fs::read_to_string(
        project_path
            .join("app")
            .join("Controllers")
            .join("UserController.rco"),
    )
    .expect("user controller should exist");
    let users_view = fs::read_to_string(
        project_path
            .join("app")
            .join("Views")
            .join("users")
            .join("index.html"),
    )
    .expect("users view should exist");
    let test = fs::read_to_string(project_path.join("tests").join("ApplicationSmokeTest.rco"))
        .expect("test should exist");

    assert!(manifest.contains("routes = \"config/routes.rco\""));
    assert!(manifest.contains("[web.static]"));
    assert!(manifest.contains("dir = \"public\""));
    assert!(manifest.contains("mount = \"/assets\""));
    assert!(
        !manifest.contains("[database.default]"),
        "fresh scaffolds should not require a database before rco serve can boot"
    );
    assert!(
        !manifest.contains("DATABASE_URL"),
        "fresh scaffolds should not require DATABASE_URL before rco serve can boot"
    );
    assert!(routes.contains("GET \"/\" HomeController \"index\" route"));
    assert!(routes.contains("GET \"/users\" UserController \"index\" route"));
    assert!(controller.contains("HomeController Controller Subclass"));
    assert!(view.contains("href=\"/assets/app.css\""));
    assert!(view.contains("{ title get }"));
    assert!(stylesheet.contains("font-family"));
    assert!(model.contains("User Model Subclass"));
    assert!(model.contains("\"displayName\""));
    assert!(user_controller.contains("UserController Controller Subclass"));
    assert!(user_controller.contains("users array"));
    assert!(user_controller.contains("push!"));
    assert!(user_controller.contains("userCount var"));
    assert!(users_view.contains("{ userCount get }"));
    assert!(test.contains("ApplicationSmokeTest TestCase Subclass"));
    assert!(test.contains("User new"));
    assert!(test.contains("displayName"));
    assert!(test.contains("users array"));
    assert!(test.contains("push!"));

    let _app = ricochet_web::server::build_app_from_dir(&project_path)
        .expect("scaffolded MVC app should build");

    let test_output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("test")
        .arg(&project_path)
        .output()
        .expect("rco test should launch");
    let test_stdout = String::from_utf8_lossy(&test_output.stdout);
    let test_stderr = String::from_utf8_lossy(&test_output.stderr);

    assert!(
        test_output.status.success(),
        "scaffolded tests should pass\nstdout:\n{test_stdout}\nstderr:\n{test_stderr}"
    );
    assert!(
        test_stdout.contains("2 tests, 0 failed"),
        "scaffolded test summary should pass, got:\n{test_stdout}"
    );

    let nested_test_output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("test")
        .arg(project_path.join("tests"))
        .output()
        .expect("rco test should launch for tests directory");
    let nested_test_stdout = String::from_utf8_lossy(&nested_test_output.stdout);
    let nested_test_stderr = String::from_utf8_lossy(&nested_test_output.stderr);

    assert!(
        nested_test_output.status.success(),
        "scaffolded tests directory should pass\nstdout:\n{nested_test_stdout}\nstderr:\n{nested_test_stderr}"
    );
    assert!(
        nested_test_stdout.contains("2 tests, 0 failed"),
        "scaffolded tests directory summary should pass, got:\n{nested_test_stdout}"
    );
}

#[test]
fn new_with_sqlite_creates_ready_database_project() {
    let source_path = temp_source_path();
    let project_path = source_path
        .parent()
        .expect("source path has parent")
        .join("sqlite_app");

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("new")
        .arg("--with-sqlite")
        .arg(&project_path)
        .output()
        .expect("rco new should launch");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "rco new --with-sqlite failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("SQLite database"),
        "stdout should mention the SQLite database, got:\n{stdout}"
    );

    let manifest =
        fs::read_to_string(project_path.join("ricochet.toml")).expect("manifest should exist");
    assert!(manifest.contains("[database.default]"));
    assert!(manifest.contains("adapter = \"sqlite\""));
    assert!(manifest.contains("url = \"db/development.sqlite3\""));
    let initial_migration = fs::read_to_string(
        project_path
            .join("db")
            .join("migrations")
            .join("0001_create_users.sql"),
    )
    .expect("initial migration should exist");
    assert!(initial_migration.contains("create table users"));

    let model = fs::read_to_string(project_path.join("app").join("Models").join("User.rco"))
        .expect("model should exist");
    assert!(model.contains("\"users\" Table"));
    assert!(model.contains("\"id\" Accessor"));

    let controller = fs::read_to_string(
        project_path
            .join("app")
            .join("Controllers")
            .join("UserController.rco"),
    )
    .expect("user controller should exist");
    assert!(controller.contains("User default-page"));
    assert!(controller.contains("firstEmail var"));

    let routes =
        fs::read_to_string(project_path.join("config/routes.rco")).expect("routes should exist");
    assert!(routes.contains("GET \"/login\" AuthController \"login\" route"));
    assert!(routes.contains("POST \"/login\" AuthController \"create\" route"));
    assert!(routes.contains("GET \"/me\" AuthController \"show\" route"));
    assert!(routes.contains("POST \"/logout\" AuthController \"destroy\" route"));

    let auth_controller = fs::read_to_string(
        project_path
            .join("app")
            .join("Controllers")
            .join("AuthController.rco"),
    )
    .expect("auth controller should exist");
    assert!(auth_controller.contains("session get \"user_email\""));
    assert!(auth_controller.contains("remove!"));
    assert!(auth_controller.contains("\"/me\" redirect"));

    let login_view = fs::read_to_string(
        project_path
            .join("app")
            .join("Views")
            .join("auth")
            .join("login.html"),
    )
    .expect("login view should exist");
    assert!(login_view.contains("method=\"post\""));
    assert!(login_view.contains("ada@example.com"));

    let database_path = project_path.join("db").join("development.sqlite3");
    assert!(
        database_path.exists(),
        "SQLite database should be created at {}",
        database_path.display()
    );
    let connection = rusqlite::Connection::open(&database_path).expect("sqlite database opens");
    let first_email: String = connection
        .query_row("select email from users order by id limit 1", [], |row| {
            row.get(0)
        })
        .expect("seeded user should be queryable");
    assert_eq!(first_email, "ada@example.com");
    let initial_version: String = connection
        .query_row("select version from schema_migrations", [], |row| {
            row.get(0)
        })
        .expect("initial migration should be recorded");
    assert_eq!(initial_version, "0001_create_users");

    fs::write(
        project_path
            .join("db")
            .join("migrations")
            .join("0002_create_notes.sql"),
        "create table notes (id integer primary key, body text not null);\n",
    )
    .expect("second migration should be written");

    let status_output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("migrate")
        .arg("status")
        .arg(&project_path)
        .output()
        .expect("rco migrate status should launch");
    let status_stdout = String::from_utf8_lossy(&status_output.stdout);
    let status_stderr = String::from_utf8_lossy(&status_output.stderr);
    assert!(
        status_output.status.success(),
        "migrate status should pass\nstdout:\n{status_stdout}\nstderr:\n{status_stderr}"
    );
    assert!(status_stdout.contains("[x] 0001_create_users"));
    assert!(status_stdout.contains("[ ] 0002_create_notes"));

    let apply_output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("migrate")
        .arg("apply")
        .arg(&project_path)
        .output()
        .expect("rco migrate apply should launch");
    let apply_stdout = String::from_utf8_lossy(&apply_output.stdout);
    let apply_stderr = String::from_utf8_lossy(&apply_output.stderr);
    assert!(
        apply_output.status.success(),
        "migrate apply should pass\nstdout:\n{apply_stdout}\nstderr:\n{apply_stderr}"
    );
    assert!(apply_stdout.contains("applied 0002_create_notes"));
    let notes_count: i64 = connection
        .query_row(
            "select count(*) from sqlite_master where type = 'table' and name = 'notes'",
            [],
            |row| row.get(0),
        )
        .expect("notes table check should run");
    assert_eq!(notes_count, 1);

    let check_output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("check")
        .arg(&project_path)
        .output()
        .expect("rco check should launch");
    let check_stdout = String::from_utf8_lossy(&check_output.stdout);
    let check_stderr = String::from_utf8_lossy(&check_output.stderr);
    assert!(
        check_output.status.success(),
        "SQLite scaffold should check\nstdout:\n{check_stdout}\nstderr:\n{check_stderr}"
    );

    let test_output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("test")
        .arg(&project_path)
        .output()
        .expect("rco test should launch");
    let test_stdout = String::from_utf8_lossy(&test_output.stdout);
    let test_stderr = String::from_utf8_lossy(&test_output.stderr);
    assert!(
        test_output.status.success(),
        "SQLite scaffolded tests should pass\nstdout:\n{test_stdout}\nstderr:\n{test_stderr}"
    );
    assert!(
        test_stdout.contains("2 tests, 0 failed"),
        "SQLite scaffolded test summary should pass, got:\n{test_stdout}"
    );
}

#[test]
fn migrate_status_recognizes_postgres_and_mysql_without_files() {
    for (adapter, url, target) in [
        (
            "postgres",
            "postgres://app:secret@db.example.com/app",
            "PostgreSQL database",
        ),
        (
            "mysql",
            "mysql://app:secret@db.example.com/app",
            "MySQL database",
        ),
    ] {
        let source_path = temp_source_path();
        let root = source_path
            .parent()
            .expect("source path has parent")
            .join(format!("{adapter}_migration_app"));
        write_source_at(
            &root,
            "ricochet.toml",
            &format!(
                "[package]\nname = \"migration_app\"\n\n[database.default]\nadapter = \"{adapter}\"\nurl = \"{url}\"\n"
            ),
        );

        let output = Command::new(env!("CARGO_BIN_EXE_rco"))
            .arg("migrate")
            .arg("status")
            .arg(&root)
            .output()
            .expect("rco migrate status should launch");

        assert_run_success_for("rco migrate status", adapter, &output);
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains(target) && stdout.contains("No migration files found"),
            "stdout should identify {adapter} target without requiring a live database, got:\n{stdout}"
        );
        assert!(
            !stdout.contains("secret"),
            "migration status should not echo database credentials, got:\n{stdout}"
        );
    }
}

#[test]
fn new_refuses_non_empty_directory() {
    let source_path = temp_source_path();
    let project_path = source_path
        .parent()
        .expect("source path has parent")
        .join("existing_app");
    fs::create_dir_all(&project_path).expect("project dir should be created");
    fs::write(project_path.join("keep.txt"), "do not overwrite")
        .expect("sentinel should be written");

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("new")
        .arg(&project_path)
        .output()
        .expect("rco new should launch");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !output.status.success(),
        "rco new should fail for non-empty dir\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stderr.contains("already exists and is not empty"),
        "stderr should explain non-empty dir refusal, got:\n{stderr}"
    );
    assert_eq!(
        fs::read_to_string(project_path.join("keep.txt")).expect("sentinel should remain"),
        "do not overwrite"
    );
}

#[test]
fn check_validates_scaffolded_mvc_project() {
    let source_path = temp_source_path();
    let project_path = source_path
        .parent()
        .expect("source path has parent")
        .join("checked_app");

    let new_output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("new")
        .arg(&project_path)
        .output()
        .expect("rco new should launch");
    assert!(
        new_output.status.success(),
        "rco new should succeed before check\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&new_output.stdout),
        String::from_utf8_lossy(&new_output.stderr)
    );

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("check")
        .arg(&project_path)
        .output()
        .expect("rco check should launch");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "rco check failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("checked"),
        "stdout should mention checked project, got:\n{stdout}"
    );
}

#[test]
fn routes_lists_scaffolded_mvc_routes() {
    let source_path = temp_source_path();
    let project_path = source_path
        .parent()
        .expect("source path has parent")
        .join("routes_app");

    let new_output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("new")
        .arg(&project_path)
        .output()
        .expect("rco new should launch");
    assert!(
        new_output.status.success(),
        "rco new should succeed before routes\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&new_output.stdout),
        String::from_utf8_lossy(&new_output.stderr)
    );

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("routes")
        .arg(&project_path)
        .output()
        .expect("rco routes should launch");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "rco routes failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("GET / HomeController#index"),
        "stdout should list home route, got:\n{stdout}"
    );
    assert!(
        stdout.contains("GET /users UserController#index"),
        "stdout should list users route, got:\n{stdout}"
    );
}

#[test]
fn check_reports_invalid_source_file() {
    let source_path = temp_source_path();
    fs::create_dir_all(source_path.parent().expect("source path has parent"))
        .expect("temp source directory should be created");
    fs::write(&source_path, "9223372036854775808").expect("temp source should be written");

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("check")
        .arg(&source_path)
        .output()
        .expect("rco check should launch");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !output.status.success(),
        "rco check should fail\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stderr.contains("invalid number literal"),
        "stderr should include parser error, got:\n{stderr}"
    );
    assert!(
        stderr.contains("main.rco:1:1"),
        "stderr should include source location, got:\n{stderr}"
    );
    assert!(
        stderr.contains("| 9223372036854775808"),
        "stderr should include source line, got:\n{stderr}"
    );
    assert!(
        stderr.contains("| ^"),
        "stderr should include a source caret, got:\n{stderr}"
    );
}

#[test]
fn run_reports_runtime_error_source_context() {
    let source_path = write_source("\"Ada\" 3 less-than?\n");

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg(&source_path)
        .output()
        .expect("rco run should launch");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !output.status.success(),
        "rco run should fail\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stderr.contains("type error in less-than?"),
        "stderr should include runtime error, got:\n{stderr}"
    );
    assert!(
        stderr.contains("main.rco:1:9"),
        "stderr should include runtime source location, got:\n{stderr}"
    );
    assert!(
        stderr.contains("| \"Ada\" 3 less-than?"),
        "stderr should include runtime source line, got:\n{stderr}"
    );
    assert!(
        stderr.contains("help: while executing CallWord(\"less-than?\") in <main>"),
        "stderr should include opcode/frame help, got:\n{stderr}"
    );
}

#[test]
fn doctor_reports_clean_source_file() {
    let source_path = write_source("42\n");

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("doctor")
        .arg(&source_path)
        .output()
        .expect("rco doctor should launch");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "rco doctor should pass\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("OK source compile: single source file compiles"),
        "stdout should include source compile check, got:\n{stdout}"
    );
    assert!(
        stdout.contains("Doctor found no issues."),
        "stdout should include clean summary, got:\n{stdout}"
    );
}

#[test]
fn doctor_reports_invalid_source_file() {
    let source_path = write_source("9223372036854775808\n");

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("doctor")
        .arg(&source_path)
        .output()
        .expect("rco doctor should launch");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !output.status.success(),
        "rco doctor should fail\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stderr.contains("FAIL source compile"),
        "stderr should include failed source check, got:\n{stderr}"
    );
    assert!(
        stderr.contains("invalid number literal"),
        "stderr should include compile diagnostic, got:\n{stderr}"
    );
    assert!(
        stderr.contains("doctor found 1 issue(s)"),
        "stderr should include failure summary, got:\n{stderr}"
    );
}

#[test]
fn doctor_reports_scaffolded_mvc_project_capabilities() {
    let source_path = temp_source_path();
    let project_path = source_path
        .parent()
        .expect("source path has parent")
        .join("doctor_app");

    let new_output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("new")
        .arg(&project_path)
        .output()
        .expect("rco new should launch");
    assert!(
        new_output.status.success(),
        "rco new should succeed before doctor\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&new_output.stdout),
        String::from_utf8_lossy(&new_output.stderr)
    );

    let manifest_path = project_path.join("ricochet.toml");
    let mut manifest = fs::read_to_string(&manifest_path).expect("manifest should exist");
    manifest.push_str(
        r#"

[web.capabilities]
fs_root = "."
allow_env = true
http_allow_hosts = ["127.0.0.1"]
"#,
    );
    fs::write(&manifest_path, manifest).expect("manifest should update");

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("doctor")
        .arg("--capabilities")
        .arg(&project_path)
        .output()
        .expect("rco doctor should launch");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "rco doctor should pass for scaffold\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("OK MVC app build"),
        "stdout should include MVC build check, got:\n{stdout}"
    );
    assert!(
        stdout.contains("Capabilities:"),
        "stdout should include capability summary, got:\n{stdout}"
    );
    assert!(
        stdout.contains("allow_env: true"),
        "stdout should include env capability, got:\n{stdout}"
    );
    assert!(
        stdout.contains("http_allow_hosts"),
        "stdout should include HTTP allowlist capability, got:\n{stdout}"
    );
}

#[test]
fn doctor_reports_package_manifest_without_web_as_package_project() {
    let source_path = temp_source_path();
    let root = source_path.parent().expect("source path has parent");
    write_source_at(root, "ricochet.toml", "[package]\nname = \"package_app\"\n");
    write_source_at(root, "main.rco", "\"hello package\" println\n");

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("doctor")
        .arg(root)
        .output()
        .expect("rco doctor should launch");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "rco doctor should pass for package manifest\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("OK project kind: package/source project"),
        "stdout should identify package/source project, got:\n{stdout}"
    );
    assert!(
        !stdout.contains("routes"),
        "package/source doctor should not run MVC route checks, got:\n{stdout}"
    );
}

#[test]
fn lsp_diagnostics_reports_compile_error_json() {
    let source_path = write_source("9223372036854775808\n");

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("lsp-diagnostics")
        .arg(&source_path)
        .output()
        .expect("rco lsp-diagnostics should launch");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "rco lsp-diagnostics should succeed for diagnostic output\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let payload: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("diagnostics output should be JSON");
    let diagnostics = payload["diagnostics"]
        .as_array()
        .expect("diagnostics should be an array");
    assert_eq!(diagnostics.len(), 1, "stdout:\n{stdout}");
    assert!(
        diagnostics[0]["message"]
            .as_str()
            .expect("message should be string")
            .contains("invalid number literal"),
        "stdout:\n{stdout}"
    );
    assert_eq!(diagnostics[0]["range"]["start"]["line"], 0);
    assert_eq!(diagnostics[0]["range"]["start"]["character"], 0);
    assert_eq!(diagnostics[0]["source"], "ricochet");
}

#[test]
fn lsp_server_initializes_and_publishes_live_diagnostics() {
    let uri = "file:///workspace/Bad.rco";
    let mut input = Vec::new();
    write_lsp_message(
        &mut input,
        &serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}),
    );
    write_lsp_message(
        &mut input,
        &serde_json::json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
    );
    write_lsp_message(
        &mut input,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":uri,"languageId":"ricochet","version":1,"text":"User Model Subclass\n  \"email\" Accessor\n"}}}),
    );
    write_lsp_message(
        &mut input,
        &serde_json::json!({"jsonrpc":"2.0","id":2,"method":"textDocument/completion","params":{"textDocument":{"uri":uri},"position":{"line":1,"character":4}}}),
    );
    write_lsp_message(
        &mut input,
        &serde_json::json!({"jsonrpc":"2.0","id":3,"method":"shutdown","params":null}),
    );
    write_lsp_message(
        &mut input,
        &serde_json::json!({"jsonrpc":"2.0","method":"exit","params":null}),
    );

    let mut child = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("lsp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("rco lsp should launch");

    child
        .stdin
        .take()
        .expect("lsp stdin should be piped")
        .write_all(&input)
        .expect("lsp input should write");

    let output = child.wait_with_output().expect("lsp should finish");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "rco lsp failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let messages = read_lsp_messages(&output.stdout);
    assert!(
        messages.iter().any(|message| message["id"] == 1
            && message["result"]["capabilities"]["semanticTokensProvider"].is_object()),
        "initialize response should advertise semantic tokens\nstdout:\n{stdout}"
    );
    assert!(
        messages.iter().any(
            |message| message["method"] == "textDocument/publishDiagnostics"
                && message["params"]["diagnostics"][0]["message"]
                    == "expected end, found end of file"
        ),
        "didOpen should publish compile diagnostics\nstdout:\n{stdout}"
    );
    assert!(
        messages.iter().any(|message| message["id"] == 2
            && message["result"]["items"]
                .as_array()
                .expect("completion result should contain items")
                .iter()
                .any(|item| item["label"] == "Accessor")),
        "completion response should include Ricochet words\nstdout:\n{stdout}"
    );
}

#[test]
fn repl_preserves_stack_between_submissions() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("repl")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("rco repl should launch");

    child
        .stdin
        .take()
        .expect("repl stdin should be piped")
        .write_all(b"2\n3\n+\n")
        .expect("repl input should write");

    let output = child.wait_with_output().expect("repl should finish");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "rco repl failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("[Number(5)]"),
        "repl should preserve stack across submissions, got:\n{stdout}"
    );
}

#[test]
fn repl_accepts_multiline_class_declarations() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("repl")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("rco repl should launch");

    child
        .stdin
        .take()
        .expect("repl stdin should be piped")
        .write_all(
            br#"User Model Subclass
  "email" Accessor
end
"User" new
"#,
        )
        .expect("repl input should write");

    let output = child.wait_with_output().expect("repl should finish");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "rco repl failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("class_name: \"User\""),
        "repl should instantiate class defined by multiline submission, got:\n{stdout}"
    );
}

#[test]
fn repl_debug_streams_instruction_events() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("repl")
        .arg("--debug")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("rco repl should launch");

    child
        .stdin
        .take()
        .expect("repl stdin should be piped")
        .write_all(b"2 3 +\n")
        .expect("repl input should write");

    let output = child.wait_with_output().expect("repl should finish");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "rco repl failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("TRACE <repl>:1 [<main>]"),
        "debug repl should stream trace events, got:\n{stdout}"
    );
}

#[test]
fn run_prints_final_stack_for_source_file() {
    let source_path = temp_source_path();
    fs::create_dir_all(source_path.parent().expect("source path has parent"))
        .expect("temp source directory should be created");
    fs::write(&source_path, "2 3 +\n").expect("temp source should be written");

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg(&source_path)
        .output()
        .expect("rco run should launch");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "rco run failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("Number(5)") || stdout.contains("[Number(5)]"),
        "stdout should show final stack with Number(5), got:\n{stdout}"
    );
}

#[test]
fn run_executes_basic_oop_script() {
    let source_path = temp_source_path();
    fs::create_dir_all(source_path.parent().expect("source path has parent"))
        .expect("temp source directory should be created");
    fs::write(
        &source_path,
        r#"
User Model Subclass
  "email" Accessor
  [ self email.get ] "displayName" Method
end

"User" new
"ada@example.com" swap email.set
displayName
"#,
    )
    .expect("temp source should be written");

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg(&source_path)
        .output()
        .expect("rco run should launch");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "rco run failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("String(\"ada@example.com\")"),
        "stdout should show final stack with display name, got:\n{stdout}"
    );
}

#[test]
fn run_refreshes_method_locals_between_calls_and_loop_iterations() {
    let source_path = temp_source_path();
    fs::create_dir_all(source_path.parent().expect("source path has parent"))
        .expect("temp source directory should be created");
    fs::write(
        &source_path,
        r#"
LookupProbe Object Subclass
  ( id ) [
    target var
    target get to-string println
  ] "echo" Method

  ( id ) [
    target var
    array xs var
    map a var
    a get "id" "a" put! a set
    xs get a get push! xs set
    map b var
    b get "id" "b" put! b set
    xs get b get push! xs set
    nil found var
    0 index var
    index get xs get count < while
      xs get index get at item var
      item get "id" at currentId var
      currentId get target get = if
        item get found set
        break
      end
      index get 1 + index set
    end
    found get to-string println
  ] "find" Method
end

LookupProbe new probe var
"a" probe get echo
"b" probe get echo
"a" probe get find
"b" probe get find
"z" probe get find
"#,
    )
    .expect("temp source should be written");

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg(&source_path)
        .output()
        .expect("rco run should launch");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "rco run failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    let printed: Vec<&str> = stdout
        .lines()
        .filter(|line| *line == "a" || *line == "b" || *line == "nil" || line.starts_with("Map({"))
        .collect();
    assert_eq!(
        printed,
        vec![
            "a",
            "b",
            "Map({\"id\": String(\"a\")})",
            "Map({\"id\": String(\"b\")})",
            "nil"
        ],
        "stdout should show fresh method locals and loop temps, got:\n{stdout}"
    );
}

#[test]
fn run_executes_postfix_if_else_script() {
    let source_path = temp_source_path();
    fs::create_dir_all(source_path.parent().expect("source path has parent"))
        .expect("temp source directory should be created");
    fs::write(&source_path, r#"false if "yes" else "no" end"#)
        .expect("temp source should be written");

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg(&source_path)
        .output()
        .expect("rco run should launch");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "rco run failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("String(\"no\")"),
        "stdout should show final stack with else result, got:\n{stdout}"
    );
}

#[test]
fn run_executes_comparison_condition_script() {
    let source_path = temp_source_path();
    fs::create_dir_all(source_path.parent().expect("source path has parent"))
        .expect("temp source directory should be created");
    fs::write(&source_path, r#"2 3 < if "lt" else "ge" end"#)
        .expect("temp source should be written");

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg(&source_path)
        .output()
        .expect("rco run should launch");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "rco run failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("String(\"lt\")"),
        "stdout should show final stack with comparison branch result, got:\n{stdout}"
    );
}

#[test]
fn run_executes_map_put_script() {
    let source_path = temp_source_path();
    fs::create_dir_all(source_path.parent().expect("source path has parent"))
        .expect("temp source directory should be created");
    fs::write(&source_path, r#"map "name" "Ada" put! "name" at"#)
        .expect("temp source should be written");

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg(&source_path)
        .output()
        .expect("rco run should launch");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "rco run failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("String(\"Ada\")"),
        "stdout should show final stack with map entry, got:\n{stdout}"
    );
}

#[test]
fn run_executes_println_script() {
    let source_path = temp_source_path();
    fs::create_dir_all(source_path.parent().expect("source path has parent"))
        .expect("temp source directory should be created");
    fs::write(&source_path, r#""Hello Ricochet" println"#).expect("temp source should be written");

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg(&source_path)
        .output()
        .expect("rco run should launch");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "rco run failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.lines().any(|line| line == "Hello Ricochet"),
        "stdout should include println output, got:\n{stdout}"
    );
    assert!(
        stdout.contains("[]"),
        "stdout should show final empty stack after println consumes value, got:\n{stdout}"
    );
}

#[test]
fn fmt_check_reports_unformatted_source() {
    let source_path = write_source(
        r#"
User Model Subclass
"email" Accessor
[ self email.get ] "label" Method
end
"#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("fmt")
        .arg("--check")
        .arg(&source_path)
        .output()
        .expect("rco fmt should launch");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !output.status.success(),
        "rco fmt --check should fail for unformatted source\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stderr.contains("would reformat"),
        "stderr should explain check failure, got:\n{stderr}"
    );
}

#[test]
fn fmt_rewrites_source_file() {
    let source_path = write_source(
        r#"
User Model Subclass
"email" Accessor
[ self email.get ] "label" Method
end
"#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("fmt")
        .arg(&source_path)
        .output()
        .expect("rco fmt should launch");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "rco fmt failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let formatted = fs::read_to_string(&source_path).expect("formatted source should be readable");
    assert_eq!(
        formatted,
        "User Model Subclass\n  \"email\" Accessor\n  [\n    self email.get\n  ] \"label\" Method\nend\n"
    );
}

#[test]
fn run_loads_static_string_imports_before_main_source() {
    let main_path = temp_source_path();
    let root = main_path.parent().expect("source path has parent");
    write_source_at(root, "lib/math.rco", "\"triple\" function\n  3 *\nend\n");
    write_source_at(root, "main.rco", "\"lib/math\" import\n7 triple\n");

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg(&main_path)
        .output()
        .expect("rco run should launch");

    assert_run_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Number(21)"),
        "stdout should show imported function result, got:\n{stdout}"
    );
}

#[test]
fn run_reads_variables_with_dollar_reference_prefix() {
    let source_path = write_source(
        r#"
"users" name var
$name array
$users "Ada" push! drop
$users count println
$users 0 at println
"#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg(&source_path)
        .output()
        .expect("rco run should launch");

    assert_run_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("1\nAda"),
        "stdout should show values read through $ references, got:\n{stdout}"
    );
}

#[test]
fn run_rejects_absolute_static_imports() {
    let main_path = temp_source_path();
    let root = main_path.parent().expect("source path has parent");
    write_source_at(root, "lib/math.rco", "\"triple\" function\n  3 *\nend\n");
    let absolute = path_to_slash_for_test(&root.join("lib/math.rco"));
    write_source_at(
        root,
        "main.rco",
        &format!("\"{absolute}\" import\n7 triple\n"),
    );

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg(&main_path)
        .output()
        .expect("rco run should launch");

    assert!(
        !output.status.success(),
        "rco run should reject absolute imports"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("absolute imports are not allowed"),
        "stderr should explain absolute import rejection, got:\n{stderr}"
    );
}

#[test]
fn run_rejects_static_import_parent_escape() {
    let main_path = temp_source_path();
    let root = main_path.parent().expect("source path has parent");
    write_source_at(
        root,
        "outside.rco",
        "\"outside\" function\n  \"outside\"\nend\n",
    );
    write_source_at(root, "app/main.rco", "\"../outside\" import\noutside\n");

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg(root.join("app/main.rco"))
        .output()
        .expect("rco run should launch");

    assert!(
        !output.status.success(),
        "rco run should reject parent imports"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("resolves outside allowed root"),
        "stderr should explain parent import rejection, got:\n{stderr}"
    );
}

#[test]
fn run_rejects_package_import_parent_escape() {
    let main_path = temp_source_path();
    let root = main_path.parent().expect("source path has parent");
    write_source_at(
        root,
        "ricochet.toml",
        "[package]\nname = \"app\"\n\n[dependencies.greeter]\npath = \"./packages/greeter\"\n",
    );
    write_source_at(
        root,
        "packages/greeter/ricochet.toml",
        "[package]\nname = \"greeter\"\n",
    );
    write_source_at(
        root,
        "secret.rco",
        "\"secret\" function\n  \"secret\"\nend\n",
    );
    write_source_at(root, "main.rco", "\"greeter/../secret\" import\nsecret\n");

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg("main.rco")
        .current_dir(root)
        .output()
        .expect("rco run should launch");

    assert!(
        !output.status.success(),
        "rco run should reject package parent imports"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("import path must not contain . or .. components"),
        "stderr should explain package import rejection, got:\n{stderr}"
    );
}

#[test]
fn run_rejects_manifest_dependency_paths_outside_project_root() {
    let main_path = temp_source_path();
    let root = main_path.parent().expect("source path has parent");
    write_source_at(
        root,
        "ricochet.toml",
        "[package]\nname = \"app\"\n\n[dependencies.greeter]\npath = \"../outside\"\n",
    );
    write_source_at(
        root,
        "main.rco",
        "\"greeter/greeting\" import\npackageHello\n",
    );

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg("main.rco")
        .current_dir(root)
        .output()
        .expect("rco run should launch");

    assert!(
        !output.status.success(),
        "rco run should reject outside dependency paths"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("dependency \"greeter\" path must not contain .. components"),
        "stderr should explain outside dependency path rejection, got:\n{stderr}"
    );
}

#[test]
fn add_records_local_path_dependency_and_package_imports_are_runnable() {
    let main_path = temp_source_path();
    let root = main_path.parent().expect("source path has parent");
    write_source_at(root, "ricochet.toml", "[package]\nname = \"app\"\n");
    write_source_at(
        root,
        "packages/greeter/ricochet.toml",
        "[package]\nname = \"greeter\"\n",
    );
    write_source_at(
        root,
        "packages/greeter/greeting.rco",
        "\"packageHello\" function\n  \"hello from package\"\nend\n",
    );
    write_source_at(
        root,
        "main.rco",
        "\"greeter/greeting\" import\npackageHello\n",
    );

    let add_output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("add")
        .arg("./packages/greeter")
        .current_dir(root)
        .output()
        .expect("rco add should launch");
    assert_run_success_for("rco add", "./packages/greeter", &add_output);
    let add_stdout = String::from_utf8_lossy(&add_output.stdout);
    assert!(
        add_stdout.contains("added greeter"),
        "stdout should mention added package, got:\n{add_stdout}"
    );

    let manifest = fs::read_to_string(root.join("ricochet.toml")).expect("manifest should exist");
    assert!(manifest.contains("[dependencies.greeter]"));
    assert!(manifest.contains("path = \"./packages/greeter\""));

    let lock = fs::read_to_string(root.join("ricochet.lock")).expect("lockfile should exist");
    assert!(lock.contains("[package.greeter]"));
    assert!(lock.contains("source = \"path+./packages/greeter\""));
    assert!(lock.contains("integrity = \"sha256:"));

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg("main.rco")
        .current_dir(root)
        .output()
        .expect("rco run should launch");

    assert_run_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("String(\"hello from package\")"),
        "stdout should show imported package result, got:\n{stdout}"
    );
}

#[test]
fn add_records_github_dependency_link_without_fetching() {
    let main_path = temp_source_path();
    let root = main_path.parent().expect("source path has parent");
    write_source_at(root, "ricochet.toml", "[package]\nname = \"app\"\n");

    let add_output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("add")
        .arg("github:BARKx4/ricochet_auth@v0.1.0")
        .arg("--no-fetch")
        .current_dir(root)
        .output()
        .expect("rco add should launch");
    assert_run_success_for("rco add", "github:BARKx4/ricochet_auth@v0.1.0", &add_output);

    let manifest = fs::read_to_string(root.join("ricochet.toml")).expect("manifest should exist");
    assert!(manifest.contains("[dependencies.ricochet_auth]"));
    assert!(manifest.contains("git = \"https://github.com/BARKx4/ricochet_auth.git\""));
    assert!(manifest.contains("rev = \"v0.1.0\""));
    assert!(manifest.contains("path = \".ricochet/packages/ricochet_auth\""));

    assert!(
        !root.join("ricochet.lock").exists(),
        "--no-fetch cannot create an immutable lock entry before rco install resolves a commit"
    );
}

#[test]
fn install_locks_existing_local_path_dependencies() {
    let main_path = temp_source_path();
    let root = main_path.parent().expect("source path has parent");
    write_source_at(
        root,
        "ricochet.toml",
        "[package]\nname = \"app\"\n\n[dependencies.greeter]\npath = \"./packages/greeter\"\n",
    );
    write_source_at(
        root,
        "packages/greeter/ricochet.toml",
        "[package]\nname = \"greeter\"\n",
    );

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("install")
        .current_dir(root)
        .output()
        .expect("rco install should launch");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "rco install failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("installed greeter from ./packages/greeter"),
        "stdout should describe installed local dependency, got:\n{stdout}"
    );

    let lock = fs::read_to_string(root.join("ricochet.lock")).expect("lockfile should exist");
    assert!(lock.contains("[package.greeter]"));
    assert!(lock.contains("source = \"path+./packages/greeter\""));
    assert!(lock.contains("path = \"./packages/greeter\""));
    assert!(lock.contains("integrity = \"sha256:"));
}

#[test]
fn install_uses_locked_git_commit_when_branch_moves() {
    let main_path = temp_source_path();
    let base = main_path.parent().expect("source path has parent");
    let upstream = base.join("upstream_greeter");
    fs::create_dir_all(&upstream).expect("upstream repo should be created");
    run_git(&upstream, &["init", "-b", "main"]);
    run_git(
        &upstream,
        &["config", "user.email", "ricochet@example.test"],
    );
    run_git(&upstream, &["config", "user.name", "Ricochet Test"]);
    write_source_at(
        &upstream,
        "greeting.rco",
        "\"packageHello\" function\n  \"hello from first commit\"\nend\n",
    );
    run_git(&upstream, &["add", "."]);
    run_git(&upstream, &["commit", "-m", "first"]);
    let first_commit = git_stdout(&upstream, &["rev-parse", "HEAD"]);

    let app_one = base.join("app_one");
    let git_source = escape_toml_string(&path_to_slash_for_test(&upstream));
    write_source_at(
        &app_one,
        "ricochet.toml",
        &format!(
            "[package]\nname = \"app\"\n\n[dependencies.greeter]\npath = \".ricochet/packages/greeter\"\ngit = \"{git_source}\"\nrev = \"main\"\n"
        ),
    );
    write_source_at(
        &app_one,
        "main.rco",
        "\"greeter/greeting\" import\npackageHello\n",
    );

    let first_install = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("install")
        .current_dir(&app_one)
        .output()
        .expect("rco install should launch");
    assert_run_success_for("rco install", "first locked git install", &first_install);
    let first_lock = fs::read_to_string(app_one.join("ricochet.lock")).expect("lock should exist");
    assert!(
        first_lock.contains(&format!("commit = \"{first_commit}\"")),
        "lock should pin the first commit, got:\n{first_lock}"
    );
    assert!(
        first_lock.contains("integrity = \"sha256:"),
        "lock should pin package contents, got:\n{first_lock}"
    );

    fs::write(
        upstream.join("greeting.rco"),
        "\"packageHello\" function\n  \"hello from moved branch\"\nend\n",
    )
    .expect("upstream source should update");
    run_git(&upstream, &["add", "."]);
    run_git(&upstream, &["commit", "-m", "second"]);

    let app_two = base.join("app_two");
    write_source_at(
        &app_two,
        "ricochet.toml",
        &fs::read_to_string(app_one.join("ricochet.toml")).expect("manifest should exist"),
    );
    write_source_at(
        &app_two,
        "ricochet.lock",
        &fs::read_to_string(app_one.join("ricochet.lock")).expect("lock should exist"),
    );
    write_source_at(
        &app_two,
        "main.rco",
        "\"greeter/greeting\" import\npackageHello\n",
    );

    let second_install = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("install")
        .current_dir(&app_two)
        .output()
        .expect("rco install should launch");
    assert_run_success_for("rco install", "second locked git install", &second_install);

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg("main.rco")
        .current_dir(&app_two)
        .output()
        .expect("rco run should launch");

    assert_run_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("String(\"hello from first commit\")")
            && !stdout.contains("hello from moved branch"),
        "locked clean install should use the original commit, got:\n{stdout}"
    );
}

#[test]
fn install_rejects_git_dependency_path_outside_project_root() {
    let main_path = temp_source_path();
    let root = main_path.parent().expect("source path has parent");
    write_source_at(
        root,
        "ricochet.toml",
        "[package]\nname = \"app\"\n\n[dependencies.greeter]\npath = \"../outside\"\ngit = \"https://github.com/example/greeter.git\"\nrev = \"main\"\n",
    );

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("install")
        .current_dir(root)
        .output()
        .expect("rco install should launch");

    assert!(
        !output.status.success(),
        "rco install should reject outside git package paths"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("git package cache path must not contain .. components"),
        "stderr should explain rejected package path, got:\n{stderr}"
    );
    assert!(
        !root.join("..").join("outside").exists(),
        "rejected install must not create the outside package directory"
    );
}

#[test]
fn verify_reports_clean_local_dependency_lock() {
    let main_path = temp_source_path();
    let root = main_path.parent().expect("source path has parent");
    write_source_at(
        root,
        "ricochet.toml",
        "[package]\nname = \"app\"\n\n[dependencies.greeter]\npath = \"./packages/greeter\"\n",
    );
    write_source_at(
        root,
        "packages/greeter/ricochet.toml",
        "[package]\nname = \"greeter\"\n",
    );

    let install = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("install")
        .current_dir(root)
        .output()
        .expect("rco install should launch");
    assert_run_success_for("rco install", "verify fixture install", &install);

    let verify = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("verify")
        .current_dir(root)
        .output()
        .expect("rco verify should launch");
    assert_run_success_for("rco verify", "local dependency lock", &verify);
    let stdout = String::from_utf8_lossy(&verify.stdout);
    assert!(
        stdout.contains("verified greeter") && stdout.contains("verified 1 dependencies"),
        "stdout should report verified dependency, got:\n{stdout}"
    );
}

#[test]
fn verify_rejects_changed_package_integrity() {
    let main_path = temp_source_path();
    let root = main_path.parent().expect("source path has parent");
    write_source_at(
        root,
        "ricochet.toml",
        "[package]\nname = \"app\"\n\n[dependencies.greeter]\npath = \"./packages/greeter\"\n",
    );
    write_source_at(
        root,
        "packages/greeter/ricochet.toml",
        "[package]\nname = \"greeter\"\n",
    );
    write_source_at(
        root,
        "packages/greeter/greeting.rco",
        "\"packageHello\" function\n  \"hello from package\"\nend\n",
    );

    let install = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("install")
        .current_dir(root)
        .output()
        .expect("rco install should launch");
    assert_run_success_for("rco install", "integrity fixture install", &install);

    fs::write(
        root.join("packages/greeter/greeting.rco"),
        "\"packageHello\" function\n  \"tampered\"\nend\n",
    )
    .expect("package file should mutate");

    let verify = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("verify")
        .current_dir(root)
        .output()
        .expect("rco verify should launch");

    assert!(
        !verify.status.success(),
        "rco verify should reject package content drift"
    );
    let stderr = String::from_utf8_lossy(&verify.stderr);
    assert!(
        stderr.contains("package integrity for greeter changed"),
        "stderr should explain integrity mismatch, got:\n{stderr}"
    );
}

#[test]
fn verify_rejects_git_dependency_without_lock() {
    let main_path = temp_source_path();
    let root = main_path.parent().expect("source path has parent");
    write_source_at(
        root,
        "ricochet.toml",
        "[package]\nname = \"app\"\n\n[dependencies.greeter]\npath = \".ricochet/packages/greeter\"\ngit = \"https://github.com/example/greeter.git\"\nrev = \"main\"\n",
    );

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("verify")
        .current_dir(root)
        .output()
        .expect("rco verify should launch");

    assert!(
        !output.status.success(),
        "rco verify should reject unlocked git dependencies"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("dependency greeter is missing from") && stderr.contains("run rco install"),
        "stderr should explain missing lock entry, got:\n{stderr}"
    );
}

#[test]
fn doc_generates_markdown_for_declarations_and_doc_comments() {
    let source_path = temp_source_path();
    let root = source_path.parent().expect("source path has parent");
    write_source_at(
        root,
        "models/user.rco",
        r#"
(( User records from an existing table. ))
User Model Subclass
  (( users table mapping ))
  "users" Table

  (( Primary email address. ))
  "email" Accessor

  (( Display name fallback. ))
  [
    self email.get
  ] "displayName" Method
end

(( Formats a greeting. ))
greeting function
  "hello"
end
"#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("doc")
        .arg(root.join("models"))
        .output()
        .expect("rco doc should launch");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "rco doc failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(stdout.contains("# Ricochet Documentation"));
    assert!(stdout.contains("## Class `User`"));
    assert!(stdout.contains("User records from an existing table."));
    assert!(stdout.contains("- Table: `users`"));
    assert!(stdout.contains("- Accessor: `email`"));
    assert!(stdout.contains("Primary email address."));
    assert!(stdout.contains("- Method: `displayName`"));
    assert!(stdout.contains("Display name fallback."));
    assert!(stdout.contains("## Function `greeting`"));
    assert!(stdout.contains("Formats a greeting."));
}

#[test]
fn run_bytecode_executes_built_chunk() {
    let main_path = temp_source_path();
    let root = main_path.parent().expect("source path has parent");
    write_source_at(root, "main.rco", "8 5 +\n");

    let build_output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("build")
        .arg("main.rco")
        .current_dir(root)
        .output()
        .expect("rco build should launch");
    assert_run_success_for("rco build", "main.rco", &build_output);

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run-bytecode")
        .arg(root.join("build").join("app.rcob"))
        .output()
        .expect("rco run-bytecode should launch");

    assert_run_success_for("rco run-bytecode", "build/app.rcob", &output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Number(13)"),
        "stdout should show bytecode result, got:\n{stdout}"
    );
}

#[test]
fn package_creates_standalone_executable_that_runs_embedded_bytecode() {
    let main_path = temp_source_path();
    let root = main_path.parent().expect("source path has parent");
    write_source_at(root, "main.rco", "\"packaged\" println\n20 2 +\n");
    let output_path = root.join(format!("hello-app{}", std::env::consts::EXE_SUFFIX));

    let package_output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("package")
        .arg("main.rco")
        .arg("--output")
        .arg(&output_path)
        .current_dir(root)
        .output()
        .expect("rco package should launch");
    assert_run_success_for("rco package", "main.rco", &package_output);

    let output = Command::new(&output_path)
        .output()
        .expect("packaged Ricochet executable should launch");
    assert_run_success_for("packaged executable", "hello-app", &output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.lines().any(|line| line == "packaged") && stdout.contains("Number(22)"),
        "stdout should show embedded app output and stack, got:\n{stdout}"
    );
}

#[test]
fn tui_command_runs_without_printing_final_stack() {
    let source_path = write_source("\"TUI beta\" tui_write value drop tui_flush value drop\n");

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("tui")
        .arg(&source_path)
        .output()
        .expect("rco tui should launch");

    assert_run_success_for("rco tui", "source", &output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        stdout, "TUI beta",
        "rco tui should write terminal UI output without a final stack dump"
    );
}

#[test]
fn tui_capability_respects_sandbox_flags() {
    let source_path = write_source("\"blocked\" tui_write value drop tui_flush value drop\n");

    let blocked = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg("--capability-profile")
        .arg("sandboxed")
        .arg(&source_path)
        .output()
        .expect("rco run should launch");

    assert!(
        !blocked.status.success(),
        "sandboxed rco run should deny TUI capability by default"
    );
    let stderr = String::from_utf8_lossy(&blocked.stderr);
    assert!(
        stderr.contains("terminal UI capability is not enabled"),
        "stderr should explain disabled TUI, got:\n{stderr}"
    );

    let allowed = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg("--capability-profile")
        .arg("sandboxed")
        .arg("--allow-tui")
        .arg(&source_path)
        .output()
        .expect("rco run should launch");

    assert_run_success_for("rco run --allow-tui", "source", &allowed);
    let stdout = String::from_utf8_lossy(&allowed.stdout);
    assert!(
        stdout.contains("blocked") && stdout.contains("[]"),
        "allowed sandboxed run should write TUI output and still print final stack, got:\n{stdout}"
    );
}

#[test]
fn package_tui_creates_terminal_executable_without_final_stack() {
    let main_path = temp_source_path();
    let root = main_path.parent().expect("source path has parent");
    write_source_at(
        root,
        "main.rco",
        "\"Packaged TUI\" tui_write value drop tui_flush value drop\n",
    );
    let output_path = root.join(format!("tui-app{}", std::env::consts::EXE_SUFFIX));

    let package_output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("package")
        .arg("main.rco")
        .arg("--tui")
        .arg("--output")
        .arg(&output_path)
        .current_dir(root)
        .output()
        .expect("rco package --tui should launch");
    assert_run_success_for("rco package --tui", "main.rco", &package_output);

    let output = Command::new(&output_path)
        .output()
        .expect("packaged Ricochet TUI executable should launch");
    assert_run_success_for("packaged TUI executable", "tui-app", &output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        stdout, "Packaged TUI",
        "packaged TUI should not print a final stack dump"
    );
}

#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
#[test]
fn package_gui_creates_standalone_executable_that_exports_webview_document() {
    let main_path = temp_source_path();
    let root = main_path.parent().expect("source path has parent");
    write_source_at(
        root,
        "main.rco",
        "\"GUI Smoke\" \"<main><p>Hello desktop</p></main>\" webview_window value document var\n",
    );
    let output_path = root.join(format!("gui-app{}", std::env::consts::EXE_SUFFIX));
    let preview_export_path = root.join("gui-preview.html");
    let package_export_path = root.join("gui-package.html");

    let preview_output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("gui")
        .arg("main.rco")
        .env("RICOCHET_GUI_EXPORT_HTML", &preview_export_path)
        .current_dir(root)
        .output()
        .expect("rco gui should launch");
    assert_run_success_for("rco gui", "main.rco", &preview_output);
    let preview_html = fs::read_to_string(&preview_export_path).expect("preview HTML should exist");
    assert!(preview_html.contains("<title>GUI Smoke</title>"));
    assert!(preview_html.contains("Hello desktop"));

    let package_output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("package")
        .arg("main.rco")
        .arg("--gui")
        .arg("--gui-launcher")
        .arg(env!("CARGO_BIN_EXE_rco-gui"))
        .arg("--output")
        .arg(&output_path)
        .current_dir(root)
        .output()
        .expect("rco package --gui should launch");
    assert_run_success_for("rco package --gui", "main.rco", &package_output);

    let output = Command::new(&output_path)
        .env("RICOCHET_GUI_EXPORT_HTML", &package_export_path)
        .output()
        .expect("packaged Ricochet GUI executable should launch");
    assert_run_success_for("packaged GUI executable", "gui-app", &output);

    let html =
        fs::read_to_string(package_export_path).expect("packaged GUI HTML should be exported");
    assert!(
        html.contains("<title>GUI Smoke</title>") && html.contains("Hello desktop"),
        "exported HTML should include the GUI document, got:\n{html}"
    );
}

#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
#[test]
fn package_mvc_gui_creates_standalone_executable_that_exports_root_route() {
    let main_path = temp_source_path();
    let root = main_path.parent().expect("source path has parent");
    let project_path = root.join("mvc_app");
    let output_path = root.join(format!("mvc-app{}", std::env::consts::EXE_SUFFIX));
    let package_export_path = root.join("mvc-package.html");
    let asset_export_path = root.join("mvc-package.css");
    let fs_export_path = root.join("mvc-package-fs.json");
    let caps_export_path = root.join("mvc-package-caps.json");

    let new_output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("new")
        .arg(&project_path)
        .output()
        .expect("rco new should launch");
    assert_run_success_for("rco new", "mvc_app", &new_output);

    let manifest_path = project_path.join("ricochet.toml");
    let manifest = fs::read_to_string(&manifest_path).expect("manifest should be readable");
    fs::write(
        &manifest_path,
        format!(
            r#"{manifest}

[web.capabilities]
fs_root = "."
env_allow = ["RICOCHET_MVC_PACKAGE_ENV_TEST"]
allow_process = true
process_root = "."
allow_pty = true
http_allow_hosts = ["127.0.0.1"]
"#
        ),
    )
    .expect("manifest should declare packaged capabilities");

    fs::write(
        project_path.join("config").join("routes.rco"),
        r#"GET "/" HomeController "index" route
GET "/users" UserController "index" route
GET "/fs-check" FsCheckController "show" route
GET "/caps" CapabilityController "show" route
"#,
    )
    .expect("routes should be extended");
    fs::write(
        project_path
            .join("app")
            .join("Controllers")
            .join("FsCheckController.rco"),
        r#"FsCheckController Controller Subclass
  [
    "ricochet.toml" fs_exists? exists var
    map data var
    data get "exists" exists get put! data set
    data get json
  ] "show" Method
end
"#,
    )
    .expect("fs check controller should be written");
    fs::write(
        project_path
            .join("app")
            .join("Controllers")
            .join("CapabilityController.rco"),
        r#"CapabilityController Controller Subclass
  [
    "RICOCHET_MVC_PACKAGE_ENV_TEST" env value envValue var
    runtime_capabilities caps var
    map data var
    data get "env" envValue get put! data set
    data get "fs_enabled" caps get "filesystem" at "enabled" at put! data set
    data get "process_enabled" caps get "process" at "enabled" at put! data set
    data get "pty_enabled" caps get "pty" at "enabled" at put! data set
    data get "http_enabled" caps get "http" at "enabled" at put! data set
    data get json
  ] "show" Method
end
"#,
    )
    .expect("capability controller should be written");

    let package_output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("package")
        .arg(&project_path)
        .arg("--gui")
        .arg("--mvc")
        .arg("--gui-launcher")
        .arg(env!("CARGO_BIN_EXE_rco-gui"))
        .arg("--output")
        .arg(&output_path)
        .current_dir(root)
        .output()
        .expect("rco package --gui --mvc should launch");
    assert_run_success_for("rco package --gui --mvc", "mvc_app", &package_output);

    let output = Command::new(&output_path)
        .env("RICOCHET_GUI_EXPORT_HTML", &package_export_path)
        .output()
        .expect("packaged Ricochet MVC GUI executable should launch");
    assert_run_success_for("packaged MVC GUI executable", "mvc-app", &output);

    let html =
        fs::read_to_string(package_export_path).expect("packaged MVC GUI HTML should be exported");
    assert!(
        html.contains("<h1>Hello Ricochet</h1>"),
        "exported MVC HTML should include the scaffolded root route, got:\n{html}"
    );

    let output = Command::new(&output_path)
        .env("RICOCHET_GUI_EXPORT_HTML", &asset_export_path)
        .env("RICOCHET_GUI_EXPORT_PATH", "/assets/app.css")
        .output()
        .expect("packaged Ricochet MVC GUI executable should export static asset");
    assert_run_success_for(
        "packaged MVC GUI executable",
        "mvc-app static asset",
        &output,
    );

    let css =
        fs::read_to_string(asset_export_path).expect("packaged MVC CSS asset should be exported");
    assert!(
        css.contains("font-family: system-ui"),
        "exported MVC CSS should include scaffolded stylesheet, got:\n{css}"
    );

    let output = Command::new(&output_path)
        .env("RICOCHET_GUI_EXPORT_HTML", &fs_export_path)
        .env("RICOCHET_GUI_EXPORT_PATH", "/fs-check")
        .output()
        .expect("packaged Ricochet MVC GUI executable should export filesystem route");
    assert_run_success_for(
        "packaged MVC GUI executable",
        "mvc-app filesystem route",
        &output,
    );

    let fs_json =
        fs::read_to_string(fs_export_path).expect("packaged MVC fs route should be exported");
    assert!(
        fs_json.contains("\"exists\":true"),
        "packaged MVC fs route should have app-local filesystem capability, got:\n{fs_json}"
    );

    let output = Command::new(&output_path)
        .env("RICOCHET_GUI_EXPORT_HTML", &caps_export_path)
        .env("RICOCHET_GUI_EXPORT_PATH", "/caps")
        .env("RICOCHET_MVC_PACKAGE_ENV_TEST", "package-visible")
        .output()
        .expect("packaged Ricochet MVC GUI executable should export capability route");
    assert_run_success_for(
        "packaged MVC GUI executable",
        "mvc-app manifest capability route",
        &output,
    );

    let caps_json = fs::read_to_string(caps_export_path)
        .expect("packaged MVC capability route should be exported");
    for expected in [
        "\"env\":\"package-visible\"",
        "\"fs_enabled\":true",
        "\"process_enabled\":true",
        "\"pty_enabled\":true",
        "\"http_enabled\":true",
    ] {
        assert!(
            caps_json.contains(expected),
            "packaged MVC caps route should contain {expected}, got:\n{caps_json}"
        );
    }
}

#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
#[test]
fn package_mvc_rejects_capability_root_outside_bundle() {
    let main_path = temp_source_path();
    let root = main_path.parent().expect("source path has parent");
    let project_path = root.join("mvc_escape_app");
    let output_path = root.join(format!("mvc-escape-app{}", std::env::consts::EXE_SUFFIX));

    let new_output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("new")
        .arg(&project_path)
        .output()
        .expect("rco new should launch");
    assert_run_success_for("rco new", "mvc_escape_app", &new_output);

    let manifest_path = project_path.join("ricochet.toml");
    let mut manifest = fs::read_to_string(&manifest_path).expect("manifest should exist");
    manifest.push_str(
        r#"

[web.capabilities]
fs_root = ".."
"#,
    );
    fs::write(&manifest_path, manifest).expect("manifest should update");

    let package_output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("package")
        .arg(&project_path)
        .arg("--gui")
        .arg("--mvc")
        .arg("--gui-launcher")
        .arg(env!("CARGO_BIN_EXE_rco-gui"))
        .arg("--output")
        .arg(&output_path)
        .output()
        .expect("rco package --gui --mvc should launch");

    assert!(
        !package_output.status.success(),
        "rco package --mvc should reject escaped capability roots"
    );
    let stderr = String::from_utf8_lossy(&package_output.stderr);
    assert!(
        stderr.contains("web.capabilities.fs_root path must not contain .. components"),
        "stderr should explain escaped capability root, got:\n{stderr}"
    );
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
#[test]
fn package_gui_rejects_unsupported_hosts() {
    let main_path = temp_source_path();
    let root = main_path.parent().expect("source path has parent");
    write_source_at(
        root,
        "main.rco",
        "\"GUI Smoke\" \"<main><p>Hello desktop</p></main>\" webview_window\n",
    );
    let output_path = root.join("gui-app");

    let package_output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("package")
        .arg("main.rco")
        .arg("--gui")
        .arg("--output")
        .arg(&output_path)
        .current_dir(root)
        .output()
        .expect("rco package --gui should launch");

    assert!(
        !package_output.status.success(),
        "rco package --gui should reject unsupported hosts"
    );
    let stderr = String::from_utf8_lossy(&package_output.stderr);
    assert!(
        stderr.contains(
            "rco package --gui is currently available from Windows, Linux, and macOS builds"
        ),
        "stderr should explain the GUI host requirement, got:\n{stderr}"
    );
}

#[test]
fn package_rejects_linux_package_artifacts_on_non_linux_hosts() {
    if cfg!(target_os = "linux") {
        return;
    }

    let main_path = temp_source_path();
    let root = main_path.parent().expect("source path has parent");
    write_source_at(root, "main.rco", "\"packaged\" println\n20 2 +\n");
    let output_path = root.join(format!("hello-app{}", std::env::consts::EXE_SUFFIX));

    let package_output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("package")
        .arg("main.rco")
        .arg("--output")
        .arg(&output_path)
        .arg("--linux-package")
        .arg("tar")
        .current_dir(root)
        .output()
        .expect("rco package should launch");

    assert!(
        !package_output.status.success(),
        "rco package should reject Linux artifacts on non-Linux hosts"
    );
    let stderr = String::from_utf8_lossy(&package_output.stderr);
    assert!(
        stderr.contains("Linux package artifacts can only be built on Linux"),
        "stderr should explain the Linux host requirement, got:\n{stderr}"
    );
    assert!(
        !output_path.exists(),
        "rejected Linux package request should not create the executable"
    );
}

#[test]
fn run_debug_prints_readable_stack_trace() {
    let source_path = temp_source_path();
    fs::create_dir_all(source_path.parent().expect("source path has parent"))
        .expect("temp source directory should be created");
    fs::write(&source_path, "2 3 +\n").expect("temp source should be written");

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg("--debug")
        .arg(&source_path)
        .output()
        .expect("rco run should launch");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "rco run failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("TRACE "),
        "stdout should include trace lines, got:\n{stdout}"
    );
    assert!(
        stdout.contains("CallWord(\"+\")"),
        "stdout should include opcode, got:\n{stdout}"
    );
    assert!(
        stdout.contains("before: [Number(2), Number(3)]"),
        "stdout should include stack before +, got:\n{stdout}"
    );
    assert!(
        stdout.contains("after:  [Number(5)]"),
        "stdout should include stack after +, got:\n{stdout}"
    );
    assert!(
        !stdout.contains("DebugEvent"),
        "stdout should not expose raw Rust debug event names, got:\n{stdout}"
    );
}

#[test]
fn run_debug_prints_fault_trace_before_error() {
    let source_path = temp_source_path();
    fs::create_dir_all(source_path.parent().expect("source path has parent"))
        .expect("temp source directory should be created");
    fs::write(&source_path, "1 +\n").expect("temp source should be written");

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg("--debug")
        .arg(&source_path)
        .output()
        .expect("rco run should launch");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !output.status.success(),
        "rco run should fail\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("FAULT "),
        "stdout should include a fault trace, got:\n{stdout}"
    );
    assert!(
        stdout.contains("stack underflow in +"),
        "stdout should include the VM fault message, got:\n{stdout}"
    );
    assert!(
        stdout.contains("stack:  [Number(1)]"),
        "stdout should include preserved fault stack, got:\n{stdout}"
    );
    assert!(
        stderr.contains("stack underflow in +")
            && stderr.contains("1 | 1 +")
            && stderr.contains("help: while executing"),
        "stderr should include source-aware runtime error, got:\n{stderr}"
    );
}

#[test]
fn run_debug_step_can_abort_before_execution() {
    let source_path = temp_source_path();
    fs::create_dir_all(source_path.parent().expect("source path has parent"))
        .expect("temp source directory should be created");
    fs::write(&source_path, "2 3 +\n").expect("temp source should be written");

    let mut child = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg("--debug")
        .arg("--step")
        .arg(&source_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("rco run debugger should launch");

    child
        .stdin
        .take()
        .expect("debugger stdin should be piped")
        .write_all(b"abort\n")
        .expect("debugger command should write");

    let output = child.wait_with_output().expect("debugger should finish");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !output.status.success(),
        "debugger abort should fail run\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("PAUSE step"),
        "stdout should include step pause, got:\n{stdout}"
    );
    assert!(
        stderr.contains("execution aborted"),
        "stderr should include abort error, got:\n{stderr}"
    );
}

#[test]
fn run_debug_breakpoint_can_continue_to_completion() {
    let source_path = temp_source_path();
    fs::create_dir_all(source_path.parent().expect("source path has parent"))
        .expect("temp source directory should be created");
    fs::write(&source_path, "2\n3\n+\n").expect("temp source should be written");

    let mut child = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg("--debug")
        .arg("--breakpoint")
        .arg("2")
        .arg(&source_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("rco run debugger should launch");

    child
        .stdin
        .take()
        .expect("debugger stdin should be piped")
        .write_all(b"continue\n")
        .expect("debugger command should write");

    let output = child.wait_with_output().expect("debugger should finish");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "breakpoint continue should succeed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("PAUSE breakpoint"),
        "stdout should include breakpoint pause, got:\n{stdout}"
    );
    assert!(
        stdout.contains("[Number(5)]"),
        "stdout should include final stack, got:\n{stdout}"
    );
}

#[test]
fn run_debug_breakpoint_pauses_inside_function_body() {
    let source_path = temp_source_path();
    fs::create_dir_all(source_path.parent().expect("source path has parent"))
        .expect("temp source directory should be created");
    fs::write(&source_path, "work function\n  2\n  3\n  +\nend\nwork\n")
        .expect("temp source should be written");

    let mut child = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg("--breakpoint")
        .arg("3")
        .arg(&source_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("rco run debugger should launch");

    child
        .stdin
        .take()
        .expect("debugger stdin should be piped")
        .write_all(b"continue\n")
        .expect("debugger command should write");

    let output = child.wait_with_output().expect("debugger should finish");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "function breakpoint should succeed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains(":3 [work] PushNumber(3)"),
        "pause should identify the function frame and exact source line, got:\n{stdout}"
    );
    assert!(
        stdout.contains("[Number(5)]"),
        "stdout should include final stack, got:\n{stdout}"
    );
}

#[test]
fn test_runs_testcase_methods() {
    let source_path = temp_source_path();
    fs::create_dir_all(source_path.parent().expect("source path has parent"))
        .expect("temp source directory should be created");
    fs::write(
        &source_path,
        r#"
UserTest TestCase Subclass
  [
    "ada@example.com"
    "ada@example.com" assert-equals
  ] "testDisplayName" Method
end
"#,
    )
    .expect("temp source should be written");

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("test")
        .arg(&source_path)
        .output()
        .expect("rco test should launch");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "rco test failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("PASS UserTest.testDisplayName"),
        "stdout should include passed test, got:\n{stdout}"
    );
    assert!(
        stdout.contains("1 tests, 0 failed"),
        "stdout should include summary, got:\n{stdout}"
    );
}

#[test]
fn test_filter_runs_only_matching_testcase_methods() {
    let source_path = temp_source_path();
    fs::create_dir_all(source_path.parent().expect("source path has parent"))
        .expect("temp source directory should be created");
    fs::write(
        &source_path,
        r#"
UserTest TestCase Subclass
  [
    "ada@example.com"
    "ada@example.com" assert-equals
  ] "testFastPass" Method

  [
    "ada@example.com"
    "grace@example.com" assert-equals
  ] "testSlowFail" Method
end
"#,
    )
    .expect("temp source should be written");

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("test")
        .arg("--filter")
        .arg("Fast")
        .arg(&source_path)
        .output()
        .expect("rco test should launch");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "filtered rco test failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("PASS UserTest.testFastPass"),
        "stdout should include matching passed test, got:\n{stdout}"
    );
    assert!(
        !stdout.contains("testSlowFail"),
        "stdout should not include filtered-out test, got:\n{stdout}"
    );
    assert!(
        stdout.contains("1 tests, 0 failed"),
        "stdout should include filtered summary, got:\n{stdout}"
    );
}

#[test]
fn test_filter_skips_nonmatching_test_files_before_top_level_effects() {
    let root = temp_source_path()
        .parent()
        .expect("source path has parent")
        .join("filtered-tests");
    let tests_dir = root.join("tests");
    fs::create_dir_all(&tests_dir).expect("tests directory should be created");
    let sentinel = root.join("side-effect.txt");
    let sentinel_source = escape_ricochet_string(&sentinel.to_string_lossy());

    fs::write(
        tests_dir.join("MatchingTest.rco"),
        r#"
MatchingTest TestCase Subclass
  [
    1 1 assert-equals
  ] "testOnlyThisRuns" Method
end
"#,
    )
    .expect("matching test should be written");
    fs::write(
        tests_dir.join("IgnoredTest.rco"),
        format!(
            r#"
"{sentinel_source}" "side effect" fs_write_text drop

IgnoredTest TestCase Subclass
  [
    1 1 assert-equals
  ] "testIgnored" Method
end
"#
        ),
    )
    .expect("ignored test should be written");

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("test")
        .arg("--filter")
        .arg("OnlyThis")
        .arg(&tests_dir)
        .output()
        .expect("rco test should launch");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "filtered rco test failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("PASS MatchingTest.testOnlyThisRuns"),
        "stdout should include matching passed test, got:\n{stdout}"
    );
    assert!(
        !sentinel.exists(),
        "filtered-out test file executed top-level side effect at {}",
        sentinel.display()
    );
}

#[test]
fn test_reports_assertion_failures() {
    let source_path = temp_source_path();
    fs::create_dir_all(source_path.parent().expect("source path has parent"))
        .expect("temp source directory should be created");
    fs::write(
        &source_path,
        r#"
UserTest TestCase Subclass
  [
    "ada@example.com"
    "grace@example.com" assert-equals
  ] "testDisplayName" Method
end
"#,
    )
    .expect("temp source should be written");

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("test")
        .arg(&source_path)
        .output()
        .expect("rco test should launch");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !output.status.success(),
        "rco test should fail\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("FAIL UserTest.testDisplayName"),
        "stdout should include failed test, got:\n{stdout}"
    );
    assert!(
        stdout.contains("1 tests, 1 failed"),
        "stdout should include failure summary, got:\n{stdout}"
    );
    assert!(
        stderr.contains("Error: 1 Ricochet test failed"),
        "stderr should include failure count error, got:\n{stderr}"
    );
}

#[test]
fn run_executes_top_level_function_script() {
    let source_path = temp_source_path();
    fs::create_dir_all(source_path.parent().expect("source path has parent"))
        .expect("temp source directory should be created");
    fs::write(&source_path, r#"hello function "hi" end hello"#)
        .expect("temp source should be written");

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg(&source_path)
        .output()
        .expect("rco run should launch");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "rco run failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("String(\"hi\")"),
        "stdout should show final stack with function result, got:\n{stdout}"
    );
}

#[test]
fn run_honors_explicit_early_return() {
    let source_path = temp_source_path();
    fs::create_dir_all(source_path.parent().expect("source path has parent"))
        .expect("temp source directory should be created");
    fs::write(
        &source_path,
        "answer function\n  42 return\n  99\nend\nanswer\n",
    )
    .expect("temp source should be written");

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg(&source_path)
        .output()
        .expect("rco run should launch");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "rco run failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("Number(42)") && !stdout.contains("Number(99)"),
        "stdout should show the early return value only, got:\n{stdout}"
    );
}

#[test]
fn run_executes_counter_machine_loop() {
    let source_path = temp_source_path();
    fs::create_dir_all(source_path.parent().expect("source path has parent"))
        .expect("temp source directory should be created");
    fs::write(
        &source_path,
        r#"
0 product var
6 multiplicand var
7 multiplier var

multiplier get 0 > while
  product get multiplicand get + product set
  multiplier get 1 - multiplier set
end

product get
"#,
    )
    .expect("temp source should be written");

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg(&source_path)
        .output()
        .expect("rco run should launch");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "rco run failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("Number(42)"),
        "stdout should show the counter-machine product, got:\n{stdout}"
    );
}

#[test]
fn run_executes_break_and_continue_inside_while() {
    let source_path = temp_source_path();
    fs::create_dir_all(source_path.parent().expect("source path has parent"))
        .expect("temp source directory should be created");
    fs::write(
        &source_path,
        r#"
0 count var
0 total var

count get 10 < while
  count get 1 + count set
  count get 3 = if
    continue
  end
  count get 6 = if
    break
  end
  total get count get + total set
end

total get
"#,
    )
    .expect("temp source should be written");

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg(&source_path)
        .output()
        .expect("rco run should launch");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "rco run failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("Number(12)"),
        "stdout should show continue/break total, got:\n{stdout}"
    );
}

#[test]
fn run_targets_break_to_the_nearest_nested_loop() {
    let source_path = temp_source_path();
    fs::create_dir_all(source_path.parent().expect("source path has parent"))
        .expect("temp source directory should be created");
    fs::write(
        &source_path,
        r#"
0 outer var
0 inner var
0 hits var

outer get 3 < while
  outer get 1 + outer set
  0 inner set

  inner get 5 < while
    inner get 1 + inner set
    inner get 2 = if
      break
    end
    hits get 1 + hits set
  end
end

hits get
"#,
    )
    .expect("temp source should be written");

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg(&source_path)
        .output()
        .expect("rco run should launch");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "rco run failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("Number(3)"),
        "stdout should show one inner-loop hit per outer iteration, got:\n{stdout}"
    );
}

#[test]
fn run_executes_while_inside_a_bytecode_method() {
    let source_path = temp_source_path();
    fs::create_dir_all(source_path.parent().expect("source path has parent"))
        .expect("temp source directory should be created");
    fs::write(
        &source_path,
        r#"
Counter Object Subclass
  ( limit ) [
    limit var
    0 current var
    0 total var

    current get limit get < while
      current get 1 + current set
      total get current get + total set
    end

    total get
  ] "sumTo" Method
end

5 Counter new sumTo
"#,
    )
    .expect("temp source should be written");

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg(&source_path)
        .output()
        .expect("rco run should launch");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "rco run failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("Number(15)"),
        "stdout should show method loop result, got:\n{stdout}"
    );
}

#[test]
fn run_executes_heap_allocated_unary_counter() {
    let source_path = temp_source_path();
    fs::create_dir_all(source_path.parent().expect("source path has parent"))
        .expect("temp source directory should be created");
    fs::write(
        &source_path,
        r#"
Counter Object Subclass
  "previous" Accessor
end

nil counter var
0 steps var

counter get Counter new previous.set counter set
counter get Counter new previous.set counter set
counter get Counter new previous.set counter set

counter get nil? false = while
  counter get previous.get counter set
  steps get 1 + steps set
end

steps get
"#,
    )
    .expect("temp source should be written");

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg(&source_path)
        .output()
        .expect("rco run should launch");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "rco run failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("Number(3)"),
        "stdout should show the number of heap counter nodes, got:\n{stdout}"
    );
}

#[test]
fn run_executes_first_class_block_call_script() {
    let source_path = temp_source_path();
    fs::create_dir_all(source_path.parent().expect("source path has parent"))
        .expect("temp source directory should be created");
    fs::write(&source_path, r#"[ "ok" ] call"#).expect("temp source should be written");

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg(&source_path)
        .output()
        .expect("rco run should launch");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "rco run failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("String(\"ok\")"),
        "stdout should show final stack with block result, got:\n{stdout}"
    );
}

#[test]
fn run_spawns_and_awaits_block_task_with_snapshot() {
    let source_path = temp_source_path();
    fs::create_dir_all(source_path.parent().expect("source path has parent"))
        .expect("temp source directory should be created");
    fs::write(
        &source_path,
        r#"
10 base var
[ base get 5 + ] spawn task var
99 base set
task get type
task get await
"#,
    )
    .expect("temp source should be written");

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg(&source_path)
        .output()
        .expect("rco run should launch");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "rco run failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("String(\"task\")"),
        "stdout should show a first-class task value type, got:\n{stdout}"
    );
    assert!(
        stdout.contains("Number(15)") && !stdout.contains("Number(104)"),
        "await should resolve against the spawn-time snapshot, got:\n{stdout}"
    );
}

#[test]
fn run_inspects_spawned_task_status() {
    let output = run_source(
        r#"
[ 100 sleep 20 2 + ] spawn task var
task get id
task get status
task get pending?
task get running?
task get completed?
task get failed?
tasks
task get info
runtime_capabilities "tasks" at "known" at
runtime_capabilities "tasks" at "running" at
runtime_capabilities "tasks" at "max_running" at
tasks count
task get await
task get status
task get completed?
task get failed?
task get pending?
task get await
tasks count
"#,
    );
    assert_run_success_for("rco run", "task inspection", &output);

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Number(0)"),
        "stdout should show the first task id, got:\n{stdout}"
    );
    assert!(
        stdout.contains("String(\"running\")") && stdout.contains("Bool(true)"),
        "stdout should show the task running before await, got:\n{stdout}"
    );
    assert!(
        stdout.contains(
            r#"Map({"completed": Bool(false), "failed": Bool(false), "id": Number(0), "pending": Bool(true), "running": Bool(true), "status": String("running")})"#
        ),
        "stdout should include running task metadata, got:\n{stdout}"
    );
    assert!(
        stdout.contains("Number(64)"),
        "stdout should show the configured max running task count, got:\n{stdout}"
    );
    assert!(
        stdout.contains("Number(22)"),
        "stdout should show the awaited task result, got:\n{stdout}"
    );
    assert!(
        stdout.contains("String(\"completed\")"),
        "stdout should show the task completed after await, got:\n{stdout}"
    );
    assert!(
        stdout.matches("Bool(false)").count() >= 3 && stdout.contains("Bool(true)"),
        "stdout should show pending/completed/failed task predicates, got:\n{stdout}"
    );
    assert!(
        stdout.matches("Number(22)").count() >= 2,
        "await should return the cached completed task result on reuse, got:\n{stdout}"
    );
}

#[test]
fn run_spawned_task_can_complete_before_await() {
    let output = run_source(
        r#"
events array
[
  50 sleep
  events get "done" push! drop
  7
] spawn task var
150 sleep
events get count
task get status
task get completed?
task get await
"#,
    );
    assert_run_success_for("rco run", "eager spawned task", &output);

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Number(1)"),
        "spawned task should mutate shared collection before await, got:\n{stdout}"
    );
    assert!(
        stdout.contains("String(\"completed\")"),
        "task should be completed before await, got:\n{stdout}"
    );
    assert!(
        stdout.contains("Bool(true)") && stdout.contains("Number(7)"),
        "await should still return the cached task value, got:\n{stdout}"
    );
}

#[test]
fn run_releases_completed_task_handles() {
    let output = run_source(
        r#"
[ 40 2 + ] spawn task var
task get await
task get release-task
task get status
runtime_capabilities "tasks" at "known" at
"#,
    );
    assert_run_success_for("rco run", "release completed task", &output);

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Number(42)"),
        "stdout should show awaited task value, got:\n{stdout}"
    );
    assert!(
        stdout.contains("Bool(true)"),
        "stdout should show release-task succeeded, got:\n{stdout}"
    );
    assert!(
        stdout.contains("String(\"consumed\")"),
        "stdout should show consumed task status after release, got:\n{stdout}"
    );
    assert!(
        stdout.contains("Number(0)"),
        "stdout should show no retained tasks after release, got:\n{stdout}"
    );
}

#[test]
fn run_awaits_multiple_tasks_with_await_all() {
    let output = run_source(
        r#"
handles array
[ 20 2 + ] spawn handles get swap push! drop
[ 30 4 + ] spawn handles get swap push! drop
handles get await-all
handles get await-all
tasks count
"#,
    );
    assert_run_success_for("rco run", "await-all", &output);

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.matches(r#"Array([Number(22), Number(34)])"#).count() >= 2,
        "stdout should show await-all resolving and reusing task results, got:\n{stdout}"
    );
    assert!(
        stdout.contains("Number(0)"),
        "stdout should show no pending tasks after await-all, got:\n{stdout}"
    );
}

#[test]
fn run_executes_dynamic_send_script() {
    let source_path = temp_source_path();
    fs::create_dir_all(source_path.parent().expect("source path has parent"))
        .expect("temp source directory should be created");
    fs::write(
        &source_path,
        r#"
User Model Subclass
  "email" Accessor
  [ self email.get ] "displayName" Method
end

"User" new
"ada@example.com" swap email.set
"displayName" send
"#,
    )
    .expect("temp source should be written");

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg(&source_path)
        .output()
        .expect("rco run should launch");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "rco run failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("String(\"ada@example.com\")"),
        "stdout should show final stack with dynamic send result, got:\n{stdout}"
    );
}

#[test]
fn run_installs_a_method_from_runtime_class_and_method_names() {
    let source_path = temp_source_path();
    fs::create_dir_all(source_path.parent().expect("source path has parent"))
        .expect("temp source directory should be created");
    fs::write(
        &source_path,
        r#"
Widget Object Subclass
  "name" Accessor
  [ self name.get ] "label" Method
end

Widget new
"dynamic" swap name.set
label
"#,
    )
    .expect("temp source should be written");

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg(&source_path)
        .output()
        .expect("rco run should launch");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "rco run failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("String(\"dynamic\")"),
        "stdout should show the dynamically installed method result, got:\n{stdout}"
    );
}

#[test]
fn run_supports_reference_collections_and_collection_algorithms() {
    let output = run_source(
        r#"
array users var
users get "Ada" push! drop
users get "Grace" push! drop
users get 1 "Lin" insert! drop
users get 1 remove-at! drop

map settings var
settings get "theme" "dark" put! drop

0 6 range numbers var
[ 2 * ] numbers get transform doubled var
[ 4 > ] doubled get select selected var

users get count
users get 0 at
settings get "theme" has?
settings get "theme" at
settings get keys count
0 [ + ] selected get reduce
[ 10 = ] doubled get any?
[ 2 > ] doubled get all?
[ 8 = ] doubled get find

list queue var
queue get 1 push! drop
queue get 2 push! drop
queue get count

Set new tags var
tags get "rco" push! drop
tags get "rco" push! drop
tags get count
"#,
    );

    assert_run_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    for expected in [
        "Number(2)",
        "String(\"Ada\")",
        "Bool(true)",
        "String(\"dark\")",
        "Number(1)",
        "Number(24)",
        "Bool(false)",
        "Number(8)",
    ] {
        assert!(
            stdout.contains(expected),
            "stdout should contain {expected}, got:\n{stdout}"
        );
    }
}

#[test]
fn run_supports_name_first_collection_declarations() {
    let output = run_source(
        r#"
users array
users get "Ada" push! drop

"dynamicUsers" array
dynamicUsers get "Grace" push! drop

settings map
settings get "theme" "dark" put! drop

queue list
queue get 1 push! drop

tags Set
tags get "rco" push! drop

users get count
users get 0 at
dynamicUsers get count
settings get "theme" at
queue get count
tags get count
"#,
    );

    assert_run_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    for expected in ["Number(1)", "String(\"Ada\")", "String(\"dark\")"] {
        assert!(
            stdout.contains(expected),
            "stdout should contain {expected}, got:\n{stdout}"
        );
    }
}

#[test]
fn examples_are_runnable_acceptance_suite() {
    for example in [
        "basic-oop.rco",
        "collections.rco",
        "loop_control.rco",
        "turing_complete.rco",
        "unary_counter.rco",
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_rco"))
            .arg("run")
            .arg(example_path(example))
            .output()
            .unwrap_or_else(|error| panic!("rco run should launch for {example}: {error}"));

        assert_run_success_for("rco run example", example, &output);
    }

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg(example_path("cli_system.rco"))
        .arg("--")
        .arg("alpha")
        .arg("beta")
        .env("RICOCHET_EXAMPLE_TEST", "present")
        .output()
        .expect("rco run should launch for cli_system.rco");

    assert_run_success_for("rco run example", "cli_system.rco", &output);
}

#[test]
fn run_supports_everyday_arithmetic_and_boolean_words() {
    let output = run_source(
        r#"
6 7 *
22 5 /
22 5 %
5 negate
0 5 - abs
3 7 min
3 7 max
15 0 10 clamp
true false and
true false or
false not
"#,
    );

    assert_run_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    for expected in [
        "Number(42)",
        "Number(4)",
        "Number(2)",
        "Number(-5)",
        "Number(5)",
        "Number(3)",
        "Number(7)",
        "Number(10)",
        "Bool(false)",
        "Bool(true)",
    ] {
        assert!(
            stdout.contains(expected),
            "stdout should contain {expected}, got:\n{stdout}"
        );
    }
}

#[test]
fn run_supports_string_conversion_and_json_words() {
    let output = run_source(
        r#"
"  Ada  " trim
"ada" uppercase
"ADA" lowercase
"ricochet" "ric" starts-with?
"ricochet" "chet" ends-with?
"ricochet" "coc" contains?
"ada,grace" "," split count
"ada,grace" "," split "-" join
"Ada" "Grace" concat
"Ada" length
42 to-string
"42" to-number value
map payload var
payload get "name" "Ada" put! drop
payload get json-encode
"{\"name\":\"Ada\"}" json-decode value "name" at
"#,
    );

    assert_run_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    for expected in [
        "String(\"Ada\")",
        "String(\"ADA\")",
        "String(\"ada\")",
        "Bool(true)",
        "Number(2)",
        "String(\"ada-grace\")",
        "String(\"AdaGrace\")",
        "Number(3)",
        "String(\"42\")",
        "Number(42)",
        "String(\"{\\\"name\\\":\\\"Ada\\\"}\")",
    ] {
        assert!(
            stdout.contains(expected),
            "stdout should contain {expected}, got:\n{stdout}"
        );
    }
}

#[test]
fn run_supports_additional_string_quality_of_life_words() {
    let output = run_source(
        r#"
"ricochet" 2 4 slice
"ricochet" "co" index-of
"ricochet" "c" last-index-of
"ha" 3 repeat
"alpha\nbeta\n" lines count
"cat" chars "," join
" \n" blank?
"  Ada" trim-start
"Ada  " trim-end
"ricochet" "zzz" index-of nil?
"ricochet" "zzz" last-index-of nil?
"#,
    );

    assert_run_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    for expected in [
        "String(\"coch\")",
        "Number(2)",
        "Number(4)",
        "String(\"hahaha\")",
        "String(\"c,a,t\")",
        "Bool(true)",
        "String(\"Ada\")",
    ] {
        assert!(
            stdout.contains(expected),
            "stdout should contain {expected}, got:\n{stdout}"
        );
    }
}

#[test]
fn run_supports_negative_number_literals() {
    let output = run_source("-1 2 + -9223372036854775808");

    assert_run_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    for expected in ["Number(1)", "Number(-9223372036854775808)"] {
        assert!(
            stdout.contains(expected),
            "stdout should contain {expected}, got:\n{stdout}"
        );
    }
}

#[test]
fn run_supports_collection_view_quality_of_life_words() {
    let output = run_source(
        r#"
names array
names get "Ada" push! drop
names get "Grace" push! drop
names get "Lin" push! drop
array first nil?
array last nil?
names get first
names get last
names get 2 take "," join
names get 1 skip "," join
names get reverse "," join
"#,
    );

    assert_run_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    for expected in [
        "String(\"Ada\")",
        "String(\"Lin\")",
        "String(\"Ada,Grace\")",
        "String(\"Grace,Lin\")",
        "String(\"Lin,Grace,Ada\")",
        "Bool(true)",
    ] {
        assert!(
            stdout.contains(expected),
            "stdout should contain {expected}, got:\n{stdout}"
        );
    }
}

#[test]
fn run_supports_assertion_and_inspection_quality_of_life_words() {
    let output = run_source(
        r#"
true assert
true assert-true
false assert-false
42 ok assert-ok
"ValidationError" "bad input" fail assert-error
bag map
bag get "name" "Ada" put! drop
bag get inspect println
"Ada" debug
bag get count
"#,
    );

    assert_run_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Map("),
        "inspect should print a debug representation, got:\n{stdout}"
    );
    assert!(
        stdout.contains("String(\"Ada\")"),
        "debug should print a debug representation without consuming the value, got:\n{stdout}"
    );
    assert!(
        stdout.contains("Number(1)"),
        "final stack should include map count, got:\n{stdout}"
    );
}

#[test]
fn run_supports_regex_words() {
    let output = run_source(
        r##"
"^[a-z0-9_-]+$" regex value slug var
"hello-world_42" slug get matches?
"bad slug!" slug get matches?
"\\d+" regex value digits var
"abc123def" digits get find "text" at
"abc123def" digits get find "start" at
"abc123def" digits get find "end" at
"([a-z]+)-(\\d+)" regex value pair var
"item-42" pair get captures "1" at
"item-42" pair get captures "2" at
digits get "abc123def456" "#" replace
"[" regex error?
"##,
    );

    assert_run_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    for expected in [
        "Bool(true)",
        "Bool(false)",
        "String(\"123\")",
        "Number(3)",
        "Number(6)",
        "String(\"item\")",
        "String(\"42\")",
        "String(\"abc#def#\")",
    ] {
        assert!(
            stdout.contains(expected),
            "stdout should contain {expected}, got:\n{stdout}"
        );
    }
}

#[test]
fn run_supports_result_construction_and_composition() {
    let output = run_source(
        r#"
42 ok ok?
"ValidationError" "bad input" fail error?
99 "ValidationError" "bad input" fail unwrap-or
[ 2 * ] 21 ok map-result value
[ 1 + ok ] 41 ok and-then value
"#,
    );

    assert_run_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    for expected in ["Bool(true)", "Number(99)", "Number(42)"] {
        assert!(
            stdout.contains(expected),
            "stdout should contain {expected}, got:\n{stdout}"
        );
    }
}

#[test]
fn run_supports_result_envelope_contract_maps() {
    let output = run_source(
        r#"
meta map
meta get "capability" "workspace.read" put! drop
meta get "duration_ms" 25 put! drop
"payload" ok meta get result_envelope okEnvelope var
okEnvelope get "ok" at
okEnvelope get "data" at
okEnvelope get "error" at nil?
okEnvelope get "meta" at "capability" at
errorMeta map
errorMeta get "capability" "process.spawn" put! drop
"ProcessTimeout" "Process exceeded timeout." fail errorMeta get result_envelope errEnvelope var
errEnvelope get "ok" at false =
errEnvelope get "data" at nil?
errEnvelope get "error" at "kind" at
errEnvelope get "error" at "code" at
errEnvelope get "error" at "message" at
errEnvelope get "error" at "capability" at
errEnvelope get "meta" at "capability" at
"#,
    );

    assert_run_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    for expected in [
        "Bool(true)",
        "String(\"payload\")",
        "String(\"workspace.read\")",
        "String(\"ProcessTimeout\")",
        "String(\"Process exceeded timeout.\")",
        "String(\"process.spawn\")",
    ] {
        assert!(
            stdout.contains(expected),
            "stdout should contain {expected}, got:\n{stdout}"
        );
    }
    assert!(
        stdout.matches("String(\"ProcessTimeout\")").count() >= 2,
        "stdout should include both error kind and code, got:\n{stdout}"
    );
    assert!(
        stdout.matches("Bool(true)").count() >= 4,
        "stdout should include envelope booleans for ok, nil error, false ok, and nil data, got:\n{stdout}"
    );
}

#[test]
fn run_supports_quality_of_life_stack_words() {
    let output = run_source(
        r#"
1 2 nip
3 4 tuck
10 20 30 1 pick
1 2 3 2 roll
depth
"#,
    );

    assert_run_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Number(11)"),
        "depth should report the stack depth after the operations, got:\n{stdout}"
    );
}

#[test]
fn run_supports_runtime_introspection_words() {
    let output = run_source(
        r#"
Widget Object Subclass
  "name" Accessor
  [ self name.get ] "label" Method
end

array type
Widget new class-of
Widget new Widget instance-of?
"label" Widget new responds-to?
Widget fields count
Widget methods "label" has?
[ 42 ] callable?
"#,
    );

    assert_run_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    for expected in [
        "String(\"array\")",
        "Class(\"Widget\")",
        "Bool(true)",
        "Number(1)",
    ] {
        assert!(
            stdout.contains(expected),
            "stdout should contain {expected}, got:\n{stdout}"
        );
    }
}

#[test]
fn run_exposes_process_environment_time_and_random_words() {
    let source_path = write_source(
        r#"
args count
"RICOCHET_QOL_TEST" env value
cwd value empty?
now 0 >
10 random 10 <
"#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg(&source_path)
        .arg("--")
        .arg("alpha")
        .arg("beta")
        .env("RICOCHET_QOL_TEST", "present")
        .output()
        .expect("rco run should launch");

    assert_run_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    for expected in [
        "Number(2)",
        "String(\"present\")",
        "Bool(false)",
        "Bool(true)",
    ] {
        assert!(
            stdout.contains(expected),
            "stdout should contain {expected}, got:\n{stdout}"
        );
    }
}

#[test]
fn run_env_allowlist_bounds_process_environment_reads() {
    let allowed_source_path = write_source(
        r#"
"RICOCHET_ALLOWED_ENV_TEST" env value
runtime_capabilities "environment" at "enabled" at
runtime_capabilities "environment" at "allowlist" at count
"#,
    );

    let allowed = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg("--capability-profile")
        .arg("sandboxed")
        .arg("--env-allow")
        .arg("RICOCHET_ALLOWED_ENV_TEST")
        .arg(&allowed_source_path)
        .env("RICOCHET_ALLOWED_ENV_TEST", "visible")
        .output()
        .expect("rco run should launch");

    assert_run_success(&allowed);
    let stdout = String::from_utf8_lossy(&allowed.stdout);
    assert!(
        stdout.contains("String(\"visible\")"),
        "stdout should include allowed environment value, got:\n{stdout}"
    );
    assert!(
        stdout.contains("Bool(true)") && stdout.contains("Number(1)"),
        "stdout should show enabled environment capability and one allowlisted name, got:\n{stdout}"
    );

    let denied_source_path = write_source(r#""RICOCHET_DENIED_ENV_TEST" env drop"#);
    let denied = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg("--capability-profile")
        .arg("sandboxed")
        .arg("--env-allow")
        .arg("RICOCHET_ALLOWED_ENV_TEST")
        .arg(&denied_source_path)
        .env("RICOCHET_DENIED_ENV_TEST", "hidden")
        .output()
        .expect("rco run should launch");

    assert!(
        !denied.status.success(),
        "rco run should fail when env var is outside allowlist"
    );
    let stderr = String::from_utf8_lossy(&denied.stderr);
    assert!(
        stderr.contains("environment variable is not allowed: RICOCHET_DENIED_ENV_TEST"),
        "stderr should explain env allowlist denial, got:\n{stderr}"
    );
}

#[test]
fn run_exposes_webview_builder_words_with_capability_controls() {
    let source_path = write_source(
        r#"
"Save <Now>" "save" webview_button println
"Counter" "<main>Ready</main>" webview_window value document var
$document "html" at println
$document "width" at println
$document "height" at println
"#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg(&source_path)
        .output()
        .expect("rco run should launch");

    assert_run_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    for expected in [
        r#"<button type="button" data-rco-action="save">Save &lt;Now&gt;</button>"#,
        "<title>Counter</title>",
        "<main>Ready</main>",
        "800",
        "600",
    ] {
        assert!(
            stdout.contains(expected),
            "stdout should contain {expected}, got:\n{stdout}"
        );
    }

    let denied = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg("--no-webview")
        .arg(&source_path)
        .output()
        .expect("rco run should launch");
    let stderr = String::from_utf8_lossy(&denied.stderr);
    assert!(
        !denied.status.success(),
        "--no-webview should deny webview access"
    );
    assert!(
        stderr.contains("webview capability is not enabled"),
        "stderr should mention webview denial, got:\n{stderr}"
    );

    let sandboxed = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg("--capability-profile")
        .arg("sandboxed")
        .arg("--allow-webview")
        .arg(&source_path)
        .output()
        .expect("rco run should launch");

    assert_run_success_for("rco run", "sandboxed webview", &sandboxed);
}

#[test]
fn run_supports_print_eprint_and_read_line() {
    let source_path = write_source(
        r#"
"Name: " print
read-line trim println
"warning" eprint
"#,
    );
    let mut child = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg(&source_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("rco run should launch");
    child
        .stdin
        .as_mut()
        .expect("stdin should be piped")
        .write_all(b"Ada\n")
        .expect("stdin should accept input");

    let output = child.wait_with_output().expect("rco run should finish");

    assert_run_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stdout.contains("Name: Ada"),
        "stdout should preserve print/println composition, got:\n{stdout}"
    );
    assert!(
        stderr.contains("warning"),
        "stderr should contain eprint output, got:\n{stderr}"
    );
}

#[test]
fn run_exposes_filesystem_capability() {
    let source_path = temp_source_path();
    let root = source_path.parent().expect("source path has parent");
    fs::create_dir_all(root).expect("temp source directory should be created");
    let data_path = root.join("data.txt");
    let data = escape_ricochet_string(&data_path.to_string_lossy());
    let directory = escape_ricochet_string(&root.to_string_lossy());
    fs::write(
        &source_path,
        format!(
            r#"
"{data}" "hello from Ricochet" fs_write_text value drop
"{data}" fs_read_text value
"{data}" fs_exists?
"{directory}" fs_list value count 1 >=
"#
        ),
    )
    .expect("temp source should be written");

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg(&source_path)
        .output()
        .expect("rco run should launch");

    assert_run_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("String(\"hello from Ricochet\")"),
        "stdout should contain file contents, got:\n{stdout}"
    );
    assert!(
        stdout.matches("Bool(true)").count() >= 2,
        "stdout should confirm existence and directory contents, got:\n{stdout}"
    );
}

#[test]
fn run_can_disable_filesystem_capability() {
    let source_path = write_source("fs drop\n");

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg("--no-fs")
        .arg(&source_path)
        .output()
        .expect("rco run should launch");

    assert!(
        !output.status.success(),
        "rco run should fail when fs is disabled"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("filesystem capability is not enabled"),
        "stderr should explain the disabled filesystem capability, got:\n{stderr}"
    );
}

#[test]
fn run_sandboxed_capability_profile_disables_host_powers() {
    let fs_source_path = write_source("fs drop\n");
    let fs_output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg("--capability-profile")
        .arg("sandboxed")
        .arg(&fs_source_path)
        .output()
        .expect("rco run should launch");

    assert!(
        !fs_output.status.success(),
        "rco run should fail when sandboxed fs is not explicitly bounded"
    );
    let stderr = String::from_utf8_lossy(&fs_output.stderr);
    assert!(
        stderr.contains("filesystem capability is not enabled"),
        "stderr should explain sandboxed filesystem denial, got:\n{stderr}"
    );

    let http_source_path = write_source("http drop\n");
    let http_output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg("--capability-profile")
        .arg("sandboxed")
        .arg(&http_source_path)
        .output()
        .expect("rco run should launch");

    assert!(
        !http_output.status.success(),
        "rco run should fail when sandboxed HTTP is not explicitly bounded"
    );
    let stderr = String::from_utf8_lossy(&http_output.stderr);
    assert!(
        stderr.contains("HTTP capability is not enabled"),
        "stderr should explain sandboxed HTTP denial, got:\n{stderr}"
    );

    let env_source_path = write_source("\"RICOCHET_QOL_TEST\" env drop\n");
    let env_output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg("--capability-profile")
        .arg("sandboxed")
        .arg(&env_source_path)
        .output()
        .expect("rco run should launch");

    assert!(
        !env_output.status.success(),
        "rco run should fail when sandboxed env is not explicitly trusted"
    );
    let stderr = String::from_utf8_lossy(&env_output.stderr);
    assert!(
        stderr.contains("environment capability is not enabled"),
        "stderr should explain sandboxed env denial, got:\n{stderr}"
    );

    let cwd_source_path = write_source("cwd drop\n");
    let cwd_output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg("--capability-profile")
        .arg("sandboxed")
        .arg(&cwd_source_path)
        .output()
        .expect("rco run should launch");

    assert!(
        !cwd_output.status.success(),
        "rco run should fail when sandboxed cwd is not explicitly trusted"
    );
    let stderr = String::from_utf8_lossy(&cwd_output.stderr);
    assert!(
        stderr.contains("environment capability is not enabled"),
        "stderr should explain sandboxed cwd denial, got:\n{stderr}"
    );
}

#[test]
fn run_can_disable_environment_and_sleep_capabilities() {
    let env_source_path = write_source("\"RICOCHET_QOL_TEST\" env drop\n");
    let env_output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg("--no-env")
        .arg(&env_source_path)
        .output()
        .expect("rco run should launch");

    assert!(
        !env_output.status.success(),
        "rco run should fail when env is disabled"
    );
    let stderr = String::from_utf8_lossy(&env_output.stderr);
    assert!(
        stderr.contains("environment capability is not enabled"),
        "stderr should explain disabled env, got:\n{stderr}"
    );

    let sleep_source_path = write_source("1 sleep\n");
    let sleep_output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg("--no-sleep")
        .arg(&sleep_source_path)
        .output()
        .expect("rco run should launch");

    assert!(
        !sleep_output.status.success(),
        "rco run should fail when sleep is disabled"
    );
    let stderr = String::from_utf8_lossy(&sleep_output.stderr);
    assert!(
        stderr.contains("sleep capability is not enabled"),
        "stderr should explain disabled sleep, got:\n{stderr}"
    );
}

#[test]
fn run_process_spawn_requires_explicit_capability() {
    let source_path = write_source(
        r#"
args array
options map
"ricochet" args get options get process_spawn drop
"#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg(&source_path)
        .output()
        .expect("rco run should launch");

    assert!(
        !output.status.success(),
        "rco run should fail when process execution is not enabled"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("process capability is not enabled"),
        "stderr should explain process denial, got:\n{stderr}"
    );
}

#[test]
fn run_process_spawn_executes_direct_command_when_allowed() {
    let source_path = temp_source_path();
    let root = source_path.parent().expect("source path has parent");
    fs::create_dir_all(root).expect("temp source directory should be created");
    let checked_source = root.join("checked.rco");
    fs::write(&checked_source, "42\n").expect("checked source should be written");
    let rco = escape_ricochet_string(env!("CARGO_BIN_EXE_rco"));
    let checked = escape_ricochet_string(&checked_source.to_string_lossy());
    fs::write(
        &source_path,
        format!(
            r#"
args array
args get "check" push! drop
args get "{checked}" push! drop
options map
options get "timeout_ms" 10000 put! drop
"{rco}" args get options get process_spawn value result var
result get "success" at
result get "stdout" at "checked" contains?
runtime_capabilities "process" at "enabled" at
"#
        ),
    )
    .expect("source should be written");

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg("--allow-process")
        .arg(&source_path)
        .output()
        .expect("rco run should launch");

    assert_run_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.matches("Bool(true)").count() >= 3,
        "stdout should show command success, captured output, and process introspection, got:\n{stdout}"
    );
}

#[test]
fn run_process_root_bounds_process_cwd_when_allowed() {
    let source_path = temp_source_path();
    let base = source_path.parent().expect("source path has parent");
    let process_root = base.join("process-root");
    let outside_root = base.join("outside-process-root");
    fs::create_dir_all(&process_root).expect("process root should be created");
    fs::create_dir_all(&outside_root).expect("outside process root should be created");
    let checked_source = process_root.join("checked.rco");
    fs::write(&checked_source, "42\n").expect("checked source should be written");
    let rco = escape_ricochet_string(env!("CARGO_BIN_EXE_rco"));
    let process_root_source = escape_ricochet_string(&process_root.to_string_lossy());
    let outside_root_source = escape_ricochet_string(&outside_root.to_string_lossy());
    fs::write(
        &source_path,
        format!(
            r#"
args array
args get "check" push! drop
args get "checked.rco" push! drop
options map
options get "cwd" "{process_root_source}" put! drop
options get "timeout_ms" 10000 put! drop
"{rco}" args get options get process_spawn value result var
result get "success" at
result get "stdout" at "checked" contains?
runtime_capabilities "process" at "root" at "{process_root_source}" =
outsideOptions map
outsideOptions get "cwd" "{outside_root_source}" put! drop
"{rco}" args get outsideOptions get process_spawn error denied var
denied get "kind" at
"#
        ),
    )
    .expect("source should be written");

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg("--allow-process")
        .arg("--process-root")
        .arg(&process_root)
        .arg(&source_path)
        .output()
        .expect("rco run should launch");

    assert_run_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.matches("Bool(true)").count() >= 3,
        "stdout should show process success, captured output, and process root introspection, got:\n{stdout}"
    );
    assert!(
        stdout.contains("String(\"PermissionError\")"),
        "stdout should report outside process cwd as PermissionError, got:\n{stdout}"
    );
}

#[test]
fn run_process_start_reads_completed_job_when_allowed() {
    let source_path = temp_source_path();
    let root = source_path.parent().expect("source path has parent");
    fs::create_dir_all(root).expect("temp source directory should be created");
    let checked_source = root.join("checked.rco");
    fs::write(&checked_source, "42\n").expect("checked source should be written");
    let rco = escape_ricochet_string(env!("CARGO_BIN_EXE_rco"));
    let checked = escape_ricochet_string(&checked_source.to_string_lossy());
    fs::write(
        &source_path,
        format!(
            r#"
args array
args get "check" push! drop
args get "{checked}" push! drop
options map
options get "timeout_ms" 10000 put! drop
"{rco}" args get options get process_start value job var
200 sleep
readOptions map
job get "id" at readOptions get process_read value read var
read get "stdout" at "checked" contains?
job get "id" at process_job value "status" at "exited" =
process_jobs count
runtime_capabilities "process" at "jobs" at
"#
        ),
    )
    .expect("source should be written");

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg("--allow-process")
        .arg(&source_path)
        .output()
        .expect("rco run should launch");

    assert_run_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.matches("Bool(true)").count() >= 2,
        "stdout should show captured output and exited status, got:\n{stdout}"
    );
    assert!(
        stdout.matches("Number(1)").count() >= 2,
        "stdout should show one process job in both jobs and capability snapshots, got:\n{stdout}"
    );
}

#[test]
fn run_process_cancel_marks_job_cancelled_when_allowed() {
    let source_path = temp_source_path();
    let root = source_path.parent().expect("source path has parent");
    fs::create_dir_all(root).expect("temp source directory should be created");
    let sleeping_source = root.join("sleeping.rco");
    fs::write(&sleeping_source, "10000 sleep\n").expect("sleeping source should be written");
    let rco = escape_ricochet_string(env!("CARGO_BIN_EXE_rco"));
    let sleeping = escape_ricochet_string(&sleeping_source.to_string_lossy());
    fs::write(
        &source_path,
        format!(
            r#"
args array
args get "run" push! drop
args get "{sleeping}" push! drop
options map
options get "timeout_ms" 30000 put! drop
"{rco}" args get options get process_start value job var
job get "id" at process_cancel value cancelResult var
200 sleep
job get "id" at process_job value detail var
cancelResult get "cancelled" at
detail get "cancelled" at
"#
        ),
    )
    .expect("source should be written");

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg("--allow-process")
        .arg(&source_path)
        .output()
        .expect("rco run should launch");

    assert_run_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.matches("Bool(true)").count() >= 2,
        "stdout should show cancellation in immediate and later snapshots, got:\n{stdout}"
    );
}

#[test]
fn run_process_output_caps_are_reported_when_allowed() {
    let source_path = temp_source_path();
    let root = source_path.parent().expect("source path has parent");
    fs::create_dir_all(root).expect("temp source directory should be created");
    let noisy_source = root.join("noisy.rco");
    fs::write(&noisy_source, r#""abcdef" print"#).expect("noisy source should be written");
    let rco = escape_ricochet_string(env!("CARGO_BIN_EXE_rco"));
    let noisy = escape_ricochet_string(&noisy_source.to_string_lossy());
    fs::write(
        &source_path,
        format!(
            r#"
args array
args get "run" push! drop
args get "{noisy}" push! drop
options map
options get "timeout_ms" 10000 put! drop
options get "stdout_max_bytes" 3 put! drop
"{rco}" args get options get process_start value job var
200 sleep
readOptions map
job get "id" at readOptions get process_read value read var
read get "stdout" at "abc" =
read get "stdout_truncated" at
"{rco}" args get options get process_spawn value blocking var
blocking get "stdout" at "abc" =
blocking get "stdout_truncated" at
"#
        ),
    )
    .expect("source should be written");

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg("--allow-process")
        .arg(&source_path)
        .output()
        .expect("rco run should launch");

    assert_run_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.matches("Bool(true)").count() >= 4,
        "stdout should show capped stdout for job and blocking process APIs, got:\n{stdout}"
    );
}

#[test]
fn run_pty_start_requires_explicit_capability() {
    let source_path = write_source(
        r#"
args array
options map
"shell" args get options get pty_start drop
"#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg(&source_path)
        .output()
        .expect("rco run should launch");

    assert!(
        !output.status.success(),
        "rco run should fail when PTY execution is not enabled"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("PTY capability is not enabled"),
        "stderr should explain PTY denial, got:\n{stderr}"
    );
}

#[test]
fn run_pty_start_captures_output_when_allowed() {
    let source_path = temp_source_path();
    fs::create_dir_all(source_path.parent().expect("source path has parent"))
        .expect("temp source directory should be created");

    #[cfg(windows)]
    let (command, args): (&str, &[&str]) = ("cmd", &["/C", "echo", "ricochet-pty"]);
    #[cfg(not(windows))]
    let (command, args): (&str, &[&str]) = ("printf", &["ricochet-pty"]);

    let mut arg_lines = String::new();
    for arg in args {
        arg_lines.push_str(&format!(
            "args get \"{}\" push! drop\n",
            escape_ricochet_string(arg)
        ));
    }
    let command = escape_ricochet_string(command);
    fs::write(
        &source_path,
        format!(
            r#"
args array
{arg_lines}options map
"{command}" args get options get pty_start value session var
500 sleep
readOptions map
session get "id" at readOptions get pty_read value read var
read get "output" at "ricochet-pty" contains?
pty_list count
runtime_capabilities "pty" at "enabled" at
runtime_capabilities "pty" at "sessions" at
"#
        ),
    )
    .expect("source should be written");

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg("--allow-pty")
        .arg(&source_path)
        .output()
        .expect("rco run should launch");

    assert_run_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.matches("Bool(true)").count() >= 2,
        "stdout should show captured PTY output and PTY capability state, got:\n{stdout}"
    );
    assert!(
        stdout.matches("Number(1)").count() >= 2,
        "stdout should show one retained PTY session in list and capability snapshots, got:\n{stdout}"
    );
}

#[test]
fn run_pty_stop_marks_session_stopped_when_allowed() {
    let source_path = temp_source_path();
    let root = source_path.parent().expect("source path has parent");
    fs::create_dir_all(root).expect("temp source directory should be created");
    let sleeping_source = root.join("sleeping-pty.rco");
    fs::write(&sleeping_source, "10000 sleep\n").expect("sleeping source should be written");
    let rco = escape_ricochet_string(env!("CARGO_BIN_EXE_rco"));
    let sleeping = escape_ricochet_string(&sleeping_source.to_string_lossy());
    fs::write(
        &source_path,
        format!(
            r#"
args array
args get "run" push! drop
args get "{sleeping}" push! drop
options map
"{rco}" args get options get pty_start value session var
stopOptions map
session get "id" at stopOptions get pty_stop value stopped var
stopped get "stopped" at
"#
        ),
    )
    .expect("source should be written");

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg("--allow-pty")
        .arg(&source_path)
        .output()
        .expect("rco run should launch");

    assert_run_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Bool(true)"),
        "stdout should show stopped PTY snapshot, got:\n{stdout}"
    );
}

#[test]
fn run_approval_words_claim_once_and_complete() {
    let source_path = write_source(
        r#"
operation map
operation get "capability" "git.commit" put! drop
operation get "summary" "Commit staged changes" put! drop
options map
options get "id" "approval-fixed" put! drop
operation get options get approval_create value created var
created get "id" at "approval-fixed" =
created get "token" at nil? false =
created get "pending" at
runtime_capabilities "approval" at "records" at
created get "id" at created get "token" at approval_claim value claim var
claim get "claimed" at
claim get "token" at nil?
created get "id" at created get "token" at approval_claim error duplicate var
duplicate get "kind" at "ApprovalAlreadyClaimed" =
result map
result get "ok" true put! drop
created get "id" at result get approval_complete value completed var
completed get "completed" at
created get "id" at approval_detail value detail var
detail get "result" at "ok" at
"#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg(&source_path)
        .output()
        .expect("rco run should launch");

    assert_run_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.matches("Bool(true)").count() >= 8,
        "stdout should show approval create, claim, duplicate denial, and completion, got:\n{stdout}"
    );
    assert!(
        stdout.contains("Number(1)"),
        "stdout should report one approval in runtime capabilities, got:\n{stdout}"
    );
}

#[test]
fn run_approval_claim_rejects_expired_records() {
    let source_path = write_source(
        r#"
operation map
operation get "capability" "filesystem.write" put! drop
options map
options get "expires_at_ms" 1 put! drop
operation get options get approval_create value created var
created get "id" at created get "token" at approval_claim error expired var
expired get "kind" at
created get "id" at approval_detail value detail var
detail get "expired" at
"#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg(&source_path)
        .output()
        .expect("rco run should launch");

    assert_run_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("String(\"ApprovalExpired\")"),
        "stdout should report expired approval claim, got:\n{stdout}"
    );
    assert!(
        stdout.contains("Bool(true)"),
        "stdout should mark approval detail as expired, got:\n{stdout}"
    );
}

#[test]
fn serve_rejects_conflicting_environment_flags() {
    let project_path = temp_source_path()
        .parent()
        .expect("source path has parent")
        .join("serve_env_conflict");
    fs::create_dir_all(project_path.join("config")).expect("config directory should be created");
    fs::write(
        project_path.join("ricochet.toml"),
        r#"
[package]
name = "serve_env_conflict"

[web]
mode = "mvc"
routes = "config/routes.rco"

[web.views]
escape = "html"
"#,
    )
    .expect("manifest should be written");
    fs::write(project_path.join("config/routes.rco"), "").expect("routes should be written");

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("serve")
        .arg("--allow-env")
        .arg("--no-env")
        .current_dir(&project_path)
        .output()
        .expect("rco serve should launch");

    assert!(
        !output.status.success(),
        "rco serve should reject conflicting env flags"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--allow-env cannot be used with --no-env"),
        "stderr should explain env flag conflict, got:\n{stderr}"
    );
}

#[test]
fn run_can_restrict_filesystem_capability_to_root() {
    let source_path = temp_source_path();
    let base = source_path.parent().expect("source path has parent");
    let root = base.join("fs-root");
    fs::create_dir_all(&root).expect("filesystem root should be created");
    let inside_path = root.join("inside.txt");
    let outside_path = base.join("outside.txt");
    fs::write(&inside_path, "inside root").expect("inside file should be written");
    fs::write(&outside_path, "outside root").expect("outside file should be written");
    let inside = escape_ricochet_string(&inside_path.to_string_lossy());
    let outside = escape_ricochet_string(&outside_path.to_string_lossy());
    fs::write(
        &source_path,
        format!(
            r#"
"{inside}" fs_read_text value
"{outside}" fs_read_text error denied var
denied get "kind" at
"{outside}" fs_exists?
"#
        ),
    )
    .expect("temp source should be written");

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg("--fs-root")
        .arg(&root)
        .arg(&source_path)
        .output()
        .expect("rco run should launch");

    assert_run_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("String(\"inside root\")"),
        "stdout should include readable file inside fs root, got:\n{stdout}"
    );
    assert!(
        stdout.contains("String(\"PermissionError\")"),
        "stdout should report outside-root reads as PermissionError, got:\n{stdout}"
    );
    assert!(
        stdout.contains("Bool(false)"),
        "stdout should report outside-root exists? as false, got:\n{stdout}"
    );
}

#[test]
fn run_workspace_words_use_bounded_filesystem_root() {
    let source_path = temp_source_path();
    let base = source_path.parent().expect("source path has parent");
    let root = base.join("workspace-root");
    fs::create_dir_all(&root).expect("workspace root should be created");
    let inside_path = root.join("inside.txt");
    let outside_path = base.join("outside-workspace.txt");
    fs::write(&inside_path, "inside workspace").expect("inside file should be written");
    fs::write(&outside_path, "outside workspace").expect("outside file should be written");
    let outside = escape_ricochet_string(&outside_path.to_string_lossy());
    fs::write(
        &source_path,
        format!(
            r#"
options map
writeOptions map
writeOptions get "create_parent_dirs" true put! drop
copyOptions map
"inside.txt" options get workspace_read_text value
"." options get workspace_resolve value resolved var
resolved get "inside_root" at
"." options get workspace_list value count 1 >=
"generated/out.txt" "created" writeOptions get workspace_write_text value written var
written get "kind" at "file" =
"generated/out.txt" workspace_metadata value meta var
meta get "relative_path" at "generated/out.txt" =
"inside.txt" "generated/copy.txt" copyOptions get workspace_copy value copied var
copied get "exists" at
"." "generated/copy.txt" workspace_contains?
"{outside}" options get workspace_read_text error denied var
denied get "kind" at
runtime_capabilities "workspace" at "enabled" at
"#
        ),
    )
    .expect("source should be written");

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg("--fs-root")
        .arg(&root)
        .arg(&source_path)
        .output()
        .expect("rco run should launch");

    assert_run_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("String(\"inside workspace\")"),
        "stdout should include readable workspace file contents, got:\n{stdout}"
    );
    assert!(
        stdout.matches("Bool(true)").count() >= 7,
        "stdout should confirm workspace metadata, copy, containment, and capability state, got:\n{stdout}"
    );
    assert!(
        stdout.contains("String(\"PermissionError\")"),
        "stdout should report outside-root workspace reads as PermissionError, got:\n{stdout}"
    );
    assert_eq!(
        fs::read_to_string(root.join("generated/out.txt")).expect("written file should exist"),
        "created"
    );
    assert_eq!(
        fs::read_to_string(root.join("generated/copy.txt")).expect("copied file should exist"),
        "inside workspace"
    );
}

#[test]
fn run_sandboxed_capability_profile_allows_bounded_filesystem() {
    let source_path = temp_source_path();
    let base = source_path.parent().expect("source path has parent");
    let root = base.join("sandbox-fs-root");
    fs::create_dir_all(&root).expect("filesystem root should be created");
    let data_path = root.join("data.txt");
    fs::write(&data_path, "bounded").expect("data file should be written");
    let data = escape_ricochet_string(&data_path.to_string_lossy());
    fs::write(
        &source_path,
        format!(
            r#"
"{data}" fs_read_text value
"#
        ),
    )
    .expect("temp source should be written");

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg("--capability-profile")
        .arg("sandboxed")
        .arg("--fs-root")
        .arg(&root)
        .arg("--fs-readonly")
        .arg(&source_path)
        .output()
        .expect("rco run should launch");

    assert_run_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("String(\"bounded\")"),
        "stdout should include file contents from bounded sandbox root, got:\n{stdout}"
    );
}

#[test]
fn run_can_make_filesystem_capability_read_only() {
    let source_path = temp_source_path();
    let root = source_path.parent().expect("source path has parent");
    fs::create_dir_all(root).expect("temp source directory should be created");
    let data_path = root.join("data.txt");
    let directory_path = root.join("created");
    fs::write(&data_path, "original").expect("data file should be written");
    let data = escape_ricochet_string(&data_path.to_string_lossy());
    let directory = escape_ricochet_string(&directory_path.to_string_lossy());
    fs::write(
        &source_path,
        format!(
            r#"
"{data}" fs_read_text value
"{data}" "changed" fs_write_text error writeDenied var
writeDenied get "kind" at
"{directory}" fs_create_dir error createDenied var
createDenied get "kind" at
"#
        ),
    )
    .expect("temp source should be written");

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg("--fs-readonly")
        .arg(&source_path)
        .output()
        .expect("rco run should launch");

    assert_run_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("String(\"original\")"),
        "stdout should include readable file contents, got:\n{stdout}"
    );
    assert!(
        stdout.matches("String(\"PermissionError\")").count() >= 2,
        "stdout should report write/create denials as PermissionError, got:\n{stdout}"
    );
    assert_eq!(
        fs::read_to_string(&data_path).expect("data file should remain readable"),
        "original"
    );
    assert!(
        !directory_path.exists(),
        "read-only filesystem policy should not create directories"
    );
}

#[test]
fn run_workspace_words_respect_readonly_filesystem() {
    let source_path = temp_source_path();
    let root = source_path.parent().expect("source path has parent");
    fs::create_dir_all(root).expect("temp source directory should be created");
    let blocked_path = root.join("blocked.txt");
    let blocked_dir = root.join("blocked-dir");
    fs::write(
        &source_path,
        r#"
options map
writeOptions map
writeOptions get "create_parent_dirs" true put! drop
"blocked.txt" "blocked" writeOptions get workspace_write_text error writeDenied var
writeDenied get "kind" at
"blocked-dir" options get workspace_mkdir error mkdirDenied var
mkdirDenied get "kind" at
"#,
    )
    .expect("source should be written");

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg("--fs-root")
        .arg(root)
        .arg("--fs-readonly")
        .arg(&source_path)
        .output()
        .expect("rco run should launch");

    assert_run_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.matches("String(\"PermissionError\")").count() >= 2,
        "stdout should report workspace write/create denials as PermissionError, got:\n{stdout}"
    );
    assert!(
        !blocked_path.exists(),
        "read-only workspace policy should not create files"
    );
    assert!(
        !blocked_dir.exists(),
        "read-only workspace policy should not create directories"
    );
}

#[test]
fn run_sandboxed_capability_profile_allows_bounded_http() {
    let (address, server) = spawn_single_response_http_server(
        b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\nConnection: close\r\n\r\npong".to_vec(),
    );
    let source_path = write_source(&format!(
        r#"
"http://{address}/ping" http_get value response var
response get "body" at
"#
    ));

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg("--capability-profile")
        .arg("sandboxed")
        .arg("--http-allow-host")
        .arg("127.0.0.1")
        .arg(&source_path)
        .output()
        .expect("rco run should launch");
    server.join().expect("HTTP server should finish");

    assert_run_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("String(\"pong\")"),
        "stdout should include HTTP body from allowed sandbox host, got:\n{stdout}"
    );
}

#[test]
fn run_exposes_http_client_capability() {
    let (address, server) = spawn_single_response_http_server(
        b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\nConnection: close\r\n\r\npong".to_vec(),
    );
    let output = run_source(&format!(
        r#"
"http://{address}/ping" http_get value response var
response get "status" at
response get "body" at
"#
    ));
    server.join().expect("HTTP server should finish");

    assert_run_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Number(200)") && stdout.contains("String(\"pong\")"),
        "stdout should contain HTTP status and body, got:\n{stdout}"
    );
}

#[test]
fn run_exposes_http_get_task_capability() {
    let (address, server) = spawn_single_response_http_server(
        b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\nConnection: close\r\n\r\npong".to_vec(),
    );
    let output = run_source(&format!(
        r#"
"http://{address}/ping" http_get_task task var
task get id
task get await value response var
response get "status" at
response get "body" at
task get status
"#
    ));
    server.join().expect("HTTP server should finish");

    assert_run_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Number(0)")
            && stdout.contains("Number(200)")
            && stdout.contains("String(\"pong\")")
            && stdout.contains("String(\"completed\")"),
        "stdout should contain task id, HTTP response, and completed status, got:\n{stdout}"
    );
}

#[test]
fn run_exposes_http_post_json_task_capability() {
    let (address, server) = spawn_single_response_http_server(
        b"HTTP/1.1 201 Created\r\nContent-Length: 7\r\nConnection: close\r\n\r\ncreated".to_vec(),
    );
    let output = run_source(&format!(
        r#"
map payload var
payload get "message" "hello" put! drop
"http://{address}/messages" payload get http_post_json_task task var
task get await value response var
response get "status" at
response get "body" at
task get completed?
"#
    ));
    server.join().expect("HTTP server should finish");

    assert_run_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Number(201)")
            && stdout.contains("String(\"created\")")
            && stdout.contains("Bool(true)"),
        "stdout should contain async POST response and completed predicate, got:\n{stdout}"
    );
}

#[test]
fn run_exposes_http_request_with_custom_headers() {
    let (address, server, request_rx) = spawn_capturing_http_server(
        b"HTTP/1.1 202 Accepted\r\nX-Seen: yes\r\nContent-Length: 8\r\nConnection: close\r\n\r\naccepted".to_vec(),
    );
    let output = run_source(&format!(
        r#"
headers map
headers get "Authorization" "Bearer test-token" put! drop
headers get "X-Provider" "solace" put! drop
hosts array
hosts get "127.0.0.1" push! drop
schemes array
schemes get "http" push! drop
body map
body get "probe" true put! drop
request map
request get "url" "http://{address}/v1/models" put! drop
request get "method" "POST" put! drop
request get "headers" headers get put! drop
request get "json" body get put! drop
request get "timeout_ms" 10000 put! drop
request get "max_response_bytes" 1024 put! drop
request get "allowed_hosts" hosts get put! drop
request get "allowed_schemes" schemes get put! drop
request get "follow_redirects" false put! drop
request get http_request value response var
response get "status" at
response get "body" at
response get "headers" at "x-seen" at
"#
    ));
    server.join().expect("HTTP server should finish");
    let request = request_rx.recv().expect("server should capture request");

    assert_run_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Number(202)")
            && stdout.contains("String(\"accepted\")")
            && stdout.contains("String(\"yes\")"),
        "stdout should contain response status, body, and headers, got:\n{stdout}"
    );
    assert!(
        request.contains("POST /v1/models HTTP/1.1")
            && request.contains("authorization: Bearer test-token")
            && request.contains("x-provider: solace")
            && request.contains("content-type: application/json")
            && request.contains(r#""probe":true"#),
        "captured request should include custom headers and JSON body, got:\n{request}"
    );
}

#[test]
fn run_exposes_http_request_task_with_custom_headers() {
    let (address, server, request_rx) = spawn_capturing_http_server(
        b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok".to_vec(),
    );
    let output = run_source(&format!(
        r#"
headers map
headers get "Authorization" "Bearer async-token" put! drop
request map
request get "url" "http://{address}/v1/models" put! drop
request get "headers" headers get put! drop
request get http_request_task task var
task get await value response var
response get "status" at
response get "body" at
task get completed?
"#
    ));
    server.join().expect("HTTP server should finish");
    let request = request_rx.recv().expect("server should capture request");

    assert_run_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Number(200)")
            && stdout.contains("String(\"ok\")")
            && stdout.contains("Bool(true)"),
        "stdout should contain async request response and completed predicate, got:\n{stdout}"
    );
    assert!(
        request.contains("GET /v1/models HTTP/1.1")
            && request.contains("authorization: Bearer async-token"),
        "captured request should include async custom header, got:\n{request}"
    );
}

#[test]
fn run_http_request_rejects_request_policy_before_connecting() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("local HTTP listener should bind");
    let address = listener.local_addr().expect("listener should have address");
    listener
        .set_nonblocking(true)
        .expect("listener should become nonblocking");
    let source_path = write_source(&format!(
        r#"
schemes array
schemes get "https" push! drop
request map
request get "url" "http://{address}/blocked" put! drop
request get "allowed_schemes" schemes get put! drop
request get http_request error denied var
denied get "kind" at
denied get "message" at
"#
    ));

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg("--http-allow-host")
        .arg("127.0.0.1")
        .arg(&source_path)
        .output()
        .expect("rco run should launch");
    let accepted = listener.accept().is_ok();

    assert_run_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("String(\"PermissionError\")")
            && stdout.contains("HTTP scheme is not allowed by request policy"),
        "stdout should contain request policy denial, got:\n{stdout}"
    );
    assert!(!accepted, "request policy denial should not connect");
}

#[test]
fn run_http_request_respects_response_byte_cap() {
    let (address, server) = spawn_single_response_http_server(
        b"HTTP/1.1 200 OK\r\nContent-Length: 6\r\nConnection: close\r\n\r\nabcdef".to_vec(),
    );
    let source_path = write_source(&format!(
        r#"
request map
request get "url" "http://{address}/large" put! drop
request get "max_response_bytes" 3 put! drop
request get http_request error denied var
denied get "kind" at
denied get "message" at
"#
    ));

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg("--http-allow-host")
        .arg("127.0.0.1")
        .arg(&source_path)
        .output()
        .expect("rco run should launch");
    server.join().expect("HTTP server should finish");

    assert_run_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("String(\"HttpBodyTooLarge\")")
            && stdout.contains("HTTP response exceeded 3 bytes"),
        "stdout should contain response byte cap error, got:\n{stdout}"
    );
}

#[test]
fn run_http_request_rejects_invalid_header_names_before_connecting() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("local HTTP listener should bind");
    let address = listener.local_addr().expect("listener should have address");
    listener
        .set_nonblocking(true)
        .expect("listener should become nonblocking");
    let source_path = write_source(&format!(
        r#"
headers map
headers get "Bad Header" "secret" put! drop
request map
request get "url" "http://{address}/blocked" put! drop
request get "headers" headers get put! drop
request get http_request error denied var
denied get "kind" at
denied get "message" at
"#
    ));

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg("--http-allow-host")
        .arg("127.0.0.1")
        .arg(&source_path)
        .output()
        .expect("rco run should launch");
    let accepted = listener.accept().is_ok();

    assert_run_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("String(\"HttpHeaderError\")")
            && stdout.contains("invalid HTTP header name"),
        "stdout should contain invalid header error, got:\n{stdout}"
    );
    assert!(!accepted, "invalid header request should not connect");
}

#[test]
fn run_can_disable_http_capability() {
    let source_path = write_source("http drop\n");

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg("--no-http")
        .arg(&source_path)
        .output()
        .expect("rco run should launch");

    assert!(
        !output.status.success(),
        "rco run should fail when HTTP is disabled"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("HTTP capability is not enabled"),
        "stderr should explain the disabled HTTP capability, got:\n{stderr}"
    );
}

#[test]
fn run_can_allow_http_capability_by_host() {
    let (address, server) = spawn_single_response_http_server(
        b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\nConnection: close\r\n\r\npong".to_vec(),
    );
    let source_path = write_source(&format!(
        r#"
"http://{address}/ping" http_get value response var
response get "status" at
response get "body" at
"#
    ));

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg("--http-allow-host")
        .arg("127.0.0.1")
        .arg(&source_path)
        .output()
        .expect("rco run should launch");
    server.join().expect("HTTP server should finish");

    assert_run_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Number(200)") && stdout.contains("String(\"pong\")"),
        "stdout should contain HTTP status and body for allowed host, got:\n{stdout}"
    );
}

#[test]
fn run_http_get_does_not_follow_redirects_past_allowlist() {
    let (address, server) = spawn_single_response_http_server(
        b"HTTP/1.1 302 Found\r\nLocation: http://example.com/blocked\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
    );
    let source_path = write_source(&format!(
        r#"
"http://{address}/redirect" http_get value response var
response get "status" at
"#
    ));

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg("--http-allow-host")
        .arg("127.0.0.1")
        .arg(&source_path)
        .output()
        .expect("rco run should launch");
    server.join().expect("HTTP server should finish");

    assert_run_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Number(302)"),
        "stdout should expose the redirect response without following it, got:\n{stdout}"
    );
}

#[test]
fn run_http_post_json_does_not_follow_redirects_past_allowlist() {
    let (address, server) = spawn_single_response_http_server(
        b"HTTP/1.1 302 Found\r\nLocation: http://example.com/blocked\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
    );
    let source_path = write_source(&format!(
        r#"
payload map
payload get "message" "hello" put! drop
"http://{address}/redirect" payload get http_post_json value response var
response get "status" at
"#
    ));

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg("--http-allow-host")
        .arg("127.0.0.1")
        .arg(&source_path)
        .output()
        .expect("rco run should launch");
    server.join().expect("HTTP server should finish");

    assert_run_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Number(302)"),
        "stdout should expose the redirect response without following it, got:\n{stdout}"
    );
}

#[test]
fn run_http_get_task_does_not_follow_redirects_past_allowlist() {
    let (address, server) = spawn_single_response_http_server(
        b"HTTP/1.1 302 Found\r\nLocation: http://example.com/blocked\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
    );
    let source_path = write_source(&format!(
        r#"
"http://{address}/redirect" http_get_task task var
task get await value response var
response get "status" at
"#
    ));

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg("--http-allow-host")
        .arg("127.0.0.1")
        .arg(&source_path)
        .output()
        .expect("rco run should launch");
    server.join().expect("HTTP server should finish");

    assert_run_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Number(302)"),
        "stdout should expose the async redirect response without following it, got:\n{stdout}"
    );
}

#[test]
fn run_http_post_json_task_does_not_follow_redirects_past_allowlist() {
    let (address, server) = spawn_single_response_http_server(
        b"HTTP/1.1 302 Found\r\nLocation: http://example.com/blocked\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
    );
    let source_path = write_source(&format!(
        r#"
payload map
payload get "message" "hello" put! drop
"http://{address}/redirect" payload get http_post_json_task task var
task get await value response var
response get "status" at
"#
    ));

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg("--http-allow-host")
        .arg("127.0.0.1")
        .arg(&source_path)
        .output()
        .expect("rco run should launch");
    server.join().expect("HTTP server should finish");

    assert_run_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Number(302)"),
        "stdout should expose the async POST redirect response without following it, got:\n{stdout}"
    );
}

#[test]
fn run_can_restrict_http_capability_by_host() {
    let source_path = write_source(
        r#"
"http://127.0.0.1:1/blocked" http_get error denied var
denied get "kind" at
denied get "message" at
"#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg("--http-allow-host")
        .arg("example.com")
        .arg(&source_path)
        .output()
        .expect("rco run should launch");

    assert_run_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("String(\"PermissionError\")")
            && stdout.contains("HTTP host is not allowed: 127.0.0.1"),
        "stdout should report blocked HTTP host before connecting, got:\n{stdout}"
    );
}

#[test]
fn run_http_get_task_respects_http_allowlist() {
    let source_path = write_source(
        r#"
"http://127.0.0.1:1/blocked" http_get_task task var
task get await error denied var
denied get "kind" at
denied get "message" at
task get completed?
"#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg("--http-allow-host")
        .arg("example.com")
        .arg(&source_path)
        .output()
        .expect("rco run should launch");

    assert_run_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("String(\"PermissionError\")")
            && stdout.contains("HTTP host is not allowed: 127.0.0.1")
            && stdout.contains("Bool(true)"),
        "stdout should report blocked async HTTP host and completed task, got:\n{stdout}"
    );
}

#[test]
fn run_limits_http_response_body_size() {
    let body = vec![b'x'; 1_048_577];
    let mut response = format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .into_bytes();
    response.extend_from_slice(&body);
    let (address, server) = spawn_single_response_http_server(response);
    let output = run_source(&format!(
        r#"
"http://{address}/large" http_get error err var
err get "kind" at
"#
    ));
    server.join().expect("HTTP server should finish");

    assert_run_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("String(\"HttpBodyTooLarge\")"),
        "stdout should contain HTTP body limit error, got:\n{stdout}"
    );
}

#[test]
fn run_limits_concurrent_spawned_tasks() {
    let mut source = String::new();
    for _ in 0..65 {
        source.push_str("[ 250 sleep ] spawn drop\n");
    }
    let source_path = write_source(&source);

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg(&source_path)
        .output()
        .expect("rco run should launch");

    assert!(
        !output.status.success(),
        "rco run should reject too many concurrent tasks"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("task limit exceeded"),
        "stderr should explain the task limit, got:\n{stderr}"
    );
}

#[test]
fn run_exit_uses_requested_process_status() {
    let source_path = write_source("7 exit\n");
    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg(&source_path)
        .output()
        .expect("rco run should launch");

    assert_eq!(output.status.code(), Some(7));
}

#[test]
fn temp_source_path_returns_unique_paths() {
    assert_ne!(temp_source_path(), temp_source_path());
}

fn run_source(source: &str) -> std::process::Output {
    let source_path = write_source(source);

    Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg(&source_path)
        .output()
        .expect("rco run should launch")
}

fn write_source(source: &str) -> PathBuf {
    let source_path = temp_source_path();
    fs::create_dir_all(source_path.parent().expect("source path has parent"))
        .expect("temp source directory should be created");
    fs::write(&source_path, source).expect("temp source should be written");
    source_path
}

fn write_source_at(root: &Path, relative_path: &str, source: &str) -> PathBuf {
    let source_path = root.join(relative_path);
    fs::create_dir_all(source_path.parent().expect("source path has parent"))
        .expect("source directory should be created");
    fs::write(&source_path, source).expect("source should be written");
    source_path
}

fn run_git(repo: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .expect("git should launch");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "git {:?} failed in {}\nstdout:\n{stdout}\nstderr:\n{stderr}",
        args,
        repo.display()
    );
}

fn git_stdout(repo: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .expect("git should launch");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "git {:?} failed in {}\nstdout:\n{stdout}\nstderr:\n{stderr}",
        args,
        repo.display()
    );
    stdout.trim().to_string()
}

fn path_to_slash_for_test(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn escape_toml_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn spawn_single_response_http_server(
    response: Vec<u8>,
) -> (std::net::SocketAddr, thread::JoinHandle<()>) {
    let (address, server, _) = spawn_capturing_http_server(response);
    (address, server)
}

fn spawn_capturing_http_server(
    response: Vec<u8>,
) -> (
    std::net::SocketAddr,
    thread::JoinHandle<()>,
    mpsc::Receiver<String>,
) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("local HTTP listener should bind");
    let address = listener.local_addr().expect("listener should have address");
    listener
        .set_nonblocking(true)
        .expect("listener should become nonblocking");
    let (request_tx, request_rx) = mpsc::channel();

    let server = thread::spawn(move || {
        // The full CLI smoke suite launches many rco subprocesses in parallel on
        // Windows CI. Give the client process enough time to start before
        // treating a missing connection as a real failure.
        let mut stream = (0..3_000)
            .find_map(|_| match listener.accept() {
                Ok((stream, _)) => Some(stream),
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(10));
                    None
                }
                Err(error) => panic!("HTTP accept failed: {error}"),
            })
            .expect("client should connect");

        stream
            .set_nonblocking(false)
            .expect("accepted HTTP stream should become blocking");
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("accepted HTTP stream should set read timeout");

        let mut request = Vec::new();
        let mut buffer = [0_u8; 1024];
        loop {
            match std::io::Read::read(&mut stream, &mut buffer) {
                Ok(0) => break,
                Ok(count) => {
                    request.extend_from_slice(&buffer[..count]);
                    if request.windows(4).any(|window| window == b"\r\n\r\n") {
                        break;
                    }
                }
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) =>
                {
                    break;
                }
                Err(error) => panic!("HTTP request read failed: {error}"),
            }
        }
        let _ = request_tx.send(String::from_utf8_lossy(&request).into_owned());

        std::io::Write::write_all(&mut stream, &response).expect("response should write");
        std::io::Write::flush(&mut stream).expect("response should flush");
        let _ = stream.shutdown(Shutdown::Write);
    });

    (address, server, request_rx)
}

fn escape_ricochet_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn example_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("examples")
        .join(name)
}

fn assert_run_success(output: &std::process::Output) {
    assert_run_success_for("rco run", "source", output);
}

fn assert_run_success_for(command: &str, name: &str, output: &std::process::Output) {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "{command} failed for {name}\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}

fn write_lsp_message(output: &mut Vec<u8>, message: &serde_json::Value) {
    let body = serde_json::to_vec(message).expect("LSP message should serialize");
    write!(output, "Content-Length: {}\r\n\r\n", body.len()).expect("LSP header should write");
    output.extend_from_slice(&body);
}

fn read_lsp_messages(output: &[u8]) -> Vec<serde_json::Value> {
    let mut messages = Vec::new();
    let mut cursor = 0usize;
    while cursor < output.len() {
        let header_end = output[cursor..]
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .map(|position| cursor + position)
            .expect("LSP response should include header terminator");
        let header = String::from_utf8_lossy(&output[cursor..header_end]);
        let length = header
            .lines()
            .find_map(|line| line.strip_prefix("Content-Length:"))
            .expect("LSP response should include Content-Length")
            .trim()
            .parse::<usize>()
            .expect("LSP Content-Length should be numeric");
        let body_start = header_end + 4;
        let body_end = body_start + length;
        messages.push(
            serde_json::from_slice(&output[body_start..body_end])
                .expect("LSP response body should be JSON"),
        );
        cursor = body_end;
    }
    messages
}

fn temp_source_path() -> PathBuf {
    static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

    let base = std::env::var_os("CARGO_TARGET_TMPDIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target"));
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after Unix epoch")
        .as_nanos();
    let sequence = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);

    base.join("cli-smoke")
        .join(format!("run-{}-{nanos}-{sequence}", std::process::id()))
        .join("main.rco")
}
