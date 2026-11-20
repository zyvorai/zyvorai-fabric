terraform {
  required_providers {
    zyvor-fabricd = {
      source  = "zyvorai/zyvor-fabricd"
      version = "~> 0.1"
    }
  }
}

provider "zyvor-fabricd" {
  endpoint = "http://localhost:9095"
}

resource "zyvor_fabric_vm" "example" {
  name   = "terraform-vm"
  image  = "/var/lib/zyvor-fabricd/images/ubuntu-22.04.qcow2"
  cpus   = 4
  memory = 4096

  cloud_init = {
    user_data = <<-EOF
      #cloud-config
      users:
        - name: ubuntu
          sudo: ALL=(ALL) NOPASSWD:ALL
          shell: /bin/bash
      packages:
        - qemu-guest-agent
    EOF
  }

  tpm = {
    enabled = true
    version = "2.0"
  }

  vnc = {
    enabled = true
    port    = 5900
  }
}

output "vm_name" {
  value = zyvor_fabric_vm.example.name
}

output "vm_state" {
  value = zyvor_fabric_vm.example.state
}
