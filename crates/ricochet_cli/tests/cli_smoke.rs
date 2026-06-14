use std::fs;
use std::io::Write;
use std::net::{Shutdown, TcpListener};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
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
    assert!(controller.contains("HomeController Controller subclass"));
    assert!(view.contains("href=\"/assets/app.css\""));
    assert!(view.contains("{ title get }"));
    assert!(stylesheet.contains("font-family"));
    assert!(model.contains("User Model subclass"));
    assert!(model.contains("\"displayName\""));
    assert!(user_controller.contains("UserController Controller subclass"));
    assert!(user_controller.contains("users array"));
    assert!(user_controller.contains(".push!"));
    assert!(user_controller.contains("userCount var"));
    assert!(users_view.contains("{ userCount get }"));
    assert!(test.contains("ApplicationSmokeTest TestCase subclass"));
    assert!(test.contains("User new"));
    assert!(test.contains(".displayName"));
    assert!(test.contains("users array"));
    assert!(test.contains(".push!"));

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

    let model = fs::read_to_string(project_path.join("app").join("Models").join("User.rco"))
        .expect("model should exist");
    assert!(model.contains("users table"));
    assert!(model.contains("id field"));

    let controller = fs::read_to_string(
        project_path
            .join("app")
            .join("Controllers")
            .join("UserController.rco"),
    )
    .expect("user controller should exist");
    assert!(controller.contains("User .default-page"));
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
    assert!(auth_controller.contains(".remove!"));
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
            br#"User Model subclass
  email field
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
User Model subclass
  email field
  "displayName" [ self .email get ] !method
end

