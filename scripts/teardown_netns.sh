#!/usr/bin/env bash
set -euo pipefail

# =====================================================================
# ORBIT-PQC: Namespace Teardown & Resource Cleanup
# =====================================================================

NS_A="sat_a"
NS_B="sat_b"

if [[ $EUID -ne 0 ]]; then
   echo "[-] Error: Must be run as root." >&2
   exit 1
fi

echo "[*] Removing network namespaces (${NS_A}, ${NS_B})..."
ip netns del "${NS_A}" 2>/dev/null || true
ip netns del "${NS_B}" 2>/dev/null || true

echo "[+] Teardown complete. All virtual interfaces and qdiscs flushed."