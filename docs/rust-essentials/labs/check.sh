#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
build_dir="$(mktemp -d "${TMPDIR:-/tmp}/grok-rust-katas.XXXXXX")"
trap 'rm -rf -- "$build_dir"' EXIT

rustc --edition 2024 --test "$script_dir/katas.rs" -o "$build_dir/katas"
"$build_dir/katas"

check_compile_fail() {
    local source="$1"
    local error_code="$2"
    local log="$build_dir/$(basename "$source").log"

    if rustc --edition 2024 "$source" -o "$build_dir/should-not-build" >"$log" 2>&1; then
        echo "expected compilation to fail: $source" >&2
        return 1
    fi

    if ! grep -q "error\[$error_code\]" "$log"; then
        echo "expected $error_code from $source" >&2
        sed -n '1,120p' "$log" >&2
        return 1
    fi
    echo "ok: $(basename "$source") reports $error_code"
}

check_compile_fail "$script_dir/compile_fail/moved_value.rs" E0382
check_compile_fail "$script_dir/compile_fail/borrow_conflict.rs" E0502
check_compile_fail "$script_dir/compile_fail/multiple_mut_borrow.rs" E0499
check_compile_fail "$script_dir/compile_fail/return_local_reference.rs" E0515
check_compile_fail "$script_dir/compile_fail/temporary_dropped.rs" E0716
check_compile_fail "$script_dir/compile_fail/rc_is_not_send.rs" E0277

async_demos=(
    select_race
    select_loop
    select_cancel_drop
    watch_latest
    actor_request_reply
    spawn_local_rc
    timer_reset
    mutex_snapshot
)

for demo in "${async_demos[@]}"; do
    echo "running async demo: $demo"
    CARGO_TARGET_DIR="$build_dir/async-target" \
        cargo run --quiet --locked \
        --manifest-path "$script_dir/async-demos/Cargo.toml" \
        --bin "$demo"
done
