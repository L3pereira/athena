/// Sync status for a symbol's order book
/// Domain concept representing the synchronization state machine
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncStatus {
    /// Not yet initialized - no data received
    Uninitialized,
    /// Waiting for snapshot (buffering updates)
    Syncing,
    /// Fully synced, applying updates normally
    Synced,
    /// Out of sync, needs resync (queued for snapshot)
    OutOfSync,
}

impl SyncStatus {
    /// Check if the symbol is ready for trading
    pub fn is_ready(&self) -> bool {
        matches!(self, SyncStatus::Synced)
    }

    /// Check if the symbol needs a snapshot
    pub fn needs_snapshot(&self) -> bool {
        matches!(
            self,
            SyncStatus::Uninitialized | SyncStatus::Syncing | SyncStatus::OutOfSync
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sync_status_ready() {
        assert!(!SyncStatus::Uninitialized.is_ready());
        assert!(!SyncStatus::Syncing.is_ready());
        assert!(SyncStatus::Synced.is_ready());
        assert!(!SyncStatus::OutOfSync.is_ready());
    }

    #[test]
    fn test_sync_status_needs_snapshot() {
        assert!(SyncStatus::Uninitialized.needs_snapshot());
        assert!(SyncStatus::Syncing.needs_snapshot());
        assert!(!SyncStatus::Synced.needs_snapshot());
        assert!(SyncStatus::OutOfSync.needs_snapshot());
    }
}
