//! Playback proxies. Port of the Swift `FPVProxy` module.
//!
//! A proxy is the second deliberate exception to "never re-encode DVR
//! footage", scoped tightly: **a view-only derivative, never an export source**.
//! It is resampled, so its frames are not the recording's — boundaries are
//! edited against the source and flights exported from it.

mod cache;
mod generator;

pub use cache::{DEFAULT_MAX_BYTES, ProxyCache};
pub use generator::{ProxyError, ProxyGenerator};