"User" new
"ada@example.com" swap .email set
.displayName
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
    fs::write(&source_path, r#"map "name" "Ada" !put .name get"#)
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
User Model subclass
email field
"label" [ self .email get ] !method
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
User Model subclass
email field
"label" [ self .email get ] !method
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
        "User Model subclass\n  email field\n  \"label\" [\n    self .email get\n  ] !method\nend\n"
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
"Ada" $users .push! drop
$users .count println
0 $users .at println
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
fn doc_generates_markdown_for_declarations_and_doc_comments() {
    let source_path = temp_source_path();
    let root = source_path.parent().expect("source path has parent");
    write_source_at(
        root,
        "models/user.rco",
        r#"
(( User records from an existing table. ))
User Model subclass
  (( users table mapping ))
  users table

  (( Primary email address. ))
  email field

  (( Display name fallback. ))
  displayName method
    self .email get
  end
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
    assert!(stdout.contains("- Field: `email`"));
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

#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
#[test]
fn package_gui_creates_standalone_executable_that_exports_webview_document() {
    let main_path = temp_source_path();
    let root = main_path.parent().expect("source path has parent");
    write_source_at(
        root,
        "main.rco",
        "\"GUI Smoke\" \"<main><p>Hello desktop</p></main>\" webview .window value document var\n",
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

    let new_output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("new")
        .arg(&project_path)
        .output()
        .expect("rco new should launch");
    assert_run_success_for("rco new", "mvc_app", &new_output);

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
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
#[test]
fn package_gui_rejects_unsupported_hosts() {
    let main_path = temp_source_path();
    let root = main_path.parent().expect("source path has parent");
    write_source_at(
        root,
        "main.rco",
        "\"GUI Smoke\" \"<main><p>Hello desktop</p></main>\" webview .window\n",
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
        stderr.contains("Error: stack underflow in +"),
        "stderr should include anyhow error, got:\n{stderr}"
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
UserTest TestCase subclass
  "testDisplayName" [
    "ada@example.com"
    "ada@example.com" assert-equals
  ] !method
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
UserTest TestCase subclass
  "testFastPass" [
    "ada@example.com"
    "ada@example.com" assert-equals
  ] !method

  "testSlowFail" [
    "ada@example.com"
    "grace@example.com" assert-equals
  ] !method
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
MatchingTest TestCase subclass
  "testOnlyThisRuns" [
    1 1 assert-equals
  ] !method
end
"#,
    )
    .expect("matching test should be written");
    fs::write(
        tests_dir.join("IgnoredTest.rco"),
        format!(
            r#"
"{sentinel_source}" "side effect" fs .write-text! drop

IgnoredTest TestCase subclass
  "testIgnored" [
    1 1 assert-equals
  ] !method
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
UserTest TestCase subclass
  "testDisplayName" [
    "ada@example.com"
    "grace@example.com" assert-equals
  ] !method
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
Counter Object subclass
  ( limit ) "sumTo" [
    limit var
    0 current var
    0 total var

    current get limit get < while
      current get 1 + current set
      total get current get + total set
    end

    total get
  ] !method
end

5 Counter new .sumTo
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
Counter Object subclass
  previous field
end

nil counter var
0 steps var

counter get Counter new .previous set counter set
counter get Counter new .previous set counter set
counter get Counter new .previous set counter set

counter get nil? false = while
  counter get .previous get counter set
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
task get .id
task get .status
task get .pending?
task get .running?
task get .completed?
task get .failed?
tasks
tasks .count
task get await
task get .status
task get .completed?
task get .failed?
task get .pending?
task get await
tasks .count
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
        stdout.contains(r#"Map({"id": Number(0), "status": String("running")})"#),
        "stdout should include running task metadata, got:\n{stdout}"
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
  "done" events get .push! drop
  7
] spawn task var
150 sleep
events get .count
task get .status
task get .completed?
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
fn run_awaits_multiple_tasks_with_await_all() {
    let output = run_source(
        r#"
handles array
[ 20 2 + ] spawn handles get .push! drop
[ 30 4 + ] spawn handles get .push! drop
handles get await-all
handles get await-all
tasks .count
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
User Model subclass
  email field
  "displayName" [ self .email get ] !method
end

"User" new
"ada@example.com" swap .email set
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
"Widget" className var
"label" methodName var
className get "Object" subclass
className get "name" field
className get methodName get [ self .name get ] !method
className get new
"dynamic" swap .name set
.label
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
"Ada" users get .push! drop
"Grace" users get .push! drop
1 "Lin" users get .insert! drop
1 users get .remove-at! drop

map settings var
"theme" "dark" settings get .put! drop

0 6 range numbers var
[ 2 * ] numbers get .transform doubled var
[ 4 > ] doubled get .select selected var

users get .count
0 users get .at
"theme" settings get .has?
"theme" settings get .at
settings get .keys .count
0 [ + ] selected get .reduce
[ 10 = ] doubled get .any?
[ 2 > ] doubled get .all?
[ 8 = ] doubled get .find

list queue var
1 queue get .push! drop
2 queue get .push! drop
queue get .count

Set new tags var
"rco" tags get .push! drop
"rco" tags get .push! drop
tags get .count
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
"Ada" users get .push! drop

"dynamicUsers" array
"Grace" dynamicUsers get .push! drop

settings map
"theme" "dark" settings get .put! drop

queue list
1 queue get .push! drop

tags Set
"rco" tags get .push! drop

users get .count
0 users get .at
dynamicUsers get .count
"theme" settings get .at
queue get .count
tags get .count
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
"  Ada  " .trim
"ada" .uppercase
"ADA" .lowercase
"ric" "ricochet" .starts-with?
"chet" "ricochet" .ends-with?
"coc" "ricochet" .contains?
"," "ada,grace" .split .count
"-" "," "ada,grace" .split .join
"Grace" "Ada" .concat
"Ada" .length
42 to-string
"42" .to-number value
map payload var
"name" "Ada" payload get .put! drop
payload get json-encode
"{\"name\":\"Ada\"}" json-decode value "name" swap .at
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
2 4 "ricochet" .slice
"co" "ricochet" .index-of
"c" "ricochet" .last-index-of
3 "ha" .repeat
"alpha\nbeta\n" .lines .count
"cat" .chars "," swap .join
" \n" .blank?
"  Ada" .trim-start
"Ada  " .trim-end
"zzz" "ricochet" .index-of nil?
"zzz" "ricochet" .last-index-of nil?
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
fn run_supports_collection_view_quality_of_life_words() {
    let output = run_source(
        r#"
names array
"Ada" names get .push! drop
"Grace" names get .push! drop
"Lin" names get .push! drop
array .first nil?
array .last nil?
names get .first
names get .last
"," 2 names get .take .join
"," 1 names get .skip .join
"," names get .reverse .join
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
"name" "Ada" bag get .put! drop
bag get inspect println
"Ada" debug
bag get .count
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
"hello-world_42" slug get .matches?
"bad slug!" slug get .matches?
"\\d+" regex value digits var
"abc123def" digits get .find "text" swap .at
"abc123def" digits get .find "start" swap .at
"abc123def" digits get .find "end" swap .at
"([a-z]+)-(\\d+)" regex value pair var
"item-42" pair get .captures "1" swap .at
"item-42" pair get .captures "2" swap .at
"abc123def456" "#" digits get .replace
"[" regex .error?
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
"ValidationError" "bad input" fail .error?
99 "ValidationError" "bad input" fail .unwrap-or
[ 2 * ] 21 ok .map-result value
[ 1 + ok ] 41 ok .and-then value
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
Widget Object subclass
  name field
  "label" [ self .name get ] !method
end

array type
Widget new class-of
Widget new Widget instance-of?
"label" Widget new responds-to?
Widget fields .count
Widget methods "label" swap .has?
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
args .count
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
fn run_exposes_webview_builder_words_with_capability_controls() {
    let source_path = write_source(
        r#"
"Save <Now>" "save" webview .button println
"Counter" "<main>Ready</main>" webview .window value document var
"html" $document .at println
"width" $document .at println
"height" $document .at println
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
read-line .trim println
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
"{data}" "hello from Ricochet" fs .write-text! value drop
"{data}" fs .read-text value
"{data}" fs .exists?
"{directory}" fs .list value .count 1 >=
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
"{inside}" fs .read-text value
"{outside}" fs .read-text error denied var
"kind" denied get .at
"{outside}" fs .exists?
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
"{data}" fs .read-text value
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
"{data}" fs .read-text value
"{data}" "changed" fs .write-text! error writeDenied var
"kind" writeDenied get .at
"{directory}" fs .create-dir! error createDenied var
"kind" createDenied get .at
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
fn run_sandboxed_capability_profile_allows_bounded_http() {
    let (address, server) = spawn_single_response_http_server(
        b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\nConnection: close\r\n\r\npong".to_vec(),
    );
    let source_path = write_source(&format!(
        r#"
"http://{address}/ping" http .get value response var
"body" response get .at
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
"http://{address}/ping" http .get value response var
"status" response get .at
"body" response get .at
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
"http://{address}/ping" http .get-task task var
task get .id
task get await value response var
"status" response get .at
"body" response get .at
task get .status
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
"message" "hello" payload get .put! drop
"http://{address}/messages" payload get http .post-json-task task var
task get await value response var
"status" response get .at
"body" response get .at
task get .completed?
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
"http://{address}/ping" http .get value response var
"status" response get .at
"body" response get .at
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
"http://{address}/redirect" http .get value response var
"status" response get .at
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
"message" "hello" payload get .put! drop
"http://{address}/redirect" payload get http .post-json value response var
"status" response get .at
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
"http://{address}/redirect" http .get-task task var
task get await value response var
"status" response get .at
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
"message" "hello" payload get .put! drop
"http://{address}/redirect" payload get http .post-json-task task var
task get await value response var
"status" response get .at
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
"http://127.0.0.1:1/blocked" http .get error denied var
"kind" denied get .at
"message" denied get .at
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
"http://127.0.0.1:1/blocked" http .get-task task var
task get await error denied var
"kind" denied get .at
"message" denied get .at
task get .completed?
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
"http://{address}/large" http .get error err var
"kind" err get .at
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
    let listener = TcpListener::bind("127.0.0.1:0").expect("local HTTP listener should bind");
    let address = listener.local_addr().expect("listener should have address");
    listener
        .set_nonblocking(true)
        .expect("listener should become nonblocking");

    let server = thread::spawn(move || {
        let mut stream = (0..500)
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

        std::io::Write::write_all(&mut stream, &response).expect("response should write");
        std::io::Write::flush(&mut stream).expect("response should flush");
        let _ = stream.shutdown(Shutdown::Write);
    });

    (address, server)
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
