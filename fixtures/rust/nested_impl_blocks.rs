pub trait Processor {
    fn process(&self, input: &str) -> String;
}

pub struct TextProcessor {
    pub prefix: String,
}

impl TextProcessor {
    pub fn new(prefix: &str) -> Self {
        Self {
            prefix: prefix.to_string(),
        }
    }

    pub fn with_suffix(&self, input: &str, suffix: &str) -> String {
        let mut result = self.process(input);
        result.push_str(suffix);
        result
    }
}

impl Processor for TextProcessor {
    fn process(&self, input: &str) -> String {
        let trimmed = input.trim();
        let result = format!("{}_{}", self.prefix, trimmed);
        result.to_uppercase()
    }
}
