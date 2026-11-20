# Terraform example: Fabric AI Workloads (preview)
#
# Requires fabricd with FLUXVM_AI_DRY_RUN=1 (or real GPUs) and a JWT token.

terraform {
  required_providers {
    zyvor-fabricd = {
      source  = "zyvorai/zyvor-fabricd"
      version = "~> 0.1"
    }
  }
}

provider "zyvor-fabricd" {
  endpoint = var.endpoint
  token    = var.token
}

variable "endpoint" {
  type    = string
  default = "https://127.0.0.1:9095"
}

variable "token" {
  type      = string
  sensitive = true
}

resource "zyvor-fabricd_model_artifact" "qwen" {
  name   = "qwen3-8b"
  source = "hf://Qwen/Qwen3-8B"
  format = "safetensors"
}

resource "zyvor-fabricd_inference_profile" "edge" {
  name             = "edge-24g"
  runtime          = "vllm"
  vendor           = "nvidia"
  gpu_count        = 1
  minimum_vram_gib = 24
  cpu              = 8
  memory_gib       = 32
}

resource "zyvor-fabricd_inference_deployment" "qwen" {
  name     = "qwen3-8b"
  model    = zyvor-fabricd_model_artifact.qwen.name
  profile  = zyvor-fabricd_inference_profile.edge.name
  replicas = 2
}

resource "zyvor-fabricd_inference_endpoint" "qwen" {
  name             = "qwen3-8b-openai"
  deployment       = zyvor-fabricd_inference_deployment.qwen.name
  routing_strategy = "least_queue"
}

output "gateway_path" {
  value = zyvor-fabricd_inference_endpoint.qwen.gateway_path
}

output "phase" {
  value = zyvor-fabricd_inference_deployment.qwen.phase
}
