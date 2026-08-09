// expected-error: E0515

fn session_label() -> &'static str {
    let label = String::from("session-1");
    label.as_str()
}

fn main() {
    println!("{}", session_label());
}
