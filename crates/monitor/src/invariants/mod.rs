//! Block-level invariant checks.

pub mod block;

use std::fmt;

/// Unique identifier for each invariant check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InvariantId {
    /// `gas_used <= gas_limit`
    BlockTotalGas,
    /// Non-payment, non-subblock tx gas fits within `general_gas_limit`.
    BlockGeneralLane,
    /// Last transaction is a system transaction targeting `Address::ZERO`.
    BlockSystemTxLast,
    /// `shared_gas_limit == gas_limit / 10`
    BlockSharedGas,
    /// Subblock transactions are contiguous per validator.
    BlockSubblockContiguous,
}

impl fmt::Display for InvariantId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BlockTotalGas => write!(f, "BLOCK-TOTAL-GAS"),
            Self::BlockGeneralLane => write!(f, "BLOCK-GENERAL-LANE"),
            Self::BlockSystemTxLast => write!(f, "BLOCK-SYSTEM-TX-LAST"),
            Self::BlockSharedGas => write!(f, "BLOCK-SHARED-GAS"),
            Self::BlockSubblockContiguous => write!(f, "BLOCK-SUBBLOCK-CONTIGUOUS"),
        }
    }
}

/// A single invariant violation.
#[derive(Debug, Clone)]
pub struct InvariantFailure {
    /// Which invariant was violated.
    pub id: InvariantId,
    /// Human-readable description of the violation.
    pub message: String,
}

impl InvariantFailure {
    fn new(id: InvariantId, message: impl Into<String>) -> Self {
        Self {
            id,
            message: message.into(),
        }
    }
}

impl fmt::Display for InvariantFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}", self.id, self.message)
    }
}

/// Result of running one or more invariant checks against a block.
#[derive(Debug, Default)]
pub struct CheckResult {
    /// Invariant violations found.
    pub failures: Vec<InvariantFailure>,
}

impl CheckResult {
    /// Returns `true` if all checked invariants passed.
    pub fn ok(&self) -> bool {
        self.failures.is_empty()
    }
}
