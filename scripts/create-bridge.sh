#!/bin/bash
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0
set -e

BRIDGE_NAME="${1:-br0}"

echo "Creating network bridge: $BRIDGE_NAME"

# Create bridge
sudo ip link add name "$BRIDGE_NAME" type bridge
sudo ip link set "$BRIDGE_NAME" up

# Enable IP forwarding
sudo sysctl -w net.ipv4.ip_forward=1

echo "Bridge $BRIDGE_NAME created successfully"
echo ""
echo "To assign an IP address:"
echo "  sudo ip addr add 192.168.100.1/24 dev $BRIDGE_NAME"
echo ""
echo "To configure DHCP, install and configure dnsmasq:"
echo "  sudo dnsmasq --interface=$BRIDGE_NAME --bind-interfaces --dhcp-range=192.168.100.50,192.168.100.150"
