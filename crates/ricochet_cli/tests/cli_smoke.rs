use std::fs;
use std::io::{Cursor, Read, Write};
use std::net::{Shutdown, TcpListener};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use flate2::write::GzEncoder;
use flate2::Compression;
use serde_json::json;
use sha2::{Digest, Sha256};
use tar::{Builder, EntryType, Header};

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
    assert!(view.contains("{ $title }"));
    assert!(stylesheet.contains("font-family"));
    assert!(model.contains("User Model Subclass"));
    assert!(model.contains("\"displayName\""));
    assert!(user_controller.contains("UserController Controller Subclass"));
    assert!(user_controller.contains("users array"));
    assert!(user_controller.contains("push!"));
    assert!(user_controller.contains("userCount var"));
    assert!(users_view.contains("{ $userCount }"));
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
    assert!(controller.contains("User default_page"));
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
    assert!(auth_controller.contains("$session \"user_email\""));
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
    let source_path = write_source("\"Ada\" 3 less_than?\n");

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
        stderr.contains("type error in less_than?"),
        "stderr should include runtime error, got:\n{stderr}"
    );
    assert!(
        stderr.contains("main.rco:1:9"),
        "stderr should include runtime source location, got:\n{stderr}"
    );
    assert!(
        stderr.contains("| \"Ada\" 3 less_than?"),
        "stderr should include runtime source line, got:\n{stderr}"
    );
    assert!(
        stderr.contains("help: while executing CallWord(\"less_than?\") in <main>"),
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
fn run_supports_float_literals_arithmetic_comparison_and_json() {
    let source_path = write_source(
        r#"
          1.5 2 + println
          4 2.0 / println
          1 1.0 = println
          "{\"value\":1.25}" json_decode value "value" at println
        "#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg(&source_path)
        .output()
        .expect("rco run should launch");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "float program should run\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert_eq!(stdout.trim(), "3.5\n2.0\ntrue\n1.25\n[]");
}

#[test]
fn run_supports_checked_numeric_conversion_words() {
    let source_path = write_source(
        r#"
          "127" to_tinyint value println
          "128" to_tinyint error "kind" at println
          255 to_unsigned_tinyint value println
          256 to_unsigned_tinyint error "kind" at println
          12.0 to_integer value println
          12.5 to_integer error "kind" at println
          "1.25" to_float value println
          1.23456789 to_float32 value println
        "#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg(&source_path)
        .output()
        .expect("rco run should launch");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "conversion program should run\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert_eq!(
        stdout.trim(),
        "127\nRangeError\n255\nRangeError\n12\nRangeError\n1.25\n1.2345678806304932\n[]"
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
fn lsp_diagnostics_warns_for_legacy_get_variable_reads() {
    let source_path = write_source(
        r#""Ada" name var
name get println
"name" get println
"Ada" ok result var
result get value println
"result" get value println
"#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("lsp-diagnostics")
        .arg(&source_path)
        .output()
        .expect("rco lsp-diagnostics should launch");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "rco lsp-diagnostics should succeed for lint output\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let payload: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("diagnostics output should be JSON");
    let diagnostics = payload["diagnostics"]
        .as_array()
        .expect("diagnostics should be an array");
    assert_eq!(diagnostics.len(), 2, "stdout:\n{stdout}");
    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic["message"]
            == "prefer $name for variable reads"
            && diagnostic["severity"] == 2
            && diagnostic["code"] == "prefer-dollar-reference"),
        "stdout should include $name warning, got:\n{stdout}"
    );
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic["message"] == "prefer $result for variable reads"),
        "stdout should include $result warning, got:\n{stdout}"
    );
}

#[test]
fn lsp_diagnostics_reports_leading_dot_replacements() {
    let source_path = write_source("http .request\n");

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("lsp-diagnostics")
        .arg(&source_path)
        .output()
        .expect("rco lsp-diagnostics should launch");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "rco lsp-diagnostics should succeed for leading-dot diagnostic output\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let payload: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("diagnostics output should be JSON");
    let diagnostics = payload["diagnostics"]
        .as_array()
        .expect("diagnostics should be an array");
    assert_eq!(diagnostics.len(), 1, "stdout:\n{stdout}");
    assert_eq!(diagnostics[0]["code"], "leading-dot-syntax");
    assert_eq!(diagnostics[0]["data"]["replacement"], "http_request");
    assert_eq!(diagnostics[0]["range"]["start"]["line"], 0);
    assert_eq!(diagnostics[0]["range"]["start"]["character"], 0);
    assert_eq!(diagnostics[0]["range"]["end"]["character"], 13);
}

#[test]
fn lint_passes_for_canonical_source() {
    let source_path = write_source(
        r#""Ada" name var
$name println
"name" get println
"#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("lint")
        .arg(&source_path)
        .output()
        .expect("rco lint should launch");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "rco lint should pass for canonical source\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("no diagnostics"),
        "stdout should report a clean lint run, got:\n{stdout}"
    );
}

#[test]
fn lint_reports_legacy_get_warnings_in_tree() {
    let source_path = temp_source_path();
    let root = source_path.parent().expect("source path has parent");
    write_source_at(
        root,
        "main.rco",
        r#""Ada" name var
name get println
"name" get println
"#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("lint")
        .arg(root)
        .output()
        .expect("rco lint should launch");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !output.status.success(),
        "rco lint should fail for style warnings\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stderr.contains("warning[prefer-dollar-reference]")
            && stderr.contains("prefer $name for variable reads"),
        "stderr should include legacy variable-read warning, got:\n{stderr}"
    );
    assert!(
        stderr.contains("lint found 1 diagnostic"),
        "stderr should summarize diagnostics, got:\n{stderr}"
    );
}

#[test]
fn lint_json_reports_diagnostics() {
    let source_path = write_source(
        r#""Ada" name var
name get println
"#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("lint")
        .arg("--json")
        .arg(&source_path)
        .output()
        .expect("rco lint should launch");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !output.status.success(),
        "rco lint --json should fail when diagnostics exist\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    let payload: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("lint output should be JSON");
    assert_eq!(payload["file_count"], 1);
    assert_eq!(payload["diagnostic_count"], 1);
    assert_eq!(
        payload["files"][0]["diagnostics"][0]["code"],
        "prefer-dollar-reference"
    );
    assert!(
        stderr.contains("lint found 1 diagnostic"),
        "stderr should summarize diagnostics, got:\n{stderr}"
    );
}

#[test]
fn words_json_lists_builtin_editor_inventory() {
    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("words")
        .arg("--json")
        .output()
        .expect("rco words should launch");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "rco words --json should pass\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let payload: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("words output should be JSON");
    let words = payload.as_array().expect("words output should be an array");
    assert!(
        words.len() > 200,
        "inventory should embed the full reference catalog, got {} entries\nstdout:\n{stdout}",
        words.len()
    );
    assert!(
        words.iter().any(|entry| entry["word"] == "env_get"),
        "inventory should include env_get\nstdout:\n{stdout}"
    );
    assert!(
        words.iter().any(|entry| entry["word"] == "secret_resolve"),
        "inventory should include secret_resolve\nstdout:\n{stdout}"
    );
    assert!(
        words
            .iter()
            .any(|entry| entry["word"] == "http_request_new"),
        "inventory should include structured HTTP helpers\nstdout:\n{stdout}"
    );
    for alias in ["env", "GET", "multiply", "length", "webview_document"] {
        assert!(
            words.iter().any(|entry| entry["word"] == alias),
            "inventory should include token alias {alias}\nstdout:\n{stdout}"
        );
    }
    for prose_alias in ["Active Record", "collection", "HTTP"] {
        assert!(
            words.iter().all(|entry| entry["word"] != prose_alias),
            "inventory should not expose prose alias {prose_alias} as a completion\nstdout:\n{stdout}"
        );
    }
}

#[test]
fn words_check_validates_reference_docs_and_textmate_inventory() {
    let root = repo_root_for_test();
    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("words")
        .arg("--check")
        .arg("--docs-app")
        .arg(root.join("docs/reference/app.js"))
        .arg("--grammar")
        .arg(root.join("editors/vscode/syntaxes/ricochet.tmLanguage.json"))
        .output()
        .expect("rco words --check should launch");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "rco words --check should pass\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("word inventory check passed"),
        "stdout should report a passing inventory check, got:\n{stdout}"
    );
    assert!(
        stdout.contains("0 duplicate reference entries"),
        "stdout should confirm the reference catalog has no duplicate primary words, got:\n{stdout}"
    );
}

#[test]
fn words_check_rejects_duplicate_reference_words() {
    let root = repo_root_for_test();
    let temp = temp_source_path()
        .parent()
        .expect("temp source path should have parent")
        .join("words-duplicate-check");
    fs::create_dir_all(&temp).expect("temp words check dir should be created");
    let docs_app = temp.join("app.js");
    let source = fs::read_to_string(root.join("docs/reference/app.js"))
        .expect("reference docs app should be readable");
    let duplicate = r#"  {
    "word": "env_get",
    "aliases": [],
    "group": "system",
    "stack": "name:string -> result(string)",
    "body": "Duplicate fixture.",
    "example": "\"RICOCHET_EXAMPLE_TEST\" env_get"
  }
];"#;
    let terminator = if source.contains("\r\n];\r\n\r\nconst groupLabels") {
        "\r\n];\r\n\r\nconst groupLabels"
    } else {
        "\n];\n\nconst groupLabels"
    };
    assert!(
        source.contains(terminator),
        "reference docs fixture should contain WORDS terminator"
    );
    let source = source.replacen(
        terminator,
        &format!(",\n{duplicate}\n\nconst groupLabels"),
        1,
    );
    assert!(
        source.contains("\"Duplicate fixture.\""),
        "duplicate docs fixture should include injected entry"
    );
    fs::write(&docs_app, source).expect("duplicate docs app fixture should be written");

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("words")
        .arg("--check")
        .arg("--docs-app")
        .arg(&docs_app)
        .arg("--grammar")
        .arg(root.join("editors/vscode/syntaxes/ricochet.tmLanguage.json"))
        .output()
        .expect("rco words --check should launch");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "rco words --check should reject duplicate reference entries\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stderr.contains("docs reference contains duplicate words: env_get"),
        "stderr should identify the duplicate word, got:\n{stderr}"
    );
}

#[test]
fn words_check_rejects_malformed_reference_word_entries() {
    let root = repo_root_for_test();
    let temp = temp_source_path()
        .parent()
        .expect("temp source path should have parent")
        .join("words-malformed-check");
    fs::create_dir_all(&temp).expect("temp words check dir should be created");
    let docs_app = temp.join("app.js");
    let source = fs::read_to_string(root.join("docs/reference/app.js"))
        .expect("reference docs app should be readable");
    let malformed = r#"  {
    "word": "bad docs entry",
    "aliases": [],
    "group": "mystery",
    "stack": "",
    "body": "",
    "example": ""
  }
];"#;
    let terminator = if source.contains("\r\n];\r\n\r\nconst groupLabels") {
        "\r\n];\r\n\r\nconst groupLabels"
    } else {
        "\n];\n\nconst groupLabels"
    };
    assert!(
        source.contains(terminator),
        "reference docs fixture should contain WORDS terminator"
    );
    let source = source.replacen(
        terminator,
        &format!(",\n{malformed}\n\nconst groupLabels"),
        1,
    );
    fs::write(&docs_app, source).expect("malformed docs app fixture should be written");

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("words")
        .arg("--check")
        .arg("--docs-app")
        .arg(&docs_app)
        .arg("--grammar")
        .arg(root.join("editors/vscode/syntaxes/ricochet.tmLanguage.json"))
        .output()
        .expect("rco words --check should launch");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "rco words --check should reject malformed reference entries\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stderr.contains("bad docs entry: unknown group 'mystery'")
            && stderr.contains("bad docs entry: missing stack")
            && stderr.contains("bad docs entry: missing body")
            && stderr.contains("bad docs entry: missing example"),
        "stderr should identify malformed docs fields, got:\n{stderr}"
    );
}

