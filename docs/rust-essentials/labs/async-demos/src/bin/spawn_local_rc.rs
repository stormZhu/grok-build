use std::cell::RefCell;
use std::rc::Rc;

async fn record_task(name: &'static str, events: Rc<RefCell<Vec<String>>>) {
    events.borrow_mut().push(format!("{name}:start"));
    tokio::task::yield_now().await;
    events.borrow_mut().push(format!("{name}:end"));
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let local = tokio::task::LocalSet::new();

    let events = local
        .run_until(async {
            let events = Rc::new(RefCell::new(Vec::new()));
            let first = tokio::task::spawn_local(record_task("first", events.clone()));
            let second = tokio::task::spawn_local(record_task("second", events.clone()));

            first.await.unwrap();
            second.await.unwrap();
            events
        })
        .await;

    let mut observed = events.borrow().clone();
    observed.sort();
    assert_eq!(
        observed,
        ["first:end", "first:start", "second:end", "second:start"]
    );
    println!("two local tasks safely shared Rc<RefCell<_>>: {observed:?}");
}
