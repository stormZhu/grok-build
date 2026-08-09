use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;

struct DropFlag {
    name: &'static str,
    dropped: Arc<AtomicBool>,
}

impl Drop for DropFlag {
    fn drop(&mut self) {
        self.dropped.store(true, Ordering::SeqCst);
        println!("drop future: {}", self.name);
    }
}

async fn candidate(name: &'static str, delay: Duration, dropped: Arc<AtomicBool>) -> &'static str {
    let _drop_flag = DropFlag { name, dropped };
    tokio::time::sleep(delay).await;
    name
}

#[tokio::main(flavor = "current_thread", start_paused = true)]
async fn main() {
    let fast_dropped = Arc::new(AtomicBool::new(false));
    let slow_dropped = Arc::new(AtomicBool::new(false));

    let winner = tokio::select! {
        name = candidate("fast", Duration::from_millis(10), fast_dropped.clone()) => name,
        name = candidate("slow", Duration::from_millis(30), slow_dropped.clone()) => name,
    };

    assert_eq!(winner, "fast");
    assert!(fast_dropped.load(Ordering::SeqCst));
    assert!(slow_dropped.load(Ordering::SeqCst));
    println!("winner: {winner}; the unfinished slow future was dropped");
}
