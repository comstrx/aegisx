#!/usr/bin/env bash

set -euo pipefail

label="${1:?usage: lab.sh <label> <workers> <binary> [name=args | name=@ENV=value ...]}"
workers="${2:?}"
binary="${3:?}"
shift 3
proxy="${PROXY_PRIVATE:?}"
load="${LOAD_PRIVATE:?}"
backend="${BACKEND_PRIVATE:?}"
out="$HOME/results"
base="$(basename "$binary")"
targets="${TARGETS:-nginx},$base"
args=(--binary "$base=$binary")

for variant in "$@"; do

    name="$base-${variant%%=*}"
    extra="${variant#*=}"
    targets="$targets,$name"

    case "$extra" in
        @*) printf '#!/bin/sh\n%s exec %s "$@"\n' "${extra#@}" "$binary" > "$HOME/bin/$name"; chmod +x "$HOME/bin/$name"; args+=(--binary "$name=$HOME/bin/$name") ;;
        *) args+=(--binary "$name=$binary" --variant "$name=$extra") ;;
    esac

done

cd "$HOME/aegisx/new/bench"

read -r -a connections <<< "${CONNECTIONS:-128 512}"

python3 compare.py --bind "$proxy" --upstream "$backend:3900" --wrk-host "ubuntu@$load" --nginx /usr/sbin/nginx --targets "$targets" --workers "$workers" --connections "${connections[@]}" --seconds "${SECONDS_PER_TRIAL:-10}" --trials "${TRIALS:-3}" "${args[@]}" --output "$out/$label.json" > "$out/$label.log" 2>&1

cat "$out/$label.md"
