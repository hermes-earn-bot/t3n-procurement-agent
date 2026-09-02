//! z-tenant-procurement — Confidential B2B Procurement Agent
//!
//! Enterprise usefulness: enforces supplier allowlist + per-supplier spending limits
//! inside the TEE, then creates purchase orders where payment/banking PII is
//! delivered to the supplier's API via `http-with-placeholders` — the contract
//! and agent never see plaintext payment credentials.
//!
//! Ease of maintenance: 2 functions, 2 KV maps (allowlist, secrets), 1 external
//! host (mock supplier API). No per-supplier code forks; add a supplier by
//! updating the allowlist map via TenantClient.

#![warn(clippy::style, missing_debug_implementations)]
#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

extern crate alloc;

pub const CONTRACT_VERSION: &str = "0.1.1";

wit_bindgen::generate!({
    world: "tenant-procurement",
    path: "wit",
    additional_derives: [
        serde::Deserialize,
        serde::Serialize,
    ],
    generate_all,
});

mod create_po;
mod policy;
mod review;

struct Component;

#[cfg(target_arch = "wasm32")]
impl exports::z::tenant_procurement::contracts::Guest for Component {
    fn review_supplier(
        req: exports::z::tenant_procurement::contracts::GenericInput,
    ) -> Result<alloc::vec::Vec<u8>, alloc::string::String> {
        let input = req.input.ok_or("review-supplier: missing input")?;
        review::review_supplier(&input)
    }

    fn create_po(
        req: exports::z::tenant_procurement::contracts::GenericInput,
    ) -> Result<alloc::vec::Vec<u8>, alloc::string::String> {
        let input = req.input.ok_or("create-po: missing input")?;
        create_po::create_po(&input)
    }
}

#[cfg(target_arch = "wasm32")]
export!(Component);

#[cfg(test)]
mod tests {
    use super::CONTRACT_VERSION;

    #[test]
    fn contract_version_is_semver() {
        let parts: Vec<&str> = CONTRACT_VERSION.split('.').collect();
        assert_eq!(parts.len(), 3);
        for p in parts {
            assert!(p.parse::<u32>().is_ok());
        }
    }
}
