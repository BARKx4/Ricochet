use anyhow::{bail, Context, Result};
use ricochet_vm::{DebugAction, DebugControl, DebugEvent, DebugPauseReason, DebugTask, Value};
use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize)]
pub(crate) struct DebugWebControlRequest {
    pub(crate) action: String,
    pub(crate) pause_id: Option<usize>,
    pub(crate) line: Option<usize>,
    pub(crate) file: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DebugCommand {
    Resume(DebugAction),
    AddBreakpoint { file: Option<String>, line: usize },
    RemoveBreakpoint { file: Option<String>, line: usize },
    ClearBreakpoints { file: Option<String> },
    ListBreakpoints,
}

pub(crate) fn debug_value_label(value: &Value) -> String {
    let debug_value = debug_value_json(value);
    debug_value
        .get("debug")
        .and_then(|value| value.as_str())
        .map(str::to_string)
        .unwrap_or_else(|| format!("{value:?}"))
}

fn debug_action_from_command(command: &str) -> Option<DebugAction> {
    match command.trim().to_ascii_lowercase().as_str() {
        "" | "s" | "step" => Some(DebugAction::Step),
        "n" | "next" | "over" | "step-over" => Some(DebugAction::StepOver),
        "o" | "out" | "step-out" => Some(DebugAction::StepOut),
        "c" | "continue" => Some(DebugAction::Continue),
        "a" | "abort" | "q" | "quit" => Some(DebugAction::Abort),
        _ => None,
    }
}

pub(crate) fn debug_command_from_command(command: &str) -> Result<DebugCommand> {
    if let Some(action) = debug_action_from_command(command) {
        return Ok(DebugCommand::Resume(action));
    }

    let parts: Vec<_> = command.split_whitespace().collect();
    match parts.as_slice() {
        [name, line]
            if matches!(
                name.to_ascii_lowercase().as_str(),
                "b" | "break" | "breakpoint" | "breakpoint_add" | "add_breakpoint"
            ) =>
        {
            Ok(DebugCommand::AddBreakpoint {
                file: None,
                line: parse_debug_breakpoint_line(line)?,
            })
        }
        [name, line]
            if matches!(
                name.to_ascii_lowercase().as_str(),
                "clear" | "delete" | "remove" | "breakpoint_remove" | "remove_breakpoint"
            ) =>
        {
            Ok(DebugCommand::RemoveBreakpoint {
                file: None,
                line: parse_debug_breakpoint_line(line)?,
            })
        }
        [name]
            if matches!(
                name.to_ascii_lowercase().as_str(),
                "breakpoints_clear" | "breakpoint_clear" | "clear_breakpoints"
            ) =>
        {
            Ok(DebugCommand::ClearBreakpoints { file: None })
        }
        [name] if matches!(name.to_ascii_lowercase().as_str(), "breakpoints" | "bp") => {
            Ok(DebugCommand::ListBreakpoints)
        }
        _ => bail!("unknown debug command"),
    }
}

pub(crate) fn debug_command_from_web_request(
    request: &DebugWebControlRequest,
) -> std::result::Result<DebugCommand, String> {
    if let Some(action) = debug_action_from_command(&request.action) {
        return Ok(DebugCommand::Resume(action));
    }

    match request.action.trim().to_ascii_lowercase().as_str() {
        "breakpoint_add" | "break" | "breakpoint" | "add_breakpoint" => {
            let line = request
                .line
                .ok_or_else(|| "breakpoint_add requires line".to_string())?;
            validate_debug_breakpoint_line(line)?;
            Ok(DebugCommand::AddBreakpoint {
                file: request.file.clone(),
                line,
            })
        }
        "breakpoint_remove" | "remove_breakpoint" | "clear" | "delete" | "remove" => {
            let line = request
                .line
                .ok_or_else(|| "breakpoint_remove requires line".to_string())?;
            validate_debug_breakpoint_line(line)?;
            Ok(DebugCommand::RemoveBreakpoint {
                file: request.file.clone(),
                line,
            })
        }
        "breakpoint_clear" | "breakpoints_clear" | "clear_breakpoints" => {
            Ok(DebugCommand::ClearBreakpoints {
                file: request.file.clone(),
            })
        }
        "breakpoints" | "bp" => Ok(DebugCommand::ListBreakpoints),
        _ => Err("unknown action".to_string()),
    }
}

fn parse_debug_breakpoint_line(value: &str) -> Result<usize> {
    let line = value
        .parse::<usize>()
        .with_context(|| format!("invalid breakpoint line {value:?}"))?;
    validate_debug_breakpoint_line(line).map_err(anyhow::Error::msg)?;
    Ok(line)
}

fn validate_debug_breakpoint_line(line: usize) -> std::result::Result<(), String> {
    if line == 0 {
        Err("breakpoint lines are 1-based".to_string())
    } else {
        Ok(())
    }
}

pub(crate) fn debug_command_resumes(command: &DebugCommand) -> bool {
    matches!(command, DebugCommand::Resume(_))
}

pub(crate) fn debug_command_label(command: &DebugCommand) -> &'static str {
    match command {
        DebugCommand::Resume(action) => debug_action_label(*action),
        DebugCommand::AddBreakpoint { .. } => "breakpoint_add",
        DebugCommand::RemoveBreakpoint { .. } => "breakpoint_remove",
        DebugCommand::ClearBreakpoints { .. } => "breakpoint_clear",
        DebugCommand::ListBreakpoints => "breakpoints",
    }
}

