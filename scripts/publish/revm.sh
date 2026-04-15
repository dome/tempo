#!/usr/bin/env bash
#
# Publish tempo-chainspec, tempo-precompiles-macros, tempo-precompiles, and
# tempo-revm to crates.io by stripping node-only dependencies and features.
#
# Usage:
#   ./scripts/publish/revm.sh              # dry-run (default)
#   ./scripts/publish/revm.sh --publish    # actually publish
#   ./scripts/publish/revm.sh --semver-check
#
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
source "$REPO_ROOT/scripts/publish/common.sh"
parse_publish_mode "${1:-}"

SANITIZE_PY="$REPO_ROOT/scripts/sanitize_toml.py"

# ── Create temp workspace ──────────────────────────────────────────────────────
TMP_WORK_DIR=$(mktemp -d)
trap 'rm -rf "$TMP_WORK_DIR"' EXIT
CRATE_MANIFESTS=(
    "$TMP_WORK_DIR/chainspec/Cargo.toml"
    "$TMP_WORK_DIR/precompiles-macros/Cargo.toml"
    "$TMP_WORK_DIR/precompiles/Cargo.toml"
    "$TMP_WORK_DIR/revm/Cargo.toml"
)
CRATE_DIRS=(
    "$TMP_WORK_DIR/chainspec"
    "$TMP_WORK_DIR/precompiles-macros"
    "$TMP_WORK_DIR/precompiles"
    "$TMP_WORK_DIR/revm"
)

copy_crates_to_tmp "$TMP_WORK_DIR" chainspec precompiles-macros precompiles revm

# ── 1. Prepare sanitized crates ────────────────────────────────────────────────
log "Sanitizing Cargo.toml files …"

WS_VERSION=$(workspace_version "$SANITIZE_PY" "$REPO_ROOT/Cargo.toml")
log "Workspace version: $WS_VERSION"

sanitize_base_manifests "$SANITIZE_PY" "$WS_VERSION" "$REPO_ROOT/Cargo.toml" "${CRATE_MANIFESTS[@]}"

python3 "$SANITIZE_PY" sanitize_chainspec "$TMP_WORK_DIR/chainspec/Cargo.toml"
python3 "$SANITIZE_PY" sanitize_precompiles "$TMP_WORK_DIR/precompiles/Cargo.toml"
python3 "$SANITIZE_PY" sanitize_revm "$TMP_WORK_DIR/revm/Cargo.toml"

# ── 2. Pre-resolve validation ─────────────────────────────────────────────────
# Validate BEFORE resolve_deps so that internal deps (which still have
# workspace/path markers) can be detected. After resolve_deps, a leaked
# internal dep like `tempo-foo.workspace = true` becomes
# `tempo-foo = { version = "1.x.0" }` and is much harder to catch.
log "Pre-resolve validation …"

INTERNAL_PATH_DEPS=$(get_internal_path_deps "$SANITIZE_PY" "$REPO_ROOT/Cargo.toml" "tempo-contracts,tempo-primitives,tempo-chainspec,tempo-precompiles-macros,tempo-precompiles,tempo-revm")
validate_no_reth_or_internal_deps "$INTERNAL_PATH_DEPS" "${CRATE_MANIFESTS[@]}"

for feat in reth cli; do
    grep -qE "^\s*${feat}\s*=" "$TMP_WORK_DIR/chainspec/Cargo.toml" && \
        err "Feature '$feat' still defined in tempo-chainspec Cargo.toml"
done

for feat in reth rpc; do
    grep -qE "^\s*${feat}\s*=" "$TMP_WORK_DIR/revm/Cargo.toml" && \
        err "Feature '$feat' still defined in tempo-revm Cargo.toml"
done

grep -qE '^\s*tempo-evm[\s.=]' "$TMP_WORK_DIR/precompiles/Cargo.toml" && \
    err "Internal dev-dependency 'tempo-evm' still in tempo-precompiles Cargo.toml"
grep -qE '^\s*tempo-evm[\s.=]' "$TMP_WORK_DIR/revm/Cargo.toml" && \
    err "Internal dev-dependency 'tempo-evm' still in tempo-revm Cargo.toml"

log "Pre-resolve validation passed ✓"

# ── 3. Resolve workspace deps to concrete versions for publishing ─────────────
log "Resolving workspace dependencies …"

resolve_workspace_dependencies "$SANITIZE_PY" "$REPO_ROOT/Cargo.toml" "${CRATE_MANIFESTS[@]}"

# ── 4. Post-resolve validation ────────────────────────────────────────────────
log "Post-resolve validation …"

validate_resolved_manifests "${CRATE_MANIFESTS[@]}"

log "Post-resolve validation passed ✓"

# ── 5. Final build check on resolved manifests ────────────────────────────────
# resolve_deps can change semantics (features, default-features, optional),
# so verify the resolved manifests still compile.
log "Final build check on resolved manifests …"

write_workspace_manifest \
    "$TMP_WORK_DIR/Cargo.toml" \
    "chainspec,precompiles-macros,precompiles,revm" \
    "tempo-chainspec=chainspec,tempo-precompiles-macros=precompiles-macros,tempo-precompiles=precompiles,tempo-revm=revm"

run_workspace_checks \
    "$TMP_WORK_DIR/Cargo.toml" \
    "Resolved crates failed to compile!" \
    "Resolved crates failed to compile with --all-features!" \
    "Final build check passed ✓"

# ── 6. Semver check (optional) ────────────────────────────────────────────────
# Runs cargo-semver-checks against the last published version on crates.io.
# Uses the sanitized + resolved workspace so the API surface matches what's
# actually published, and derives the intended release type from pending
# changelog entries (including fixed groups) instead of the current manifest.
if $SEMVER_CHECK; then
    run_semver_checks \
        "$TMP_WORK_DIR/Cargo.toml" \
        noop_semver_prep \
        "tempo-chainspec,tempo-precompiles-macros,tempo-precompiles,tempo-revm" \
        "${CRATE_DIRS[@]}"
fi

# ── 7. Publish ─────────────────────────────────────────────────────────────────
# Publish in dependency order so inter-crate deps resolve from crates.io.
publish_crates "All revm stack crates published successfully! 🎉" "${CRATE_DIRS[@]}"
