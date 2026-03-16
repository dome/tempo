use alloy_primitives::B256;
use eyre::WrapErr as _;
use tempo_node::TempoFullNode;
use tracing::{Level, instrument};

/// Reads the `nextFullDkgCeremony` epoch value from the ValidatorConfig precompile.
///
/// This is used to determine if the next DKG ceremony should be a full ceremony
/// (new polynomial) instead of a reshare.
#[instrument(
    skip_all,
    fields(
        %at_hash,
    ),
    err,
    ret(level = Level::INFO)
)]
pub(super) fn read_next_full_dkg_ceremony(
    node: &TempoFullNode,
    at_hash: B256,
) -> eyre::Result<u64> {
    crate::validators::read_validator_config_at_hash(node, at_hash, |config| {
        config
            .get_next_full_dkg_ceremony()
            .wrap_err("failed to query contract for next full dkg ceremony")
    })
}
