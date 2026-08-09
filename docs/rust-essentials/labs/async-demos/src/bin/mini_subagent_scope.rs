#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ContextMode {
    Fresh,
    Fork,
    Resume,
}

#[derive(Clone, Debug)]
struct Parent {
    session_id: &'static str,
    prompt_id: &'static str,
    cwd: &'static str,
    conversation: Vec<String>,
    permission_manager: &'static str,
}

#[derive(Clone, Debug)]
struct ResumeSource {
    subagent_type: &'static str,
    persona: &'static str,
    model: &'static str,
    conversation: Vec<String>,
    worktree_path: Option<&'static str>,
    snapshot_available: bool,
}

struct Request {
    id: &'static str,
    subagent_type: &'static str,
    persona: &'static str,
    requested_model: &'static str,
    task: &'static str,
    mode: ContextMode,
    isolation_worktree: bool,
    worktree_created: bool,
    depth: usize,
    max_depth: usize,
}

#[derive(Debug)]
struct Child {
    session_id: String,
    parent_session_id: &'static str,
    parent_prompt_id: &'static str,
    model: &'static str,
    cwd: &'static str,
    conversation: Vec<String>,
    permission_manager: &'static str,
    task_tool_enabled: bool,
    warning: Option<&'static str>,
    events: Vec<&'static str>,
    cancelled: bool,
    usage_incomplete: bool,
}

fn spawn_child(
    parent: &Parent,
    request: Request,
    source: Option<&ResumeSource>,
) -> Result<Child, &'static str> {
    let (model, mut conversation, resume_worktree, snapshot_available) = match request.mode {
        ContextMode::Fresh => (
            request.requested_model,
            vec!["child-system".to_owned()],
            None,
            false,
        ),
        ContextMode::Fork => (
            request.requested_model,
            vec![
                "child-system".to_owned(),
                format!(
                    "<background_context>{}</background_context>",
                    parent.conversation.join("|")
                ),
            ],
            None,
            false,
        ),
        ContextMode::Resume => {
            let source = source.ok_or("resume source missing")?;
            if source.subagent_type != request.subagent_type || source.persona != request.persona {
                return Err("resume identity mismatch");
            }
            (
                source.model,
                source.conversation.clone(),
                source.worktree_path,
                source.snapshot_available,
            )
        }
    };
    conversation.push(request.task.to_owned());

    let (cwd, warning) = if request.mode == ContextMode::Resume {
        match (resume_worktree, snapshot_available) {
            (Some(path), _) => (path, None),
            (None, true) => ("/tmp/rehydrated-child", None),
            (None, false) => (parent.cwd, Some("resume fell back to shared workspace")),
        }
    } else if request.isolation_worktree && request.worktree_created {
        ("/tmp/child-worktree", None)
    } else if request.isolation_worktree {
        (
            parent.cwd,
            Some("worktree creation failed; using shared workspace"),
        )
    } else {
        (parent.cwd, None)
    };

    Ok(Child {
        session_id: format!("child-{}", request.id),
        parent_session_id: parent.session_id,
        parent_prompt_id: parent.prompt_id,
        model,
        cwd,
        conversation,
        permission_manager: parent.permission_manager,
        task_tool_enabled: request.depth < request.max_depth,
        warning,
        events: Vec::new(),
        cancelled: false,
        usage_incomplete: false,
    })
}

fn cancel_via_owner(child: &mut Child, had_activity: bool) {
    child.events.push("coordinator:cancel-command");
    child.cancelled = true;
    child.events.push("child:drained");
    child.usage_incomplete = had_activity;
    child.events.push("coordinator:completion-reported");
}

fn main() {
    let parent = Parent {
        session_id: "parent-s1",
        prompt_id: "parent-p9",
        cwd: "/workspace/main",
        conversation: vec![
            "user:root task".to_owned(),
            "assistant:delegating".to_owned(),
        ],
        permission_manager: "permission-manager-parent",
    };

    let fresh = spawn_child(
        &parent,
        Request {
            id: "fresh",
            subagent_type: "explore",
            persona: "reader",
            requested_model: "model-b",
            task: "find entrypoint",
            mode: ContextMode::Fresh,
            isolation_worktree: false,
            worktree_created: false,
            depth: 1,
            max_depth: 2,
        },
        None,
    )
    .unwrap();
    assert_eq!(fresh.conversation, ["child-system", "find entrypoint"]);
    assert_eq!(fresh.cwd, parent.cwd);
    assert!(fresh.task_tool_enabled);

    let mut forked = spawn_child(
        &parent,
        Request {
            id: "fork",
            subagent_type: "review",
            persona: "reviewer",
            requested_model: "model-c",
            task: "review change",
            mode: ContextMode::Fork,
            isolation_worktree: true,
            worktree_created: true,
            depth: 2,
            max_depth: 2,
        },
        None,
    )
    .unwrap();
    assert!(forked.conversation[1].contains("<background_context>"));
    forked.conversation.push("assistant:child-only".to_owned());
    assert_eq!(parent.conversation.len(), 2);
    assert_eq!(forked.cwd, "/tmp/child-worktree");
    assert!(!forked.task_tool_enabled);
    assert_eq!(forked.permission_manager, parent.permission_manager);
    assert_eq!(forked.parent_session_id, parent.session_id);
    assert_eq!(forked.parent_prompt_id, parent.prompt_id);

    let source = ResumeSource {
        subagent_type: "explore",
        persona: "reader",
        model: "source-model",
        conversation: vec!["child-system".to_owned(), "assistant:prior".to_owned()],
        worktree_path: None,
        snapshot_available: true,
    };
    let resumed = spawn_child(
        &parent,
        Request {
            id: "resume",
            subagent_type: "explore",
            persona: "reader",
            requested_model: "ignored-model",
            task: "continue",
            mode: ContextMode::Resume,
            isolation_worktree: false,
            worktree_created: false,
            depth: 1,
            max_depth: 2,
        },
        Some(&source),
    )
    .unwrap();
    assert_eq!(resumed.model, "source-model");
    assert_eq!(resumed.cwd, "/tmp/rehydrated-child");
    assert_eq!(resumed.warning, None);
    assert_eq!(resumed.session_id, "child-resume");

    let mismatch = spawn_child(
        &parent,
        Request {
            id: "bad",
            subagent_type: "review",
            persona: "reader",
            requested_model: "model",
            task: "continue",
            mode: ContextMode::Resume,
            isolation_worktree: false,
            worktree_created: false,
            depth: 1,
            max_depth: 2,
        },
        Some(&source),
    );
    assert!(matches!(mismatch, Err("resume identity mismatch")));

    cancel_via_owner(&mut forked, true);
    assert!(forked.cancelled);
    assert!(forked.usage_incomplete);
    assert_eq!(
        forked.events,
        [
            "coordinator:cancel-command",
            "child:drained",
            "coordinator:completion-reported"
        ]
    );

    println!("fresh/fork/resume + independent history + real cwd + owner-driven cancel");
}
