use tokio::sync::{mpsc, oneshot};

#[derive(Debug, PartialEq, Eq)]
enum ClientUpdate {
    Text(String),
    ToolCall(String),
    TurnCompleted,
}

struct ReplayBuffer {
    pending_text: Option<String>,
}

impl ReplayBuffer {
    fn new() -> Self {
        Self { pending_text: None }
    }

    fn consume(&mut self, update: ClientUpdate) -> Vec<ClientUpdate> {
        match update {
            ClientUpdate::Text(text) => {
                self.pending_text.get_or_insert_default().push_str(&text);
                Vec::new()
            }
            non_streaming => {
                let mut ready = self.flush().into_iter().collect::<Vec<_>>();
                ready.push(non_streaming);
                ready
            }
        }
    }

    fn flush(&mut self) -> Option<ClientUpdate> {
        self.pending_text.take().map(ClientUpdate::Text)
    }
}

struct SessionEvent {
    update: ClientUpdate,
    processed: oneshot::Sender<()>,
}

async fn run_session(
    mut event_rx: mpsc::Receiver<SessionEvent>,
    mut completion_rx: mpsc::Receiver<()>,
) -> Vec<ClientUpdate> {
    let mut replay_buffer = ReplayBuffer::new();
    let mut emitted = Vec::new();

    loop {
        tokio::select! {
            biased;

            Some(event) = event_rx.recv() => {
                emitted.extend(replay_buffer.consume(event.update));
                let _ = event.processed.send(());
            }
            Some(()) = completion_rx.recv() => {
                if let Some(pending) = replay_buffer.flush() {
                    emitted.push(pending);
                }
                emitted.push(ClientUpdate::TurnCompleted);
                return emitted;
            }
        }
    }
}

async fn send_event(event_tx: &mpsc::Sender<SessionEvent>, update: ClientUpdate) {
    let (processed_tx, processed_rx) = oneshot::channel();
    event_tx
        .send(SessionEvent {
            update,
            processed: processed_tx,
        })
        .await
        .unwrap();
    processed_rx.await.unwrap();
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let (event_tx, event_rx) = mpsc::channel(4);
    let (completion_tx, completion_rx) = mpsc::channel(1);
    let session = tokio::spawn(run_session(event_rx, completion_rx));

    send_event(&event_tx, ClientUpdate::Text("Hel".to_owned())).await;
    send_event(&event_tx, ClientUpdate::Text("lo".to_owned())).await;
    send_event(
        &event_tx,
        ClientUpdate::ToolCall("read_file started".to_owned()),
    )
    .await;
    send_event(&event_tx, ClientUpdate::Text(" world".to_owned())).await;

    completion_tx.send(()).await.unwrap();
    let emitted = session.await.unwrap();
    assert_eq!(
        emitted,
        [
            ClientUpdate::Text("Hello".to_owned()),
            ClientUpdate::ToolCall("read_file started".to_owned()),
            ClientUpdate::Text(" world".to_owned()),
            ClientUpdate::TurnCompleted,
        ]
    );
    println!("emitted in order: {emitted:?}");
}
