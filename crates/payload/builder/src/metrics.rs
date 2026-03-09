use metrics::Gauge;
use reth_metrics::{Metrics, metrics::Histogram};

/// State-size statistics from a finalized payload, used to correlate with
/// `payload_finalization_duration_seconds` when diagnosing slow state root computation.
#[derive(Debug, Clone, Copy)]
pub(crate) struct FinalizationStateStats {
    /// Number of accounts in the hashed post-state (created, modified, or destroyed).
    pub accounts_modified: usize,
    /// Total number of explicit storage slot changes across all accounts.
    pub storage_slots_modified: usize,
    /// Number of storage tries fully wiped (e.g. via SELFDESTRUCT).
    pub storage_tries_wiped: usize,
    /// Total intermediate trie nodes updated or removed (account + storage tries).
    pub trie_nodes_updated: usize,
}

#[derive(Metrics, Clone)]
#[metrics(scope = "tempo_payload_builder")]
pub(crate) struct TempoPayloadBuilderMetrics {
    /// Block time in milliseconds.
    pub(crate) block_time_millis: Histogram,
    /// Block time in milliseconds.
    pub(crate) block_time_millis_last: Gauge,
    /// Number of transactions in the payload.
    pub(crate) total_transactions: Histogram,
    /// Number of transactions in the payload.
    pub(crate) total_transactions_last: Gauge,
    /// Number of payment transactions in the payload.
    pub(crate) payment_transactions: Histogram,
    /// Number of payment transactions in the payload.
    pub(crate) payment_transactions_last: Gauge,
    /// Number of subblocks in the payload.
    pub(crate) subblocks: Histogram,
    /// Number of subblocks in the payload.
    pub(crate) subblocks_last: Gauge,
    /// Number of subblock transactions in the payload.
    pub(crate) subblock_transactions: Histogram,
    /// Number of subblock transactions in the payload.
    pub(crate) subblock_transactions_last: Gauge,
    /// Amount of gas used in the payload.
    pub(crate) gas_used: Histogram,
    /// Amount of gas used in the payload.
    pub(crate) gas_used_last: Gauge,
    /// The time it took to prepare system transactions in seconds.
    pub(crate) prepare_system_transactions_duration_seconds: Histogram,
    /// The time it took to execute one transaction in seconds.
    pub(crate) transaction_execution_duration_seconds: Histogram,
    /// The time it took to execute normal transactions in seconds.
    pub(crate) total_normal_transaction_execution_duration_seconds: Histogram,
    /// The time it took to execute subblock transactions in seconds.
    pub(crate) total_subblock_transaction_execution_duration_seconds: Histogram,
    /// The time it took to execute all transactions in seconds.
    pub(crate) total_transaction_execution_duration_seconds: Histogram,
    /// The time it took to execute system transactions in seconds.
    pub(crate) system_transactions_execution_duration_seconds: Histogram,
    /// The time it took to finalize the payload in seconds. Includes merging transitions and calculating the state root.
    pub(crate) payload_finalization_duration_seconds: Histogram,
    /// Number of accounts modified in the payload (from hashed post-state).
    pub(crate) accounts_modified: Histogram,
    /// Number of accounts modified in the latest payload.
    pub(crate) accounts_modified_last: Gauge,
    /// Number of storage slots modified in the payload (from hashed post-state).
    pub(crate) storage_slots_modified: Histogram,
    /// Number of storage slots modified in the latest payload.
    pub(crate) storage_slots_modified_last: Gauge,
    /// Number of storage tries fully wiped (e.g. via SELFDESTRUCT).
    pub(crate) storage_tries_wiped: Histogram,
    /// Number of storage tries wiped in the latest payload.
    pub(crate) storage_tries_wiped_last: Gauge,
    /// Number of intermediate trie nodes updated or removed during state root calculation.
    pub(crate) trie_nodes_updated: Histogram,
    /// Number of trie nodes updated in the latest payload.
    pub(crate) trie_nodes_updated_last: Gauge,
    /// Total time it took to build the payload in seconds.
    pub(crate) payload_build_duration_seconds: Histogram,
    /// Gas per second calculated as gas_used / payload_build_duration.
    pub(crate) gas_per_second: Histogram,
    /// Gas per second for the last payload calculated as gas_used / payload_build_duration.
    pub(crate) gas_per_second_last: Gauge,
    /// RLP-encoded block size in bytes.
    pub(crate) rlp_block_size_bytes: Histogram,
    /// RLP-encoded block size in bytes for the last payload.
    pub(crate) rlp_block_size_bytes_last: Gauge,
}
