#!/usr/bin/env bash

set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
label="${1:?usage: profile.sh <label> [binary] [workers] [seconds] [connections]}"
binary="${2:-$here/../server/target/profiling/aegisx}"
workers="${3:-4}"
seconds="${4:-10}"
connections="${5:-128}"
perf="${PERF:-/usr/lib/linux-tools/6.8.0-142-generic/perf}"
out="$here/results/profile-$label"
upstream=3900
listen=3901

"$here/backend/backend" "$upstream" 2 > /dev/null 2>&1 &
backend=$!
"$binary" --listen "127.0.0.1:$listen" --upstream "127.0.0.1:$upstream" --workers "$workers" --log warn > /dev/null 2>&1 &
proxy=$!
trap 'kill -9 $proxy $backend 2>/dev/null || true' EXIT

sleep 0.5
wrk -t2 -c"$connections" -d3s "http://127.0.0.1:$listen/" > /dev/null
wrk -t2 -c"$connections" -d"$((seconds + 2))s" "http://127.0.0.1:$listen/" > "$out.wrk" &
load=$!
sleep 1
if [ "${CALLGRAPH:-0}" = "1" ]; then
    "$perf" record -F 299 --call-graph dwarf,8192 -p "$proxy" -o "$out.data" -- sleep "$seconds" > /dev/null 2>&1
else
    "$perf" record -F 1999 -p "$proxy" -o "$out.data" -- sleep "$seconds" > /dev/null 2>&1
fi
wait $load

{
    echo "# profile $label: $(basename "$binary") workers=$workers connections=$connections seconds=$seconds"
    echo
    grep -E "Requests/sec|Latency" "$out.wrk"
    echo
    echo "## by dso"
    "$perf" report -i "$out.data" --stdio --no-children --sort dso 2>/dev/null | grep -E "^ +[0-9]" | sed -n "1,8p"
    echo
    echo "## top symbols"
    "$perf" report -i "$out.data" --stdio --no-children --sort dso,symbol 2>/dev/null | grep -E "^ +[0-9]" | sed -n "1,70p"

    if [ "${CALLGRAPH:-0}" = "1" ]; then

        echo
        "$perf" script -i "$out.data" 2>/dev/null | python3 "$here/stacks.py"

    fi
} > "$out.txt"

rm -f "$out.data"
cat "$out.txt"
