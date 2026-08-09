use std::collections::VecDeque;
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Failure {
    Transient,
    EmptyResponse,
    Auth,
    ContextOverflow,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Response {
    Text(&'static str),
    ToolCall(&'static str),
    Empty,
}

struct Attempt {
    deltas: Vec<&'static str>,
    terminal: Result<Response, Failure>,
}

#[derive(Debug, PartialEq, Eq)]
struct RequestRun {
    attempts: usize,
    visible_text: String,
    retry_events: usize,
    terminal: Result<Response, Failure>,
}

fn can_retry(error: Failure, emitted_output: bool, retry_only_before_output: bool) -> bool {
    matches!(error, Failure::Transient | Failure::EmptyResponse)
        && !(retry_only_before_output && emitted_output)
}

async fn run_logical_request(
    mut attempts: VecDeque<Attempt>,
    max_attempts: usize,
    retry_only_before_output: bool,
) -> RequestRun {
    let mut run = RequestRun {
        attempts: 0,
        visible_text: String::new(),
        retry_events: 0,
        terminal: Err(Failure::Transient),
    };

    while let Some(attempt) = attempts.pop_front() {
        run.attempts += 1;
        let emitted_output = !attempt.deltas.is_empty();
        for delta in attempt.deltas {
            run.visible_text.push_str(delta);
        }

        let terminal = match attempt.terminal {
            Ok(Response::Empty) => Err(Failure::EmptyResponse),
            other => other,
        };

        match terminal {
            Ok(response) => {
                run.terminal = Ok(response);
                return run;
            }
            Err(error)
                if run.attempts < max_attempts
                    && can_retry(error, emitted_output, retry_only_before_output) =>
            {
                run.terminal = Err(error);
                run.retry_events += 1;
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
            Err(error) => {
                // The sampler stops here; the Session owner decides how to recover.
                run.terminal = Err(error);
                return run;
            }
        }
    }

    run
}

fn attempt(deltas: Vec<&'static str>, terminal: Result<Response, Failure>) -> Attempt {
    Attempt { deltas, terminal }
}

#[tokio::main(flavor = "current_thread", start_paused = true)]
async fn main() {
    let recovered = run_logical_request(
        VecDeque::from([
            attempt(Vec::new(), Err(Failure::Transient)),
            attempt(vec!["done"], Ok(Response::Text("done"))),
        ]),
        3,
        true,
    )
    .await;
    assert_eq!(recovered.attempts, 2);
    assert_eq!(recovered.retry_events, 1);
    assert_eq!(recovered.visible_text, "done");
    assert_eq!(recovered.terminal, Ok(Response::Text("done")));

    let partial = run_logical_request(
        VecDeque::from([
            attempt(vec!["half"], Err(Failure::Transient)),
            attempt(vec![" duplicate"], Ok(Response::Text("should not run"))),
        ]),
        3,
        true,
    )
    .await;
    assert_eq!(partial.attempts, 1);
    assert_eq!(partial.retry_events, 0);
    assert_eq!(partial.visible_text, "half");
    assert_eq!(partial.terminal, Err(Failure::Transient));

    let empty_then_tool = run_logical_request(
        VecDeque::from([
            attempt(Vec::new(), Ok(Response::Empty)),
            attempt(Vec::new(), Ok(Response::ToolCall("read_file"))),
        ]),
        3,
        true,
    )
    .await;
    assert_eq!(empty_then_tool.attempts, 2);
    assert_eq!(empty_then_tool.retry_events, 1);
    assert_eq!(
        empty_then_tool.terminal,
        Ok(Response::ToolCall("read_file"))
    );

    for failure in [Failure::Auth, Failure::ContextOverflow] {
        let delegated = run_logical_request(
            VecDeque::from([
                attempt(Vec::new(), Err(failure)),
                attempt(vec!["wrong"], Ok(Response::Text("wrong"))),
            ]),
            3,
            true,
        )
        .await;
        assert_eq!(delegated.attempts, 1);
        assert_eq!(delegated.retry_events, 0);
        assert_eq!(delegated.terminal, Err(failure));
    }

    println!("one logical terminal; retry only before output; auth/context return to Session");
}
