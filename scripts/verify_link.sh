#!/usr/bin/env bash
set -euo pipefail

# =====================================================================
# ORBIT-PQC: Link Verification & RTT/Loss Assertion Harness
# =====================================================================

NS_A="sat_a"
NS_B="sat_b"
TARGET_IP="10.0.0.2"
PING_COUNT=20

if [[ $EUID -ne 0 ]]; then
   echo "[-] Error: Must be run as root." >&2
   exit 1
fi

echo "======================================================================"
echo "ORBIT-PQC: Verifying Active Emulated Channel"
echo "======================================================================"

echo "[*] Active tc qdisc state on sat_a (veth_a):"
ip netns exec "${NS_A}" tc -s qdisc show dev veth_a

echo -e "\n[*] Active tc qdisc state on sat_b (veth_b):"
ip netns exec "${NS_B}" tc -s qdisc show dev veth_b

echo -e "\n[*] Executing ICMP benchmark (${PING_COUNT} probes from ${NS_A} -> ${TARGET_IP})..."
ip netns exec "${NS_A}" ping -c "${PING_COUNT}" -i 0.1 "${TARGET_IP}"