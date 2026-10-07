#!/usr/bin/env bash

set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$here/../../.." && pwd)"
key="${AEGISX_BENCH_KEY:-$here/.secret/aegisx-bench}"
nodes="$here/.secret/nodes.json"
ssh_opts=(-i "$key" -o BatchMode=yes -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o LogLevel=ERROR -o ConnectTimeout=10 -o ControlMaster=auto -o "ControlPath=$here/.secret/%C" -o ControlPersist=600)
hop="ssh -o BatchMode=yes -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o LogLevel=ERROR"

address () {

    [ "$nodes" -nt "$here/terraform.tfstate" ] || tofu -chdir="$here" output -json > "$nodes"

    jq -er --arg kind "$1_ips" --arg node "$2" '.[$kind].value[$node]' "$nodes" || { echo "node $2 is not up" >&2; return 1; }

}

run () {

    local host; host="$(address public "$1")"; shift

    ssh -T "${ssh_opts[@]}" "ubuntu@$host" "$@"

}

wait_ready () {

    local node

    address public build > /dev/null

    for node in $(jq -r '.public_ips.value | keys[]' "$nodes"); do

        for _ in $(seq 1 90); do

            if run "$node" test -f /var/lib/aegisx-bench-ready 2>/dev/null; then echo "ready $node"; break; fi

            sleep 10

        done

    done

}

sync () {

    local host; host="$(address public build)"

    rsync -az --no-times --checksum --delete -e "ssh ${ssh_opts[*]}" \
        --exclude 'target/' --exclude '.git/' --exclude '__pycache__/' --exclude '.terraform/' --exclude '.secret*' --exclude '*.tfstate*' --exclude '*.tfvars' \
        --exclude '/new/bench/results/' --exclude '/new/bench/backend/backend' \
        --include '/new/***' --include '/model/' --include '/model/weights/***' --exclude '/*' --exclude '/model/*' \
        "$root/" "ubuntu@$host:aegisx/"

}

lock () {

    local host; host="$(address public build)"

    rsync -az -e "ssh ${ssh_opts[*]}" "ubuntu@$host:aegisx/new/server/Cargo.lock" "$root/new/server/Cargo.lock"

}

remote_cargo () {

    local args; printf -v args '%q ' "$@"

    run build ". \"\$HOME/.cargo/env\" && cd aegisx/new/server && cargo $args"
    lock

}

job () {

    local node=build name="$1" home="cd \"\$HOME/aegisx/new/server\"" exports=""; shift

    case "$name" in *:*) node="${name%%:*}"; name="${name#*:}" ;; esac

    if [ "$node" != build ]; then

        home="cd \"\$HOME/aegisx/new/bench\""
        exports="export PROXY_PRIVATE=$(address private proxy) LOAD_PRIVATE=$(address private load) BACKEND_PRIVATE=$(address private backend)"

    fi

    run "$node" "mkdir -p jobs && rm -f jobs/$name.exit && cat > jobs/$name.sh && { nohup bash -c 'bash -e jobs/$name.sh > jobs/$name.log 2>&1; echo \$? > jobs/$name.exit' > /dev/null 2>&1 < /dev/null & }" <<JOB
[ -f "\$HOME/.cargo/env" ] && . "\$HOME/.cargo/env"
$exports
$home
$*
JOB

}

await () {

    local node=build name="$1" code=""

    case "$name" in *:*) node="${name%%:*}"; name="${name#*:}" ;; esac

    until [ -n "$code" ]; do code="$(run "$node" "cat jobs/$name.exit 2>/dev/null" || true)"; [ -n "$code" ] || sleep 15; done

    run "$node" "tail -n ${2:-40} jobs/$name.log"

    [ "$node" != build ] || lock

    return "$code"

}

deploy () {

    local node proxy backend load

    proxy="$(address private proxy)"
    backend="$(address private backend)"
    load="$(address private load)"

    for node in build proxy; do rsync -az -e "ssh ${ssh_opts[*]}" "$key" "ubuntu@$(address public "$node"):.ssh/id_ed25519"; run "$node" chmod 600 .ssh/id_ed25519; done

    for node in proxy load backend; do run "$node" mkdir -p bin results aegisx/new; done

    run build "rsync -az --delete -e '$hop' aegisx/new/bench ubuntu@$proxy:aegisx/new/ && rsync -az -e '$hop' bin/ ubuntu@$proxy:bin/ && rsync -az -e '$hop' bin/backend ubuntu@$backend:bin/backend && { [ ! -f bin/app ] || rsync -az -e '$hop' bin/app ubuntu@$backend:bin/app; } && { [ ! -f bin/h3load ] || { ssh -o BatchMode=yes -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o LogLevel=ERROR ubuntu@$load mkdir -p bin && rsync -az -e '$hop' bin/h3load ubuntu@$load:bin/h3load; }; } && rsync -az --exclude tokens.lua -e '$hop' aegisx/new/bench/load/ ubuntu@$load:load/"

}

start_app () {

    local load; load="$(address public load)"

    run backend "pkill -x app || true; APP_WORKERS=${1:-2} APP_POOL=${2:-24} nohup ./bin/app > results/app.log 2>&1 < /dev/null & for _ in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20; do ss -ltn | grep -q ':3800 ' && break; sleep 1; done; ss -ltn | grep -c ':3800 '"
    run backend "./bin/app --tokens 400" | run load "mkdir -p load && cat > load/tokens.lua && wc -l < load/tokens.lua"

}

start_backend () {

    run backend "pkill -x backend || true; nohup ./bin/backend 3900 2 > results/backend.log 2>&1 < /dev/null & sleep 0.5; ss -ltn | grep -c ':3900 '"

}

bench () {

    local label="$1" proxy load backend; shift

    proxy="$(address private proxy)"
    load="$(address private load)"
    backend="$(address private backend)"

    run proxy "cd aegisx/new/bench && nohup python3 compare.py --bind $proxy --upstream $backend:3900 --wrk-host ubuntu@$load --aegisx /home/ubuntu/bin/aegisx --nginx /usr/sbin/nginx --output /home/ubuntu/results/$label.json $* > /home/ubuntu/results/$label.log 2>&1 < /dev/null &"

}

fetch () {

    local host; host="$(address public proxy)"

    rsync -az -e "ssh ${ssh_opts[*]}" "ubuntu@$host:results/" "$root/new/bench/results/aws/"

}

case "${1:-}" in
    wait) wait_ready ;;
    sync) sync ;;
    cargo) shift; remote_cargo "$@" ;;
    job) shift; job "$@" ;;
    await) shift; await "$@" ;;
    deploy) deploy ;;
    backend) start_backend ;;
    app) shift; start_app "$@" ;;
    bench) shift; bench "$@" ;;
    fetch) fetch ;;
    ssh) shift; run "$@" ;;
    *) echo "usage: run.sh wait|sync|cargo <args>|job [node:]<name> <script>|await [node:]<name> [lines]|deploy|backend|app [workers] [pool]|bench <label> <compare args>|fetch|ssh <node> <cmd>"; exit 2 ;;
esac
