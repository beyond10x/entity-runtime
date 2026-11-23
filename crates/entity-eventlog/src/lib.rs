//! Durable complete-record storage over Eventlog.
//!
//! This crate keeps Entity Runtime's canonical record, request, and batch bytes intact while
//! using Eventlog events as durable references and inline projections as rebuildable indexes.

mod adapter;
mod encoding;
#[cfg(feature = "sync-bridge")]
mod facade;
mod projection;

#[cfg(feature = "sync-bridge")]
pub mod sync;

pub use adapter::{
    AsyncBindingProvisioner, AsyncImportedAnchorWriter, CapturePolicy, EventlogBackend,
    EventlogBindingProvisioner, EventlogOperationContext, EventlogOperationStore,
    EventlogRecordedStore, ImportAnchorFailure, ImportAnchorOutcome, ImportAnchorUncertainty,
    OpenVerification, ProvisionBindingFailure, ProvisionBindingOutcome,
    ProvisionBindingUncertainty, StoreCalls,
};
pub use encoding::{Authority, PhysicalRef};
#[cfg(all(feature = "sync-bridge", feature = "file"))]
pub use facade::EventlogFileStore;
#[cfg(feature = "sync-bridge")]
pub use facade::{
    LegacyImportError, LegacyImportReport, RecordedProviderFacade, ScopedRecordedProviderFacade,
};
pub use projection::{ErRecordedProjector, projection_specs};

/// Event counters for the repository's KPI gate (`checks/kpi`), compiled only with the
/// non-default `kpi-counters` feature. Not a supported API.
///
/// Review finding E05: batch blob reads during an inline projection pass.
#[cfg(feature = "kpi-counters")]
#[doc(hidden)]
pub mod kpi_counters {
    use std::sync::atomic::{AtomicU64, Ordering};

    /// Batch blobs the inline projector read from its projection store.
    pub static BATCH_BLOB_FETCHES: AtomicU64 = AtomicU64::new(0);

    /// Bytes of those batch blobs.
    pub static BATCH_BLOB_BYTES: AtomicU64 = AtomicU64::new(0);

    pub(crate) fn add(counter: &AtomicU64, amount: u64) {
        counter.fetch_add(amount, Ordering::Relaxed);
    }
}
