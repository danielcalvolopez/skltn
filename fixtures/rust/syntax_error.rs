pub fn valid_function(x: i32) -> i32 {
    x + 1
}

pub fn another_valid(s: &str) -> String {
    s.to_uppercase()
}

// This line has a syntax error
pub fn broken(x: i32 -> {
    x
}

pub fn after_error(y: i32) -> i32 {
    y * 2
}
