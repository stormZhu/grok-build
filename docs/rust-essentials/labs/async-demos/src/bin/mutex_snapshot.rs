use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{Mutex, oneshot};

async fn read_then_wait(
    state: Arc<Mutex<Vec<&'static str>>>,
    snapshot_ready: oneshot::Sender<()>,
    continue_after_check: oneshot::Receiver<()>,
) -> Vec<&'static str> {
    let snapshot = {
        let guard = state.lock().await;
        guard.clone()
    };

    snapshot_ready.send(()).unwrap();
    continue_after_check.await.unwrap();
    snapshot
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let state = Arc::new(Mutex::new(vec!["before"]));
    let (ready_tx, ready_rx) = oneshot::channel();
    let (continue_tx, continue_rx) = oneshot::channel();

    let reader = tokio::spawn(read_then_wait(state.clone(), ready_tx, continue_rx));
    ready_rx.await.unwrap();

    let mut guard = tokio::time::timeout(Duration::from_millis(10), state.lock())
        .await
        .expect("the reader must release its mutex guard before awaiting");
    guard.push("after");
    drop(guard);

    continue_tx.send(()).unwrap();
    let snapshot = reader.await.unwrap();
    assert_eq!(snapshot, ["before"]);
    assert_eq!(*state.lock().await, ["before", "after"]);
    println!("reader kept an owned snapshot while another task acquired the mutex");
}
