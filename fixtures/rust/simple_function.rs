use std::collections::HashMap;

pub fn add(a: i32, b: i32) -> i32 {
    let result = a + b;
    println!("Adding {} + {} = {}", a, b, result);
    result
}

fn helper(x: &str) -> String {
    let mut s = String::from(x);
    s.push_str("_processed");
    s.to_uppercase()
}
