use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;

struct Cleanup(Arc<AtomicBool>);

impl Drop for Cleanup {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
        println!("cleanup guard dropped");
    }
}

async fn operation(
    started: Arc<AtomicBool>,
    committed: Arc<AtomicBool>,
    cleaned_up: Arc<AtomicBool>,
) {
    started.store(true, Ordering::SeqCst);
    println!("external side effect: operation marked as started");
    let _cleanup = Cleanup(cleaned_up);

    tokio::time::sleep(Duration::from_millis(50)).await;
    committed.store(true, Ordering::SeqCst);
}

#[tokio::main(flavor = "current_thread", start_paused = true)]
async fn main() {
    let started = Arc::new(AtomicBool::new(false));
    let committed = Arc::new(AtomicBool::new(false));
    let cleaned_up = Arc::new(AtomicBool::new(false));

    let outcome = tokio::select! {
        biased;

        () = operation(started.clone(), committed.clone(), cleaned_up.clone()) => "completed",
        () = tokio::time::sleep(Duration::from_millis(10)) => "timed out",
    };

    assert_eq!(outcome, "timed out");
    assert!(started.load(Ordering::SeqCst));
    assert!(!committed.load(Ordering::SeqCst));
    assert!(cleaned_up.load(Ordering::SeqCst));
    println!("outcome: {outcome}; dropping ran cleanup but did not undo `started`");
}
