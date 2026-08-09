use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Correlation {
    session_id: &'static str,
    prompt_id: &'static str,
    traceparent: &'static str,
}

tokio::task_local! {
    static CORRELATION: Correlation;
}

#[derive(Debug, PartialEq, Eq)]
struct Event {
    kind: &'static str,
    session_id: Option<&'static str>,
    prompt_id: Option<&'static str>,
    request_id: Option<&'static str>,
    tool_call_id: Option<&'static str>,
}

fn event(
    kind: &'static str,
    request_id: Option<&'static str>,
    tool_call_id: Option<&'static str>,
) -> Event {
    let correlation = CORRELATION.try_with(Clone::clone).ok();
    Event {
        kind,
        session_id: correlation.as_ref().map(|context| context.session_id),
        prompt_id: correlation.as_ref().map(|context| context.prompt_id),
        request_id,
        tool_call_id,
    }
}

fn spawn_with_correlation(
    context: Correlation,
) -> tokio::task::JoinHandle<(Event, Event, &'static str)> {
    tokio::spawn(CORRELATION.scope(context, async {
        let sampler = event("sampler.attempt", Some("request-9"), None);
        let outbound_traceparent = CORRELATION.with(|context| context.traceparent);
        let tool = event("tool.completed", Some("request-9"), Some("call-3"));
        (sampler, tool, outbound_traceparent)
    }))
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Attribute {
    Text(String),
    Integer(i64),
    Boolean(bool),
}

fn external_export(mut attributes: BTreeMap<String, Attribute>) -> BTreeMap<String, Attribute> {
    const ALLOWED_TEXT_KEYS: &[&str] = &[
        "session_id",
        "prompt_id",
        "request_id",
        "tool_call_id",
        "model_id",
        "tool_name",
        "error_kind",
    ];
    attributes.retain(|key, value| {
        !matches!(value, Attribute::Text(_)) || ALLOWED_TEXT_KEYS.contains(&key.as_str())
    });
    attributes
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let context = Correlation {
        session_id: "session-1",
        prompt_id: "prompt-7",
        traceparent: "00-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-bbbbbbbbbbbbbbbb-01",
    };

    let (prompt, bare_child, sampler, tool, outbound_traceparent) = CORRELATION
        .scope(context.clone(), async {
            let prompt = event("prompt.accepted", None, None);
            let bare_child = tokio::spawn(async { event("bare.spawn", None, None) })
                .await
                .unwrap();
            let (sampler, tool, outbound_traceparent) =
                spawn_with_correlation(context.clone()).await.unwrap();
            (prompt, bare_child, sampler, tool, outbound_traceparent)
        })
        .await;

    assert_eq!(prompt.session_id, Some("session-1"));
    assert_eq!(prompt.prompt_id, Some("prompt-7"));
    assert_eq!(bare_child.session_id, None);
    assert_eq!(bare_child.prompt_id, None);
    assert_eq!(sampler.session_id, Some("session-1"));
    assert_eq!(sampler.prompt_id, Some("prompt-7"));
    assert_eq!(sampler.request_id, Some("request-9"));
    assert_eq!(tool.request_id, Some("request-9"));
    assert_eq!(tool.tool_call_id, Some("call-3"));
    assert_eq!(outbound_traceparent, context.traceparent);

    let timeline = [&prompt, &sampler, &tool];
    assert_eq!(
        timeline.map(|entry| entry.kind),
        ["prompt.accepted", "sampler.attempt", "tool.completed"]
    );

    let exported = external_export(BTreeMap::from([
        (
            "session_id".to_owned(),
            Attribute::Text("session-1".to_owned()),
        ),
        (
            "request_id".to_owned(),
            Attribute::Text("request-9".to_owned()),
        ),
        ("attempt".to_owned(), Attribute::Integer(1)),
        ("success".to_owned(), Attribute::Boolean(true)),
        (
            "user_prompt".to_owned(),
            Attribute::Text("private source code".to_owned()),
        ),
        (
            "authorization".to_owned(),
            Attribute::Text("Bearer secret".to_owned()),
        ),
    ]));
    assert_eq!(exported.len(), 4);
    assert!(exported.contains_key("session_id"));
    assert!(exported.contains_key("request_id"));
    assert!(exported.contains_key("attempt"));
    assert!(exported.contains_key("success"));
    assert!(!exported.contains_key("user_prompt"));
    assert!(!exported.contains_key("authorization"));

    println!("explicit task context + distinct correlation IDs + default-deny text export");
}
