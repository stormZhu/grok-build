use serde::Deserialize;
use serde_json::Value;
use tokio::sync::mpsc;

struct ModelToolCall {
    id: &'static str,
    name: &'static str,
    arguments: Value,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadArgs {
    path: String,
}

struct PreparedToolCall {
    id: &'static str,
    args: ReadArgs,
}

#[derive(Debug, PartialEq, Eq)]
struct ToolOutput {
    prompt_text: String,
    ui_summary: String,
}

enum ToolStreamItem {
    Progress(String),
    Terminal(Result<ToolOutput, String>),
}

#[derive(Debug)]
struct ToolRun {
    call_id: &'static str,
    progress: Vec<String>,
    terminal: Result<ToolOutput, String>,
}

fn prepare_tool_call(call: ModelToolCall) -> Result<PreparedToolCall, String> {
    if call.name != "read_file" {
        return Err(format!("unknown tool: {}", call.name));
    }

    let args = serde_json::from_value(call.arguments)
        .map_err(|error| format!("invalid arguments: {error}"))?;
    Ok(PreparedToolCall { id: call.id, args })
}

fn permission_denial(args: &ReadArgs) -> Option<String> {
    args.path
        .starts_with("/etc/")
        .then(|| format!("permission denied: {}", args.path))
}

fn execute_tool(call: &PreparedToolCall) -> mpsc::Receiver<ToolStreamItem> {
    let (stream_tx, stream_rx) = mpsc::channel(4);
    let path = call.args.path.clone();

    tokio::spawn(async move {
        stream_tx
            .send(ToolStreamItem::Progress("opening file".to_owned()))
            .await
            .unwrap();
        stream_tx
            .send(ToolStreamItem::Progress("reading bytes".to_owned()))
            .await
            .unwrap();
        stream_tx
            .send(ToolStreamItem::Terminal(Ok(ToolOutput {
                prompt_text: format!("contents of {path}"),
                ui_summary: format!("read {path}"),
            })))
            .await
            .unwrap();
    });

    stream_rx
}

async fn collect_tool_stream(
    mut stream: mpsc::Receiver<ToolStreamItem>,
) -> Result<(Vec<String>, ToolOutput), String> {
    let mut progress = Vec::new();
    let mut terminal = None;

    while let Some(item) = stream.recv().await {
        assert!(terminal.is_none(), "a tool emitted an item after Terminal");
        match item {
            ToolStreamItem::Progress(message) => progress.push(message),
            ToolStreamItem::Terminal(result) => terminal = Some(result),
        }
    }

    let output = terminal
        .ok_or_else(|| "tool stream ended without Terminal".to_owned())?
        .map_err(|error| format!("tool failed: {error}"))?;
    Ok((progress, output))
}

async fn run_tool_call(call: ModelToolCall) -> ToolRun {
    let call_id = call.id;
    let prepared = match prepare_tool_call(call) {
        Ok(prepared) => prepared,
        Err(error) => {
            return ToolRun {
                call_id,
                progress: Vec::new(),
                terminal: Err(error),
            };
        }
    };

    if let Some(error) = permission_denial(&prepared.args) {
        return ToolRun {
            call_id: prepared.id,
            progress: Vec::new(),
            terminal: Err(error),
        };
    }

    match collect_tool_stream(execute_tool(&prepared)).await {
        Ok((progress, output)) => ToolRun {
            call_id: prepared.id,
            progress,
            terminal: Ok(output),
        },
        Err(error) => ToolRun {
            call_id: prepared.id,
            progress: Vec::new(),
            terminal: Err(error),
        },
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let allowed = run_tool_call(ModelToolCall {
        id: "call-1",
        name: "read_file",
        arguments: serde_json::json!({ "path": "docs/README.md" }),
    })
    .await;

    assert_eq!(allowed.call_id, "call-1");
    assert_eq!(allowed.progress, ["opening file", "reading bytes"]);
    let output = allowed.terminal.unwrap();
    assert_eq!(output.prompt_text, "contents of docs/README.md");
    assert_eq!(output.ui_summary, "read docs/README.md");

    let mut chat_state = Vec::new();
    chat_state.push(format!(
        "tool_result:{}:{}",
        allowed.call_id, output.prompt_text
    ));
    assert_eq!(
        chat_state,
        ["tool_result:call-1:contents of docs/README.md"]
    );

    let denied = run_tool_call(ModelToolCall {
        id: "call-2",
        name: "read_file",
        arguments: serde_json::json!({ "path": "/etc/passwd" }),
    })
    .await;
    assert!(denied.progress.is_empty());
    assert_eq!(
        denied.terminal.unwrap_err(),
        "permission denied: /etc/passwd"
    );

    let (broken_tx, broken_rx) = mpsc::channel(1);
    broken_tx
        .send(ToolStreamItem::Progress("started".to_owned()))
        .await
        .unwrap();
    drop(broken_tx);
    assert_eq!(
        collect_tool_stream(broken_rx).await.unwrap_err(),
        "tool stream ended without Terminal"
    );

    println!("JSON args -> permission -> Progress* -> Terminal -> ChatState tool result");
}
