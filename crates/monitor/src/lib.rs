//! Tempo chain monitor.
//!
//! Independent verification of block structure invariants as defined in the Tempo protocol
//! specification. Although these constraints are enforced by consensus, the monitor re-checks them
//! on committed blocks to catch potential consensus or executor bugs.

#![cfg_attr(not(test), warn(unused_crate_dependencies))]

pub mod invariants;
