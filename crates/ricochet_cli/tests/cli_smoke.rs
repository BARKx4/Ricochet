use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

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
