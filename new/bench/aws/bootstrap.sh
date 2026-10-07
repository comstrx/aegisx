#!/usr/bin/env bash
set -euxo pipefail
export DEBIAN_FRONTEND=noninteractive
apt-get update
apt-get install -y build-essential pkg-config libssl-dev clang lld cmake nginx wrk nghttp2-client linux-tools-common "linux-tools-$(uname -r)" python3 jq rsync git htop sysstat shellcheck
systemctl disable --now nginx || true
cat > /etc/sysctl.d/90-aegisx-bench.conf <<'SYSCTL'
net.core.somaxconn = 65535
net.core.netdev_max_backlog = 65535
net.ipv4.ip_local_port_range = 1024 65535
net.ipv4.tcp_tw_reuse = 1
net.ipv4.tcp_fin_timeout = 15
net.ipv4.tcp_max_syn_backlog = 65535
fs.file-max = 2097152
kernel.perf_event_paranoid = -1
kernel.kptr_restrict = 0
SYSCTL
sysctl --system
cat > /etc/security/limits.d/90-aegisx-bench.conf <<'LIMITS'
* soft nofile 1048576
* hard nofile 1048576
LIMITS
mkdir -p /etc/systemd/system/user@.service.d
printf '[Service]\nLimitNOFILE=1048576\n' > /etc/systemd/system/user@.service.d/limits.conf
{ git clone --depth 1 https://github.com/giltene/wrk2 /opt/wrk2 && make -C /opt/wrk2 -j"$(nproc)" && install -m 755 /opt/wrk2/wrk /usr/local/bin/wrk2; } || true
sudo -iu ubuntu bash -c 'curl --proto "=https" --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain none && printf "[build]\nincremental = false\n\n[profile.dev]\ndebug = 0\n" > "$HOME/.cargo/config.toml"'
touch /var/lib/aegisx-bench-ready
