use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::time::Duration;

use tokio::sync::{mpsc, oneshot, watch};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TurnOutcome {
    Completed,
    Cancelled,
}

struct StartTurn {
    id: &'static str,
    duration: Duration,
    respond_to: oneshot::Sender<TurnOutcome>,
}

enum SessionCommand {
    Start(StartTurn),
    Cancel { respond_to: oneshot::Sender<()> },
    Shutdown { respond_to: oneshot::Sender<()> },
}

struct TurnCompletion {
    id: &'static str,
    outcome: TurnOutcome,
}

struct RunningTurn {
    id: &'static str,
    cancel_tx: watch::Sender<bool>,
    respond_to: oneshot::Sender<TurnOutcome>,
}

struct Cleanup(Arc<AtomicUsize>);

impl Drop for Cleanup {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

async fn run_turn(
    id: &'static str,
    duration: Duration,
    mut cancel_rx: watch::Receiver<bool>,
    completion_tx: mpsc::Sender<TurnCompletion>,
    cleanup_count: Arc<AtomicUsize>,
) {
    let outcome = {
        let _cleanup = Cleanup(cleanup_count);
        tokio::select! {
            biased;

            changed = cancel_rx.changed() => {
                assert!(changed.is_ok());
                assert!(*cancel_rx.borrow_and_update());
                TurnOutcome::Cancelled
            }
            () = tokio::time::sleep(duration) => TurnOutcome::Completed,
        }
    };

    completion_tx
        .send(TurnCompletion { id, outcome })
        .await
        .unwrap();
}

async fn run_session(
    mut command_rx: mpsc::Receiver<SessionCommand>,
    completion_tx: mpsc::Sender<TurnCompletion>,
    mut completion_rx: mpsc::Receiver<TurnCompletion>,
    turn_cleanup_count: Arc<AtomicUsize>,
    workflow_cleanup_count: Arc<AtomicUsize>,
) -> Vec<String> {
    let (workflow_shutdown_tx, workflow_shutdown_rx) = oneshot::channel();
    let workflow = tokio::task::spawn_local(async move {
        let _cleanup = Cleanup(workflow_cleanup_count);
        let _ = workflow_shutdown_rx.await;
    });

    let mut running: Option<RunningTurn> = None;
    let mut cancel_waiter: Option<oneshot::Sender<()>> = None;
    let mut events = Vec::new();

    loop {
        tokio::select! {
            biased;

            Some(completion) = completion_rx.recv() => {
                let turn = running.take().expect("completion must belong to a running turn");
                assert_eq!(turn.id, completion.id);
                events.push(match completion.outcome {
                    TurnOutcome::Completed => format!("turn_completed:{}", completion.id),
                    TurnOutcome::Cancelled => format!("turn_cancelled:{}", completion.id),
                });
                let _ = turn.respond_to.send(completion.outcome);
                if let Some(waiter) = cancel_waiter.take() {
                    let _ = waiter.send(());
                }
            }
            command = command_rx.recv() => match command {
                Some(SessionCommand::Start(turn)) => {
                    assert!(running.is_none(), "this model allows one foreground turn");
                    let (cancel_tx, cancel_rx) = watch::channel(false);
                    events.push(format!("turn_started:{}", turn.id));
                    tokio::task::spawn_local(run_turn(
                        turn.id,
                        turn.duration,
                        cancel_rx,
                        completion_tx.clone(),
                        turn_cleanup_count.clone(),
                    ));
                    running = Some(RunningTurn {
                        id: turn.id,
                        cancel_tx,
                        respond_to: turn.respond_to,
                    });
                }
                Some(SessionCommand::Cancel { respond_to }) => {
                    let turn = running.as_ref().expect("Cancel requires a running turn");
                    events.push(format!("cancel_requested:{}", turn.id));
                    turn.cancel_tx.send_replace(true);
                    cancel_waiter = Some(respond_to);
                }
                Some(SessionCommand::Shutdown { respond_to }) => {
                    assert!(running.is_none(), "Shutdown follows foreground turn cleanup");
                    events.push("shutdown:requested".to_owned());
                    let _ = workflow_shutdown_tx.send(());
                    workflow.await.unwrap();
                    events.push("shutdown:workflow_joined".to_owned());
                    events.push("shutdown:flush".to_owned());
                    let _ = respond_to.send(());
                    return events;
                }
                None => return events,
            }
        }
    }
}

async fn start_turn(
    command_tx: &mpsc::Sender<SessionCommand>,
    id: &'static str,
    duration: Duration,
) -> oneshot::Receiver<TurnOutcome> {
    let (respond_to, response) = oneshot::channel();
    command_tx
        .send(SessionCommand::Start(StartTurn {
            id,
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
            let turn_cleanup_count = Arc::new(AtomicUsize::new(0));
            let workflow_cleanup_count = Arc::new(AtomicUsize::new(0));
            let (command_tx, command_rx) = mpsc::channel(4);
            let (completion_tx, completion_rx) = mpsc::channel(1);
            let session = tokio::task::spawn_local(run_session(
                command_rx,
                completion_tx,
                completion_rx,
                turn_cleanup_count.clone(),
                workflow_cleanup_count.clone(),
            ));

            let first = start_turn(&command_tx, "first", Duration::from_millis(50)).await;
            let (cancel_tx, cancel_rx) = oneshot::channel();
            command_tx
                .send(SessionCommand::Cancel {
                    respond_to: cancel_tx,
                })
                .await
                .unwrap();
            cancel_rx.await.unwrap();
            assert_eq!(first.await.unwrap(), TurnOutcome::Cancelled);
            assert_eq!(turn_cleanup_count.load(Ordering::SeqCst), 1);

            let second = start_turn(&command_tx, "second", Duration::from_millis(10)).await;
            assert_eq!(second.await.unwrap(), TurnOutcome::Completed);
            assert_eq!(turn_cleanup_count.load(Ordering::SeqCst), 2);

            let (shutdown_tx, shutdown_rx) = oneshot::channel();
            command_tx
                .send(SessionCommand::Shutdown {
                    respond_to: shutdown_tx,
                })
                .await
                .unwrap();
            shutdown_rx.await.unwrap();

            let events = session.await.unwrap();
            assert_eq!(workflow_cleanup_count.load(Ordering::SeqCst), 1);
            assert_eq!(
                events,
                [
                    "turn_started:first",
                    "cancel_requested:first",
                    "turn_cancelled:first",
                    "turn_started:second",
                    "turn_completed:second",
                    "shutdown:requested",
                    "shutdown:workflow_joined",
                    "shutdown:flush",
                ]
            );
            println!("events: {}", events.join(" -> "));
        })
        .await;
}
