use alloy_primitives::{Address, B256};
use commonware_codec::DecodeExt as _;
use commonware_cryptography::ed25519::PublicKey;
use commonware_utils::ordered;
use eyre::{OptionExt as _, WrapErr as _};
use futures::StreamExt as _;
use prometheus_client::metrics::counter::Counter;
use reth_ethereum::{
    evm::revm::{State, database::StateProviderDatabase},
    network::NetworkInfo,
    rpc::eth::primitives::BlockNumHash,
};
use reth_node_builder::ConfigureEvm as _;
use reth_provider::{
    BlockNumReader as _, CanonStateSubscriptions as _, HeaderProvider as _,
    StateProviderFactory as _,
};
use std::{collections::HashMap, net::SocketAddr, time::Duration};
use tempo_node::TempoFullNode;
use tempo_precompiles::{
    storage::StorageCtx,
    validator_config::{IValidatorConfig, ValidatorConfig},
};

use tracing::{Level, debug, info, instrument, warn};

/// Attempts to read the validator config from the smart contract, retrying
/// until the required block height is available.
pub(crate) async fn read_validator_config_with_retry(
    context: &impl commonware_runtime::Clock,
    node: &TempoFullNode,
    target: BlockNumHash,
    total_attempts: &Counter,
) -> ordered::Map<PublicKey, DecodedValidator> {
    let mut attempts = 0;
    const MIN_RETRY: Duration = Duration::from_secs(1);
    const MAX_RETRY: Duration = Duration::from_secs(30);

    let mut canon_events = node.provider.canonical_state_stream();

    'read_contract: loop {
        total_attempts.inc();
        attempts += 1;

        if let Ok(validators) = read_from_contract_at_hash(attempts, node, target.hash) {
            break 'read_contract validators;
        }

        let retry_after = MIN_RETRY.saturating_mul(attempts).min(MAX_RETRY);
        let is_syncing = node.network.is_syncing();
        let best_block = node.provider.best_block_number();
        let blocks_behind = best_block
            .as_ref()
            .ok()
            .map(|best| target.number.saturating_sub(*best));
        tracing::warn_span!("read_validator_config_with_retry").in_scope(|| {
            warn!(
                attempts,
                retry_after = %tempo_telemetry_util::display_duration(retry_after),
                is_syncing,
                best_block = %tempo_telemetry_util::display_result(&best_block),
                target.number,
                %target.hash,
                blocks_behind = %tempo_telemetry_util::display_option(&blocks_behind),
                "reading validator config from contract failed; will retry",
            );
        });
        tokio::select! {
            _ = canon_events.next() => {
                tracing::info_span!("read_validator_config_with_retry").in_scope(|| {
                    debug!("woke from canonical state notification");
                });
            }
            _ = context.sleep(retry_after) => {
                tracing::info_span!("read_validator_config_with_retry").in_scope(|| {
                    debug!("woke from retry timeout");
                });
            }
        }
    }
}
/// Reads state from the ValidatorConfig precompile at a given block height.
pub(crate) fn read_validator_config_at_hash<T>(
    node: &TempoFullNode,
    hash: B256,
    read_fn: impl FnOnce(&ValidatorConfig) -> eyre::Result<T>,
) -> eyre::Result<T> {
    let header = node
        .provider
        .header(hash)
        .map_err(Into::<eyre::Report>::into)
        .and_then(|maybe| maybe.ok_or_eyre("execution layer returned empty header"))
        .wrap_err_with(|| {
            format!("failed getting header for hash `{hash}` from execution layer")
        })?;

    let db = State::builder()
        .with_database(StateProviderDatabase::new(
            node.provider.state_by_block_hash(hash).wrap_err_with(|| {
                format!("failed to construct EVM state for execution layer at hash `{hash}`")
            })?,
        ))
        .build();

    let mut evm = node
        .evm_config
        .evm_for_block(db, &header)
        .wrap_err("failed instantiating EVM for block")?;

    let ctx = evm.ctx_mut();
    StorageCtx::enter_evm(
        &mut ctx.journaled_state,
        &ctx.block,
        &ctx.cfg,
        &ctx.tx,
        || read_fn(&ValidatorConfig::new()),
    )
}

