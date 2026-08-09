#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PromptMode {
    Extend,
    Full,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Audience {
    Primary,
    Subagent,
}

struct ToolRegistry {
    read_name: &'static str,
    definitions: Vec<&'static str>,
}

impl ToolRegistry {
    fn with_read_override(read_name: &'static str) -> Self {
        Self {
            read_name,
            definitions: vec![read_name, "search_code"],
        }
    }
}

fn render_stable_system(
    mode: PromptMode,
    audience: Audience,
    body: &str,
    tools: &ToolRegistry,
) -> String {
    let base = match audience {
        Audience::Primary => format!(
            "You are the primary coding agent. Read with {}. Parent catalog enabled.",
            tools.read_name
        ),
        Audience::Subagent => format!(
            "You are a focused child agent. Read with {}.",
            tools.read_name
        ),
    };
    match mode {
        PromptMode::Extend => format!("{base}\n{body}"),
        PromptMode::Full => body.to_owned(),
    }
}

fn render_initial_preamble(
    workspace: &str,
    project_rules: &str,
    skills: &[&str],
    mcp_servers: &[&str],
) -> String {
    format!(
        "workspace={workspace}\nproject_rules={project_rules}\nskills={}\nmcp={}",
        skills.join(","),
        mcp_servers.join(",")
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SyntheticReason {
    ProjectInstructions,
    RuntimeReminder,
}

#[derive(Debug, PartialEq, Eq)]
struct ConversationItem {
    text: String,
    reason: Option<SyntheticReason>,
}

fn ensure_project_instructions(conversation: &mut Vec<ConversationItem>, preamble: &str) {
    if conversation
        .iter()
        .any(|item| item.reason == Some(SyntheticReason::ProjectInstructions))
    {
        return;
    }
    conversation.push(ConversationItem {
        text: preamble.to_owned(),
        reason: Some(SyntheticReason::ProjectInstructions),
    });
}

fn push_runtime_reminder(conversation: &mut Vec<ConversationItem>, reminder: &str) {
    conversation.push(ConversationItem {
        text: reminder.to_owned(),
        reason: Some(SyntheticReason::RuntimeReminder),
    });
}

fn main() {
    let tools = ToolRegistry::with_read_override("inspect_file");
    let body = "Review narrowly and cite evidence.";
    let primary = render_stable_system(PromptMode::Extend, Audience::Primary, body, &tools);
    let child = render_stable_system(PromptMode::Extend, Audience::Subagent, body, &tools);
    assert!(primary.contains("inspect_file"));
    assert!(tools.definitions.contains(&"inspect_file"));
    assert!(primary.contains("Parent catalog enabled"));
    assert!(!child.contains("Parent catalog enabled"));
    assert!(primary.ends_with(body));
    assert!(child.ends_with(body));

    let full = render_stable_system(PromptMode::Full, Audience::Primary, body, &tools);
    assert_eq!(full, body);
    assert!(!full.contains("inspect_file"));

    let preamble = render_initial_preamble(
        "/workspace/project",
        "Run tests before editing.",
        &["code-review"],
        &["docs-server"],
    );
    assert!(preamble.contains("workspace=/workspace/project"));
    assert!(preamble.contains("project_rules=Run tests before editing."));
    assert!(preamble.contains("skills=code-review"));
    assert!(preamble.contains("mcp=docs-server"));

    let mut conversation = vec![ConversationItem {
        text: "User request".to_owned(),
        reason: None,
    }];
    ensure_project_instructions(&mut conversation, &preamble);
    ensure_project_instructions(&mut conversation, &preamble);
    assert_eq!(
        conversation
            .iter()
            .filter(|item| item.reason == Some(SyntheticReason::ProjectInstructions))
            .count(),
        1
    );

    let stable_before_runtime_change = primary.clone();
    push_runtime_reminder(&mut conversation, "Plan mode is now active.");
    assert_eq!(primary, stable_before_runtime_change);
    assert_eq!(
        conversation.last().unwrap().reason,
        Some(SyntheticReason::RuntimeReminder)
    );
    assert!(conversation.last().unwrap().text.contains("Plan mode"));

    println!("stable system + initial preamble + idempotent conversation + runtime reminder");
}
