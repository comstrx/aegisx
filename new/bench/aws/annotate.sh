#!/usr/bin/env bash

set -euo pipefail

label="${1:?usage: annotate.sh <label> <binary> <workers> <symbol patterns (| separated)> [extra args]}"
binary="${2:?}"
workers="${3:?}"
patterns="${4:?}"
shift 4
proxy_private="${PROXY_PRIVATE:?}"
backend_private="${BACKEND_PRIVATE:?}"
load_private="${LOAD_PRIVATE:?}"
listen=3904
url="http://$proxy_private:$listen/"
out="$HOME/results/annotate-$label"
hop=(ssh -o BatchMode=yes -o StrictHostKeyChecking=no "ubuntu@$load_private")

"$binary" --listen "$proxy_private:$listen" --upstream "$backend_private:3900" --workers "$workers" --log warn "$@" > "$out.proxy.log" 2>&1 &
proxy=$!
trap 'kill $proxy 2>/dev/null || true' EXIT

sleep 0.5
"${hop[@]}" wrk -t2 -c128 -d3s "$url" > /dev/null
"${hop[@]}" wrk -t2 -c128 -d10s "$url" > "$out.wrk" &
load=$!
sleep 1
perf record -F 9999 -p "$proxy" -o "$out.data" -- sleep 8 > /dev/null 2>&1
wait $load

IFS='|' read -r -a wanted <<< "$patterns"

{
    grep -E "Requests/sec" "$out.wrk"

    for pattern in "${wanted[@]}"; do

        symbol="$(perf report -i "$out.data" --stdio --no-children --sort symbol --dsos "$(basename "$binary")" 2> /dev/null | grep -F "$pattern" | head -n 1 | sed -E 's/^ +[0-9.]+% +\[\.\] //; s/ +$//')"

        echo
        echo "## $symbol"
        perf annotate -i "$out.data" --stdio -l -s "$symbol" 2> /dev/null | sed -n '/Sorted summary/,/^ *$/p' | cut -c1-140 | sed -n '1,45p'

    done
} > "$out.txt"

rm -f "$out.data"