/// Reads the validator config from the boundary block of `epoch`.
///
/// If `epoch` is not set, reads the genesis block.
///
/// Note that this returns all validators, active and inactive.
#[instrument(
    skip_all,
    fields(
        attempt = _attempt,
        %hash,
    ),
    err
)]
pub(crate) fn read_from_contract_at_hash(
    _attempt: u32,
    node: &TempoFullNode,
    hash: B256,
) -> eyre::Result<ordered::Map<PublicKey, DecodedValidator>> {
    let raw_validators = read_validator_config_at_hash(node, hash, |config| {
        config
            .get_validators()
            .wrap_err("failed to query contract for validator config")
    })?;

    info!(?raw_validators, "read validators from contract",);

    Ok(decode_from_contract(raw_validators))
}

#[instrument(skip_all, fields(validators_to_decode = contract_vals.len()))]
fn decode_from_contract(
    contract_vals: Vec<IValidatorConfig::Validator>,
) -> ordered::Map<PublicKey, DecodedValidator> {
    let mut decoded = HashMap::new();
    for val in contract_vals.into_iter() {
        // NOTE: not reporting errors because `decode_from_contract` emits
        // events on success and error
        if let Ok(val) = DecodedValidator::decode_from_contract(val)
            && let Some(old) = decoded.insert(val.public_key.clone(), val)
        {
            warn!(
                %old,
                new = %decoded.get(&old.public_key).expect("just inserted it"),
                "replaced peer because public keys were duplicated",
            );
        }
    }
    ordered::Map::from_iter_dedup(decoded)
}

/// A ContractValidator is a peer read from the validator config smart const.
///
/// The inbound and outbound addresses stored herein are guaranteed to be of the
/// form `<host>:<port>` for inbound, and `<ip>:<port>` for outbound. Here,
/// `<host>` is either an IPv4 or IPV6 address, or a fully qualified domain name.
/// `<ip>` is an IPv4 or IPv6 address.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DecodedValidator {
    pub(crate) active: bool,
    /// The `publicKey` field of the contract. Used by other validators to
    /// identify a peer by verifying the signatures of its p2p messages and
    /// as a dealer/player/participant in DKG ceremonies and consensus for a
    /// given epoch. Part of the set registered with the lookup p2p manager.
    pub(crate) public_key: PublicKey,
    /// The `inboundAddress` field of the contract. Used by other validators
    /// to dial a peer and ensure that messages from that peer are coming from
    /// this address. Part of the set registered with the lookup p2p manager.
    pub(crate) inbound: SocketAddr,
    /// The `outboundAddress` field of the contract. Currently ignored because
    /// all p2p communication is symmetric (outbound and inbound) via the
    /// `inboundAddress` field.
    pub(crate) outbound: SocketAddr,
    /// The `index` field of the contract. Not used by consensus and just here
    /// for debugging purposes to identify the contract entry. Emitted in
    /// tracing events.
    pub(crate) index: u64,
    /// The `address` field of the contract. Not used by consensus and just here
    /// for debugging purposes to identify the contract entry. Emitted in
    /// tracing events.
    pub(crate) address: Address,
}

impl DecodedValidator {
    /// Attempts to decode a single validator from the values read in the smart contract.
    ///
    /// This function does not perform hostname lookup on either of the addresses.
    /// Instead, only the shape of the addresses are checked for whether they are
    /// socket addresses (IP:PORT pairs), or fully qualified domain names.
    #[instrument(ret(Display, level = Level::INFO), err(level = Level::WARN))]
    fn decode_from_contract(
        IValidatorConfig::Validator {
            active,
            publicKey,
            index,
            validatorAddress,
            inboundAddress,
            outboundAddress,
        }: IValidatorConfig::Validator,
    ) -> eyre::Result<Self> {
        let public_key = PublicKey::decode(publicKey.as_ref())
            .wrap_err("failed decoding publicKey field as ed25519 public key")?;
        let inbound = inboundAddress
            .parse()
            .wrap_err("inboundAddress was not valid")?;
        let outbound = outboundAddress
            .parse()
            .wrap_err("outboundAddress was not valid")?;
        Ok(Self {
            active,
            public_key,
            inbound,
            outbound,
            index,
            address: validatorAddress,
        })
    }
}

impl std::fmt::Display for DecodedValidator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "public key = `{}`, inbound = `{}`, outbound = `{}`, index = `{}`, address = `{}`",
            self.public_key, self.inbound, self.outbound, self.index, self.address
        ))
    }
}
