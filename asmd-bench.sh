#!/bin/bash
# Alarm at 180s. Do not kill. Encode keeps running.
set -u
LB="${LB:-$(dirname "$0")/target/release/lb}"
ALARM_S="${ALARM_S:-180}"
file=$1
out=${2:-/dev/null}
if [ -z "${file:-}" ] || [ ! -f "$file" ]; then
  echo "usage: asmd-bench.sh FILE [OUT]" >&2
  exit 2
fi
raw=$(stat -c%s "$file")
name=$(basename "$file")
"$LB" asmd "$file" "$out" &
enc_pid=$!
(
  sleep "$ALARM_S"
  if kill -0 "$enc_pid" 2>/dev/null; then
    echo "ALARM ${ALARM_S}s still encoding $name raw=$raw pid=$enc_pid"
  fi
) &
alarm_pid=$!
wait "$enc_pid"
st=$?
kill "$alarm_pid" 2>/dev/null || true
wait "$alarm_pid" 2>/dev/null || true
if [ "$st" -eq 0 ]; then
  echo "DONE $name raw=$raw"
else
  echo "FAILED $name raw=$raw exit=$st"
fi
exit "$st"
