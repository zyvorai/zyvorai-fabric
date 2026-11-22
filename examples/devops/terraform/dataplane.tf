# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0
# Illustrative Terraform for VM-edge policy (provider apply uses REST).

variable "vm_name" {
  type    = string
  default = "web-1"
}

# Mapped to POST /api/vms/${vm}/dataplane/policy/control
locals {
  dataplane_control = {
    action = "guard"
  }
  dataplane_policy = {
    default_allow   = false
    allow_cidrs     = ["10.0.0.0/8"]
    allow_ports     = ["tcp/443", "udp/53"]
    deny_cidrs      = []
    sample_rate     = 1
    audit_mode      = false
    max_egress_mbps = null
    max_egress_pps  = null
    allow_icmp      = false
    groups          = []
    labels          = ["app=web"]
    allow_fqdns     = []
    entities        = []
  }
}

output "control_endpoint" {
  value = "/api/vms/${var.vm_name}/dataplane/policy/control"
}

output "policy_body" {
  value = local.dataplane_policy
}
