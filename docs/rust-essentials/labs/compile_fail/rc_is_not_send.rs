// expected-error: E0277

use std::rc::Rc;
use std::thread;

fn main() {
    let session = Rc::new(String::from("session-1"));
    let worker = thread::spawn(move || println!("{session}"));
    worker.join().unwrap();
}
