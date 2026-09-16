//! Durable complete-record storage over Eventlog.
//!
//! This crate keeps Entity Runtime's canonical record, request, and batch bytes intact while
//! using Eventlog events as durable references and inline projections as rebuildable indexes.

mod adapter;
mod encoding;
mod projection;

#[cfg(feature = "sync-bridge")]
pub mod sync;

pub use adapter::{
    AsyncBindingProvisioner, AsyncImportedAnchorWriter, EventlogBackend,
    EventlogBindingProvisioner, EventlogOperationContext, EventlogOperationStore,
    EventlogRecordedStore, ImportAnchorFailure, ImportAnchorOutcome, ImportAnchorUncertainty,
    ProvisionBindingFailure, ProvisionBindingOutcome, ProvisionBindingUncertainty,
};
pub use encoding::{Authority, PhysicalRef};
pub use projection::{ErRecordedProjector, projection_specs};
