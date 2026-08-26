#!/usr/bin/env bash
set -euo pipefail

# =====================================================================
# ORBIT-PQC: Network Namespace & Virtual Link Setup
# =====================================================================

NS_A="sat_a"
NS_B="sat_b"
VETH_A="veth_a"
VETH_B="veth_b"
IP_A="10.0.0.1/24"
IP_B="10.0.0.2/24"
MTU=1500

# Default initial link conditions (2000 km separation, 128 kbps ISL)
INIT_DELAY_MS="6.67ms"   # One-way light propagation delay (2000 km / c)
INIT_RATE="128kbit"
INIT_LOSS="0%"
INIT_LIMIT="1000"        # Queue packet buffer limit

# Ensure root privileges
if [[ $EUID -ne 0 ]]; then
   echo "[-] Error: This script must be run as root." >&2
   exit 1
fi

echo "[*] Cleaning up any existing namespaces..."
ip netns del "${NS_A}" 2>/dev/null || true
ip netns del "${NS_B}" 2>/dev/null || true

echo "[*] Creating isolated network namespaces: ${NS_A} and ${NS_B}..."
ip netns add "${NS_A}"
ip netns add "${NS_B}"

echo "[*] Creating virtual ethernet pair (${VETH_A} <-> ${VETH_B})..."
ip link add "${VETH_A}" type veth peer name "${VETH_B}"

echo "[*] Assigning interfaces to namespaces..."
ip link set "${VETH_A}" netns "${NS_A}"
ip link set "${VETH_B}" netns "${NS_B}"

echo "[*] Configuring IP addresses and MTU (${MTU} bytes)..."
ip netns exec "${NS_A}" ip addr add "${IP_A}" dev "${VETH_A}"
ip netns exec "${NS_A}" ip link set dev "${VETH_A}" mtu "${MTU}"
ip netns exec "${NS_A}" ip link set dev "${VETH_A}" up
ip netns exec "${NS_A}" ip link set dev lo up

ip netns exec "${NS_B}" ip addr add "${IP_B}" dev "${VETH_B}"
ip netns exec "${NS_B}" ip link set dev "${VETH_B}" mtu "${MTU}"
ip netns exec "${NS_B}" ip link set dev "${VETH_B}" up
ip netns exec "${NS_B}" ip link set dev lo up

echo "[*] Initializing tc-netem egress qdiscs..."
# Configure egress throttling on Node A
ip netns exec "${NS_A}" tc qdisc add dev "${VETH_A}" root netem \
    delay "${INIT_DELAY_MS}" \
    rate "${INIT_RATE}" \
    loss "${INIT_LOSS}" \
    limit "${INIT_LIMIT}"

# Configure egress throttling on Node B
ip netns exec "${NS_B}" tc qdisc add dev "${VETH_B}" root netem \
    delay "${INIT_DELAY_MS}" \
    rate "${INIT_RATE}" \
    loss "${INIT_LOSS}" \
    limit "${INIT_LIMIT}"

echo "[+] Link initialized successfully."
echo "    - Node A: ${NS_A} (${IP_A}) on ${VETH_A}"
echo "    - Node B: ${NS_B} (${IP_B}) on ${VETH_B}"
echo "    - Initial Parameters: Delay=${INIT_DELAY_MS}, Rate=${INIT_RATE}, Loss=${INIT_LOSS}, MTU=${MTU}"