use ricochet_compiler::compile_source;
use ricochet_vm::{Value, Vm};

fn load(vm: &mut Vm, source: &str) {
    let chunk = compile_source("shared-code.rco", source).unwrap();
    vm.run_chunk(&chunk).unwrap();
    // Calls happen after this source chunk is dropped.
}

#[test]
fn function_survives_source_drop_and_replacement_uses_the_new_body() {
    let mut vm = Vm::default();
    load(&mut vm, "( -> Number ) live function\n7\nend\n");
    load(&mut vm, "live\n");
    assert_eq!(vm.stack(), &[Value::Number(7)]);
    load(&mut vm, "( -> Number ) live function\n13\nend\n");
    load(&mut vm, "live\n");
    assert_eq!(vm.stack(), &[Value::Number(7), Value::Number(13)]);
}

#[test]
fn image_owns_callable_code_after_the_original_vm_is_dropped() {
    let mut vm = Vm::default();
    load(&mut vm, "( -> Number ) live function\n13\nend\n");
    let encoded = serde_json::to_vec(&vm.to_image().unwrap()).unwrap();
    drop(vm);
    let mut restored = Vm::default();
    restored
        .restore_image(serde_json::from_slice(&encoded).unwrap())
        .unwrap();
    load(&mut restored, "live\n");
    assert_eq!(restored.stack(), &[Value::Number(13)]);
}

#[test]
fn failed_shared_callable_preserves_its_callers_stack() {
    let mut vm = Vm::default();
    load(
        &mut vm,
        "42\n( -> Number ) broken function\n99 missing_body_word\nend\n",
    );
    let before = vm.stack().to_vec();
    let invoke = compile_source("shared-code.rco", "broken\n").unwrap();
    assert!(vm.run_chunk(&invoke).is_err());
    assert_eq!(vm.stack(), before.as_slice());
}

#[test]
fn replacing_an_executing_function_keeps_its_current_body_alive() {
    use ricochet_bytecode::{Chunk, Op, SourceSpan};
    let span = SourceSpan {
        file: "replacement.rco".into(),
        start: 0,
        end: 1,
        line: 1,
        column: 1,
    };
    let mut replacement = Chunk::new("replacement.rco");
    replacement.push(Op::PushNumber(13), span.clone());
    let mut active = Chunk::new("replacement.rco");
    let block = active.push_block(replacement);
    active.push(
        Op::AddFunction {
            name: "live".into(),
            block,
            args: None,
        },
        span.clone(),
    );
    active.push(Op::PushNumber(7), span.clone());
    let mut main = Chunk::new("replacement.rco");
    let block = main.push_block(active);
    main.push(
        Op::AddFunction {
            name: "live".into(),
            block,
            args: None,
        },
        span.clone(),
    );
    main.push(Op::CallWord("live".into()), span.clone());
    main.push(Op::CallWord("live".into()), span);
    let mut vm = Vm::default();
    vm.run_chunk(&main).unwrap();
    assert_eq!(vm.stack(), &[Value::Number(7), Value::Number(13)]);
}
