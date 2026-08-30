//! A transport that goes nowhere, and says so.
//!
//! Serves requests from a store in this process, through the **real** JSON round trip: every
//! request is serialised, parsed back, answered, serialised and parsed back again. So the wire shape
//! is exercised — a field that failed to round-trip fails here — while nothing opens a socket.
//!
//! # Why this is honest and not a mock
//!
//! It does not stand in for the far side's *logic*: it runs the same [`crate::answer`] a server
//! would. What it stands in for is the **network**, and it is labelled as doing exactly that. A
//! test using it proves the protocol; it proves nothing about latency, partitions or TLS, and this
//! module says so rather than letting a green suite imply it.
//!
//! It can also be told to fail, which is the case worth having: a store must tell *unreachable*
//! from *absent*, and the only way to check that is to make reaching it fail.

use std::cell::RefCell;

use entity_store::RecordedStore;

use crate::{answer, Answer, Request, Transport};

/// A [`Transport`] backed by a store in this process.
pub struct LoopbackTransport<S> {
    store: RefCell<S>,
    unreachable: RefCell<Option<String>>,
}

impl<S: RecordedStore> LoopbackTransport<S> {
    /// A transport serving `store`.
    pub const fn new(store: S) -> Self {
        Self {
            store: RefCell::new(store),
            unreachable: RefCell::new(None),
        }
    }

    /// Makes every later call fail as unreachable, with this reason.
    pub fn go_dark(&self, reason: impl Into<String>) {
        *self.unreachable.borrow_mut() = Some(reason.into());
    }

    /// Makes calls succeed again.
    pub fn come_back(&self) {
        *self.unreachable.borrow_mut() = None;
    }

    /// The store behind it, for a test that wants to look at the far side directly.
    pub fn store(&self) -> std::cell::Ref<'_, S> {
        self.store.borrow()
    }

    /// The store behind it, writable — for a test that needs the far side to move **on its own**.
    ///
    /// The case this exists for: a replica that somebody else wrote to while this side was dark.
    /// Without a way to move it independently, every reconciliation test is a test of a replica
    /// that only ever received what this side sent it, which is the case that cannot conflict.
    ///
    /// # Panics
    ///
    /// If the store is already borrowed — which, in a single-threaded test, means a `store()`
    /// still held in scope.
    pub fn store_mut(&self) -> std::cell::RefMut<'_, S> {
        self.store.borrow_mut()
    }
}

impl<S: RecordedStore> Transport for LoopbackTransport<S> {
    fn call(&self, request: &Request) -> Result<Answer, String> {
        if let Some(reason) = self.unreachable.borrow().as_ref() {
            return Err(reason.clone());
        }

        // The real round trip. A field that does not survive JSON fails here rather than in
        // somebody's deployment.
        let wire = serde_json::to_string(request).map_err(|error| error.to_string())?;
        let parsed: Request = serde_json::from_str(&wire).map_err(|error| error.to_string())?;

        let answered = answer(&mut *self.store.borrow_mut(), &parsed)?;
        let wire = serde_json::to_string(&answered).map_err(|error| error.to_string())?;
        serde_json::from_str(&wire).map_err(|error| error.to_string())
    }

    fn name(&self) -> String {
        "the loopback store".to_owned()
    }
}
