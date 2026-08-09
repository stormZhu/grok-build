// expected-error: E0716

fn main() {
    let label = String::from("session-1").as_str();
    println!("{label}");
}
