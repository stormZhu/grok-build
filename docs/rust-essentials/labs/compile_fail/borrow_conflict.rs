// expected-error: E0502

fn main() {
    let mut tools = vec![String::from("read")];
    let first = &tools[0];

    tools.push(String::from("write"));
    println!("first tool: {first}");
}
