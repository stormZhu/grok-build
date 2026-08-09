#[derive(Clone, Debug, PartialEq, Eq)]
enum Item {
    System(String),
    User(String),
    Assistant(String),
    ToolResult(String),
    Reminder(String),
    Summary(String),
}

fn estimated_tokens(items: &[Item]) -> usize {
    items
        .iter()
        .map(|item| match item {
            Item::System(text)
            | Item::User(text)
            | Item::Assistant(text)
            | Item::ToolResult(text)
            | Item::Reminder(text)
            | Item::Summary(text) => text.len().div_ceil(4),
        })
        .sum()
}

fn build_request_with_pruning(conversation: &[Item]) -> Vec<Item> {
    let mut request = conversation.to_vec();
    let tool_result_indexes: Vec<_> = request
        .iter()
        .enumerate()
        .filter_map(|(index, item)| matches!(item, Item::ToolResult(_)).then_some(index))
        .collect();

    for index in tool_result_indexes.into_iter().rev().skip(1) {
        request[index] = Item::ToolResult("[older tool result pruned]".to_owned());
    }
    request
}

fn should_compact(estimated_usage: usize, context_window: usize) -> bool {
    estimated_usage.saturating_mul(100) >= context_window.saturating_mul(85)
}

fn compact_authoritative_history(conversation: &mut Vec<Item>) {
    let system = conversation
        .iter()
        .find_map(|item| match item {
            Item::System(text) => Some(text.clone()),
            _ => None,
        })
        .expect("system prompt");
    let original_goal = conversation
        .iter()
        .find_map(|item| match item {
            Item::User(text) => Some(text.clone()),
            _ => None,
        })
        .expect("original user goal");

    *conversation = vec![
        Item::System(system),
        Item::Summary("Inspected auth flow; next verify refresh handling.".to_owned()),
        Item::User(original_goal),
        Item::Reminder("Continue from the summary; do not repeat finished work.".to_owned()),
    ];
}

fn main() {
    let mut conversation = vec![
        Item::System("You are a coding agent.".to_owned()),
        Item::User("Trace and improve the authentication flow.".to_owned()),
        Item::Assistant("I will inspect the configuration and token paths.".repeat(4)),
        Item::ToolResult("first large search result\n".repeat(20)),
        Item::Assistant("Now I will inspect refresh handling.".repeat(4)),
        Item::ToolResult("second large search result\n".repeat(20)),
    ];

    let authoritative_before_pruning = conversation.clone();
    let request = build_request_with_pruning(&conversation);
    assert_eq!(conversation, authoritative_before_pruning);
    assert_ne!(request, conversation);
    assert!(matches!(
        &request[3],
        Item::ToolResult(text) if text == "[older tool result pruned]"
    ));
    assert!(matches!(&request[5], Item::ToolResult(text) if text.len() > 100));

    assert!(!should_compact(70, 100));
    assert!(should_compact(70, 80));

    let tokens_before = estimated_tokens(&conversation);
    compact_authoritative_history(&mut conversation);
    let tokens_after = estimated_tokens(&conversation);
    assert!(tokens_after < tokens_before);
    assert_eq!(conversation.len(), 4);
    assert!(matches!(&conversation[0], Item::System(_)));
    assert!(matches!(&conversation[1], Item::Summary(_)));
    assert!(matches!(&conversation[2], Item::User(goal) if goal.contains("authentication")));
    assert!(matches!(&conversation[3], Item::Reminder(_)));

    println!(
        "request pruning kept authoritative history; full compaction: {tokens_before} -> {tokens_after} tokens"
    );
}
