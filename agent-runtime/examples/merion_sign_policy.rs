// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0
//! Sign Keep policy YAML with Ed25519 (portable; no OpenSSL Ed25519 required).
//!
//! ```text
//! cargo run --example merion_sign_policy -- pubkey <seed32hex>
//! cargo run --example merion_sign_policy -- sign  <seed32hex> <policy.yaml>
//! ```

use ed25519_dalek::SigningKey;
use std::{env, fs, process};

fn seed_from_hex(s: &str) -> [u8; 32] {
    let bytes = hex::decode(s.trim()).unwrap_or_else(|e| {
        eprintln!("invalid seed hex: {e}");
        process::exit(2);
    });
    bytes.try_into().unwrap_or_else(|_| {
        eprintln!("seed must be exactly 32 bytes (64 hex chars)");
        process::exit(2);
    })
}

fn main() {
    let mut args = env::args().skip(1);
    let cmd = args.next().unwrap_or_default();
    match cmd.as_str() {
        "pubkey" => {
            let seed = seed_from_hex(&args.next().expect("usage: pubkey <seed32hex>"));
            let pk = SigningKey::from_bytes(&seed).verifying_key().to_bytes();
            println!("{}", hex::encode(pk));
        }
        "sign" => {
            let seed = seed_from_hex(&args.next().expect("usage: sign <seed32hex> <file>"));
            let path = args.next().expect("usage: sign <seed32hex> <file>");
            let yaml = fs::read(&path).unwrap_or_else(|e| {
                eprintln!("read {path}: {e}");
                process::exit(1);
            });
            let sig = zyvor_fabric_agent_runtime::policy::sign_policy_yaml(&yaml, &seed);
            println!("{sig}");
        }
        _ => {
            eprintln!(
                "usage:\n  merion_sign_policy pubkey <seed32hex>\n  merion_sign_policy sign  <seed32hex> <policy.yaml>"
            );
            process::exit(2);
        }
    }
}
