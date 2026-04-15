#!/usr/bin/env bash
#
# Publish tempo-contracts, tempo-primitives, and tempo-alloy to crates.io
# by stripping all reth-specific code and dependencies.
#
# Usage:
#   ./scripts/publish/alloy.sh              # dry-run (default)
#   ./scripts/publish/alloy.sh --publish    # actually publish
#   ./scripts/publish/alloy.sh --semver-check
#
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
source "$REPO_ROOT/scripts/publish/common.sh"
parse_publish_mode "${1:-}"

SANITIZE_PY="$REPO_ROOT/scripts/sanitize_toml.py"
SANITIZE_RS="$REPO_ROOT/scripts/sanitize_source.py"

append_contracts_semver_overrides() {
    local cargo_toml="$1"
    cat >> "$cargo_toml" <<'EOF'

[package.metadata.cargo-semver-checks.lints]
# `alloy-sol-types::sol!` can reshuffle generated Rust surface area when the ABI
# evolves, even when the Solidity-facing SDK contract bindings remain compatible.
constructible_struct_adds_field = "warn"
enum_variant_added = "warn"
enum_variant_missing = "warn"
inherent_method_missing = "warn"
struct_missing = "warn"
struct_pub_field_missing = "warn"
EOF
}

prepare_alloy_semver() {
    local crate_dir="$1"
    if [ "$(basename "$crate_dir")" = "contracts" ]; then
        append_contracts_semver_overrides "$crate_dir/Cargo.toml"
    fi
}

# ── Create temp workspace ──────────────────────────────────────────────────────
TMP_WORK_DIR=$(mktemp -d)
trap 'rm -rf "$TMP_WORK_DIR"' EXIT
CRATE_MANIFESTS=(
    "$TMP_WORK_DIR/contracts/Cargo.toml"
    "$TMP_WORK_DIR/primitives/Cargo.toml"
    "$TMP_WORK_DIR/alloy/Cargo.toml"
)
CRATE_DIRS=(
    "$TMP_WORK_DIR/contracts"
    "$TMP_WORK_DIR/primitives"
    "$TMP_WORK_DIR/alloy"
)

copy_crates_to_tmp "$TMP_WORK_DIR" contracts primitives alloy

# ── 1. Prepare sanitized crates ────────────────────────────────────────────────
log "Deleting reth_compat modules …"
rm -rf "$TMP_WORK_DIR/primitives/src/reth_compat"
rm -f  "$TMP_WORK_DIR/alloy/src/rpc/reth_compat.rs"

log "Stripping reth references from source …"
python3 "$SANITIZE_RS" "$TMP_WORK_DIR/primitives" "$TMP_WORK_DIR/alloy"

log "Sanitizing Cargo.toml files …"

WS_VERSION=$(workspace_version "$SANITIZE_PY" "$REPO_ROOT/Cargo.toml")
log "Workspace version: $WS_VERSION"

sanitize_base_manifests "$SANITIZE_PY" "$WS_VERSION" "$REPO_ROOT/Cargo.toml" "${CRATE_MANIFESTS[@]}"

python3 "$SANITIZE_PY" sanitize_primitives "$TMP_WORK_DIR/primitives/Cargo.toml"
python3 "$SANITIZE_PY" sanitize_alloy "$TMP_WORK_DIR/alloy/Cargo.toml" "$REPO_ROOT/Cargo.toml"

# ── 4. Verify compilation (before resolving workspace deps) ───────────────────
# Use a temp workspace that provides all workspace deps via the real root,
# plus local path overrides for the three internal crates.
log "Verifying compilation …"

write_workspace_manifest "$TMP_WORK_DIR/Cargo.toml" "contracts,primitives,alloy"

# Generate workspace deps, dynamically filtering out reth-* and all internal
# path-only crates, then overriding the 3 publish targets with local paths.
python3 "$SANITIZE_PY" gen_workspace "$REPO_ROOT/Cargo.toml" "$TMP_WORK_DIR/Cargo.toml" \
    "tempo-contracts,tempo-primitives,tempo-alloy"

run_workspace_checks \
    "$TMP_WORK_DIR/Cargo.toml" \
    "Stripped crates failed to compile!" \
    "Stripped crates failed to compile with --all-features!" \
    "Compilation verified ✓"

# ── 2. Pre-resolve validation ─────────────────────────────────────────────────
# Validate BEFORE resolve_deps so that internal deps (which still have
# workspace/path markers) can be detected. After resolve_deps, a leaked
# internal dep like `tempo-foo.workspace = true` becomes
# `tempo-foo = { version = "1.x.0" }` and is much harder to catch.
log "Pre-resolve validation …"

INTERNAL_PATH_DEPS=$(get_internal_path_deps "$SANITIZE_PY" "$REPO_ROOT/Cargo.toml" "tempo-contracts,tempo-primitives,tempo-alloy")
validate_no_reth_or_internal_deps "$INTERNAL_PATH_DEPS" "${CRATE_MANIFESTS[@]}"

# Primitives: no forbidden features
for feat in reth reth-codec serde-bincode-compat rpc; do
    grep -qE "^\s*${feat}\s*=" "$TMP_WORK_DIR/primitives/Cargo.toml" && \
        err "Feature '$feat' still defined in tempo-primitives Cargo.toml"
done

# Alloy: no reth feature
grep -qE "^\s*reth\s*=" "$TMP_WORK_DIR/alloy/Cargo.toml" && \
    err "Feature 'reth' still defined in tempo-alloy Cargo.toml"

# Source: no forbidden references
(
    grep -rq 'feature = "reth"' "$TMP_WORK_DIR/primitives/src/" || \
    grep -rq 'feature = "reth-codec"' "$TMP_WORK_DIR/primitives/src/" || \
    grep -rq 'reth_codecs' "$TMP_WORK_DIR/primitives/src/" || \
    grep -rq 'feature = "rpc"' "$TMP_WORK_DIR/primitives/src/"
) && err "reth-gated code still in tempo-primitives source"

grep -rq 'feature = "reth"' "$TMP_WORK_DIR/alloy/src/" && \
    err "reth-gated code still in tempo-alloy source"

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
    "contracts,primitives,alloy" \
    "tempo-contracts=contracts,tempo-primitives=primitives,tempo-alloy=alloy"

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
        prepare_alloy_semver \
        "tempo-contracts,tempo-primitives,tempo-alloy" \
        "${CRATE_DIRS[@]}"
fi

# ── 7. Publish ─────────────────────────────────────────────────────────────────
# Publish order: contracts → primitives → alloy
publish_crates "All alloy crates published successfully! 🎉" "${CRATE_DIRS[@]}"
