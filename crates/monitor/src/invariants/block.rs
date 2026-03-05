//! Block structure invariant checks.
//!
//! Verifies that committed blocks satisfy the structural constraints defined in the Tempo protocol
//! specification. Each check mirrors a consensus rule and is run independently on every committed
//! block so that a violation is surfaced even if the consensus layer has a bug.

use super::{CheckResult, InvariantFailure, InvariantId};
use alloy_consensus::Transaction;
use alloy_primitives::Address;
use tempo_primitives::TempoTxEnvelope;

/// Divisor used to derive `shared_gas_limit` from the block `gas_limit`.
const SHARED_GAS_DIVISOR: u64 = 10;

/// Input required for block-level invariant checks.
pub struct BlockInput<'a> {
    /// `header.gas_limit`
    pub gas_limit: u64,
    /// `header.gas_used`
    pub gas_used: u64,
    /// `header.general_gas_limit`
    pub general_gas_limit: u64,
    /// `header.shared_gas_limit`
    pub shared_gas_limit: u64,
    /// Ordered list of transactions in the block body.
    pub transactions: &'a [TempoTxEnvelope],
}

/// Run all block-level invariant checks and return the combined result.
pub fn check(input: &BlockInput<'_>) -> CheckResult {
    let mut failures = Vec::new();

    check_total_gas(input, &mut failures);
    check_general_lane(input, &mut failures);
    check_system_tx_last(input, &mut failures);
    check_shared_gas(input, &mut failures);
    check_subblock_contiguous(input, &mut failures);

    CheckResult { failures }
}

/// BLOCK-TOTAL-GAS: `gas_used <= gas_limit`
fn check_total_gas(input: &BlockInput<'_>, failures: &mut Vec<InvariantFailure>) {
    if input.gas_used > input.gas_limit {
        failures.push(InvariantFailure::new(
            InvariantId::BlockTotalGas,
            format!(
                "gas_used ({}) > gas_limit ({})",
                input.gas_used, input.gas_limit
            ),
        ));
    }
}

/// BLOCK-GENERAL-LANE: sum of `gas_limit` for non-payment, non-system, non-subblock txs must fit
/// within `general_gas_limit`.
///
/// Uses per-tx `gas_limit` as a conservative upper bound since the monitor does not have per-tx
/// receipts in delta mode. If this passes with `gas_limit`, it necessarily passes with `gas_used`.
fn check_general_lane(input: &BlockInput<'_>, failures: &mut Vec<InvariantFailure>) {
    let general_gas: u64 = input
        .transactions
        .iter()
        .filter(|tx| !tx.is_system_tx() && !tx.is_payment() && tx.subblock_proposer().is_none())
        .map(|tx| tx.gas_limit())
        .sum();

    if general_gas > input.general_gas_limit {
        failures.push(InvariantFailure::new(
            InvariantId::BlockGeneralLane,
            format!(
                "general lane tx gas ({general_gas}) > general_gas_limit ({})",
                input.general_gas_limit
            ),
        ));
    }
}

/// BLOCK-SYSTEM-TX-LAST: the last transaction must be a system tx targeting `Address::ZERO`.
fn check_system_tx_last(input: &BlockInput<'_>, failures: &mut Vec<InvariantFailure>) {
    let Some(last) = input.transactions.last() else {
        failures.push(InvariantFailure::new(
            InvariantId::BlockSystemTxLast,
            "block has no transactions",
        ));
        return;
    };

    if !last.is_system_tx() {
        failures.push(InvariantFailure::new(
            InvariantId::BlockSystemTxLast,
            "last transaction is not a system tx",
        ));
        return;
    }

    if last.to().unwrap_or_default() != Address::ZERO {
        failures.push(InvariantFailure::new(
            InvariantId::BlockSystemTxLast,
            format!(
                "last system tx targets {:?}, expected Address::ZERO",
                last.to()
            ),
        ));
    }
}

/// BLOCK-SHARED-GAS: `shared_gas_limit == gas_limit / SHARED_GAS_DIVISOR`
fn check_shared_gas(input: &BlockInput<'_>, failures: &mut Vec<InvariantFailure>) {
    let expected = input.gas_limit / SHARED_GAS_DIVISOR;
    if input.shared_gas_limit != expected {
        failures.push(InvariantFailure::new(
            InvariantId::BlockSharedGas,
            format!(
                "shared_gas_limit ({}) != gas_limit / {} ({expected})",
                input.shared_gas_limit, SHARED_GAS_DIVISOR
            ),
        ));
    }
}

