#!/usr/bin/env bash
# PCC Dial A FAIL_LOUD harness — exit 2 if packed >= zstd-9 or DECODE_OK false.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MOZ="${1:-${MOZILLA:-/workspace/corpora/silesia-members/mozilla}}"
ZSTD9=16735963
export LBR1_PARSE="${LBR1_PARSE:-hc4}"
export LBR1_WINDOW="${LBR1_WINDOW:-1048576}"
export LBR1_CHAIN="${LBR1_CHAIN:-8}"
export LBR1_LAZY="${LBR1_LAZY:-0}"
export LBR1_PACK="${LBR1_PACK:-ml4}"
cd "$ROOT"
if [[ ! -x target/release/examples/profile_ml4_hot ]]; then
  cargo build --release -p splb --example profile_ml4_hot
fi
set +e
OUT=$(./target/release/examples/profile_ml4_hot "$MOZ")
RC=$?
set -e
echo "$OUT"
if [[ $RC -ne 0 ]] || echo "$OUT" | grep -q 'FAIL_LOUD=true'; then
  echo "FAIL_LOUD gate tripped" >&2
  exit 2
fi
PACK=$(echo "$OUT" | sed -n 's/.*pack_bytes=\([0-9]*\).*/\1/p')
if [[ -n "$PACK" && "$PACK" -ge "$ZSTD9" ]]; then
  echo "FAIL_LOUD: pack_bytes=$PACK >= zstd-9=$ZSTD9" >&2
  exit 2
fi
echo "Dial A PASS vs zstd-9 (pack_bytes=$PACK < $ZSTD9)"
