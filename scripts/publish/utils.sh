#!/usr/bin/env bash

UTILS_PY="$( cd "$(dirname "${BASH_SOURCE[0]}")" && pwd )/utils.py"

log() { printf '  \033[1;34m→\033[0m %s\n' "$*"; }
err() { printf '  \033[1;31m✗\033[0m %s\n' "$*" >&2; exit 1; }

parse_publish_mode() {
    DRY_RUN=true
    SEMVER_CHECK=false

    case "${1:-}" in
        "")             ;;
        --publish)      DRY_RUN=false ;;
        --semver-check) SEMVER_CHECK=true ;;
        *)              echo "Usage: $0 [--publish|--semver-check]" >&2; exit 1 ;;
    esac
}

copy_crates_to_tmp() {
    local tmp_work_dir="$1"
    shift

    log "Copying crates to temporary directory …"
    local crate
    for crate in "$@"; do
        cp -R "$REPO_ROOT/crates/$crate" "$tmp_work_dir/$crate"
    done
}

workspace_version() {
    local sanitize_py="$1"
    local workspace_toml="$2"
    python3 "$sanitize_py" get_version "$workspace_toml"
}

sanitize_base_manifests() {
    local sanitize_py="$1"
    local ws_version="$2"
    local workspace_toml="$3"
    shift 3

    local crate_toml
    for crate_toml in "$@"; do
        python3 "$sanitize_py" sanitize_base "$crate_toml" "$ws_version" "$workspace_toml"
    done
}

run_workspace_checks() {
    local manifest_path="$1"
    local check_err="$2"
    local all_features_err="$3"
    local success_message="$4"

    log "Running cargo check …"
    if ! cargo check --manifest-path "$manifest_path" 2>&1; then
        err "$check_err"
    fi

    log "Running cargo check --all-features …"
    if ! cargo check --manifest-path "$manifest_path" --all-features 2>&1; then
        err "$all_features_err"
    fi

    log "$success_message"
}

# setup_tmp_workspace <crate_dir...>
#   Creates a temp directory, copies crates, and sets:
#   TMP_WORK_DIR, TMP_CARGO_TOML, CRATE_MANIFESTS, CRATE_PATHS, MEMBERS_CSV, PATCHES_CSV
setup_tmp_workspace() {
    TMP_WORK_DIR=$(mktemp -d)
    TMP_CARGO_TOML="$TMP_WORK_DIR/Cargo.toml"
    trap 'rm -rf "$TMP_WORK_DIR"' EXIT

    copy_crates_to_tmp "$TMP_WORK_DIR" "$@"

    CRATE_MANIFESTS=()
    CRATE_PATHS=()
    MEMBERS_CSV=""
    PATCHES_CSV=""
    local d crate_name
    for d in "$@"; do
        CRATE_MANIFESTS+=("$TMP_WORK_DIR/$d/Cargo.toml")
        CRATE_PATHS+=("$TMP_WORK_DIR/$d")
        crate_name=$(crate_name_from_dir "$TMP_WORK_DIR/$d")
        MEMBERS_CSV="${MEMBERS_CSV:+$MEMBERS_CSV,}$d"
        PATCHES_CSV="${PATCHES_CSV:+$PATCHES_CSV,}$crate_name=$d"
    done
}

resolve_workspace_dependencies() {
    local sanitize_py="$1"
    local workspace_toml="$2"
    shift 2

    local crate_toml
    for crate_toml in "$@"; do
        python3 "$sanitize_py" resolve_deps "$crate_toml" "$workspace_toml"
    done
}

crate_name_from_dir() {
    local crate_dir="$1"
    grep -m1 'name = ' "$crate_dir/Cargo.toml" | sed 's/.*"\(.*\)".*/\1/'
}

crate_version_from_dir() {
    local crate_dir="$1"
    grep -m1 'version = ' "$crate_dir/Cargo.toml" | sed 's/.*"\(.*\)".*/\1/'
}

