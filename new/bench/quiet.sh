#!/usr/bin/env bash

set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
label="${1:?usage: quiet.sh <label> [load-max] [settle-checks]}"
load_max="${2:-1.5}"
settle="${3:-3}"
lock="$here/results/.benchmarking"
quiet=0

while true; do

    load="$(cut -d' ' -f1 /proc/loadavg)"

    if awk -v load="$load" -v max="$load_max" 'BEGIN { exit !(load < max) }'; then
        quiet=$((quiet + 1))
    else
        quiet=0
    fi

    if [ "$quiet" -ge "$settle" ]; then break; fi

    sleep 30

done

touch "$lock"
trap 'rm -f "$lock"' EXIT

echo "quiet window at $(date -Is), load $load" > "$here/results/$label.note"

python3 "$here/compare.py" --targets nginx,aegisx --workers 4 --connections 128 256 --trials 4 --output "$here/results/$label-4w.json" > "$here/results/$label-4w.log" 2>&1
python3 "$here/compare.py" --targets nginx,aegisx --workers 8 --connections 128 --trials 4 --output "$here/results/$label-8w.json" > "$here/results/$label-8w.log" 2>&1

echo "finished at $(date -Is), load $(cut -d' ' -f1 /proc/loadavg)" >> "$here/results/$label.note"
