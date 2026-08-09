// expected-error: E0382

fn consume(command: String) {
    println!("running {command}");
}

fn main() {
    let command = String::from("cargo check");
    consume(command);
    println!("finished {command}");
}