latest_published_version() {
    local crate_name="$1"
    curl -sL "https://crates.io/api/v1/crates/$crate_name" -H "User-Agent: tempo-publish-script" | \
        python3 -c "import sys,json; d=json.load(sys.stdin); print(d['crate']['max_stable_version'] or d['crate']['max_version'])" 2>/dev/null
}

noop_semver_prep() {
    :
}

run_semver_checks() {
    local workspace_manifest="$1"
    local semver_prep_hook="$2"
    local publish_crates_csv="$3"
    shift 3

    local publish_crates=()
    local crate_dir
    local crate_name
    local crate_ver
    local release_type
    local internal_deps=()
    local dep
    local published_ver
    local semver_failed=false
    local semver_skipped_all=true

    IFS=, read -r -a publish_crates <<< "$publish_crates_csv"

    log "Running cargo-semver-checks …"
    for crate_dir in "$@"; do
        "$semver_prep_hook" "$crate_dir"
        crate_name=$(crate_name_from_dir "$crate_dir")
        crate_ver=$(crate_version_from_dir "$crate_dir")
        log "Checking $crate_name@$crate_ver …"

        release_type=$(python3 "$UTILS_PY" release_type "$crate_name" "$REPO_ROOT")
        if [ -z "$release_type" ]; then
            log "$crate_name has no pending changelog release type, skipping semver-check"
            continue
        fi

        internal_deps=()
        for dep in "${publish_crates[@]}"; do
            [ "$dep" = "$crate_name" ] && continue
            if grep -qE "^\s*${dep}\s*=" "$crate_dir/Cargo.toml"; then
                internal_deps+=("$dep")
            fi
        done
        if ((${#internal_deps[@]} > 0)); then
            log "$crate_name depends on releasable internal crates (${internal_deps[*]}), skipping semver-check"
            continue
        fi

        published_ver=$(latest_published_version "$crate_name")
        if [ -z "$published_ver" ] || [ "$published_ver" = "null" ]; then
            log "$crate_name not yet published, skipping"
            continue
        fi

        if [ "$crate_ver" != "$published_ver" ]; then
            log "$crate_name version bumped ($published_ver → $crate_ver), skipping"
            continue
        fi

        semver_skipped_all=false
        if ! cargo semver-checks \
            --manifest-path "$workspace_manifest" \
            --package "$crate_name" \
            --release-type "$release_type" \
            --default-features 2>&1; then
            semver_failed=true
        fi
    done

    if $semver_skipped_all; then
        log "All crates have bumped versions, nothing to semver-check"
    elif $semver_failed; then
        printf '\n  \033[1;33m⚠\033[0m Semver-incompatible changes detected.\n'
        printf '    If intentional, add a changelog entry with the appropriate bump level.\n\n'
        return 1
    else
        log "Semver checks passed ✓"
    fi
}

retry_publish() {
    local crate_dir="$1"
    local name
    name=$(crate_name_from_dir "$crate_dir")
    local max_attempts=10
    local delay=15

    for ((i = 1; i <= max_attempts; i++)); do
        log "Publishing $name (attempt $i/$max_attempts) …"
        local output
        if output=$(cargo publish --manifest-path "$crate_dir/Cargo.toml" --allow-dirty 2>&1); then
            log "$name published ✓"
            return 0
        fi
        echo "$output"
        if echo "$output" | grep -qE 'already uploaded|already exists'; then
            log "$name already published, skipping ✓"
            return 0
        fi
        if ((i < max_attempts)); then
            log "Publish failed, retrying in ${delay}s …"
            sleep "$delay"
        fi
    done
    err "Failed to publish $name after $max_attempts attempts"
}

publish_crates() {
    local success_message="$1"
    shift

    if $DRY_RUN; then
        log "Dry-run complete. Use --publish to actually publish."
        return 0
    fi

    local crate_dir
    for crate_dir in "$@"; do
        retry_publish "$crate_dir"
    done
    log "$success_message"
}
