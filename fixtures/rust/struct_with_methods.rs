use std::fmt;

/// A token counter that tracks usage statistics.
pub struct TokenCounter {
    pub raw_count: u64,
    pub skeleton_count: u64,
    compression_ratio: f64,
}

impl TokenCounter {
    /// Creates a new TokenCounter with zero counts.
    pub fn new() -> Self {
        Self {
            raw_count: 0,
            skeleton_count: 0,
            compression_ratio: 0.0,
        }
    }

    /// Records a raw token count and updates the ratio.
    pub fn record(&mut self, raw: u64, skeleton: u64) {
        self.raw_count += raw;
        self.skeleton_count += skeleton;
        if self.raw_count > 0 {
            self.compression_ratio =
                1.0 - (self.skeleton_count as f64 / self.raw_count as f64);
        }
    }

    /// Returns the compression ratio as a percentage.
    pub fn ratio(&self) -> f64 {
        self.compression_ratio * 100.0
    }
}

impl fmt::Display for TokenCounter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Tokens: {} raw, {} skeleton ({:.1}% compression)",
            self.raw_count,
            self.skeleton_count,
            self.ratio()
        )
    }
}
