#!/usr/bin/env bash

set -euo pipefail

label="${1:?usage: callers.sh <label> <binary> <workers> <symbol filters (| separated)> [extra args]}"
binary="${2:?}"
workers="${3:?}"
filters="${4:?}"
shift 4
proxy_private="${PROXY_PRIVATE:?}"
backend_private="${BACKEND_PRIVATE:?}"
load_private="${LOAD_PRIVATE:?}"
listen=3905
url="http://$proxy_private:$listen/"
out="$HOME/results/callers-$label"
hop=(ssh -o BatchMode=yes -o StrictHostKeyChecking=no "ubuntu@$load_private")

"$binary" --listen "$proxy_private:$listen" --upstream "$backend_private:3900" --workers "$workers" --log warn "$@" > "$out.proxy.log" 2>&1 &
proxy=$!
trap 'kill $proxy 2>/dev/null || true' EXIT

sleep 0.5
"${hop[@]}" wrk -t2 -c128 -d3s "$url" > /dev/null
"${hop[@]}" wrk -t2 -c128 -d9s "$url" > "$out.wrk" &
load=$!
sleep 1
perf record -F 4999 --call-graph fp -p "$proxy" -o "$out.data" -- sleep 7 > /dev/null 2>&1
wait $load

IFS='|' read -r -a wanted <<< "$filters"

{
    grep -E "Requests/sec" "$out.wrk"

    for filter in "${wanted[@]}"; do

        echo
        echo "## callers of $filter"
        perf report -i "$out.data" --stdio --no-children --no-inline --sort symbol -g caller,0.5,callee,function,percent --symbol-filter="$filter" 2> /dev/null | grep -vE "^$|^#" | cut -c1-200 | sed -n "1,70p"

    done
} > "$out.txt"

rm -f "$out.data"
