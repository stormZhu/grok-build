use std::time::Duration;

use tokio::time::{Instant, sleep_until};

#[tokio::main(flavor = "current_thread", start_paused = true)]
async fn main() {
    let start = Instant::now();
    let deadlines = [10_u64, 25, 40];
    let timer = sleep_until(start + Duration::from_millis(deadlines[0]));
    tokio::pin!(timer);

    let mut observed = Vec::new();
    for (index, deadline_ms) in deadlines.into_iter().enumerate() {
        timer.as_mut().await;
        observed.push(Instant::now().duration_since(start).as_millis());

        if let Some(next_ms) = deadlines.get(index + 1) {
            timer
                .as_mut()
                .reset(start + Duration::from_millis(*next_ms));
        }

        assert_eq!(observed[index], u128::from(deadline_ms));
    }

    assert_eq!(observed, [10, 25, 40]);
    println!("one pinned Sleep fired after resets at {observed:?} ms");
}
