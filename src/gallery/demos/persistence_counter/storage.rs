#[cfg(all(
    feature = "std",
    feature = "persistence",
    target_arch = "wasm32",
    feature = "web-canvas",
))]
pub(super) fn pick_storage()
-> Result<crate::core::storage::StorageHandle, crate::core::storage::StorageError> {
    use crate::core::storage::{LocalStorageStorage, Storage};
    LocalStorageStorage::with_prefix("mirui_gallery").map(Storage::into_handle)
}

// Desktop / std non-wasm: persist to a temp file so the counter
// survives reruns without leaving permanent state on the user's
// machine. A real app would pick a stable config directory instead.
#[cfg(all(feature = "std", feature = "persistence", not(target_arch = "wasm32")))]
pub(super) fn pick_storage()
-> Result<crate::core::storage::StorageHandle, crate::core::storage::StorageError> {
    use crate::core::storage::Storage;
    let mut path = std::env::temp_dir();
    path.push("mirui_persistence_counter.bin");
    crate::core::storage::FileStorage::open(path).map(Storage::into_handle)
}
