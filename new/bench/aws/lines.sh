#!/usr/bin/env bash

set -euo pipefail

label="${1:?usage: lines.sh <label> <binary> [workers] [extra args]}"
binary="${2:?}"
workers="${3:-1}"
shift $(( $# > 3 ? 3 : $# ))
proxy_private="${PROXY_PRIVATE:?}"
backend_private="${BACKEND_PRIVATE:?}"
load_private="${LOAD_PRIVATE:?}"
listen=3902
url="http://$proxy_private:$listen/"
out="$HOME/results/lines-$label"
hop=(ssh -o BatchMode=yes -o StrictHostKeyChecking=no "ubuntu@$load_private")

"$binary" --listen "$proxy_private:$listen" --upstream "$backend_private:3900" --workers "$workers" --log warn "$@" > "$out.proxy.log" 2>&1 &
proxy=$!
trap 'kill $proxy 2>/dev/null || true' EXIT

sleep 0.5
"${hop[@]}" wrk -t2 -c128 -d3s "$url" > /dev/null
"${hop[@]}" wrk -t2 -c128 -d8s "$url" > "$out.wrk" &
load=$!
sleep 1
perf record -F 4999 -p "$proxy" -o "$out.data" -- sleep 6 > /dev/null 2>&1
wait $load

{
    grep -E "Requests/sec" "$out.wrk"
    echo "## hottest source lines (self), $(basename "$binary")"
    perf report -i "$out.data" --stdio --no-children --sort symbol,srcline --dsos "$(basename "$binary")" 2> /dev/null | grep -E "^ +[0-9]" | sed -E 's/ +/ /g' | cut -c1-230 | sed -n "1,160p"
} > "$out.txt"

rm -f "$out.data"
