use tokio::sync::watch;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let (tx, mut rx) = watch::channel(0_u32);

    tx.send(1).unwrap();
    tx.send(2).unwrap();
    tx.send(3).unwrap();

    rx.changed().await.unwrap();
    let latest = *rx.borrow_and_update();
    assert_eq!(latest, 3);
    println!("slow receiver observed only the latest value: {latest}");

    tx.send(3).unwrap();
    rx.changed().await.unwrap();
    assert_eq!(*rx.borrow_and_update(), 3);
    println!("sending the same value still created a new watch version");

    drop(tx);
    assert!(rx.changed().await.is_err());
    println!("changed() reported closure after the final sender was dropped");
}
