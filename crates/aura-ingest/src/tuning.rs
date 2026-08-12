//! How many threads to point at a volume. Getting this wrong turns a card
//! reader into a seek storm.

use std::path::Path;

/// The storage class we believe a root lives on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageClass {
    /// Internal `NVMe` or a fast local SSD.
    FastLocal,
    /// SATA SSD, USB 3 SSD or an unknown local volume.
    ModerateLocal,
    /// Spinning disk or an SD card reader.
    Slow,
    /// A network share. Latency bound, and a candidate for refusal.
    Network,
}

/// Classify a root by the shape of its path.
///
/// This is deliberately conservative: a wrong guess towards fewer threads costs
/// throughput, a wrong guess towards more costs a stalled import.
#[must_use]
pub fn classify_root(root: &Path) -> StorageClass {
    let text = root.to_string_lossy().to_lowercase();
    if text.starts_with("\\\\") || text.starts_with("//") || text.starts_with("/volumes/") {
        return StorageClass::Network;
    }
    if text.contains("dcim") || text.contains("card") || text.contains("untitled") {
        return StorageClass::Slow;
    }
    StorageClass::ModerateLocal
}

/// Hash threads for a storage class, given the machine's parallelism.
#[must_use]
pub fn hash_threads_for_class(class: StorageClass, available: usize) -> usize {
    match class {
        StorageClass::FastLocal => available.clamp(1, 8),
        StorageClass::ModerateLocal => available.clamp(1, 4),
        StorageClass::Slow => 1,
        StorageClass::Network => 2,
    }
}

/// Hash threads for one root.
#[must_use]
pub fn hash_threads_for(root: &Path) -> usize {
    let available = std::thread::available_parallelism().map_or(4, std::num::NonZeroUsize::get);
    hash_threads_for_class(classify_root(root), available)
}
