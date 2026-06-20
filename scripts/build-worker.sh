#!/usr/bin/env bash
set -euo pipefail

if [ -f "$HOME/.cargo/env" ]; then
  # shellcheck source=/dev/null
  . "$HOME/.cargo/env"
fi

if ! command -v cargo >/dev/null 2>&1; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
  # shellcheck source=/dev/null
  . "$HOME/.cargo/env"
fi

if ! command -v worker-build >/dev/null 2>&1; then
  cargo install worker-build@^0.8
fi

worker-build --release
