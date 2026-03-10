# ValidatorConfigV2 Invariants

The ValidatorConfigV2 precompile replaces V1 with append-only, delete-once semantics. Validators are immutable after creation, tracked by `addedAtHeight` and `deactivatedAtHeight` for historical reconstruction. Ed25519 signature verification proves key ownership at registration. Both owner and validator can call dual-auth functions (rotate, setIpAddresses, transferValidatorOwnership).

## Per-Handler Assertions

- **TEMPO-VALV2-1**: Dual-auth enforcement - functions callable by owner or validator (`deactivateValidator`, `setIpAddresses`, `rotateValidator`, `transferValidatorOwnership`) succeed when called by owner or the validator itself; fail when called by third parties.
- **TEMPO-VALV2-2**: Owner-only enforcement - functions callable only by owner (`addValidator`, `transferOwnership`, `setNetworkIdentityRotationEpoch`, `migrateValidator`, `initializeIfMigrated`) succeed when called by owner; fail when called by non-owners.
- **TEMPO-VALV2-3**: Validator count changes - active and total validator counts change only as follows: `addValidator` (+1 active, +1 total), `rotateValidator` (+0 active, +1 total), `deactivateValidator` (-1 active, +0 total); all other operations leave counts unchanged.
- **TEMPO-VALV2-4**: Height field updates - validator height fields are set only by specific operations and always equal `block.number` when set:
  - `addValidator`: sets new validator's `addedAtHeight = block.number`, `deactivatedAtHeight = 0`
  - `rotateValidator`: sets old validator's `deactivatedAtHeight = block.number`; sets new validator's `addedAtHeight = block.number`, `deactivatedAtHeight = 0`
  - `deactivateValidator`: sets validator's `deactivatedAtHeight = block.number`
  - `migrateValidator`: sets new validator's `addedAtHeight = block.number`, `deactivatedAtHeight = 0` (if V1 active) or `block.number` (if V1 inactive)
- **TEMPO-VALV2-5**: Init gate enforcement - post-init functions (`addValidator`, `rotateValidator`, `setIpAddresses`, `transferValidatorOwnership`, `setNextDkgCeremony`) fail with `NotInitialized` when `isInitialized() == false`; pre-init functions (`migrateValidator`, `initializeIfMigrated`) fail with `AlreadyInitialized` when `isInitialized() == true`.
- **TEMPO-VALV2-6**: Address uniqueness per-handler - `addValidator` rejects addresses already in use by an active validator; `rotateValidator` verifies address mapping points to the new entry after deactivating the old (per-handler supplement to global VALV2-11).
- **TEMPO-VALV2-7**: Public key validation per-handler - `addValidator` and `rotateValidator` reject zero public keys and public keys already registered (per-handler supplement to global VALV2-12).
- **TEMPO-VALV2-25**: Migration preserves v1 values - V2 active status matches V1 (`V1.active == true` ↔ `V2.deactivatedAtHeight == 0`) immediately after `migrateValidator`.

## Global Invariants

These are checked after every fuzz run:

- **TEMPO-VALV2-8**: Append-only - `validatorCount` is monotonically increasing; never decreases across any sequence of operations.
- **TEMPO-VALV2-9**: Delete-once - no validator can have `deactivatedAtHeight` transition from non-zero back to zero or to a different non-zero value; once deactivated, the validator remains deactivated permanently.
- **TEMPO-VALV2-10**: Height tracking - for all validators: `addedAtHeight > 0` (set when added); `deactivatedAtHeight` is either `0` (active) or `>= addedAtHeight` (deactivated at or after addition).
- **TEMPO-VALV2-11**: Address uniqueness among active - at most one active validator (where `deactivatedAtHeight == 0`) has any given address; deactivated addresses may be reused.
- **TEMPO-VALV2-12**: Public key uniqueness - all public keys are globally unique and non-zero across all validators (including deactivated); once registered, a public key cannot be reused.
- **TEMPO-VALV2-13**: Ingress IP uniqueness - no two active validators share the same ingress IP (port excluded from comparison); deactivated validators' ingress IPs may be reused.
- **TEMPO-VALV2-14**: Sequential indices - each validator's `index` field equals its position in the validators array (validator at array position `i` has `index == i`).
- **TEMPO-VALV2-15**: Active validator subset correctness - `getActiveValidators()` returns exactly the set of validators where `deactivatedAtHeight == 0` (no more, no fewer).
- **TEMPO-VALV2-16**: Validator data consistency - all validator data (publicKey, validatorAddress, ingress, egress, index, addedAtHeight, deactivatedAtHeight) in contract matches ghost state for each validator.
- **TEMPO-VALV2-17**: Validator count consistency - `validatorCount()` equals the actual length of the validators array; both are always in sync.
- **TEMPO-VALV2-18**: Address lookup correctness - for every validator, `validatorByAddress(validator.validatorAddress)` returns that exact validator; `addressToIndex` mapping is accurate.
- **TEMPO-VALV2-19**: Public key lookup correctness - for every validator, `validatorByPublicKey(validator.publicKey)` returns that exact validator; `pubkeyToIndex` mapping is accurate.
- **TEMPO-VALV2-20**: Owner consistency - `owner()` always equals the ghost-tracked owner; ownership transfers are correctly reflected.
- **TEMPO-VALV2-21**: Network identity rotation (DKG ceremony) consistency - `getNextNetworkIdentityRotationEpoch()` always equals the ghost-tracked epoch; updates via `setNetworkIdentityRotationEpoch` are correctly stored.
- **TEMPO-VALV2-22**: Initialization one-way - once `isInitialized() == true`, it remains true forever; `isInitialized()` only transitions from false to true, never back.
- **TEMPO-VALV2-23**: Migration completeness - if `isInitialized() == false`, then `validatorCount <= V1.getAllValidators().length`; migration cannot exceed V1 validator count.
- **TEMPO-VALV2-24**: Migration preserves identity - for each validator at index `i < V1.getAllValidators().length`: `V2.validator[i].publicKey == V1.validator[i].publicKey` and `V2.validator[i].validatorAddress == V1.validator[i].validatorAddress`.