pub(crate) fn apply_debug_control_command<F>(
    command: DebugCommand,
    control: &mut DebugControl<'_>,
    mut publish: F,
) -> Option<DebugAction>
where
    F: FnMut(serde_json::Value),
{
    match command {
        DebugCommand::Resume(action) => Some(action),
        DebugCommand::AddBreakpoint { file, line } => {
            control.add_line_breakpoint(file.as_deref(), line);
            publish(debug_breakpoints_event(
                "breakpoint_added",
                control.line_breakpoints(),
                json!({ "line": line, "file": file }),
            ));
            None
        }
        DebugCommand::RemoveBreakpoint { file, line } => {
            let removed = control.remove_line_breakpoint(file.as_deref(), line);
            publish(debug_breakpoints_event(
                "breakpoint_removed",
                control.line_breakpoints(),
                json!({ "line": line, "file": file, "removed": removed }),
            ));
            None
        }
        DebugCommand::ClearBreakpoints { file } => {
            let cleared = control.clear_line_breakpoints(file.as_deref());
            publish(debug_breakpoints_event(
                "breakpoints_cleared",
                control.line_breakpoints(),
                json!({ "file": file, "cleared": cleared }),
            ));
            None
        }
        DebugCommand::ListBreakpoints => {
            publish(debug_breakpoints_event(
                "breakpoints",
                control.line_breakpoints(),
                json!({}),
            ));
            None
        }
    }
}

fn debug_breakpoints_event(
    event: &str,
    breakpoints: Vec<(String, usize)>,
    details: serde_json::Value,
) -> serde_json::Value {
    json!({
        "event": event,
        "details": details,
        "breakpoints": breakpoints
            .into_iter()
            .map(|(file, line)| json!({ "file": file, "line": line }))
            .collect::<Vec<_>>(),
    })
}

fn debug_action_label(action: DebugAction) -> &'static str {
    match action {
        DebugAction::Step => "step",
        DebugAction::StepOver => "next",
        DebugAction::StepOut => "out",
        DebugAction::Continue => "continue",
        DebugAction::Abort => "abort",
    }
}

