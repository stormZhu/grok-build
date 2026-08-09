#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PreHook {
    Allow,
    ExplicitDeny,
    RunnerFailed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Permission {
    Allow,
    Deny,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ToolExecution {
    Success,
    Failure,
}

#[derive(Debug, PartialEq, Eq)]
enum Terminal {
    Success,
    HookDenied,
    PermissionDenied,
    ToolFailed,
}

#[derive(Debug, PartialEq, Eq)]
struct ToolRun {
    dispatched: bool,
    terminal: Terminal,
    events: Vec<&'static str>,
}

fn run_tool(pre_hook: PreHook, permission: Permission, execution: ToolExecution) -> ToolRun {
    let mut events = vec!["hook:pre"];
    match pre_hook {
        PreHook::ExplicitDeny => {
            events.push("hook:explicit-deny");
            return ToolRun {
                dispatched: false,
                terminal: Terminal::HookDenied,
                events,
            };
        }
        PreHook::RunnerFailed => events.push("hook:runner-failed-open"),
        PreHook::Allow => events.push("hook:allow"),
    }

    events.push("permission:check");
    if permission == Permission::Deny {
        events.push("permission:deny");
        return ToolRun {
            dispatched: false,
            terminal: Terminal::PermissionDenied,
            events,
        };
    }

    events.push("tool:dispatch");
    match execution {
        ToolExecution::Success => {
            events.push("hook:post-success");
            ToolRun {
                dispatched: true,
                terminal: Terminal::Success,
                events,
            }
        }
        ToolExecution::Failure => {
            events.push("hook:post-failure");
            ToolRun {
                dispatched: true,
                terminal: Terminal::ToolFailed,
                events,
            }
        }
    }
}

fn dispatch_lifecycle(contributors: &[&'static str], event: &'static str) -> Vec<String> {
    contributors
        .iter()
        .map(|name| format!("{name}:{event}"))
        .collect()
}

fn main() {
    let contributors = ["metrics", "memory"];
    assert_eq!(
        dispatch_lifecycle(&contributors, "turn-start"),
        ["metrics:turn-start", "memory:turn-start"]
    );
    assert_eq!(
        dispatch_lifecycle(&contributors, "turn-done"),
        ["metrics:turn-done", "memory:turn-done"]
    );

    let denied_by_hook = run_tool(
        PreHook::ExplicitDeny,
        Permission::Allow,
        ToolExecution::Success,
    );
    assert!(!denied_by_hook.dispatched);
    assert_eq!(denied_by_hook.terminal, Terminal::HookDenied);
    assert!(!denied_by_hook.events.contains(&"permission:check"));

    let denied_by_permission = run_tool(PreHook::Allow, Permission::Deny, ToolExecution::Success);
    assert!(!denied_by_permission.dispatched);
    assert_eq!(denied_by_permission.terminal, Terminal::PermissionDenied);
    assert_eq!(
        denied_by_permission.events,
        [
            "hook:pre",
            "hook:allow",
            "permission:check",
            "permission:deny"
        ]
    );

    let fail_open = run_tool(
        PreHook::RunnerFailed,
        Permission::Allow,
        ToolExecution::Success,
    );
    assert!(fail_open.dispatched);
    assert_eq!(fail_open.terminal, Terminal::Success);
    assert!(fail_open.events.contains(&"hook:runner-failed-open"));
    assert!(fail_open.events.contains(&"hook:post-success"));
    assert!(!fail_open.events.contains(&"hook:post-failure"));

    let tool_failed = run_tool(PreHook::Allow, Permission::Allow, ToolExecution::Failure);
    assert_eq!(tool_failed.terminal, Terminal::ToolFailed);
    assert!(tool_failed.events.contains(&"hook:post-failure"));
    assert!(!tool_failed.events.contains(&"hook:post-success"));

    println!("ordered lifecycle + hook gate + independent permission + one post terminal");
}
