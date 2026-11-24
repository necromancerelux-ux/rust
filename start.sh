#!/usr/bin/env bash
# OmniCore process supervisor for VPS / HidenCloud shells.
# Usage:
#   chmod +x start.sh
#   cp .env.example .env   # fill DISCORD_BOT_TOKEN + OWNER_ID
#   ./start.sh
set -euo pipefail
cd "$(dirname "$0")"

BIN="${OMNICORE_BIN:-./omnicore}"
if [[ ! -x "$BIN" ]]; then
  BIN="./target/release/omnicore"
fi
if [[ ! -x "$BIN" ]]; then
  echo "No binary found. Build first:"
  echo "  cargo build --release --no-default-features"
  echo "Or download the GitHub Actions artifact (omnicore-linux-x86_64)."
  exit 1
fi

if [[ ! -f .env ]]; then
  echo "Missing .env — copy .env.example and set DISCORD_BOT_TOKEN + OWNER_ID"
  exit 1
fi

# shellcheck disable=SC1091
set -a
source .env
set +a

export DATABASE_PATH="${DATABASE_PATH:-./data/bot.sqlite}"
mkdir -p "$(dirname "$DATABASE_PATH")"
export RUST_LOG="${RUST_LOG:-info}"

# Exit code 42 = owner requested restart
while true; do
  echo "[start] launching $BIN at $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  set +e
  "$BIN"
  code=$?
  set -e
  if [[ $code -eq 42 ]]; then
    echo "[start] restart requested (exit 42)"
    sleep 2
    continue
  fi
  echo "[start] exited with $code — retry in 5s"
  sleep 5
done