pub(crate) fn debug_event_json(event: &DebugEvent) -> serde_json::Value {
    match event {
        DebugEvent::Paused(pause) => json!({
            "event": "paused",
            "reason": match pause.reason {
                DebugPauseReason::Step => "step",
                DebugPauseReason::Breakpoint => "breakpoint",
            },
            "frame": pause.frame,
            "source": pause.source,
            "opcode": pause.opcode,
            "stack": debug_stack_json(&pause.stack),
            "locals": debug_bindings_json(&pause.locals),
            "globals": debug_bindings_json(&pause.globals),
            "self": pause.current_self.as_ref().map(debug_value_json),
            "tasks": debug_tasks_json(&pause.tasks),
        }),
        DebugEvent::Instruction {
            frame,
            source,
            opcode,
            stack_before,
            stack_after,
        } => json!({
            "event": "instruction",
            "frame": frame,
            "source": source,
            "opcode": opcode,
            "stack_before": debug_stack_json(stack_before),
            "stack_after": debug_stack_json(stack_after),
        }),
        DebugEvent::Fault {
            frame,
            message,
            stack,
        } => json!({
            "event": "fault",
            "frame": frame,
            "message": message,
            "stack": debug_stack_json(stack),
        }),
    }
}

fn debug_stack_json(stack: &[Value]) -> Vec<serde_json::Value> {
    stack.iter().map(debug_value_json).collect()
}

fn debug_bindings_json(bindings: &[(String, Value)]) -> Vec<serde_json::Value> {
    bindings
        .iter()
        .map(|(name, value)| {
            json!({
                "name": name,
                "value": debug_value_json(value),
            })
        })
        .collect()
}

fn debug_tasks_json(tasks: &[DebugTask]) -> Vec<serde_json::Value> {
    tasks
        .iter()
        .map(|task| {
            json!({
                "id": task.id,
                "operation": task.operation,
                "status": task.status,
                "pending": task.pending,
                "running": task.running,
                "completed": task.completed,
                "failed": task.failed,
                "fault": task.fault,
                "frames": debug_task_frames_json(task),
            })
        })
        .collect()
}

fn debug_task_frames_json(task: &DebugTask) -> Vec<serde_json::Value> {
    task.frames
        .iter()
        .map(|frame| {
            json!({
                "frame": frame.frame,
                "source": frame.source,
                "opcode": frame.opcode,
                "stack": debug_stack_json(&frame.stack),
                "locals": debug_bindings_json(&frame.locals),
                "self": frame.current_self.as_ref().map(debug_value_json),
            })
        })
        .collect()
}

fn debug_value_json(value: &Value) -> serde_json::Value {
    json!({
        "debug": format!("{value:?}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ricochet_vm::{DebugTask, DebugTaskFrame, Value};

    fn running_task(frames: Vec<DebugTaskFrame>) -> DebugTask {
        DebugTask {
            id: 0,
            operation: "spawn".to_string(),
            status: "running".to_string(),
            pending: true,
            running: true,
            completed: false,
            failed: false,
            fault: None,
            frames,
        }
    }

    #[test]
    fn debug_tasks_json_preserves_zero_frame_running_task() {
        let tasks = debug_tasks_json(&[running_task(Vec::new())]);

        assert_eq!(tasks[0]["status"], "running");
        assert_eq!(tasks[0]["pending"], true);
        assert!(tasks[0]["frames"]
            .as_array()
            .expect("frames should be an array")
            .is_empty());
    }

    #[test]
    fn debug_tasks_json_serializes_published_worker_frame() {
        let tasks = debug_tasks_json(&[running_task(vec![DebugTaskFrame {
            frame: "<task>".to_string(),
            source: "fixture.rco:6".to_string(),
            opcode: "CallWord(\"sleep\")".to_string(),
            stack: vec![Value::Number(20)],
            locals: vec![("release_attempts".to_string(), Value::Number(0))],
            current_self: Some(Value::String("worker".to_string())),
        }])]);
        let frame = &tasks[0]["frames"][0];

        assert_eq!(frame["frame"], "<task>");
        assert_eq!(frame["source"], "fixture.rco:6");
        assert_eq!(frame["opcode"], "CallWord(\"sleep\")");
        assert_eq!(frame["stack"][0]["debug"], "Number(20)");
        assert_eq!(frame["locals"][0]["name"], "release_attempts");
        assert_eq!(frame["locals"][0]["value"]["debug"], "Number(0)");
        assert_eq!(frame["self"]["debug"], "String(\"worker\")");
    }
}
