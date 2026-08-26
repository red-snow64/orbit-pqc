#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 3 ]]; then
    echo "Usage: $0 <delay_ms> <loss_pct> <rate_kbps>" >&2
    exit 1
fi

DELAY_MS="$1"
LOSS_PCT="$2"
RATE_KBPS="$3"
LIMIT="1000"

NS_A="sat_a"
NS_B="sat_b"
VETH_A="veth_a"
VETH_B="veth_b"

if [[ $EUID -ne 0 ]]; then
   echo "[-] Error: Must be run as root." >&2
   exit 1
fi

DELAY_SPEC="${DELAY_MS}ms"
RATE_SPEC="${RATE_KBPS}kbit"
LOSS_SPEC="${LOSS_PCT}%"

# Replace the qdisc to flush stale buffered packets
ip netns exec "${NS_A}" tc qdisc replace dev "${VETH_A}" root netem \
    delay ${DELAY_SPEC} \
    rate "${RATE_SPEC}" \
    loss "${LOSS_SPEC}" \
    limit "${LIMIT}"

ip netns exec "${NS_B}" tc qdisc replace dev "${VETH_B}" root netem \
    delay ${DELAY_SPEC} \
    rate "${RATE_SPEC}" \
    loss "${LOSS_SPEC}" \
    limit "${LIMIT}"