/// BLOCK-SUBBLOCK-CONTIGUOUS: subblock transactions from the same validator must appear as a
/// contiguous run. If validator A's subblock txs appear, then B's, then A's again, that is a
/// violation.
fn check_subblock_contiguous(input: &BlockInput<'_>, failures: &mut Vec<InvariantFailure>) {
    use std::collections::HashSet;

    let mut current_proposer = None;
    let mut finished_proposers = HashSet::new();

    for tx in input.transactions {
        if let Some(proposer) = tx.subblock_proposer() {
            if Some(proposer) != current_proposer {
                if finished_proposers.contains(&proposer) {
                    failures.push(InvariantFailure::new(
                        InvariantId::BlockSubblockContiguous,
                        format!("validator {proposer} subblock is non-contiguous"),
                    ));
                    return;
                }
                if let Some(prev) = current_proposer.take() {
                    finished_proposers.insert(prev);
                }
                current_proposer = Some(proposer);
            }
        } else if let Some(prev) = current_proposer.take() {
            finished_proposers.insert(prev);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_consensus::{Signed, TxLegacy};
    use alloy_primitives::{Signature, TxKind, U256};

    fn system_tx(to: Address) -> TempoTxEnvelope {
        let tx = TxLegacy {
            chain_id: Some(1),
            nonce: 0,
            gas_price: 0,
            gas_limit: 0,
            to: TxKind::Call(to),
            value: U256::ZERO,
            input: Default::default(),
        };
        TempoTxEnvelope::Legacy(Signed::new_unhashed(
            tx,
            Signature::new(U256::ZERO, U256::ZERO, false),
        ))
    }

    fn regular_tx(gas_limit: u64) -> TempoTxEnvelope {
        let tx = TxLegacy {
            chain_id: Some(1),
            nonce: 1,
            gas_price: 1_000_000_000,
            gas_limit,
            to: TxKind::Call(Address::repeat_byte(0x42)),
            value: U256::from(100),
            input: Default::default(),
        };
        TempoTxEnvelope::Legacy(Signed::new_unhashed(tx, Signature::test_signature()))
    }

    fn default_input(transactions: &[TempoTxEnvelope]) -> BlockInput<'_> {
        BlockInput {
            gas_limit: 500_000_000,
            gas_used: 1_000_000,
            general_gas_limit: 30_000_000,
            shared_gas_limit: 50_000_000, // 500M / 10
            transactions,
        }
    }

    #[test]
    fn all_pass_valid_block() {
        let txs = vec![regular_tx(21_000), system_tx(Address::ZERO)];
        let result = check(&default_input(&txs));
        assert!(result.ok(), "expected no failures: {:?}", result.failures);
    }

    #[test]
    fn total_gas_violation() {
        let txs = vec![system_tx(Address::ZERO)];
        let mut input = default_input(&txs);
        input.gas_used = input.gas_limit + 1;
        let result = check(&input);
        assert!(
            result
                .failures
                .iter()
                .any(|f| f.id == InvariantId::BlockTotalGas)
        );
    }

    #[test]
    fn general_lane_violation() {
        let txs = vec![regular_tx(30_000_001), system_tx(Address::ZERO)];
        let result = check(&default_input(&txs));
        assert!(
            result
                .failures
                .iter()
                .any(|f| f.id == InvariantId::BlockGeneralLane)
        );
    }

    #[test]
    fn system_tx_last_missing() {
        let txs = vec![regular_tx(21_000)];
        let result = check(&default_input(&txs));
        assert!(
            result
                .failures
                .iter()
                .any(|f| f.id == InvariantId::BlockSystemTxLast)
        );
    }

    #[test]
    fn system_tx_last_wrong_target() {
        let txs = vec![system_tx(Address::repeat_byte(0x01))];
        let result = check(&default_input(&txs));
        assert!(
            result
                .failures
                .iter()
                .any(|f| f.id == InvariantId::BlockSystemTxLast)
        );
    }

    #[test]
    fn shared_gas_mismatch() {
        let txs = vec![system_tx(Address::ZERO)];
        let mut input = default_input(&txs);
        input.shared_gas_limit = 999;
        let result = check(&input);
        assert!(
            result
                .failures
                .iter()
                .any(|f| f.id == InvariantId::BlockSharedGas)
        );
    }

    #[test]
    fn empty_block_fails() {
        let result = check(&default_input(&[]));
        assert!(
            result
                .failures
                .iter()
                .any(|f| f.id == InvariantId::BlockSystemTxLast)
        );
    }
}
