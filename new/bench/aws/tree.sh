#!/usr/bin/env bash

set -euo pipefail

label="${1:?usage: tree.sh <label> <binary> <workers> <parents (| separated)> [extra args]}"
binary="${2:?}"
workers="${3:?}"
parents="${4:?}"
shift 4
proxy_private="${PROXY_PRIVATE:?}"
backend_private="${BACKEND_PRIVATE:?}"
load_private="${LOAD_PRIVATE:?}"
listen=3903
url="http://$proxy_private:$listen/"
out="$HOME/results/tree-$label"
hop=(ssh -o BatchMode=yes -o StrictHostKeyChecking=no "ubuntu@$load_private")

"$binary" --listen "$proxy_private:$listen" --upstream "$backend_private:3900" --workers "$workers" --log warn "$@" > "$out.proxy.log" 2>&1 &
proxy=$!
trap 'kill $proxy 2>/dev/null || true' EXIT

sleep 0.5
"${hop[@]}" wrk -t2 -c128 -d3s "$url" > /dev/null
"${hop[@]}" wrk -t2 -c128 -d8s "$url" > "$out.wrk" &
load=$!
sleep 1
perf record -F 2999 --call-graph fp -p "$proxy" -o "$out.data" -- sleep 6 > /dev/null 2>&1
wait $load

IFS='|' read -r -a wanted <<< "$parents"

{
    grep -E "Requests/sec" "$out.wrk"
    perf script -i "$out.data" --no-inline -F ip,sym,dso 2> /dev/null | python3 "$HOME/aegisx/new/bench/children.py" "${wanted[@]}"
} > "$out.txt"

rm -f "$out.data"
