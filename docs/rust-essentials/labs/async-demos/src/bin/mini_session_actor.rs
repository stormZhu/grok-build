use std::collections::VecDeque;
use std::time::Duration;

use tokio::sync::{mpsc, oneshot};

struct Prompt {
    id: &'static str,
    text: &'static str,
    duration: Duration,
    respond_to: oneshot::Sender<String>,
}

enum SessionCommand {
    Prompt(Prompt),
    Status { respond_to: oneshot::Sender<Status> },
    Shutdown { respond_to: oneshot::Sender<()> },
}

struct TurnCompletion {
    id: &'static str,
    output: String,
    respond_to: oneshot::Sender<String>,
}

#[derive(Debug, PartialEq, Eq)]
struct Status {
    running: Option<&'static str>,
    queued: usize,
    completed: usize,
}

#[derive(Default)]
struct SessionState {
    pending_inputs: VecDeque<Prompt>,
    running_task: Option<&'static str>,
    completed: Vec<&'static str>,
    events: Vec<String>,
}

fn maybe_start_running_task(
    state: &mut SessionState,
    completion_tx: &mpsc::Sender<TurnCompletion>,
) {
    if state.running_task.is_some() {
        return;
    }

    let Some(prompt) = state.pending_inputs.pop_front() else {
        return;
    };

    state.running_task = Some(prompt.id);
    state.events.push(format!("started:{}", prompt.id));
    let completion_tx = completion_tx.clone();

    tokio::task::spawn_local(async move {
        tokio::time::sleep(prompt.duration).await;
        let completion = TurnCompletion {
            id: prompt.id,
            output: format!("reply to {}", prompt.text),
            respond_to: prompt.respond_to,
        };
        completion_tx.send(completion).await.unwrap();
    });
}

async fn run_session(
    mut command_rx: mpsc::Receiver<SessionCommand>,
    completion_tx: mpsc::Sender<TurnCompletion>,
    mut completion_rx: mpsc::Receiver<TurnCompletion>,
) -> SessionState {
    let mut state = SessionState::default();

    loop {
        tokio::select! {
            biased;

            Some(completion) = completion_rx.recv() => {
                assert_eq!(state.running_task, Some(completion.id));
                state.events.push(format!("completed:{}", completion.id));
                state.completed.push(completion.id);
                state.running_task = None;
                let _ = completion.respond_to.send(completion.output);
                maybe_start_running_task(&mut state, &completion_tx);
            }
            command = command_rx.recv() => match command {
                Some(SessionCommand::Prompt(prompt)) => {
                    state.events.push(format!("queued:{}", prompt.id));
                    state.pending_inputs.push_back(prompt);
                    maybe_start_running_task(&mut state, &completion_tx);
                }
                Some(SessionCommand::Status { respond_to }) => {
                    let _ = respond_to.send(Status {
                        running: state.running_task,
                        queued: state.pending_inputs.len(),
                        completed: state.completed.len(),
                    });
                }
                Some(SessionCommand::Shutdown { respond_to }) => {
                    assert!(state.running_task.is_none());
                    assert!(state.pending_inputs.is_empty());
                    state.events.push("shutdown:flush".to_owned());
                    let _ = respond_to.send(());
                    return state;
                }
                None => return state,
            }
        }
    }
}

async fn send_prompt(
    command_tx: &mpsc::Sender<SessionCommand>,
    id: &'static str,
    text: &'static str,
    duration: Duration,
) -> oneshot::Receiver<String> {
    let (respond_to, response) = oneshot::channel();
    command_tx
        .send(SessionCommand::Prompt(Prompt {
            id,
            text,
            duration,
            respond_to,
        }))
        .await
        .unwrap();
    response
}

#[tokio::main(flavor = "current_thread", start_paused = true)]
async fn main() {
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async {
            let (command_tx, command_rx) = mpsc::channel(8);
            let (completion_tx, completion_rx) = mpsc::channel(1);
            let actor =
                tokio::task::spawn_local(run_session(command_rx, completion_tx, completion_rx));

            let first = send_prompt(
                &command_tx,
                "first",
                "slow prompt",
                Duration::from_millis(30),
            )
            .await;
            let second = send_prompt(
                &command_tx,
                "second",
                "fast prompt",
                Duration::from_millis(10),
            )
            .await;

            let (status_tx, status_rx) = oneshot::channel();
            command_tx
                .send(SessionCommand::Status {
                    respond_to: status_tx,
                })
                .await
                .unwrap();
            assert_eq!(
                status_rx.await.unwrap(),
                Status {
                    running: Some("first"),
                    queued: 1,
                    completed: 0,
                }
            );

            assert_eq!(first.await.unwrap(), "reply to slow prompt");
            assert_eq!(second.await.unwrap(), "reply to fast prompt");

            let (shutdown_tx, shutdown_rx) = oneshot::channel();
            command_tx
                .send(SessionCommand::Shutdown {
                    respond_to: shutdown_tx,
                })
                .await
                .unwrap();
            shutdown_rx.await.unwrap();

            let state = actor.await.unwrap();
            assert_eq!(state.completed, ["first", "second"]);
            assert_eq!(
                state.events,
                [
                    "queued:first",
                    "started:first",
                    "queued:second",
                    "completed:first",
                    "started:second",
                    "completed:second",
                    "shutdown:flush",
                ]
            );
            println!("events: {}", state.events.join(" -> "));
        })
        .await;
}
