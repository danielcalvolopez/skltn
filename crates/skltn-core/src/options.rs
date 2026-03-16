/// Configuration for the skeletonization process.
#[derive(Debug, Clone, Default)]
pub struct SkeletonOptions {
    /// Maximum nesting depth of leaf structural nodes to skeletonize.
    /// None means unlimited depth.
    pub max_depth: Option<usize>,
}

