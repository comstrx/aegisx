#!/usr/bin/env bash

set -euo pipefail

label="${1:?usage: profile.sh <label> [workers] [connections] [seconds] [extra args]}"
workers="${2:-1}"
connections="${3:-128}"
seconds="${4:-10}"
shift $(( $# > 4 ? 4 : $# ))
proxy_private="${PROXY_PRIVATE:?}"
backend_private="${BACKEND_PRIVATE:?}"
load_private="${LOAD_PRIVATE:?}"
binary="${AEGISX_BIN:-$HOME/bin/aegisx}"
listen=3901
url="http://$proxy_private:$listen/"
out="$HOME/results/profile-$label"
hop=(ssh -o BatchMode=yes -o StrictHostKeyChecking=no "ubuntu@$load_private")

"$binary" --listen "$proxy_private:$listen" --upstream "$backend_private:3900" --workers "$workers" --log warn "$@" > "$out.proxy.log" 2>&1 &
proxy=$!
trap 'kill $proxy 2>/dev/null || true' EXIT

sleep 0.5
"${hop[@]}" wrk -t2 -c"$connections" -d3s "$url" > /dev/null
"${hop[@]}" wrk -t2 -c"$connections" -d"$((seconds + 2))s" --latency "$url" > "$out.wrk" &
load=$!
sleep 1
perf record -F 2999 --call-graph fp -p "$proxy" -o "$out.data" -- sleep "$seconds" > /dev/null 2>&1
wait $load

sudo perf probe -q -d 'probe_alloc:*' 2> /dev/null || true

for symbol in mi_malloc_aligned mi_zalloc_aligned mi_realloc_aligned mi_free; do sudo perf probe -q -x "$binary" --add "probe_alloc:$symbol=$symbol" 2> /dev/null || true; done

"${hop[@]}" wrk -t2 -c"$connections" -d6s "$url" > "$out.alloc.wrk" &
load=$!
sleep 1
sudo perf stat -x, -e 'probe_alloc:*' -p "$proxy" -o "$out.alloc" -- sleep 4 || true
wait $load
sudo perf probe -q -d 'probe_alloc:*' 2> /dev/null || true

{
    echo "# profile $label: binary=$(basename "$binary") workers=$workers connections=$connections seconds=$seconds args=$*"
    echo
    grep -E "Requests/sec|Latency|99%" "$out.wrk"
    echo
    echo "## allocator calls per request"
    rate="$(grep -oE 'Requests/sec: +[0-9.]+' "$out.alloc.wrk" | grep -oE '[0-9.]+$' || echo 0)"
    awk -F, -v rate="$rate" '$3 ~ /probe_alloc/ && rate > 0 { printf "%-28s %.2f\n", $3, $1 / (rate * 4) }' "$out.alloc" 2> /dev/null || true
    echo
    echo "## by dso"
    perf report -i "$out.data" --stdio --no-children --sort dso 2> /dev/null | grep -E "^ +[0-9]" | sed -n "1,8p"
    echo
    echo "## top symbols (self)"
    perf report -i "$out.data" --stdio --no-children --sort dso,symbol -g none 2> /dev/null | grep -E "^ +[0-9]" | cut -c1-200 | sed -n "1,90p"
    echo
    echo "## inclusive (children), user space"
    perf report -i "$out.data" --stdio --children --sort symbol -g none --dsos "$(basename "$binary")" 2> /dev/null | grep -E "^ +[0-9]" | cut -c1-220 | sed -n "1,90p"
} > "$out.txt"

rm -f "$out.data"
cut -c1-180 "$out.txt"
