// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

//! Inference runtime drivers.
//!
//! Known runtimes launch by default. `FLUXVM_AI_DENY_RUNTIMES` blocks a name.
//! When `FLUXVM_AI_ALLOW_RUNTIMES` is set, it is a strict allowlist (air-gapped
//! sites). Guest images must still contain the binary; a missing binary fails
//! at VM ready, not at profile create.

pub struct RuntimeSpec {
    pub runtime: String,
    pub command: String,
    pub port: u16,
    pub executes_models: bool,
}

pub fn require_supported(runtime: &str) -> Result<(), String> {
    launch_allowed(runtime, &allowlist(), &denylist())
}

pub fn launch_allowed(runtime: &str, allow: &[String], deny: &[String]) -> Result<(), String> {
    let name = normalize(runtime);
    if name.is_empty() || !known(name) {
        return Err(format!("runtime '{runtime}' is not supported"));
    }
    if deny.iter().any(|item| item.eq_ignore_ascii_case(name)) {
        return Err(format!(
            "runtime '{name}' is blocked by FLUXVM_AI_DENY_RUNTIMES"
        ));
    }
    if allow.is_empty() {
        return Ok(());
    }
    if allow.iter().any(|item| item.eq_ignore_ascii_case(name)) {
        Ok(())
    } else {
        Err(format!(
            "runtime '{name}' is registered but not enabled; add it to FLUXVM_AI_ALLOW_RUNTIMES"
        ))
    }
}

pub fn guest_exec(runtime: &str, model_path: &str, gpus: u32) -> Result<String, String> {
    require_supported(runtime)?;
    let parallel = if gpus > 1 {
        format!(" --tensor-parallel-size {gpus}")
    } else {
        String::new()
    };
    let command = match normalize(runtime) {
        "vllm" => {
            format!("/usr/local/bin/vllm serve {model_path} --host 0.0.0.0 --port 8000{parallel}")
        }
        "tensorrt-llm" => {
            format!("/usr/local/bin/trtllm-serve {model_path} --host 0.0.0.0 --port 8000{parallel}")
        }
        "triton" => format!("/opt/tritonserver/bin/tritonserver --model-repository {model_path}"),
        "llama.cpp" => {
            format!("/usr/local/bin/llama-server --model {model_path} --host 0.0.0.0 --port 8000")
        }
        "tei" => {
            format!("/usr/local/bin/text-embeddings-router --model-id {model_path} --port 8000")
        }
        other => return Err(format!("runtime '{other}' is not supported")),
    };
    Ok(command)
}

pub fn spec(runtime: &str, model_path: &str, gpus: u32) -> Result<RuntimeSpec, String> {
    Ok(RuntimeSpec {
        runtime: normalize(runtime).to_string(),
        command: guest_exec(runtime, model_path, gpus)?,
        port: 8000,
        executes_models: true,
    })
}

pub fn workload_path(kind: &str) -> Result<&'static str, String> {
    match kind.trim().to_ascii_lowercase().as_str() {
        "chat" => Ok("/v1/chat/completions"),
        "completions" => Ok("/v1/completions"),
        "embeddings" => Ok("/v1/embeddings"),
        "rerank" => Ok("/v1/rerank"),
        "batch" => Ok("/v1/batches"),
        other => Err(format!("workload '{other}' has no inference path")),
    }
}

fn normalize(runtime: &str) -> &'static str {
    let lower = runtime.trim().to_ascii_lowercase();
    match lower.as_str() {
        "vllm" => "vllm",
        "tensorrt-llm" | "tensorrt_llm" | "trtllm" => "tensorrt-llm",
        "triton" => "triton",
        "llama.cpp" | "llamacpp" | "llama" => "llama.cpp",
        "tei" | "text-embeddings-inference" => "tei",
        _ => "",
    }
}

fn known(name: &str) -> bool {
    matches!(
        name,
        "vllm" | "tensorrt-llm" | "triton" | "llama.cpp" | "tei"
    )
}

fn allowlist() -> Vec<String> {
    split_env("FLUXVM_AI_ALLOW_RUNTIMES")
}

fn denylist() -> Vec<String> {
    split_env("FLUXVM_AI_DENY_RUNTIMES")
}

fn split_env(key: &str) -> Vec<String> {
    std::env::var(key)
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_runtimes_launch_unless_denied_or_allowlisted() {
        assert!(launch_allowed("vllm", &[], &[]).is_ok());
        assert!(launch_allowed("triton", &[], &[]).is_ok());
        assert!(launch_allowed("tensorrt-llm", &[], &[]).is_ok());
        assert!(launch_allowed("llama.cpp", &[], &[]).is_ok());
        assert!(launch_allowed("tei", &[], &[]).is_ok());
        assert!(launch_allowed("triton", &[], &["triton".into()]).is_err());
        assert!(launch_allowed("triton", &["vllm".into()], &[]).is_err());
        assert!(launch_allowed("triton", &["triton".into()], &[]).is_ok());
        assert!(launch_allowed("mystery", &[], &[]).is_err());
    }

    #[test]
    fn tensor_parallel_is_a_vllm_argument_and_workloads_stay_separate() {
        let command = guest_exec("vllm", "/models", 2).unwrap();
        assert!(command.contains("--tensor-parallel-size 2"));
        assert!(!guest_exec("vllm", "/models", 1)
            .unwrap()
            .contains("tensor-parallel"));
        assert_eq!(workload_path("embeddings").unwrap(), "/v1/embeddings");
        assert_eq!(workload_path("rerank").unwrap(), "/v1/rerank");
        assert!(workload_path("rag").is_err());
    }
}
