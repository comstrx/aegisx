#!/usr/bin/env bash

set -euo pipefail

label="${1:?usage: real.sh <label> [rows]}"
rows="${2:-mix cache forced blob upload}"
proxy="${PROXY_PRIVATE:?}"
load="${LOAD_PRIVATE:?}"
backend="${BACKEND_PRIVATE:?}"
bench="$HOME/aegisx/new/bench"
out="$HOME/results"
common=(--bind "$proxy" --upstream "$backend:3800" --wrk-host "ubuntu@$load" --nginx /usr/sbin/nginx --nginx-main "$HOME/nginx-mainline/usr/sbin/nginx" --caddy "$HOME/bin/caddy" --aegisx "${AEGISX_BIN:-$HOME/bin/aegisx}" --workers 2 --seconds 10 --trials 3)

cd "$bench"

for row in $rows; do

    case "$row" in
        mix) args=(--targets "nginx,nginx-main,caddy,aegisx,aegisx-prod" --variant "aegisx-prod=--config=$bench/configs/app.lua" --script load/mix.lua --connections 64 256) ;;
        cache) args=(--mode cache --cache-headers honor --script load/mix.lua --connections 64 256) ;;
        forced) args=(--mode cache --cache-headers ignore --script load/mix.lua --connections 64 256) ;;
        blob) args=(--targets "nginx,nginx-main,caddy,aegisx" --script load/blob.lua --connections 32) ;;
        upload) args=(--targets "nginx,nginx-main,caddy,aegisx" --script load/upload.lua --connections 32) ;;
        *) echo "unknown row $row"; exit 2 ;;
    esac

    case "$row" in
        cache|forced)

            for target in nginx nginx-main aegisx; do

                python3 compare.py "${common[@]}" "${args[@]}" --targets "$target" --output "$out/$label-$row-$target.json" > "$out/$label-$row-$target.log" 2>&1

                tail -n 9 "$out/$label-$row-$target.md"

            done ;;
        *)

            python3 compare.py "${common[@]}" "${args[@]}" --output "$out/$label-$row.json" > "$out/$label-$row.log" 2>&1

            tail -n 8 "$out/$label-$row.md" ;;
    esac

done
