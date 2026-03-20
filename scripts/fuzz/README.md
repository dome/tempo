# Tempo Fuzz Testing

Swarm-style fuzzing for txpool and payload builder components.

## Quick Start

```bash
# Build fuzz targets
cd crates/transaction-pool/fuzz && cargo fuzz build

# Run a single target
cargo fuzz run merge_best_ordering -- -max_total_time=300

# Run swarm fuzzing (8 processes, 30 min each)
./scripts/fuzz/run-swarm.sh crates/transaction-pool/fuzz merge_best_ordering 8 1800

# Reproduce a crash
./scripts/fuzz/repro.sh crates/transaction-pool/fuzz merge_best_ordering path/to/crash-file
```

## Targets

### Transaction Pool (`crates/transaction-pool/fuzz/`)
- `merge_best_ordering` - MergeBestTransactions ordering correctness
- `aa2d_state_machine` - AA2dPool state machine fuzzing

### Payload Builder (`crates/payload/builder/fuzz/`)
- *(planned)* `payload_build_scenario`
- *(planned)* `payload_subblock_lifecycle`
- *(planned)* `payload_limits`

## Swarm Testing

Each fuzz process gets a unique `TEMPO_FUZZ_SWARM_SEED` which can be used
to select different configuration profiles (fork preset, tx mix, pool limits).
This follows the [swarm testing](https://users.cs.utah.edu/~regehr/papers/swarm12.pdf)
approach to improve coverage diversity.

## Running on dev-yk

```bash
ssh -o StrictHostKeyChecking=no ubuntu@dev-yk
cd /path/to/tempo
./scripts/fuzz/run-swarm.sh crates/transaction-pool/fuzz aa2d_state_machine 16 3600
```
