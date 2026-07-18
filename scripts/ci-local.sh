#!/usr/bin/env bash
# Run the exact same build/test/lint/coverage/audit steps CI runs, so "green in CI" is
# never a surprise. CI calls this script rather than reimplementing steps in YAML.
#
# Usage: scripts/ci-local.sh
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

echo "==> cargo fmt --check"
cargo fmt --all --check

echo "==> cargo clippy --workspace --all-targets -- -D warnings"
cargo clippy --workspace --all-targets --all-features -- -D warnings

echo "==> cargo test --workspace"
cargo test --workspace --all-features

if command -v cargo-llvm-cov >/dev/null 2>&1; then
  echo "==> cargo llvm-cov --workspace (coverage ratchet)"
  # main.rs files are pure composition roots (env parsing, wiring dependencies together) with
  # no branching logic of their own — everything they call is unit-tested via lib.rs. Excluding
  # them keeps the ratchet meaningful instead of penalizing every new service's boilerplate.
  cargo llvm-cov --workspace --all-features --ignore-filename-regex '(^|/)main\.rs$' --fail-under-lines 85
else
  echo "==> cargo-llvm-cov not installed, skipping coverage (install: cargo install cargo-llvm-cov)"
fi

if command -v cargo-audit >/dev/null 2>&1; then
  echo "==> cargo audit"
  cargo audit
else
  echo "==> cargo-audit not installed, skipping (install: cargo install cargo-audit)"
fi

if command -v cargo-deny >/dev/null 2>&1; then
  echo "==> cargo deny check"
  cargo deny check
else
  echo "==> cargo-deny not installed, skipping (install: cargo install cargo-deny)"
fi

echo "==> ci-local.sh: all steps completed"
