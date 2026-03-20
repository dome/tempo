#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use std::sync::Arc;

use alloy_primitives::{Address, TxHash, U256};
use reth_transaction_pool::{PoolTransaction, TransactionOrigin};
use tempo_chainspec::hardfork::TempoHardfork;
use tempo_transaction_pool::{
    AA2dPool,
    test_utils::{TxBuilder, wrap_valid_tx},
};

const NUM_SENDERS: u8 = 4;
const NUM_NONCE_KEYS: u8 = 3;

#[derive(Debug, Arbitrary)]
enum PoolOp {
    AddTx {
        sender_idx: u8,
        nonce_key: u8,
        nonce: u8,
        priority_fee: u16,
    },
    RemoveByHash {
        tx_slot: u8,
    },
    IterateBest {
        steps: u8,
    },
}

#[derive(Debug, Arbitrary)]
struct Aa2dInput {
    ops: Vec<PoolOp>,
}

fuzz_target!(|input: Aa2dInput| {
    if input.ops.is_empty() || input.ops.len() > 200 {
        return;
    }

    let senders: Vec<Address> = (0..NUM_SENDERS)
        .map(|i| Address::with_last_byte(i + 1))
        .collect();

    let mut pool = AA2dPool::default();
    let mut tracked_hashes: Vec<TxHash> = Vec::new();

    for op in &input.ops {
        match op {
            PoolOp::AddTx {
                sender_idx,
                nonce_key,
                nonce,
                priority_fee,
            } => {
                let sender = senders[(*sender_idx % NUM_SENDERS) as usize];
                let nk = U256::from(*nonce_key % NUM_NONCE_KEYS);
                let n = *nonce as u64;
                let fee = (*priority_fee as u128).saturating_add(1);

                let tx = TxBuilder::aa(sender)
                    .nonce_key(nk)
                    .nonce(n)
                    .max_priority_fee(fee)
                    .build();
                let hash = *tx.hash();
                let valid = wrap_valid_tx(tx, TransactionOrigin::External);

                if pool
                    .add_transaction(Arc::new(valid), 0, TempoHardfork::T1)
                    .is_ok()
                {
                    tracked_hashes.push(hash);
                }
            }
            PoolOp::RemoveByHash { tx_slot } => {
                if !tracked_hashes.is_empty() {
                    let idx = (*tx_slot as usize) % tracked_hashes.len();
                    let hash = tracked_hashes[idx];
                    pool.remove_transactions([&hash].into_iter());
                    tracked_hashes.swap_remove(idx);
                }
            }
            PoolOp::IterateBest { steps } => {
                let mut best = pool.best_transactions();
                let steps = (*steps).min(100);
                let mut prev_priority: Option<u128> = None;
                for _ in 0..steps {
                    match best.next_tx_and_priority() {
                        Some((_tx, priority)) => {
                            if let reth_transaction_pool::Priority::Value(p) = priority {
                                if let Some(prev) = prev_priority {
                                    assert!(
                                        prev >= p,
                                        "Best iterator order violation: {} < {}",
                                        prev,
                                        p
                                    );
                                }
                                prev_priority = Some(p);
                            }
                        }
                        None => break,
                    }
                }
            }
        }

        // Check pool doesn't panic on size query
        let (pending, queued) = pool.pending_and_queued_txn_count();
        assert!(
            pending + queued <= tracked_hashes.len() + input.ops.len(),
            "Pool size {} exceeds reasonable bound",
            pending + queued
        );
    }

    // Final invariant check
    pool.assert_invariants();
});
