//! The library database. Its schema is the one the retired macOS app created
//! through GRDB — same tables, same migration ledger, same text encodings for
//! UUIDs and dates — so the libraries that app built open here unchanged.

mod codec;
mod schema;
mod store;

pub use store::{LibraryStore, ProxyEntry, StoreError};