#[test]
fn words_check_rejects_stale_textmate_builtin_words() {
    let root = repo_root_for_test();
    let temp = temp_source_path()
        .parent()
        .expect("temp source path should have parent")
        .join("words-stale-grammar-check");
    fs::create_dir_all(&temp).expect("temp words check dir should be created");
    let grammar_path = temp.join("ricochet.tmLanguage.json");
    let grammar = fs::read_to_string(root.join("editors/vscode/syntaxes/ricochet.tmLanguage.json"))
        .expect("TextMate grammar should be readable");
    let grammar = grammar.replace(
        "webview_document)(?!\\\\S)",
        "webview_document|stale_builtin)(?!\\\\S)",
    );
    assert!(
        grammar.contains("stale_builtin"),
        "stale builtin fixture should include injected word"
    );
    fs::write(&grammar_path, grammar).expect("stale grammar fixture should be written");

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("words")
        .arg("--check")
        .arg("--docs-app")
        .arg(root.join("docs/reference/app.js"))
        .arg("--grammar")
        .arg(&grammar_path)
        .output()
        .expect("rco words --check should launch");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "rco words --check should reject stale grammar builtins\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stderr.contains("TextMate builtin regex contains undocumented words: stale_builtin"),
        "stderr should identify the stale builtin word, got:\n{stderr}"
    );
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
    assert!(
        messages.iter().any(|message| message["id"] == 2
            && message["result"]["items"]
                .as_array()
                .expect("completion result should contain items")
                .iter()
                .any(|item| item["label"] == "workspace_read_text")),
        "completion response should include embedded reference catalog words\nstdout:\n{stdout}"
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
    target get to_string println
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
    found get to_string println
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
fn fmt_preserves_unsupported_leading_dot_syntax() {
    let source_path = write_source(
        r#"
self .email get println
http .request value
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
        "self .email get println\n\nhttp .request value\n"
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
fn run_rejects_static_imports_with_invalid_string_escapes() {
    let main_path = temp_source_path();
    let root = main_path.parent().expect("source path has parent");
    write_source_at(root, "main.rco", "\"lib/ma\\qth\" import\n7\n");

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg(&main_path)
        .output()
        .expect("rco run should launch");
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !output.status.success(),
        "rco run should reject invalid static import escapes"
    );
    assert!(
        stderr.contains("invalid import string escape \\q"),
        "stderr should explain invalid import escape, got:\n{stderr}"
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
fn install_locks_semver_satisfied_dependency_version() {
    let main_path = temp_source_path();
    let root = main_path.parent().expect("source path has parent");
    write_source_at(
        root,
        "ricochet.toml",
        "[package]\nname = \"app\"\n\n[dependencies.greeter]\npath = \"./packages/greeter\"\nversion = \"^0.2.0\"\n",
    );
    write_source_at(
        root,
        "packages/greeter/ricochet.toml",
        "[package]\nname = \"greeter\"\nversion = \"0.2.3\"\n",
    );

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("install")
        .current_dir(root)
        .output()
        .expect("rco install should launch");

    assert_run_success_for("rco install", "semver satisfied dependency", &output);

    let lock = fs::read_to_string(root.join("ricochet.lock")).expect("lockfile should exist");
    assert!(
        lock.contains("version_req = \"^0.2.0\""),
        "lock should record requested semver constraint, got:\n{lock}"
    );
    assert!(
        lock.contains("version = \"0.2.3\""),
        "lock should record package version, got:\n{lock}"
    );

    let verify = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("verify")
        .current_dir(root)
        .output()
        .expect("rco verify should launch");
    assert_run_success_for("rco verify", "semver satisfied lock", &verify);
}

#[test]
fn audit_reports_locked_dependency_status_as_json() {
    let main_path = temp_source_path();
    let root = main_path.parent().expect("source path has parent");
    write_source_at(
        root,
        "ricochet.toml",
        "[package]\nname = \"app\"\n\n[dependencies.greeter]\npath = \"./packages/greeter\"\nversion = \"^0.2.0\"\n",
    );
    write_source_at(
        root,
        "packages/greeter/ricochet.toml",
        "[package]\nname = \"greeter\"\nversion = \"0.2.3\"\n",
    );

    let install = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("install")
        .current_dir(root)
        .output()
        .expect("rco install should launch");
    assert_run_success_for("rco install", "audit fixture", &install);

    let audit = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("audit")
        .arg("--json")
        .current_dir(root)
        .output()
        .expect("rco audit should launch");
    assert_run_success_for("rco audit --json", "locked dependency", &audit);
    let report: serde_json::Value =
        serde_json::from_slice(&audit.stdout).expect("audit output should be JSON");
    assert_eq!(report["ok"], true);
    assert_eq!(report["dependencies"][0]["name"], "greeter");
    assert_eq!(report["dependencies"][0]["kind"], "path");
    assert_eq!(report["dependencies"][0]["status"], "ok");
    assert_eq!(report["dependencies"][0]["version_req"], "^0.2.0");
    assert_eq!(report["dependencies"][0]["locked_version"], "0.2.3");
    assert!(report["dependencies"][0]["locked_integrity"]
        .as_str()
        .expect("integrity should be a string")
        .starts_with("sha256:"));
}

#[test]
fn install_rejects_unsatisfied_semver_dependency_version() {
    let main_path = temp_source_path();
    let root = main_path.parent().expect("source path has parent");
    write_source_at(
        root,
        "ricochet.toml",
        "[package]\nname = \"app\"\n\n[dependencies.greeter]\npath = \"./packages/greeter\"\nversion = \"^1.0.0\"\n",
    );
    write_source_at(
        root,
        "packages/greeter/ricochet.toml",
        "[package]\nname = \"greeter\"\nversion = \"0.2.3\"\n",
    );

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("install")
        .current_dir(root)
        .output()
        .expect("rco install should launch");

    assert!(
        !output.status.success(),
        "rco install should reject unsatisfied version requirement"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("dependency greeter version 0.2.3 does not satisfy requirement ^1.0.0"),
        "stderr should explain semver mismatch, got:\n{stderr}"
    );
}

#[test]
fn publish_and_install_local_registry_dependency() {
    let main_path = temp_source_path();
    let base = main_path.parent().expect("source path has parent");
    let package_dir = base.join("greeter_pkg");
    let registry = base.join("registry");
    write_source_at(
        &package_dir,
        "ricochet.toml",
        "[package]\nname = \"greeter\"\nversion = \"0.2.3\"\n",
    );
    write_source_at(
        &package_dir,
        "greeting.rco",
        "\"packageHello\" function\n  \"hello from registry\"\nend\n",
    );

    let publish = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("publish")
        .arg(&package_dir)
        .arg("--registry")
        .arg(&registry)
        .output()
        .expect("rco publish should launch");
    assert_run_success_for("rco publish", "local registry package", &publish);
    assert!(
        registry
            .join("greeter")
            .join("0.2.3")
            .join("package")
            .join("greeting.rco")
            .is_file(),
        "published registry package should contain package source"
    );

    let app = base.join("app");
    write_source_at(&app, "ricochet.toml", "[package]\nname = \"app\"\n");
    write_source_at(
        &app,
        "main.rco",
        "\"greeter/greeting\" import\npackageHello\n",
    );

    let add = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("add")
        .arg("registry:greeter")
        .arg("--registry")
        .arg(&registry)
        .arg("--version")
        .arg("^0.2.0")
        .current_dir(&app)
        .output()
        .expect("rco add should launch");
    assert_run_success_for("rco add", "local registry dependency", &add);

    let manifest = fs::read_to_string(app.join("ricochet.toml")).expect("manifest should exist");
    assert!(manifest.contains("[dependencies.greeter]"));
    assert!(manifest.contains("path = \".ricochet/packages/greeter\""));
    assert!(manifest.contains("registry = \""));
    assert!(manifest.contains("version = \"^0.2.0\""));

    let lock = fs::read_to_string(app.join("ricochet.lock")).expect("lockfile should exist");
    assert!(lock.contains("source = \"registry+"));
    assert!(lock.contains("version_req = \"^0.2.0\""));
    assert!(lock.contains("version = \"0.2.3\""));
    assert!(lock.contains("integrity = \"sha256:"));

    let verify = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("verify")
        .current_dir(&app)
        .output()
        .expect("rco verify should launch");
    assert_run_success_for("rco verify", "registry dependency", &verify);

    let run = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg("main.rco")
        .current_dir(&app)
        .output()
        .expect("rco run should launch");
    assert_run_success(&run);
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(
        stdout.contains("String(\"hello from registry\")"),
        "stdout should show imported registry package result, got:\n{stdout}"
    );
}

#[test]
fn registry_rebuild_writes_static_index_and_searches_scoped_packages() {
    let main_path = temp_source_path();
    let base = main_path.parent().expect("source path has parent");
    let package_dir = base.join("greeter_pkg");
    let registry = base.join("registry");
    write_source_at(
        &package_dir,
        "ricochet.toml",
        "[package]\nname = \"@ricochet/greeter\"\nversion = \"0.2.3\"\n",
    );
    write_source_at(
        &package_dir,
        "greeting.rco",
        "\"packageHello\" function\n  \"hello from static registry\"\nend\n",
    );

    let publish = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("publish")
        .arg(&package_dir)
        .arg("--registry")
        .arg(&registry)
        .output()
        .expect("rco publish should launch");
    assert_run_success_for("rco publish", "scoped registry package", &publish);

    let rebuild = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("registry")
        .arg("rebuild")
        .arg(&registry)
        .output()
        .expect("rco registry rebuild should launch");
    assert_run_success_for("rco registry rebuild", "static registry index", &rebuild);

    let index = fs::read_to_string(registry.join("index.toml")).expect("index should exist");
    assert!(index.contains("format = \"ricochet-static-registry-v1\""));
    assert!(index.contains("\"@ricochet/greeter\""));
    assert!(index.contains("packages/@ricochet/greeter.toml"));

    let package = fs::read_to_string(
        registry
            .join("packages")
            .join("@ricochet")
            .join("greeter.toml"),
    )
    .expect("package metadata should exist");
    assert!(package.contains("name = \"@ricochet/greeter\""));
    assert!(package.contains("version = \"0.2.3\""));
    assert!(
        package.contains("archive = \"artifacts/@ricochet/greeter/0.2.3/greeter-0.2.3.tar.gz\"")
    );
    assert!(package.contains("archive_integrity = \"sha256:"));
    assert!(
        registry
            .join("artifacts")
            .join("@ricochet")
            .join("greeter")
            .join("0.2.3")
            .join("greeter-0.2.3.tar.gz")
            .is_file(),
        "registry rebuild should create a static package archive"
    );

    let registry_url = file_url_for_test(&registry.join("index.toml"));
    let search = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("search")
        .arg("greeter")
        .arg("--registry-url")
        .arg(&registry_url)
        .output()
        .expect("rco search should launch");
    assert_run_success_for("rco search", "scoped static registry package", &search);
    let stdout = String::from_utf8_lossy(&search.stdout);
    assert!(
        stdout.contains("@ricochet/greeter 0.2.3"),
        "search should show scoped package and version, got:\n{stdout}"
    );
}

#[test]
fn add_installs_static_registry_url_dependency_with_local_alias() {
    let main_path = temp_source_path();
    let base = main_path.parent().expect("source path has parent");
    let package_dir = base.join("greeter_pkg");
    let registry = base.join("registry");
    write_source_at(
        &package_dir,
        "ricochet.toml",
        "[package]\nname = \"@ricochet/greeter\"\nversion = \"0.2.3\"\n",
    );
    write_source_at(
        &package_dir,
        "greeting.rco",
        "\"packageHello\" function\n  \"hello from static registry\"\nend\n",
    );

    let publish = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("publish")
        .arg(&package_dir)
        .arg("--registry")
        .arg(&registry)
        .output()
        .expect("rco publish should launch");
    assert_run_success_for("rco publish", "scoped registry package", &publish);
    let rebuild = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("registry")
        .arg("rebuild")
        .arg(&registry)
        .output()
        .expect("rco registry rebuild should launch");
    assert_run_success_for("rco registry rebuild", "static registry index", &rebuild);

    let app = base.join("app");
    write_source_at(&app, "ricochet.toml", "[package]\nname = \"app\"\n");
    write_source_at(
        &app,
        "main.rco",
        "\"greeter/greeting\" import\npackageHello\n",
    );
    let registry_url = file_url_for_test(&registry.join("index.toml"));

    let add = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("add")
        .arg("registry:@ricochet/greeter")
        .arg("--registry-url")
        .arg(&registry_url)
        .arg("--as")
        .arg("greeter")
        .arg("--version")
        .arg("^0.2.0")
        .current_dir(&app)
        .output()
        .expect("rco add should launch");
    assert_run_success_for("rco add", "static registry dependency", &add);

    let manifest = fs::read_to_string(app.join("ricochet.toml")).expect("manifest should exist");
    assert!(manifest.contains("[dependencies.greeter]"));
    assert!(manifest.contains("package = \"@ricochet/greeter\""));
    assert!(manifest.contains("path = \".ricochet/packages/greeter\""));
    assert!(manifest.contains(&format!(
        "registry = \"{}\"",
        escape_toml_string(&registry_url)
    )));
    assert!(manifest.contains("version = \"^0.2.0\""));

    let lock = fs::read_to_string(app.join("ricochet.lock")).expect("lockfile should exist");
    assert!(lock.contains("[package.greeter]"));
    assert!(lock.contains("package = \"@ricochet/greeter\""));
    assert!(lock.contains("source = \"registry+"));
    assert!(lock.contains("#@ricochet/greeter\""));
    assert!(lock.contains("version_req = \"^0.2.0\""));
    assert!(lock.contains("version = \"0.2.3\""));
    assert!(lock.contains("integrity = \"sha256:"));

    let verify = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("verify")
        .current_dir(&app)
        .output()
        .expect("rco verify should launch");
    assert_run_success_for("rco verify", "static registry dependency", &verify);

    let run = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg("main.rco")
        .current_dir(&app)
        .output()
        .expect("rco run should launch");
    assert_run_success(&run);
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(
        stdout.contains("String(\"hello from static registry\")"),
        "stdout should show imported static registry package result, got:\n{stdout}"
    );
}

#[test]
fn static_registry_install_rejects_same_version_lock_replacement() {
    let main_path = temp_source_path();
    let base = main_path.parent().expect("source path has parent");
    let package_dir = base.join("greeter_pkg");
    let replacement_package_dir = base.join("replacement_greeter_pkg");
    let registry = base.join("registry");
    let replacement_registry = base.join("replacement_registry");
    write_source_at(
        &package_dir,
        "ricochet.toml",
        "[package]\nname = \"greeter\"\nversion = \"0.2.3\"\n",
    );
    write_source_at(
        &package_dir,
        "greeting.rco",
        "\"packageHello\" function\n  \"locked static registry version\"\nend\n",
    );
    let first_publish = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("publish")
        .arg(&package_dir)
        .arg("--registry")
        .arg(&registry)
        .output()
        .expect("rco publish should launch");
    assert_run_success_for(
        "rco publish",
        "first static registry package",
        &first_publish,
    );
    let first_rebuild = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("registry")
        .arg("rebuild")
        .arg(&registry)
        .output()
        .expect("rco registry rebuild should launch");
    assert_run_success_for(
        "rco registry rebuild",
        "first static registry",
        &first_rebuild,
    );

    let app = base.join("app");
    let registry_url = file_url_for_test(&registry.join("index.toml"));
    write_source_at(
        &app,
        "ricochet.toml",
        &format!(
            "[package]\nname = \"app\"\n\n[dependencies.greeter]\npath = \".ricochet/packages/greeter\"\nregistry = \"{}\"\nversion = \"^0.2.0\"\n",
            escape_toml_string(&registry_url)
        ),
    );
    write_source_at(
        &app,
        "main.rco",
        "\"greeter/greeting\" import\npackageHello\n",
    );

    let first_install = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("install")
        .current_dir(&app)
        .output()
        .expect("rco install should launch");
    assert_run_success_for(
        "rco install",
        "first static registry install",
        &first_install,
    );
    let first_lock = fs::read_to_string(app.join("ricochet.lock")).expect("lockfile should exist");
    assert!(first_lock.contains("version = \"0.2.3\""));
    assert!(first_lock.contains("integrity = \"sha256:"));

    write_source_at(
        &replacement_package_dir,
        "ricochet.toml",
        "[package]\nname = \"greeter\"\nversion = \"0.2.3\"\n",
    );
    write_source_at(
        &replacement_package_dir,
        "greeting.rco",
        "\"packageHello\" function\n  \"replacement static registry version\"\nend\n",
    );
    let replacement_publish = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("publish")
        .arg(&replacement_package_dir)
        .arg("--registry")
        .arg(&replacement_registry)
        .output()
        .expect("rco publish should launch");
    assert_run_success_for(
        "rco publish",
        "replacement static registry package",
        &replacement_publish,
    );
    let replacement_rebuild = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("registry")
        .arg("rebuild")
        .arg(&replacement_registry)
        .output()
        .expect("rco registry rebuild should launch");
    assert_run_success_for(
        "rco registry rebuild",
        "replacement static registry",
        &replacement_rebuild,
    );

    fs::copy(
        replacement_registry.join("packages").join("greeter.toml"),
        registry.join("packages").join("greeter.toml"),
    )
    .expect("replacement static metadata should overwrite original metadata");
    let artifact_relative = Path::new("artifacts")
        .join("greeter")
        .join("0.2.3")
        .join("greeter-0.2.3.tar.gz");
    fs::copy(
        replacement_registry.join(&artifact_relative),
        registry.join(&artifact_relative),
    )
    .expect("replacement static artifact should overwrite original artifact");
    fs::remove_dir_all(app.join(".ricochet").join("packages").join("greeter"))
        .expect("generated package cache should be removable for reinstall");

    let second_install = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("install")
        .current_dir(&app)
        .output()
        .expect("rco install should launch");
    assert!(
        !second_install.status.success(),
        "ordinary install should reject same-version static registry replacement"
    );
    let stderr = String::from_utf8_lossy(&second_install.stderr);
    assert!(
        stderr.contains("static registry package greeter 0.2.3 integrity changed"),
        "stderr should explain the locked package mismatch, got:\n{stderr}"
    );
    let second_lock =
        fs::read_to_string(app.join("ricochet.lock")).expect("lockfile should still exist");
    assert_eq!(
        first_lock, second_lock,
        "failed static registry reinstall must leave the lockfile unchanged"
    );
}

#[test]
fn static_registry_install_rejects_archive_traversal_entries() {
    let main_path = temp_source_path();
    let base = main_path.parent().expect("source path has parent");
    let registry = base.join("registry");
    let app = base.join("app");
    let archive = static_registry_archive_with_regular_entry("../escape.txt", b"nope");
    write_static_registry_fixture(&registry, "greeter", "0.2.3", &archive);
    write_source_at(
        &app,
        "ricochet.toml",
        &format!(
            "[package]\nname = \"app\"\n\n[dependencies.greeter]\npath = \".ricochet/packages/greeter\"\nregistry = \"{}\"\nversion = \"^0.2.0\"\n",
            escape_toml_string(&file_url_for_test(&registry.join("index.toml")))
        ),
    );

    let install = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("install")
        .current_dir(&app)
        .output()
        .expect("rco install should launch");

    assert!(
        !install.status.success(),
        "rco install should reject traversal archive entries"
    );
    let stderr = String::from_utf8_lossy(&install.stderr);
    assert!(
        stderr.contains("static registry archive path must not contain .."),
        "stderr should explain rejected traversal entry, got:\n{stderr}"
    );
    assert!(
        !base.join("escape.txt").exists(),
        "rejected archive must not write outside the package cache"
    );
}

#[test]
fn static_registry_install_rejects_archive_link_entries() {
    let main_path = temp_source_path();
    let base = main_path.parent().expect("source path has parent");
    let registry = base.join("registry");
    let app = base.join("app");
    let archive = static_registry_archive_with_symlink_entry("link.rco", "ricochet.toml");
    write_static_registry_fixture(&registry, "greeter", "0.2.3", &archive);
    write_source_at(
        &app,
        "ricochet.toml",
        &format!(
            "[package]\nname = \"app\"\n\n[dependencies.greeter]\npath = \".ricochet/packages/greeter\"\nregistry = \"{}\"\nversion = \"^0.2.0\"\n",
            escape_toml_string(&file_url_for_test(&registry.join("index.toml")))
        ),
    );

    let install = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("install")
        .current_dir(&app)
        .output()
        .expect("rco install should launch");

    assert!(
        !install.status.success(),
        "rco install should reject archive links"
    );
    let stderr = String::from_utf8_lossy(&install.stderr);
    assert!(
        stderr.contains("static registry package archives must not contain links"),
        "stderr should explain rejected link entry, got:\n{stderr}"
    );
}

#[test]
fn static_registry_rejects_duplicate_versions_in_metadata() {
    let main_path = temp_source_path();
    let base = main_path.parent().expect("source path has parent");
    let registry = base.join("registry");
    let archive = static_registry_archive_with_regular_entry("ricochet.toml", b"");
    write_static_registry_fixture(&registry, "greeter", "0.2.3", &archive);
    let package_metadata = registry.join("packages").join("greeter.toml");
    let archive_integrity = sha256_integrity_for_bytes(&archive);
    fs::write(
        &package_metadata,
        format!(
            "[package]\nname = \"greeter\"\n\n[[versions]]\nversion = \"0.2.3\"\narchive = \"artifacts/greeter-0.2.3.tar.gz\"\narchive_integrity = \"{archive_integrity}\"\npackage_integrity = \"sha256:{}\"\nyanked = false\n\n[[versions]]\nversion = \"0.2.3\"\narchive = \"artifacts/greeter-0.2.3.tar.gz\"\narchive_integrity = \"{archive_integrity}\"\npackage_integrity = \"sha256:{}\"\nyanked = false\n",
            "0".repeat(64),
            "0".repeat(64)
        ),
    )
    .expect("duplicate metadata should be written");

    let search = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("search")
        .arg("greeter")
        .arg("--registry-url")
        .arg(file_url_for_test(&registry.join("index.toml")))
        .output()
        .expect("rco search should launch");

    assert!(
        !search.status.success(),
        "rco search should reject duplicate static registry versions"
    );
    let stderr = String::from_utf8_lossy(&search.stderr);
    assert!(
        stderr.contains("static registry package greeter lists duplicate version 0.2.3"),
        "stderr should explain duplicate version metadata, got:\n{stderr}"
    );
}

#[test]
fn registry_check_rejects_archive_integrity_mismatch() {
    let main_path = temp_source_path();
    let base = main_path.parent().expect("source path has parent");
    let package_dir = base.join("greeter_pkg");
    let registry = base.join("registry");
    write_source_at(
        &package_dir,
        "ricochet.toml",
        "[package]\nname = \"greeter\"\nversion = \"0.2.3\"\n",
    );
    write_source_at(
        &package_dir,
        "greeting.rco",
        "\"packageHello\" function\n  \"hello\"\nend\n",
    );
    let publish = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("publish")
        .arg(&package_dir)
        .arg("--registry")
        .arg(&registry)
        .output()
        .expect("rco publish should launch");
    assert_run_success_for("rco publish", "hash mismatch package", &publish);
    let rebuild = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("registry")
        .arg("rebuild")
        .arg(&registry)
        .output()
        .expect("rco registry rebuild should launch");
    assert_run_success_for("rco registry rebuild", "hash mismatch package", &rebuild);

    let package_metadata = registry.join("packages").join("greeter.toml");
    replace_toml_string_line(
        &package_metadata,
        "archive_integrity",
        &format!("sha256:{}", "0".repeat(64)),
    );

    let check = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("registry")
        .arg("check")
        .arg(&registry)
        .output()
        .expect("rco registry check should launch");

    assert!(
        !check.status.success(),
        "rco registry check should reject archive integrity mismatch"
    );
    let stderr = String::from_utf8_lossy(&check.stderr);
    assert!(
        stderr.contains("static registry archive for greeter 0.2.3 has integrity"),
        "stderr should explain archive integrity mismatch, got:\n{stderr}"
    );
}

#[test]
fn static_registry_rejects_malformed_index() {
    let main_path = temp_source_path();
    let registry = main_path
        .parent()
        .expect("source path has parent")
        .join("registry");
    write_source_at(&registry, "index.toml", "[packages]\ngreeter = 42\n");

    let search = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("search")
        .arg("greeter")
        .arg("--registry-url")
        .arg(file_url_for_test(&registry.join("index.toml")))
        .output()
        .expect("rco search should launch");

    assert!(
        !search.status.success(),
        "rco search should reject malformed static registry index"
    );
    let stderr = String::from_utf8_lossy(&search.stderr);
    assert!(
        stderr.contains("static registry index must include [registry] format"),
        "stderr should explain malformed index, got:\n{stderr}"
    );
}

#[test]
fn static_registry_rejects_plain_http_registry_urls() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("local HTTP listener should bind");
    let address = listener.local_addr().expect("listener should have address");
    drop(listener);

    let search = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("search")
        .arg("greeter")
        .arg("--registry-url")
        .arg(format!("http://{address}/index.toml"))
        .output()
        .expect("rco search should launch");

    assert!(
        !search.status.success(),
        "rco search should reject plain HTTP registry URLs"
    );
    let stderr = String::from_utf8_lossy(&search.stderr);
    assert!(
        stderr.contains("must start with https:// or file://"),
        "stderr should explain rejected HTTP registry URL, got:\n{stderr}"
    );
}

#[test]
fn static_registry_install_rejects_absolute_archive_urls_in_metadata() {
    let main_path = temp_source_path();
    let base = main_path.parent().expect("source path has parent");
    let registry = base.join("registry");
    let app = base.join("app");
    let archive = static_registry_archive_with_regular_entry("ricochet.toml", b"");
    write_static_registry_fixture(&registry, "greeter", "0.2.3", &archive);
    replace_toml_string_line(
        &registry.join("packages").join("greeter.toml"),
        "archive",
        "https://packages.example.test/greeter-0.2.3.tar.gz",
    );
    write_source_at(
        &app,
        "ricochet.toml",
        &format!(
            "[package]\nname = \"app\"\n\n[dependencies.greeter]\npath = \".ricochet/packages/greeter\"\nregistry = \"{}\"\nversion = \"^0.2.0\"\n",
            escape_toml_string(&file_url_for_test(&registry.join("index.toml")))
        ),
    );

    let install = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("install")
        .current_dir(&app)
        .output()
        .expect("rco install should launch");

    assert!(
        !install.status.success(),
        "rco install should reject absolute archive URLs from metadata"
    );
    let stderr = String::from_utf8_lossy(&install.stderr);
    assert!(
        stderr.contains("archive must be a registry-relative path"),
        "stderr should explain rejected archive URL, got:\n{stderr}"
    );
}

#[test]
fn static_registry_install_rejects_archive_unpacked_size_over_cap() {
    let main_path = temp_source_path();
    let base = main_path.parent().expect("source path has parent");
    let registry = base.join("registry");
    let app = base.join("app");
    let archive =
        static_registry_archive_with_raw_entry("huge.bin", &[], b'0', "", Some(129 * 1024 * 1024));
    write_static_registry_fixture(&registry, "greeter", "0.2.3", &archive);
    write_source_at(
        &app,
        "ricochet.toml",
        &format!(
            "[package]\nname = \"app\"\n\n[dependencies.greeter]\npath = \".ricochet/packages/greeter\"\nregistry = \"{}\"\nversion = \"^0.2.0\"\n",
            escape_toml_string(&file_url_for_test(&registry.join("index.toml")))
        ),
    );

    let install = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("install")
        .current_dir(&app)
        .output()
        .expect("rco install should launch");

    assert!(
        !install.status.success(),
        "rco install should reject archives whose declared unpacked size exceeds the cap"
    );
    let stderr = String::from_utf8_lossy(&install.stderr);
    assert!(
        stderr.contains("static registry archive entry huge.bin is too large")
            || stderr.contains("static registry package archive unpacks to more than"),
        "stderr should explain rejected archive size, got:\n{stderr}"
    );
}

#[test]
fn first_party_packages_publish_to_static_registry_and_import_from_aliases() {
    let main_path = temp_source_path();
    let base = main_path.parent().expect("source path has parent");
    let repo = repo_root_for_test();
    let registry = base.join("registry");
    let packages = [
        ("ricochet_auth", "@ricochet/auth", "auth"),
        ("ricochet_ai", "@ricochet/ai", "ai"),
        ("ricochet_forms", "@ricochet/forms", "forms"),
        (
            "ricochet_test_helpers",
            "@ricochet/test_helpers",
            "test_helpers",
        ),
    ];

    for (directory, identity, _) in packages {
        let package_dir = repo.join("packages").join(directory);
        assert!(
            package_dir.is_dir(),
            "first-party package {identity} should exist at {}",
            package_dir.display()
        );
        let publish = Command::new(env!("CARGO_BIN_EXE_rco"))
            .arg("publish")
            .arg(&package_dir)
            .arg("--registry")
            .arg(&registry)
            .output()
            .expect("rco publish should launch");
        assert_run_success_for("rco publish", identity, &publish);
    }

    let rebuild = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("registry")
        .arg("rebuild")
        .arg(&registry)
        .output()
        .expect("rco registry rebuild should launch");
    assert_run_success_for("rco registry rebuild", "first-party packages", &rebuild);

    let check = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("registry")
        .arg("check")
        .arg(&registry)
        .output()
        .expect("rco registry check should launch");
    assert_run_success_for("rco registry check", "first-party packages", &check);

    let registry_url = file_url_for_test(&registry.join("index.toml"));
    let search = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("search")
        .arg("ricochet")
        .arg("--registry-url")
        .arg(&registry_url)
        .output()
        .expect("rco search should launch");
    assert_run_success_for("rco search", "first-party packages", &search);
    let search_stdout = String::from_utf8_lossy(&search.stdout);
    for (_, identity, _) in packages {
        assert!(
            search_stdout.contains(identity),
            "search should list {identity}, got:\n{search_stdout}"
        );
    }

    let app = base.join("app");
    write_source_at(
        &app,
        "ricochet.toml",
        "[package]\nname = \"first_party_app\"\n",
    );
    write_source_at(
        &app,
        "main.rco",
        r#"
"auth/session" import
"ai/openai" import
"forms/validation" import
"test_helpers/assertions" import

session map
session get "user_id" "ada" put! drop
session get auth_user_present

headers map
"Authorization" "Bearer token" headers get ai_header_put
headers get "Authorization" at

"body" "Hello" form_field
"value" at

"auth ok" "auth ok" test_assert_equal
"package imports ready"
"#,
    );

    for (_, identity, alias) in packages {
        let add = Command::new(env!("CARGO_BIN_EXE_rco"))
            .arg("add")
            .arg(format!("registry:{identity}"))
            .arg("--registry-url")
            .arg(&registry_url)
            .arg("--as")
            .arg(alias)
            .arg("--version")
            .arg("^0.1.0")
            .current_dir(&app)
            .output()
            .expect("rco add should launch");
        assert_run_success_for("rco add", identity, &add);
    }

    let run = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg("main.rco")
        .current_dir(&app)
        .output()
        .expect("rco run should launch");
    assert_run_success(&run);
    let stdout = String::from_utf8_lossy(&run.stdout);
    for expected in [
        "Bool(true)",
        "String(\"Bearer token\")",
        "String(\"Hello\")",
        "String(\"package imports ready\")",
    ] {
        assert!(
            stdout.contains(expected),
            "first-party package app should output {expected}, got:\n{stdout}"
        );
    }
}

#[test]
fn publish_records_provenance_and_signature_hooks() {
    let main_path = temp_source_path();
    let base = main_path.parent().expect("source path has parent");
    let package_dir = base.join("greeter_pkg");
    let registry = base.join("registry");
    let provenance_file = base.join("provenance.json");
    let signature_file = base.join("signature.sig");
    write_source_at(
        &package_dir,
        "ricochet.toml",
        "[package]\nname = \"greeter\"\nversion = \"0.2.3\"\n",
    );
    write_source_at(
        &package_dir,
        "greeting.rco",
        "\"packageHello\" function\n  \"hello from signed registry\"\nend\n",
    );
    fs::create_dir_all(base).expect("base temp directory should exist");
    fs::write(
        &provenance_file,
        r#"{"builder":"local-ci","source":"workspace"}"#,
    )
    .expect("provenance file should be written");
    fs::write(&signature_file, "detached-signature").expect("signature file should be written");

    let publish = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("publish")
        .arg(&package_dir)
        .arg("--registry")
        .arg(&registry)
        .arg("--provenance-file")
        .arg(&provenance_file)
        .arg("--signature-file")
        .arg(&signature_file)
        .arg("--signature-kind")
        .arg("minisign")
        .output()
        .expect("rco publish should launch");
    assert_run_success_for("rco publish", "provenance package", &publish);

    let version_root = registry.join("greeter").join("0.2.3");
    let metadata =
        fs::read_to_string(version_root.join("metadata.toml")).expect("metadata should exist");
    assert!(metadata.contains("provenance"));
    assert!(metadata.contains("attestation = \"provenance.attestation\""));
    assert!(metadata.contains("attestation_integrity = \"sha256:"));
    assert!(metadata.contains("signature = \"signature.sig\""));
    assert!(metadata.contains("signature_integrity = \"sha256:"));
    assert!(metadata.contains("signature_kind = \"minisign\""));
    assert!(version_root.join("provenance.attestation").is_file());
    assert!(version_root.join("signature.sig").is_file());

    let app = base.join("app");
    write_source_at(&app, "ricochet.toml", "[package]\nname = \"app\"\n");
    let add = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("add")
        .arg("registry:greeter")
        .arg("--registry")
        .arg(&registry)
        .arg("--version")
        .arg("^0.2.0")
        .current_dir(&app)
        .output()
        .expect("rco add should launch");
    assert_run_success_for("rco add", "provenance registry dependency", &add);

    let lock = fs::read_to_string(app.join("ricochet.lock")).expect("lockfile should exist");
    assert!(lock.contains("provenance = \"sha256:"));
    assert!(lock.contains("signature = \"sha256:"));
    assert!(lock.contains("signature_kind = \"minisign\""));

    let audit = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("audit")
        .arg("--json")
        .current_dir(&app)
        .output()
        .expect("rco audit should launch");
    assert_run_success_for("rco audit --json", "provenance dependency", &audit);
    let report: serde_json::Value =
        serde_json::from_slice(&audit.stdout).expect("audit output should be JSON");
    let dependency = &report["dependencies"][0];
    assert!(dependency["locked_provenance"]
        .as_str()
        .expect("provenance should be present")
        .starts_with("sha256:"));
    assert!(dependency["locked_signature"]
        .as_str()
        .expect("signature should be present")
        .starts_with("sha256:"));
    assert_eq!(dependency["locked_signature_kind"], "minisign");
}

#[test]
fn publish_dry_run_does_not_create_registry_directory() {
    let main_path = temp_source_path();
    let base = main_path.parent().expect("source path has parent");
    let package_dir = base.join("greeter_pkg");
    let registry = base.join("missing_registry");
    write_source_at(
        &package_dir,
        "ricochet.toml",
        "[package]\nname = \"greeter\"\nversion = \"0.2.3\"\n",
    );
    write_source_at(
        &package_dir,
        "greeting.rco",
        "\"packageHello\" function\n  \"hello\"\nend\n",
    );

    let publish = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("publish")
        .arg(&package_dir)
        .arg("--registry")
        .arg(&registry)
        .arg("--dry-run")
        .output()
        .expect("rco publish should launch");
    assert_run_success_for("rco publish --dry-run", "local registry package", &publish);
    assert!(
        !registry.exists(),
        "dry-run publish should not create the registry directory"
    );
}

#[test]
fn registry_install_keeps_locked_version_when_newer_version_exists() {
    let main_path = temp_source_path();
    let base = main_path.parent().expect("source path has parent");
    let package_dir = base.join("greeter_pkg");
    let registry = base.join("registry");
    write_source_at(
        &package_dir,
        "ricochet.toml",
        "[package]\nname = \"greeter\"\nversion = \"0.2.3\"\n",
    );
    write_source_at(
        &package_dir,
        "greeting.rco",
        "\"packageHello\" function\n  \"locked registry version\"\nend\n",
    );
    let first_publish = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("publish")
        .arg(&package_dir)
        .arg("--registry")
        .arg(&registry)
        .output()
        .expect("rco publish should launch");
    assert_run_success_for("rco publish", "first registry version", &first_publish);

    let app = base.join("app");
    let registry_source = escape_toml_string(&path_to_slash_for_test(&registry));
    write_source_at(
        &app,
        "ricochet.toml",
        &format!(
            "[package]\nname = \"app\"\n\n[dependencies.greeter]\npath = \".ricochet/packages/greeter\"\nregistry = \"{registry_source}\"\nversion = \"^0.2.0\"\n"
        ),
    );
    write_source_at(
        &app,
        "main.rco",
        "\"greeter/greeting\" import\npackageHello\n",
    );

    let first_install = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("install")
        .current_dir(&app)
        .output()
        .expect("rco install should launch");
    assert_run_success_for("rco install", "first registry install", &first_install);
    let first_lock = fs::read_to_string(app.join("ricochet.lock")).expect("lockfile should exist");
    assert!(first_lock.contains("version = \"0.2.3\""));

    write_source_at(
        &package_dir,
        "ricochet.toml",
        "[package]\nname = \"greeter\"\nversion = \"0.2.4\"\n",
    );
    write_source_at(
        &package_dir,
        "greeting.rco",
        "\"packageHello\" function\n  \"newer registry version\"\nend\n",
    );
    let second_publish = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("publish")
        .arg(&package_dir)
        .arg("--registry")
        .arg(&registry)
        .output()
        .expect("rco publish should launch");
    assert_run_success_for("rco publish", "second registry version", &second_publish);

    let second_install = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("install")
        .current_dir(&app)
        .output()
        .expect("rco install should launch");
    assert_run_success_for("rco install", "locked registry install", &second_install);
    let second_lock = fs::read_to_string(app.join("ricochet.lock")).expect("lockfile should exist");
    assert!(
        second_lock.contains("version = \"0.2.3\""),
        "install should preserve locked version, got:\n{second_lock}"
    );

    let run = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg("main.rco")
        .current_dir(&app)
        .output()
        .expect("rco run should launch");
    assert_run_success(&run);
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(
        stdout.contains("String(\"locked registry version\")"),
        "stdout should use locked package contents, got:\n{stdout}"
    );
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
fn run_bytecode_trace_file_records_json_debug_events() {
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

    let trace_path = root.join("bytecode-trace.json");
    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run-bytecode")
        .arg("--trace-file")
        .arg(&trace_path)
        .arg(root.join("build").join("app.rcob"))
        .output()
        .expect("rco run-bytecode should launch");
    assert_run_success_for("rco run-bytecode --trace-file", "build/app.rcob", &output);

    let trace = fs::read_to_string(&trace_path).expect("trace file should exist");
    let trace: serde_json::Value = serde_json::from_str(&trace).expect("trace should be JSON");
    assert!(
        trace
            .as_array()
            .expect("trace should be an array")
            .iter()
            .any(|event| event["event"] == "instruction" && event["opcode"] == "CallWord(\"+\")"),
        "bytecode trace should include plus instruction, got:\n{trace:#?}"
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
fn gui_exports_state_actions_and_dispatches_action_callbacks() {
    let main_path = temp_source_path();
    let root = main_path.parent().expect("source path has parent");
    write_source_at(
        root,
        "main.rco",
        r#"
( state -> Map ) render_counter function
  state var
  "Count: " state get "count" at to_string concat webview_text body var
  actions array
  actions get "Increment" "increment" "increment_counter" webview_action push! drop
  "Counter" body get state get actions get webview_window_state value
end

( state event -> Map ) increment_counter function
  event var
  state var
  state get "count" state get "count" at 1 + put! drop
  state get render_counter
end

state map
state get "count" 1 put! drop
state get render_counter document var
"#,
    );
    let initial_export_path = root.join("gui-initial.html");
    let event_export_path = root.join("gui-event.html");

    let preview_output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("gui")
        .arg("main.rco")
        .env("RICOCHET_GUI_EXPORT_HTML", &initial_export_path)
        .current_dir(root)
        .output()
        .expect("rco gui should launch");
    assert_run_success_for("rco gui", "GUI v2 initial document", &preview_output);
    let initial_html =
        fs::read_to_string(&initial_export_path).expect("initial GUI HTML should exist");
    assert!(initial_html.contains("Count: 1"));
    assert!(initial_html.contains("window.__RICOCHET_STATE__"));
    assert!(initial_html.contains("\"count\":1"));
    assert!(initial_html.contains("\"callback\":\"increment_counter\""));

    let event_output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("gui")
        .arg("main.rco")
        .env("RICOCHET_GUI_EXPORT_HTML", &event_export_path)
        .env(
            "RICOCHET_GUI_EVENT",
            r#"{"type":"action","action":"increment"}"#,
        )
        .current_dir(root)
        .output()
        .expect("rco gui should launch with event");
    assert_run_success_for("rco gui", "GUI v2 action dispatch", &event_output);
    let event_html = fs::read_to_string(&event_export_path).expect("event GUI HTML should exist");
    assert!(event_html.contains("Count: 2"));
    assert!(event_html.contains("\"count\":2"));
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
"RICOCHET_MVC_PACKAGE_ENV_TEST" env_get value envValue var
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
fn run_debug_next_steps_over_function_body() {
    let source_path = temp_source_path();
    fs::create_dir_all(source_path.parent().expect("source path has parent"))
        .expect("temp source directory should be created");
    fs::write(&source_path, "work function\n  2\n  3\n  +\nend\nwork\n9\n")
        .expect("temp source should be written");

    let mut child = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg("--breakpoint")
        .arg("6")
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
        .write_all(b"next\ncontinue\n")
        .expect("debugger commands should write");

    let output = child.wait_with_output().expect("debugger should finish");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "next debugger command should succeed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout_has_pause_at(&stdout, "breakpoint", 6, "<main>"),
        "first pause should be at function call, got:\n{stdout}"
    );
    assert!(
        stdout_has_pause_at(&stdout, "step", 7, "<main>"),
        "next should pause after the function call returns, got:\n{stdout}"
    );
    assert!(
        !stdout_has_pause_at(&stdout, "step", 2, "work"),
        "next should not step into function body, got:\n{stdout}"
    );
    assert!(
        !stdout_has_pause_at(&stdout, "step", 3, "work"),
        "next should not step into function body, got:\n{stdout}"
    );
}

#[test]
fn run_debug_out_steps_to_caller_frame() {
    let source_path = temp_source_path();
    fs::create_dir_all(source_path.parent().expect("source path has parent"))
        .expect("temp source directory should be created");
    fs::write(&source_path, "work function\n  2\n  3\n  +\nend\nwork\n9\n")
        .expect("temp source should be written");

    let mut child = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
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
        .write_all(b"out\ncontinue\n")
        .expect("debugger commands should write");

    let output = child.wait_with_output().expect("debugger should finish");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "out debugger command should succeed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout_has_pause_at(&stdout, "breakpoint", 2, "work"),
        "first pause should be inside function, got:\n{stdout}"
    );
    assert!(
        stdout_has_pause_at(&stdout, "step", 7, "<main>"),
        "out should pause in caller after return, got:\n{stdout}"
    );
    assert!(
        !stdout_has_pause_at(&stdout, "step", 3, "work"),
        "out should not keep stepping inside function body, got:\n{stdout}"
    );
    assert!(
        !stdout_has_pause_at(&stdout, "step", 4, "work"),
        "out should not keep stepping inside function body, got:\n{stdout}"
    );
}

#[test]
fn run_debug_breakpoint_prints_locals_for_current_frame() {
    let source_path = temp_source_path();
    fs::create_dir_all(source_path.parent().expect("source path has parent"))
        .expect("temp source directory should be created");
    fs::write(
        &source_path,
        "work function\n  41 answer var\n  answer get\nend\nwork\n",
    )
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
        "locals breakpoint should succeed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("locals: [(\"answer\", Number(41))]"),
        "pause should include current frame locals, got:\n{stdout}"
    );
}

#[test]
fn run_trace_file_records_json_debug_events() {
    let source_path = temp_source_path();
    let root = source_path.parent().expect("source path has parent");
    fs::create_dir_all(root).expect("temp source directory should be created");
    fs::write(&source_path, "2\n3\n+\n").expect("temp source should be written");
    let trace_path = root.join("trace.json");

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg("--trace-file")
        .arg(&trace_path)
        .arg(&source_path)
        .output()
        .expect("rco run should launch");
    assert_run_success_for("rco run --trace-file", "trace fixture", &output);

    let trace = fs::read_to_string(&trace_path).expect("trace file should exist");
    let trace: serde_json::Value = serde_json::from_str(&trace).expect("trace should be JSON");
    let events = trace.as_array().expect("trace should be a JSON array");
    assert!(
        events
            .iter()
            .any(|event| event["event"] == "instruction" && event["opcode"] == "CallWord(\"+\")"),
        "trace should include plus instruction event, got:\n{trace:#?}"
    );
    assert!(
        events.iter().any(|event| {
            event["event"] == "instruction"
                && event["stack_after"]
                    .as_array()
                    .is_some_and(|stack| stack.iter().any(|value| value["debug"] == "Number(5)"))
        }),
        "trace should include stack state after addition, got:\n{trace:#?}"
    );
}

#[test]
fn debug_json_streams_json_lines_events() {
    let source_path = write_source("2 3 + println\n");
    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("debug")
        .arg("--json")
        .arg(&source_path)
        .output()
        .expect("rco debug should launch");
    assert_run_success_for("rco debug --json", "source", &output);

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains("TRACE "),
        "json debug stream should not contain text trace lines, got:\n{stdout}"
    );
    let events: Vec<serde_json::Value> = stdout
        .lines()
        .map(|line| serde_json::from_str(line).expect("debug line should be JSON"))
        .collect();
    assert!(
        events.iter().any(|event| {
            event["event"] == "instruction" && event["opcode"] == "CallWord(\"+\")"
        }),
        "json debug stream should include instruction events, got:\n{stdout}"
    );
    assert!(
        events.iter().any(|event| {
            event["event"] == "output" && event["stream"] == "stdout" && event["text"] == "5\n"
        }),
        "json debug stream should carry program stdout as an output event, got:\n{stdout}"
    );
}

#[test]
fn debug_json_pause_includes_task_snapshot() {
    let source_path = write_source("[ 100 sleep 40 2 + ] spawn task var\ntask get id\n");
    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("debug")
        .arg("--json")
        .arg("--breakpoint")
        .arg("2")
        .arg(&source_path)
        .output()
        .expect("rco debug should launch");
    assert_run_success_for("rco debug --json --breakpoint", "task source", &output);

    let stdout = String::from_utf8_lossy(&output.stdout);
    let events: Vec<serde_json::Value> = stdout
        .lines()
        .map(|line| serde_json::from_str(line).expect("debug line should be JSON"))
        .collect();
    let pause = events
        .iter()
        .find(|event| event["event"] == "paused")
        .expect("debug stream should include pause event");
    assert_eq!(pause["reason"], "breakpoint");
    assert_eq!(pause["tasks"][0]["id"], 0);
    assert_eq!(pause["tasks"][0]["status"], "running");
    assert_eq!(pause["tasks"][0]["running"], true);
}

#[test]
fn debug_adapter_serves_breakpoint_stack_scopes_and_variables() {
    let source_path = write_source(
        r#"
"Ada" name var
name get
"Ada" assert_equals
"done"
"#,
    );
    let source = path_to_slash_for_test(&source_path);
    let mut input = Vec::new();
    for message in [
        json!({"seq":1,"type":"request","command":"initialize","arguments":{"adapterID":"ricochet","linesStartAt1":true,"columnsStartAt1":true}}),
        json!({"seq":2,"type":"request","command":"launch","arguments":{"program":source}}),
        json!({"seq":3,"type":"request","command":"setBreakpoints","arguments":{"source":{"path":source},"breakpoints":[{"line":3}]}}),
        json!({"seq":4,"type":"request","command":"configurationDone","arguments":{}}),
        json!({"seq":5,"type":"request","command":"stackTrace","arguments":{"threadId":1}}),
        json!({"seq":6,"type":"request","command":"scopes","arguments":{"frameId":1}}),
        json!({"seq":7,"type":"request","command":"variables","arguments":{"variablesReference":3}}),
        json!({"seq":8,"type":"request","command":"continue","arguments":{"threadId":1}}),
    ] {
        write_lsp_message(&mut input, &message);
    }

    let mut child = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("debug-adapter")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("rco debug-adapter should launch");
    child
        .stdin
        .as_mut()
        .expect("debug adapter stdin should be open")
        .write_all(&input)
        .expect("debug adapter messages should write");
    let output = child
        .wait_with_output()
        .expect("debug adapter should finish");
    assert_run_success_for("rco debug-adapter", "DAP smoke", &output);
    let messages = read_lsp_messages(&output.stdout);

    assert!(messages.iter().any(|message| {
        message["type"] == "event"
            && message["event"] == "stopped"
            && message["body"]["reason"] == "breakpoint"
    }));
    assert!(messages.iter().any(|message| {
        message["type"] == "response"
            && message["command"] == "stackTrace"
            && message["body"]["stackFrames"][0]["line"] == 3
            && message["body"]["stackFrames"][0]["name"] == "<main>"
    }));
    assert!(messages.iter().any(|message| {
        message["type"] == "response"
            && message["command"] == "scopes"
            && message["body"]["scopes"]
                .as_array()
                .expect("scopes should be an array")
                .iter()
                .any(|scope| scope["name"] == "Locals")
    }));
    assert!(messages.iter().any(|message| {
        message["type"] == "response"
            && message["command"] == "variables"
            && message["body"]["variables"]
                .as_array()
                .expect("variables should be an array")
                .iter()
                .any(|variable| variable["name"] == "name")
    }));
    assert!(messages
        .iter()
        .any(|message| { message["type"] == "event" && message["event"] == "terminated" }));
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
    "ada@example.com" assert_equals
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
    "ada@example.com" assert_equals
  ] "testFastPass" Method

  [
    "ada@example.com"
    "grace@example.com" assert_equals
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
    1 1 assert_equals
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
    1 1 assert_equals
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
    "grace@example.com" assert_equals
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
task get task_status
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
task get task_status
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
task get task_status
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
task get release_task
task get task_status
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
        "stdout should show release_task succeeded, got:\n{stdout}"
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
fn run_formats_parses_and_manipulates_timestamps_and_dates() {
    let output = run_source(
        r#"
"2026-06-18T13:14:15.250Z" timestamp_parse value ts var
ts get timestamp_format value
ts get "%Y-%m-%d %H:%M:%S" timestamp_format_pattern value
ts get timestamp_parts value parts var
parts get "year" at
parts get "month" at
parts get "day" at
parts get "hour" at
parts get "minute" at
parts get "second" at
parts get "millisecond" at
parts get timestamp_from_parts value ts get =
ts get 2 duration_hours value timestamp_add value later var
ts get later get timestamp_diff value
later get timestamp_format value
"2026-02-28" date_parse value date var
date get "%Y/%m/%d" date_format value
date get date_to_timestamp value timestamp_format value
date get 1 date_add_days value nextDate var
nextDate get "%Y-%m-%d" date_format value
date get nextDate get date_diff_days value
1 duration_weeks value
93784005 duration_parts value duration var
duration get "days" at
duration get "hours" at
duration get "minutes" at
duration get "seconds" at
duration get "milliseconds" at
"not-a-date" timestamp_parse error "kind" at
"2026-02-30" date_parse error "kind" at
"#,
    );
    assert_run_success_for("rco run", "date/time library", &output);

    let stdout = String::from_utf8_lossy(&output.stdout);
    for expected in [
        "String(\"2026-06-18T13:14:15.250Z\")",
        "String(\"2026-06-18 13:14:15\")",
        "Number(2026)",
        "Number(6)",
        "Number(18)",
        "Number(13)",
        "Number(14)",
        "Number(15)",
        "Number(250)",
        "Bool(true)",
        "Number(7200000)",
        "String(\"2026-06-18T15:14:15.250Z\")",
        "String(\"2026/02/28\")",
        "String(\"2026-02-28T00:00:00.000Z\")",
        "String(\"2026-03-01\")",
        "Number(1)",
        "Number(604800000)",
        "Number(1)",
        "Number(2)",
        "Number(3)",
        "Number(4)",
        "Number(5)",
        "String(\"DateTimeParseError\")",
        "String(\"DateParseError\")",
    ] {
        assert!(
            stdout.contains(expected),
            "stdout should contain {expected}, got:\n{stdout}"
        );
    }
}

#[test]
fn run_awaits_multiple_tasks_with_await_all() {
    let output = run_source(
        r#"
handles array
[ 20 2 + ] spawn handles get swap push! drop
[ 30 4 + ] spawn handles get swap push! drop
handles get await_all
handles get await_all
tasks count
"#,
    );
    assert_run_success_for("rco run", "await_all", &output);

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.matches(r#"Array([Number(22), Number(34)])"#).count() >= 2,
        "stdout should show await_all resolving and reusing task results, got:\n{stdout}"
    );
    assert!(
        stdout.contains("Number(0)"),
        "stdout should show no pending tasks after await_all, got:\n{stdout}"
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
users get 1 remove_at! drop

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
fn showcase_examples_are_runnable_acceptance_suite() {
    let repo = repo_root_for_test();
    let showcase = repo.join("examples").join("showcase");

    let sqlite_notes = showcase.join("sqlite_notes");
    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("check")
        .arg(&sqlite_notes)
        .output()
        .expect("rco check should launch for sqlite notes showcase");
    assert_run_success_for("rco check", "showcase sqlite_notes", &output);

    for script in [
        showcase.join("package_auth_forms").join("main.rco"),
        showcase.join("ai_provider_probe").join("main.rco"),
        showcase.join("debugger_demo.rco"),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_rco"))
            .arg("run")
            .arg(&script)
            .output()
            .unwrap_or_else(|error| {
                panic!("rco run should launch for {}: {error}", script.display())
            });
        assert_run_success_for("rco run", &script.display().to_string(), &output);
    }

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("check")
        .arg(showcase.join("ai_provider_probe").join("live_probe.rco"))
        .output()
        .expect("rco check should launch for live AI probe showcase");
    assert_run_success_for("rco check", "showcase live_probe.rco", &output);

    let export_source = temp_source_path();
    let export_root = export_source
        .parent()
        .expect("temp source should have a parent");
    fs::create_dir_all(export_root).expect("temp export directory should be created");
    let export_path = export_root.join("showcase-gui-task-monitor.html");
    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("gui")
        .arg(showcase.join("gui_task_monitor.rco"))
        .env("RICOCHET_GUI_EXPORT_HTML", &export_path)
        .output()
        .expect("rco gui should launch for showcase task monitor");
    assert_run_success_for("rco gui", "showcase gui_task_monitor", &output);

    let html = fs::read_to_string(&export_path).expect("GUI showcase export should exist");
    assert!(html.contains("Ricochet GUI Task Monitor"));
    assert!(html.contains("data-rco-action=\"increment\""));
    assert!(html.contains("__RICOCHET_STATE__"));
    assert!(html.contains("__RICOCHET_ACTIONS__"));
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
"ricochet" "ric" starts_with?
"ricochet" "chet" ends_with?
"ricochet" "coc" contains?
"ada,grace" "," split count
"ada,grace" "," split "-" join
"Ada" "Grace" concat
"Ada" length
42 to_string
"42" to_number value
map payload var
payload get "name" "Ada" put! drop
payload get json_encode
"{\"name\":\"Ada\"}" json_decode value "name" at
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
"ricochet" "co" index_of
"ricochet" "c" last_index_of
"ha" 3 repeat
"alpha\nbeta\n" lines count
"cat" chars "," join
" \n" blank?
"  Ada" trim_start
"Ada  " trim_end
"ricochet" "zzz" index_of nil?
"ricochet" "zzz" last_index_of nil?
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
true assert_true
false assert_false
42 ok assert_ok
"ValidationError" "bad input" fail assert_error
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
"abc123def" digits get regex_find "text" at
"abc123def" digits get regex_find "start" at
"abc123def" digits get regex_find "end" at
"([a-z]+)-(\\d+)" regex value pair var
"item-42" pair get captures "1" at
"item-42" pair get captures "2" at
digits get "abc123def456" "#" regex_replace
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
99 "ValidationError" "bad input" fail unwrap_or
[ 2 * ] 21 ok map_result value
[ 1 + ok ] 41 ok and_then value
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
Widget new class_of
Widget new Widget instance_of?
"label" Widget new responds_to?
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
"RICOCHET_QOL_TEST" env_get value
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
fn run_supports_env_get_and_env_set_words() {
    let source_path = write_source(
        r#"
"RICOCHET_ENV_WORD_TEST" "from-script" env_set value drop
"RICOCHET_ENV_WORD_TEST" env_get value
"" "bad" env_set error "message" at
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
        stdout.contains("String(\"from-script\")"),
        "stdout should include environment value set by env_set, got:\n{stdout}"
    );
    assert!(
        stdout.contains("String(\"environment variable name must not be empty\")"),
        "stdout should expose invalid env_set assignment as a Result error, got:\n{stdout}"
    );
}

#[test]
fn run_env_allowlist_bounds_process_environment_access() {
    let allowed_source_path = write_source(
        r#"
"RICOCHET_ALLOWED_ENV_TEST" env_get value
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

    let denied_source_path = write_source(r#""RICOCHET_DENIED_ENV_TEST" env_get drop"#);
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
fn run_env_allowlist_bounds_process_environment_writes() {
    let allowed_source_path = write_source(
        r#"
"RICOCHET_ALLOWED_ENV_SET_TEST" "visible" env_set value drop
"RICOCHET_ALLOWED_ENV_SET_TEST" env_get value
"#,
    );

    let allowed = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg("--capability-profile")
        .arg("sandboxed")
        .arg("--env-allow")
        .arg("RICOCHET_ALLOWED_ENV_SET_TEST")
        .arg(&allowed_source_path)
        .output()
        .expect("rco run should launch");

    assert_run_success(&allowed);
    let stdout = String::from_utf8_lossy(&allowed.stdout);
    assert!(
        stdout.contains("String(\"visible\")"),
        "stdout should include allowlisted environment value set by env_set, got:\n{stdout}"
    );

    let denied_source_path =
        write_source(r#""RICOCHET_DENIED_ENV_SET_TEST" "hidden" env_set drop"#);
    let denied = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg("--capability-profile")
        .arg("sandboxed")
        .arg("--env-allow")
        .arg("RICOCHET_ALLOWED_ENV_SET_TEST")
        .arg(&denied_source_path)
        .output()
        .expect("rco run should launch");

    assert!(
        !denied.status.success(),
        "rco run should fail when env_set target is outside allowlist"
    );
    let stderr = String::from_utf8_lossy(&denied.stderr);
    assert!(
        stderr.contains("environment variable is not allowed: RICOCHET_DENIED_ENV_SET_TEST"),
        "stderr should explain env_set allowlist denial, got:\n{stderr}"
    );

    let disabled_source_path = write_source(r#""RICOCHET_DISABLED_ENV_SET_TEST" "x" env_set drop"#);
    let disabled = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg("--no-env")
        .arg(&disabled_source_path)
        .output()
        .expect("rco run should launch");

    assert!(
        !disabled.status.success(),
        "rco run should fail when env_set is disabled"
    );
    let stderr = String::from_utf8_lossy(&disabled.stderr);
    assert!(
        stderr.contains("environment capability is not enabled"),
        "stderr should explain disabled env_set capability, got:\n{stderr}"
    );
}

#[test]
fn run_supports_secret_config_http_and_process_helper_words() {
    let source_path = write_source(
        r#"
"RICOCHET_SECRET_HELPER_TEST" secret_env secretRef var
secretRef get "type" at
secretRef get "name" at
secretRef get secret_resolve value
"dry-token" secret_literal secret_resolve value

config map
provider map
provider get "token" secretRef get put! drop
config get "provider" provider get put! drop

path array
path get "provider" push! drop
path get "token" push! drop
config get path get config_get value secret_resolve value

"POST" "https://api.example.test/v1/chat" http_request_new value request var
request get "secret-token" http_bearer_auth value request set
payload map
payload get "ok" true put! drop
request get payload get http_json_body value request set
request get 3000 http_timeout value request set
request get "headers" at "Authorization" at
request get "json" at "ok" at
request get "timeout_ms" at

options map
options get "RICOCHET_CHILD_SECRET" "child" process_env_put value options set
options get "env" at "RICOCHET_CHILD_SECRET" at
config get "missing" config_get error "message" at
"#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg(&source_path)
        .env("RICOCHET_SECRET_HELPER_TEST", "resolved-token")
        .output()
        .expect("rco run should launch");

    assert_run_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    for expected in [
        "String(\"env\")",
        "String(\"RICOCHET_SECRET_HELPER_TEST\")",
        "String(\"resolved-token\")",
        "String(\"dry-token\")",
        "String(\"Bearer secret-token\")",
        "Bool(true)",
        "Number(3000)",
        "String(\"child\")",
        "String(\"missing config value: missing\")",
    ] {
        assert!(
            stdout.contains(expected),
            "stdout should contain {expected}, got:\n{stdout}"
        );
    }
}

#[test]
fn run_secret_resolve_honors_environment_capability_bounds() {
    let denied_source_path =
        write_source(r#""RICOCHET_SECRET_DENIED_TEST" secret_env secret_resolve drop"#);
    let denied = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg("--capability-profile")
        .arg("sandboxed")
        .arg("--env-allow")
        .arg("RICOCHET_OTHER_SECRET_TEST")
        .arg(&denied_source_path)
        .env("RICOCHET_SECRET_DENIED_TEST", "hidden")
        .output()
        .expect("rco run should launch");

    assert!(
        !denied.status.success(),
        "rco run should fail when secret_resolve target is outside env allowlist"
    );
    let stderr = String::from_utf8_lossy(&denied.stderr);
    assert!(
        stderr.contains("environment variable is not allowed: RICOCHET_SECRET_DENIED_TEST"),
        "stderr should explain secret_resolve allowlist denial, got:\n{stderr}"
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
read_line trim println
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
"{data}" fs_delete value drop
"{data}" fs_exists?
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
    assert!(
        stdout.contains("Bool(false)"),
        "stdout should confirm deleted file absence, got:\n{stdout}"
    );
    assert!(
        !data_path.exists(),
        "fs_delete should remove the requested file"
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
    let bounded_rco = process_root.join(if cfg!(windows) {
        "rco-under-root.exe"
    } else {
        "rco-under-root"
    });
    fs::copy(env!("CARGO_BIN_EXE_rco"), &bounded_rco)
        .expect("test rco executable should copy under process root");
    let rco = escape_ricochet_string(env!("CARGO_BIN_EXE_rco"));
    let bounded_rco_source = escape_ricochet_string(&bounded_rco.to_string_lossy());
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
"{bounded_rco_source}" args get options get process_spawn value result var
result get "success" at
result get "stdout" at "checked" contains?
runtime_capabilities "process" at "root" at "{process_root_source}" =
outsideOptions map
outsideOptions get "cwd" "{outside_root_source}" put! drop
"{bounded_rco_source}" args get outsideOptions get process_spawn error deniedCwd var
deniedCwd get "kind" at
outsideCommandOptions map
outsideCommandOptions get "cwd" "{process_root_source}" put! drop
"{rco}" args get outsideCommandOptions get process_spawn error deniedCommand var
deniedCommand get "kind" at
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
nil snapshot var
0 attempts var
attempts get 50 < while
  job get "id" at process_job value snapshot set
  snapshot get "status" at "exited" = if
    break
  end
  50 sleep
  attempts get 1 + attempts set
end
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
fn run_process_release_drops_completed_job_when_allowed() {
    let source_path = temp_source_path();
    let root = source_path.parent().expect("source path has parent");
    fs::create_dir_all(root).expect("temp source directory should be created");
    let checked_source = root.join("checked-release.rco");
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
nil snapshot var
0 attempts var
attempts get 50 < while
  job get "id" at process_job value snapshot set
  snapshot get "status" at "exited" = if
    break
  end
  50 sleep
  attempts get 1 + attempts set
end
job get "id" at process_release value
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
        stdout.contains("Bool(true)"),
        "stdout should show process_release succeeded, got:\n{stdout}"
    );
    assert!(
        stdout.matches("Number(0)").count() >= 2,
        "stdout should show no retained process jobs after release, got:\n{stdout}"
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
nil read var
0 attempts var
attempts get 50 < while
  readOptions map
  job get "id" at readOptions get process_read value read set
  read get "stdout" at "abc" = if
    break
  end
  50 sleep
  attempts get 1 + attempts set
end
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
nil read var
0 attempts var
attempts get 50 < while
  readOptions map
  session get "id" at readOptions get pty_read value read set
  read get "output" at "ricochet-pty" contains? if
    break
  end
  50 sleep
  attempts get 1 + attempts set
end
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
fn run_pty_release_drops_exited_session_when_allowed() {
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
nil detail var
0 attempts var
attempts get 50 < while
  session get "id" at pty_detail value detail set
  detail get "running" at false = if
    break
  end
  50 sleep
  attempts get 1 + attempts set
end
session get "id" at pty_release value
pty_list count
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
        stdout.contains("Bool(true)"),
        "stdout should show pty_release succeeded, got:\n{stdout}"
    );
    assert!(
        stdout.matches("Number(0)").count() >= 2,
        "stdout should show no retained PTY sessions after release, got:\n{stdout}"
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
$writeOptions "create_parent_dirs" true put! drop
copyOptions map
"inside.txt" $options workspace_read_text value
"." $options workspace_resolve value resolved var
$resolved "inside_root" at
"." $options workspace_list value count 1 >=
"generated/out.txt" "created" $writeOptions workspace_write_text value written var
$written "kind" at "file" =
"generated/out.txt" workspace_metadata value meta var
$meta "relative_path" at "generated/out.txt" =
"inside.txt" "generated/copy.txt" $copyOptions workspace_copy value copied var
$copied "exists" at
"." "generated/copy.txt" workspace_contains?
"generated/delete-me.txt" "remove" $writeOptions workspace_write_text value drop
"generated/delete-me.txt" $options workspace_delete value deletedFile var
$deletedFile "deleted" at
"generated/delete-me.txt" fs_exists? false =
recursiveOptions map
$recursiveOptions "recursive" true put! drop
"generated/delete-dir/nested.txt" "remove" $writeOptions workspace_write_text value drop
"generated/delete-dir" $recursiveOptions workspace_delete value deletedDir var
$deletedDir "deleted" at
"generated/delete-dir" fs_exists? false =
missingOptions map
$missingOptions "missing_ok" true put! drop
"generated/missing.txt" $missingOptions workspace_delete value missingDelete var
$missingDelete "deleted" at false =
"." $options workspace_delete error rootDenied var
$rootDenied "kind" at
"." fs_delete error fsRootDenied var
$fsRootDenied "kind" at
"{outside}" $options workspace_read_text error denied var
$denied "kind" at
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
        stdout.matches("Bool(true)").count() >= 12,
        "stdout should confirm workspace metadata, copy, containment, and capability state, got:\n{stdout}"
    );
    assert!(
        stdout.contains("String(\"PermissionError\")"),
        "stdout should report outside-root and root-delete workspace denials as PermissionError, got:\n{stdout}"
    );
    assert_eq!(
        fs::read_to_string(root.join("generated/out.txt")).expect("written file should exist"),
        "created"
    );
    assert_eq!(
        fs::read_to_string(root.join("generated/copy.txt")).expect("copied file should exist"),
        "inside workspace"
    );
    assert!(
        !root.join("generated/delete-me.txt").exists(),
        "workspace_delete should remove direct file targets"
    );
    assert!(
        !root.join("generated/delete-dir").exists(),
        "workspace_delete should remove recursive directory targets when requested"
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
$writeDenied "kind" at
"{directory}" fs_create_dir error createDenied var
$createDenied "kind" at
"{data}" fs_delete error deleteDenied var
$deleteDenied "kind" at
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
        stdout.matches("String(\"PermissionError\")").count() >= 3,
        "stdout should report write/create/delete denials as PermissionError, got:\n{stdout}"
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
    let existing_path = root.join("existing.txt");
    fs::write(&existing_path, "keep").expect("existing file should be written");
    fs::write(
        &source_path,
        r#"
options map
writeOptions map
$writeOptions "create_parent_dirs" true put! drop
"blocked.txt" "blocked" $writeOptions workspace_write_text error writeDenied var
$writeDenied "kind" at
"blocked-dir" $options workspace_mkdir error mkdirDenied var
$mkdirDenied "kind" at
"existing.txt" $options workspace_delete error deleteDenied var
$deleteDenied "kind" at
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
        stdout.matches("String(\"PermissionError\")").count() >= 3,
        "stdout should report workspace write/create/delete denials as PermissionError, got:\n{stdout}"
    );
    assert!(
        !blocked_path.exists(),
        "read-only workspace policy should not create files"
    );
    assert!(
        !blocked_dir.exists(),
        "read-only workspace policy should not create directories"
    );
    assert_eq!(
        fs::read_to_string(&existing_path).expect("existing file should remain readable"),
        "keep"
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
task get task_status
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
fn run_exposes_http_stream_reads_with_offsets() {
    let (address, server) = spawn_chunked_http_server(vec![
        (b"data: first\n\n".to_vec(), Duration::from_millis(0)),
        (b"data: second\n\n".to_vec(), Duration::from_millis(150)),
    ]);
    let output = run_source(&format!(
        r#"
request map
request get "url" "http://{address}/stream" put! drop
request get "timeout_ms" 10000 put! drop
request get "max_response_bytes" 1024 put! drop
request get http_stream_start value stream var
stream get "id" at id var
50 sleep
options map
id get options get http_stream_read value first var
first get "body" at
first get "offset" at offset var
200 sleep
nextOptions map
nextOptions get "offset" offset get put! drop
id get nextOptions get http_stream_read value second var
second get "body" at
id get http_stream value "status" at
runtime_capabilities "http" at "streams" at
"#
    ));
    server.join().expect("HTTP streaming server should finish");

    assert_run_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("data: first")
            && stdout.contains("data: second")
            && stdout.contains("String(\"completed\")")
            && stdout.contains("Number(1)"),
        "stdout should contain first chunk, second chunk, completed status, and stream count, got:\n{stdout}"
    );
}

#[test]
fn run_http_stream_release_drops_completed_stream() {
    let (address, server) =
        spawn_chunked_http_server(vec![(b"data: done\n\n".to_vec(), Duration::from_millis(0))]);
    let output = run_source(&format!(
        r#"
request map
request get "url" "http://{address}/stream" put! drop
request get "timeout_ms" 10000 put! drop
request get "max_response_bytes" 1024 put! drop
request get http_stream_start value stream var
stream get "id" at id var
nil detail var
0 attempts var
attempts get 50 < while
  id get http_stream value detail set
  detail get "status" at "completed" = if
    break
  end
  50 sleep
  attempts get 1 + attempts set
end
detail get "status" at
id get http_stream_release value
http_streams count
runtime_capabilities "http" at "streams" at
"#
    ));
    server.join().expect("HTTP streaming server should finish");

    assert_run_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("String(\"completed\")") && stdout.contains("Bool(true)"),
        "stdout should show completed stream and successful release, got:\n{stdout}"
    );
    assert!(
        stdout.matches("Number(0)").count() >= 2,
        "stdout should show no retained HTTP streams after release, got:\n{stdout}"
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
fn run_socket_words_require_explicit_capability() {
    let source_path = write_source(
        r#"
options map
"127.0.0.1" 9 $options tcp_connect drop
"#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg(&source_path)
        .output()
        .expect("rco run should launch");

    assert!(
        !output.status.success(),
        "rco run should fail when socket capability is disabled"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("socket capability is not enabled"),
        "stderr should explain disabled socket capability, got:\n{stderr}"
    );
}

#[test]
fn run_sandboxed_profile_allows_bounded_tcp_socket_echo() {
    let (address, server) = spawn_tcp_echo_server();
    let source_path = write_source(&format!(
        r#"
options map
$options "timeout_ms" 5000 put! drop
"127.0.0.1" {port} $options tcp_connect value connection var
$connection "status" at
$connection "id" at id var
$id "ping" tcp_write value "bytes_written" at
readOptions map
$readOptions "timeout_ms" 5000 put! drop
$readOptions "max_bytes" 64 put! drop
$id $readOptions tcp_read value read var
$read "data" at
$id tcp_close value "closed" at
$id tcp_release value
tcp_connections count
runtime_capabilities "sockets" at "tcp_connections" at
"#,
        port = address.port()
    ));

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg("--capability-profile")
        .arg("sandboxed")
        .arg("--socket-allow-host")
        .arg("127.0.0.1")
        .arg(&source_path)
        .output()
        .expect("rco run should launch");
    server.join().expect("TCP echo server should finish");

    assert_run_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("String(\"connected\")")
            && stdout.contains("Number(4)")
            && stdout.contains("String(\"tcp:pong\")")
            && stdout.contains("Bool(true)"),
        "stdout should show connected TCP echo, write count, close, and release, got:\n{stdout}"
    );
    assert!(
        stdout.matches("Number(0)").count() >= 2,
        "stdout should show no retained TCP connections after release, got:\n{stdout}"
    );
}

#[test]
fn run_sandboxed_profile_allows_bounded_websocket_echo() {
    let (address, server) = spawn_websocket_echo_server();
    let source_path = write_source(&format!(
        r#"
options map
$options "timeout_ms" 5000 put! drop
"ws://127.0.0.1:{port}/echo" $options ws_connect value socket var
$socket "status" at
$socket "id" at id var
$id "hello" ws_send value "messages_sent" at
readOptions map
$readOptions "timeout_ms" 5000 put! drop
$id $readOptions ws_read value message var
$message "message_type" at
$message "message" at
$id ws_close value "closed" at
$id ws_release value
ws_connections count
runtime_capabilities "sockets" at "websocket_connections" at
"#,
        port = address.port()
    ));

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg("--capability-profile")
        .arg("sandboxed")
        .arg("--socket-allow-host")
        .arg("127.0.0.1")
        .arg(&source_path)
        .output()
        .expect("rco run should launch");
    server.join().expect("WebSocket echo server should finish");

    assert_run_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("String(\"connected\")")
            && stdout.contains("Number(1)")
            && stdout.contains("String(\"text\")")
            && stdout.contains("String(\"ws:hello\")")
            && stdout.contains("Bool(true)"),
        "stdout should show connected WebSocket echo, sent count, close, and release, got:\n{stdout}"
    );
    assert!(
        stdout.matches("Number(0)").count() >= 2,
        "stdout should show no retained WebSocket connections after release, got:\n{stdout}"
    );
}

#[test]
fn run_websocket_read_rejects_messages_above_max_bytes() {
    let (address, server) = spawn_websocket_echo_server();
    let source_path = write_source(&format!(
        r#"
options map
$options "timeout_ms" 5000 put! drop
"ws://127.0.0.1:{port}/echo" $options ws_connect value socket var
$socket "id" at id var
$id "hello" ws_send value drop
readOptions map
$readOptions "timeout_ms" 5000 put! drop
$readOptions "max_bytes" 2 put! drop
$id $readOptions ws_read error denied var
$id ws_close value drop
$id ws_release value drop
denied get
"#,
        port = address.port()
    ));

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg("--capability-profile")
        .arg("sandboxed")
        .arg("--socket-allow-host")
        .arg("127.0.0.1")
        .arg(&source_path)
        .output()
        .expect("rco run should launch");
    server.join().expect("WebSocket echo server should finish");

    assert_run_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("String(\"WebSocketMessageTooLarge\")"),
        "stdout should show WebSocket read cap rejection, got:\n{stdout}"
    );
}

#[test]
fn run_sandboxed_profile_allows_bounded_tcp_listener_echo() {
    let source_path = write_source(
        r#"
listenOptions map
"127.0.0.1" 0 $listenOptions tcp_listen value listener var
$listener "status" at
$listener "port" at port var
[
  options map
  $options "timeout_ms" 5000 put! drop
  "127.0.0.1" $port $options tcp_connect value client var
  $client "id" at clientId var
  $clientId "from-client" tcp_write value drop
  readOptions map
  $readOptions "timeout_ms" 5000 put! drop
  $readOptions "max_bytes" 64 put! drop
  $clientId $readOptions tcp_read value clientRead var
  $clientId tcp_close value drop
  $clientId tcp_release value drop
  $clientRead "data" at
] spawn clientTask var
acceptOptions map
$acceptOptions "timeout_ms" 5000 put! drop
$listener "id" at $acceptOptions tcp_accept value accepted var
$accepted "id" at serverId var
readOptions map
$readOptions "timeout_ms" 5000 put! drop
$readOptions "max_bytes" 64 put! drop
$serverId $readOptions tcp_read value serverRead var
$serverRead "data" at
$serverId "from-server" tcp_write value "bytes_written" at
$serverId tcp_close value "closed" at
$serverId tcp_release value
$clientTask await
$listener "id" at tcp_listener_close value "closed" at
$listener "id" at tcp_listener_release value
tcp_listeners count
tcp_connections count
runtime_capabilities "sockets" at "tcp_listeners" at
"#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg("--capability-profile")
        .arg("sandboxed")
        .arg("--socket-allow-host")
        .arg("127.0.0.1")
        .arg(&source_path)
        .output()
        .expect("rco run should launch");

    assert_run_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("String(\"listening\")")
            && stdout.contains("String(\"from-client\")")
            && stdout.contains("String(\"from-server\")")
            && stdout.contains("Number(11)")
            && stdout.contains("Bool(true)"),
        "stdout should show TCP listener accept, echo, close, and release, got:\n{stdout}"
    );
    assert!(
        stdout.matches("Number(0)").count() >= 3,
        "stdout should show no retained TCP listeners or connections after release, got:\n{stdout}"
    );
}

#[test]
fn run_sandboxed_profile_allows_bounded_websocket_listener_echo() {
    let source_path = write_source(
        r#"
listenOptions map
"127.0.0.1" 0 $listenOptions ws_listen value listener var
$listener "status" at
"ws://127.0.0.1:" $listener "port" at to_string concat "/echo" concat url var
[
  options map
  $options "timeout_ms" 5000 put! drop
  $url $options ws_connect value client var
  $client "id" at clientId var
  $clientId "from-client" ws_send value drop
  readOptions map
  $readOptions "timeout_ms" 5000 put! drop
  $clientId $readOptions ws_read value clientRead var
  $clientId ws_close value drop
  $clientId ws_release value drop
  $clientRead "message" at
] spawn clientTask var
acceptOptions map
$acceptOptions "timeout_ms" 5000 put! drop
$listener "id" at $acceptOptions ws_accept value accepted var
$accepted "id" at serverId var
readOptions map
$readOptions "timeout_ms" 5000 put! drop
$serverId $readOptions ws_read value serverRead var
$serverRead "message_type" at
$serverRead "message" at
$serverId "from-server" ws_send value "messages_sent" at
$serverId ws_close value "closed" at
$serverId ws_release value
$clientTask await
$listener "id" at ws_listener_close value "closed" at
$listener "id" at ws_listener_release value
ws_listeners count
ws_connections count
runtime_capabilities "sockets" at "websocket_listeners" at
"#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg("--capability-profile")
        .arg("sandboxed")
        .arg("--socket-allow-host")
        .arg("127.0.0.1")
        .arg(&source_path)
        .output()
        .expect("rco run should launch");

    assert_run_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("String(\"listening\")")
            && stdout.contains("String(\"text\")")
            && stdout.contains("String(\"from-client\")")
            && stdout.contains("String(\"from-server\")")
            && stdout.contains("Number(1)")
            && stdout.contains("Bool(true)"),
        "stdout should show WebSocket listener accept, echo, close, and release, got:\n{stdout}"
    );
    assert!(
        stdout.matches("Number(0)").count() >= 3,
        "stdout should show no retained WebSocket listeners or connections after release, got:\n{stdout}"
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

fn file_url_for_test(path: &Path) -> String {
    format!("file:///{}", path_to_slash_for_test(path))
}

fn write_static_registry_fixture(
    registry: &Path,
    package: &str,
    version: &str,
    archive_bytes: &[u8],
) {
    let archive_relative = format!("artifacts/{package}-{version}.tar.gz");
    let archive_path = registry.join(&archive_relative);
    fs::create_dir_all(
        archive_path
            .parent()
            .expect("archive path should have parent"),
    )
    .expect("archive directory should be created");
    fs::write(&archive_path, archive_bytes).expect("archive should be written");

    write_source_at(
        registry,
        "index.toml",
        &format!(
            "[registry]\nformat = \"ricochet-static-registry-v1\"\n\n[packages]\n{package} = \"packages/{package}.toml\"\n"
        ),
    );
    write_source_at(
        registry,
        &format!("packages/{package}.toml"),
        &format!(
            "[package]\nname = \"{package}\"\n\n[[versions]]\nversion = \"{version}\"\narchive = \"{archive_relative}\"\narchive_integrity = \"{}\"\npackage_integrity = \"sha256:{}\"\nyanked = false\n",
            sha256_integrity_for_bytes(archive_bytes),
            "0".repeat(64)
        ),
    );
}

fn static_registry_archive_with_regular_entry(path: &str, contents: &[u8]) -> Vec<u8> {
    if path.contains("..") || path.starts_with('/') || path.contains('\\') {
        return static_registry_archive_with_raw_entry(path, contents, b'0', "", None);
    }
    let encoder = GzEncoder::new(Vec::new(), Compression::default());
    let mut builder = Builder::new(encoder);
    let mut header = Header::new_gnu();
    header.set_mode(0o644);
    header.set_size(contents.len() as u64);
    header.set_cksum();
    builder
        .append_data(&mut header, path, Cursor::new(contents))
        .expect("archive entry should append");
    finish_test_archive(builder)
}

fn static_registry_archive_with_raw_entry(
    path: &str,
    contents: &[u8],
    entry_type: u8,
    link_name: &str,
    size_override: Option<u64>,
) -> Vec<u8> {
    let mut tar = Vec::new();
    let mut header = [0_u8; 512];
    write_tar_header_string(&mut header[0..100], path);
    write_tar_header_octal(&mut header[100..108], 0o644);
    write_tar_header_octal(&mut header[108..116], 0);
    write_tar_header_octal(&mut header[116..124], 0);
    write_tar_header_octal(
        &mut header[124..136],
        size_override.unwrap_or(contents.len() as u64),
    );
    write_tar_header_octal(&mut header[136..148], 0);
    for byte in &mut header[148..156] {
        *byte = b' ';
    }
    header[156] = entry_type;
    write_tar_header_string(&mut header[157..257], link_name);
    header[257..263].copy_from_slice(b"ustar\0");
    header[263..265].copy_from_slice(b"00");
    let checksum = header.iter().map(|byte| *byte as u32).sum::<u32>();
    let checksum_text = format!("{checksum:06o}\0 ");
    header[148..156].copy_from_slice(checksum_text.as_bytes());
    tar.extend_from_slice(&header);
    tar.extend_from_slice(contents);
    let padding = (512 - (contents.len() % 512)) % 512;
    tar.resize(tar.len() + padding, 0);
    tar.extend([0_u8; 1024]);

    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder
        .write_all(&tar)
        .expect("raw test archive should gzip");
    encoder
        .finish()
        .expect("raw test archive gzip should finish")
}

fn write_tar_header_string(field: &mut [u8], value: &str) {
    let bytes = value.as_bytes();
    let length = bytes.len().min(field.len());
    field[..length].copy_from_slice(&bytes[..length]);
}

fn write_tar_header_octal(field: &mut [u8], value: u64) {
    let text = format!("{value:0width$o}\0", width = field.len() - 1);
    field.copy_from_slice(text.as_bytes());
}

fn static_registry_archive_with_symlink_entry(path: &str, target: &str) -> Vec<u8> {
    let encoder = GzEncoder::new(Vec::new(), Compression::default());
    let mut builder = Builder::new(encoder);
    let mut header = Header::new_gnu();
    header
        .set_path(path)
        .expect("symlink archive path should set");
    header.set_entry_type(EntryType::Symlink);
    header
        .set_link_name(target)
        .expect("symlink archive target should set");
    header.set_mode(0o777);
    header.set_size(0);
    header.set_cksum();
    builder
        .append(&header, std::io::empty())
        .expect("symlink archive entry should append");
    finish_test_archive(builder)
}

fn finish_test_archive(mut builder: Builder<GzEncoder<Vec<u8>>>) -> Vec<u8> {
    builder.finish().expect("test archive should finish");
    let encoder = builder
        .into_inner()
        .expect("test archive encoder should finish");
    encoder.finish().expect("test archive gzip should finish")
}

fn sha256_integrity_for_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let hex = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("sha256:{hex}")
}

fn replace_toml_string_line(path: &Path, key: &str, new_value: &str) {
    let source = fs::read_to_string(path).expect("TOML fixture should exist");
    let prefix = format!("{key} = ");
    let replaced = source
        .lines()
        .map(|line| {
            if line.starts_with(&prefix) {
                format!("{key} = \"{new_value}\"")
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(path, format!("{replaced}\n")).expect("TOML fixture should update");
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

fn spawn_chunked_http_server(
    chunks: Vec<(Vec<u8>, Duration)>,
) -> (std::net::SocketAddr, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("local HTTP listener should bind");
    let address = listener.local_addr().expect("listener should have address");
    listener
        .set_nonblocking(true)
        .expect("listener should become nonblocking");

    let server = thread::spawn(move || {
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

        std::io::Write::write_all(
            &mut stream,
            b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n",
        )
        .expect("stream headers should write");
        std::io::Write::flush(&mut stream).expect("stream headers should flush");
        for (chunk, delay) in chunks {
            if !delay.is_zero() {
                thread::sleep(delay);
            }
            std::io::Write::write_all(&mut stream, &chunk).expect("stream chunk should write");
            std::io::Write::flush(&mut stream).expect("stream chunk should flush");
        }
        let _ = stream.shutdown(Shutdown::Write);
    });

    (address, server)
}

fn spawn_tcp_echo_server() -> (std::net::SocketAddr, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("local TCP listener should bind");
    let address = listener.local_addr().expect("listener should have address");
    listener
        .set_nonblocking(true)
        .expect("listener should become nonblocking");

    let server = thread::spawn(move || {
        let Some(mut stream) = accept_local_test_connection(listener) else {
            return;
        };
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("accepted TCP stream should set read timeout");
        let mut buffer = [0_u8; 64];
        let count = stream.read(&mut buffer).expect("TCP request should read");
        assert_eq!(&buffer[..count], b"ping");
        stream
            .write_all(b"tcp:pong")
            .expect("TCP response should write");
        stream.flush().expect("TCP response should flush");
        let _ = stream.shutdown(Shutdown::Both);
    });

    (address, server)
}

fn spawn_websocket_echo_server() -> (std::net::SocketAddr, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("local WebSocket listener should bind");
    let address = listener.local_addr().expect("listener should have address");
    listener
        .set_nonblocking(true)
        .expect("listener should become nonblocking");

    let server = thread::spawn(move || {
        let Some(stream) = accept_local_test_connection(listener) else {
            return;
        };
        stream
            .set_nonblocking(false)
            .expect("accepted WebSocket stream should become blocking");
        let mut socket = tungstenite::accept(stream).expect("WebSocket handshake should complete");
        let message = socket.read().expect("WebSocket message should read");
        let text = message.to_text().expect("WebSocket message should be text");
        socket
            .send(tungstenite::Message::Text(format!("ws:{text}").into()))
            .expect("WebSocket echo should write");
        let _ = socket.close(None);
    });

    (address, server)
}

fn accept_local_test_connection(listener: TcpListener) -> Option<std::net::TcpStream> {
    for _ in 0..500 {
        match listener.accept() {
            Ok((stream, _)) => return Some(stream),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(10));
            }
            Err(error) => panic!("local test accept failed: {error}"),
        }
    }
    None
}

fn escape_ricochet_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn example_path(name: &str) -> PathBuf {
    repo_root_for_test().join("examples").join(name)
}

fn repo_root_for_test() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
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

fn stdout_has_pause_at(stdout: &str, reason: &str, source_line: usize, frame: &str) -> bool {
    let prefix = format!("PAUSE {reason} ");
    let location = format!(":{source_line} [{frame}]");
    stdout
        .lines()
        .any(|line| line.starts_with(&prefix) && line.contains(&location))
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
