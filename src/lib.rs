//! # Semver Compatibility
//!
//! cargo-release's versioning tracks compatibility for the binaries, not the API.  We upload to
//! crates.io to distribute the binary.  If using this as a library, be sure to pin the version
//! with a `=` version requirement operator.

#![cfg_attr(docsrs, feature(doc_auto_cfg))]

pub mod config;
mod diff;
pub mod error;
pub mod ops;
pub mod steps;
