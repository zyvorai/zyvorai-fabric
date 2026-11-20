terraform {
  required_providers {
    zyvor-fabricd = {
      source  = "zyvorai/zyvor-fabricd"
      version = "~> 0.1"
    }
  }
}

variable "endpoint" {
  type    = string
  default = "http://127.0.0.1:9095"
}

variable "token" {
  type      = string
  sensitive = true
  default   = ""
}

variable "image" {
  type    = string
  default = "/var/lib/zyvor-fabricd/images/ubuntu-22.04.qcow2"
}

provider "zyvor-fabricd" {
  endpoint = var.endpoint
  token    = var.token
}

resource "zyvor-fabricd_vm" "web" {
  name   = "web-tf"
  image  = var.image
  cpus   = 2
  memory = 2048
}

output "vm_name" {
  value = zyvor-fabricd_vm.web.name
}
