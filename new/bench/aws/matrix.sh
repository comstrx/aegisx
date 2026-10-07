#!/usr/bin/env bash

set -euo pipefail

label="${1:?usage: matrix.sh <label> [rows] [extra compare args]}"
rows="${2:-proxy1 proxy2 static cache h1tls h2}"
shift $(( $# > 2 ? 2 : $# ))
proxy="${PROXY_PRIVATE:?}"
load="${LOAD_PRIVATE:?}"
backend="${BACKEND_PRIVATE:?}"
out="$HOME/results"
common=(--bind "$proxy" --upstream "$backend:3900" --wrk-host "ubuntu@$load" --aegisx "${AEGISX_BIN:-$HOME/bin/aegisx}" --nginx /usr/sbin/nginx --targets "${TARGETS:-nginx,aegisx}" --connections 128 512 --seconds 10 --trials 3)

cd "$HOME/aegisx/new/bench"

for row in $rows; do

    case "$row" in
        proxy1) args=(--workers 1) ;;
        proxy2) args=(--workers 2) ;;
        static) args=(--workers 2 --mode static) ;;
        cache) args=(--workers 2 --mode cache) ;;
        h1tls) args=(--workers 2 --protocol h1tls) ;;
        h2) args=(--workers 2 --protocol h2) ;;
        h3) args=(--workers 2 --protocol h3 --connections 16 64) ;;
        *) echo "unknown row $row"; exit 2 ;;
    esac

    python3 compare.py "${common[@]}" "${args[@]}" "$@" --output "$out/$label-$row.json" > "$out/$label-$row.log" 2>&1

    tail -n 8 "$out/$label-$row.md"

done
