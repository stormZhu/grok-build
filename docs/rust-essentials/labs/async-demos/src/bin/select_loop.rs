use std::time::Duration;

use tokio::sync::mpsc;
use tokio::time::{Instant, sleep_until};

#[tokio::main(flavor = "current_thread", start_paused = true)]
async fn main() {
    let (tx, mut rx) = mpsc::channel(2);
    tx.send("queued").await.unwrap();

    let delayed_at = Instant::now() + Duration::from_millis(25);
    let sender = tokio::spawn(async move {
        sleep_until(delayed_at).await;
        tx.send("delayed").await.unwrap();
        // Dropping the final sender closes the channel.
    });

    let timeout = sleep_until(Instant::now() + Duration::from_millis(10));
    tokio::pin!(timeout);
    let mut timeout_enabled = true;
    let mut events = Vec::new();

    loop {
        tokio::select! {
            biased;

            message = rx.recv() => match message {
                Some(message) => events.push(message),
                None => {
                    events.push("closed");
                    break;
                }
            },
            () = timeout.as_mut(), if timeout_enabled => {
                events.push("timeout");
                timeout_enabled = false;
            }
        }
    }

    sender.await.unwrap();
    assert_eq!(events, ["queued", "timeout", "delayed", "closed"]);
    println!("events: {}", events.join(" -> "));
}
