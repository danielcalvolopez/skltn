use std::collections::HashMap;

pub fn process_items(items: &[String]) -> Vec<String> {
    // Block-bodied closure — should be pruned
    let transform = |s: &String| {
        let trimmed = s.trim().to_lowercase();
        let mut result = String::with_capacity(trimmed.len() + 10);
        result.push_str("processed_");
        result.push_str(&trimmed);
        result
    };

    items.iter().map(transform).collect()
}

pub fn create_handler() -> impl Fn(i32) -> i32 {
    // Expression closure — should NOT be pruned
    |x| x * 2
}

pub fn sort_by_key(items: &mut [(String, i32)]) {
    // Short expression closure in method chain — should NOT be pruned
    items.sort_by(|a, b| a.1.cmp(&b.1));
}
