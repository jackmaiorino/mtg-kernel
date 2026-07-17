#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

UV_BIN="${UV_BIN:-uv}"
expected_uv="uv 0.11.29"
actual_uv="$("$UV_BIN" --version)"
case "$actual_uv" in
    "$expected_uv"|"$expected_uv "*) ;;
    *)
        echo "expected $expected_uv, got $actual_uv from $UV_BIN" >&2
        exit 1
        ;;
esac

echo "==> Locked Python environment"
"$UV_BIN" lock --check
"$UV_BIN" sync --locked --extra test

expected_python="$(tr -d '\r\n' < .python-version)"
actual_python="$("$UV_BIN" run --no-sync python -c 'import platform; print(platform.python_version())')"
if [[ "$actual_python" != "$expected_python" ]]; then
    echo "expected Python $expected_python, got $actual_python from uv" >&2
    exit 1
fi

case "$(uname -s 2>/dev/null || true)" in
    CYGWIN*|MINGW*|MSYS*) kernel_suffix=".exe" ;;
    *) kernel_suffix="" ;;
esac

default_kernel_bin="$ROOT/target/release/kernel_rl_env${kernel_suffix}"
export MTG_KERNEL_RL_ENV_BIN="${MTG_KERNEL_RL_ENV_BIN:-$default_kernel_bin}"

echo "==> Rust formatting"
cargo fmt --all -- --check

echo "==> Rust lint"
cargo clippy --release --locked --workspace --all-targets -- -D warnings

echo "==> Rust release tests"
cargo test --release --locked --workspace --all-targets

echo "==> Canonical Pauper manifest check"
"$UV_BIN" run --no-sync python python/tools/generate_pauper_manifests.py --check --repo-root "$ROOT"

echo "==> Release JSONL environment"
cargo build --release --locked --bin kernel_rl_env
test -f "$MTG_KERNEL_RL_ENV_BIN"

echo "==> Python tests with the real Rust environment"
"$UV_BIN" run --no-sync python -m unittest discover -s python/tests -v
