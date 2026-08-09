#[derive(Debug, PartialEq, Eq)]
enum ChatItem {
    User(&'static str),
    AssistantToolCall { call_id: &'static str },
    ToolResult { call_id: &'static str },
    AssistantText(&'static str),
}

#[derive(Default)]
struct ChatState {
    conversation: Vec<ChatItem>,
}

#[derive(Default)]
struct Pager {
    user_entries: Vec<&'static str>,
    updates: Vec<String>,
    prompt_response_received: bool,
}

#[derive(Default)]
struct Persistence {
    queued: Vec<String>,
    durable: Vec<String>,
}

impl Persistence {
    fn enqueue(&mut self, event: impl Into<String>) {
        self.queued.push(event.into());
    }

    fn flush_and_ack(&mut self) {
        self.durable.append(&mut self.queued);
    }
}

#[derive(Default)]
struct ReplayBuffer {
    pending: String,
}

impl ReplayBuffer {
    fn push_text(&mut self, text: &str) {
        self.pending.push_str(text);
    }

    fn flush(&mut self, pager: &mut Pager) {
        if !self.pending.is_empty() {
            pager.updates.push(std::mem::take(&mut self.pending));
        }
    }
}

struct TurnResult {
    samples: usize,
    tool_runs: usize,
    timeline: Vec<&'static str>,
}

fn run_turn(
    prompt_id: &'static str,
    pager: &mut Pager,
    chat: &mut ChatState,
    persistence: &mut Persistence,
) -> TurnResult {
    let mut timeline = Vec::new();
    let mut replay = ReplayBuffer::default();

    pager.user_entries.push("Read src/main.rs");
    timeline.push("pager:local-echo");

    timeline.push("session:admit");
    chat.conversation.push(ChatItem::User("Read src/main.rs"));
    persistence.enqueue(format!("chat:user:{prompt_id}"));
    persistence.flush_and_ack();
    timeline.push("persistence:user-ack");

    timeline.push("sampler:request-1");
    replay.push_text("Inspecting");
    chat.conversation
        .push(ChatItem::AssistantToolCall { call_id: "tc-17" });

    timeline.push("permission:allow");
    timeline.push("tool:tc-17");
    chat.conversation
        .push(ChatItem::ToolResult { call_id: "tc-17" });

    timeline.push("sampler:request-2");
    replay.push_text("Entry is main.");
    chat.conversation
        .push(ChatItem::AssistantText("Entry is main."));

    replay.flush(pager);
    timeline.push("replay:flush");
    pager.prompt_response_received = true;
    timeline.push("rpc:prompt-response");

    persistence.enqueue(format!("update:turn-completed:{prompt_id}"));
    persistence.flush_and_ack();
    timeline.push("persistence:turn-completed");
    timeline.push("session:start-next");

    TurnResult {
        samples: 2,
        tool_runs: 1,
        timeline,
    }
}

fn main() {
    let mut pager = Pager::default();
    let mut chat = ChatState::default();
    let mut persistence = Persistence::default();
    let result = run_turn("p-42", &mut pager, &mut chat, &mut persistence);

    assert_eq!(result.samples, 2);
    assert_eq!(result.tool_runs, 1);
    assert_eq!(pager.user_entries, ["Read src/main.rs"]);
    assert_eq!(pager.updates, ["InspectingEntry is main."]);
    assert!(pager.prompt_response_received);

    assert_eq!(
        chat.conversation,
        [
            ChatItem::User("Read src/main.rs"),
            ChatItem::AssistantToolCall { call_id: "tc-17" },
            ChatItem::ToolResult { call_id: "tc-17" },
            ChatItem::AssistantText("Entry is main."),
        ]
    );
    assert!(persistence.queued.is_empty());
    assert_eq!(
        persistence.durable,
        ["chat:user:p-42", "update:turn-completed:p-42"]
    );

    let flush = result
        .timeline
        .iter()
        .position(|event| *event == "replay:flush")
        .unwrap();
    let response = result
        .timeline
        .iter()
        .position(|event| *event == "rpc:prompt-response")
        .unwrap();
    let terminal = result
        .timeline
        .iter()
        .position(|event| *event == "persistence:turn-completed")
        .unwrap();
    let next = result
        .timeline
        .iter()
        .position(|event| *event == "session:start-next")
        .unwrap();
    assert!(flush < response);
    assert!(response < terminal);
    assert!(terminal < next);

    println!("one turn + two samples + tool join key + four confirmation boundaries");
}
