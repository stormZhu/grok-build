use tokio::sync::{mpsc, oneshot};

enum Command {
    Add {
        value: i32,
        reply: oneshot::Sender<i32>,
    },
    Get {
        reply: oneshot::Sender<i32>,
    },
    Shutdown,
}

async fn run_actor(mut commands: mpsc::Receiver<Command>) -> i32 {
    let mut total = 0;

    while let Some(command) = commands.recv().await {
        match command {
            Command::Add { value, reply } => {
                total += value;
                let _ = reply.send(total);
            }
            Command::Get { reply } => {
                let _ = reply.send(total);
            }
            Command::Shutdown => break,
        }
    }

    total
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let (command_tx, command_rx) = mpsc::channel(1);
    let actor = tokio::spawn(run_actor(command_rx));

    let (reply_tx, reply_rx) = oneshot::channel();
    command_tx
        .send(Command::Add {
            value: 7,
            reply: reply_tx,
        })
        .await
        .unwrap();
    assert_eq!(reply_rx.await.unwrap(), 7);

    let (reply_tx, reply_rx) = oneshot::channel();
    command_tx
        .send(Command::Get { reply: reply_tx })
        .await
        .unwrap();
    assert_eq!(reply_rx.await.unwrap(), 7);

    command_tx.send(Command::Shutdown).await.unwrap();
    let final_total = actor.await.unwrap();
    assert_eq!(final_total, 7);
    println!("actor replied through oneshot and shut down with total {final_total}");
}
