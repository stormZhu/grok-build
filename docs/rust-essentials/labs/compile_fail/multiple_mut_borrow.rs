// expected-error: E0499

fn main() {
    let mut tools = vec![String::from("read"), String::from("write")];
    let first = &mut tools[0];
    let second = &mut tools[1];

    first.push_str("_file");
    second.push_str("_file");
}
