use std::fs;
use std::io::Write;
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

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
    assert!(view.contains("{ title get }"));
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
fn run_exposes_http_client_capability() {
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
                    thread::sleep(std::time::Duration::from_millis(10));
                    None
                }
                Err(error) => panic!("HTTP accept failed: {error}"),
            })
            .expect("client should connect");
        let mut request = [0_u8; 1024];
        let _ = std::io::Read::read(&mut stream, &mut request);
        std::io::Write::write_all(
            &mut stream,
            b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\nConnection: close\r\n\r\npong",
        )
        .expect("response should write");
    });
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
fn run_limits_http_response_body_size() {
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
                    thread::sleep(std::time::Duration::from_millis(10));
                    None
                }
                Err(error) => panic!("HTTP accept failed: {error}"),
            })
            .expect("client should connect");
        let mut request = [0_u8; 1024];
        let _ = std::io::Read::read(&mut stream, &mut request);
        let body = vec![b'x'; 1_048_577];
        std::io::Write::write_all(
            &mut stream,
            format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .as_bytes(),
        )
        .expect("response headers should write");
        std::io::Write::write_all(&mut stream, &body).expect("response body should write");
    });
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
fn run_exit_uses_requested_process_status() {
    let source_path = write_source("7 exit\n");
    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg(&source_path)
        .output()
        .expect("rco run should launch");

    assert_eq!(output.status.code(), Some(7));
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
    let base = std::env::var_os("CARGO_TARGET_TMPDIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target"));
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after Unix epoch")
        .as_nanos();

    base.join("cli-smoke")
        .join(format!("run-{}-{nanos}", std::process::id()))
        .join("main.rco")
}
