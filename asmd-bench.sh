#!/bin/bash
# ALARM at 180s. Skip this seat at 15 min. Encode otherwise keeps going.
set -u
LB="${LB:-$(cd "$(dirname "$0")" && pwd)/target/release/lb}"
ALARM_S="${ALARM_S:-180}"
SKIP_S="${SKIP_S:-900}"
if [ $# -lt 1 ]; then
  echo "usage: asmd-bench.sh FILE [--max] [--seat bw22|lbr1|hybrid] [OUT]" >&2
  exit 2
fi
file=$1
shift
out=/dev/null
extra=()
while [ $# -gt 0 ]; do
  case "$1" in
    --max) extra+=(--max); shift ;;
    --seat)
      extra+=(--seat "$2")
      shift 2
      ;;
    *)
      out=$1
      shift
      ;;
  esac
done
if [ ! -f "$file" ]; then
  echo "missing $file" >&2
  exit 2
fi
raw=$(stat -c%s "$file")
name=$(basename "$file")
"$LB" asmd "${extra[@]}" "$file" "$out" &
enc_pid=$!
(
  sleep "$ALARM_S"
  if kill -0 "$enc_pid" 2>/dev/null; then
    echo "ALARM ${ALARM_S}s still encoding $name extra=${extra[*]} pid=$enc_pid"
  fi
) &
alarm_pid=$!
(
  sleep "$SKIP_S"
  if kill -0 "$enc_pid" 2>/dev/null; then
    echo "SKIP_SEAT ${SKIP_S}s $name pid=$enc_pid"
    kill "$enc_pid" 2>/dev/null || true
  fi
) &
skip_pid=$!
wait "$enc_pid"
st=$?
kill "$alarm_pid" "$skip_pid" 2>/dev/null || true
wait "$alarm_pid" 2>/dev/null || true
wait "$skip_pid" 2>/dev/null || true
coded=0
[ -f "$out" ] && [ "$out" != /dev/null ] && coded=$(stat -c%s "$out")
if [ "$st" -eq 0 ]; then
  echo "DONE $name raw=$raw coded=$coded"
else
  echo "FAILED $name raw=$raw coded=$coded exit=$st"
fi
exit "$st"
