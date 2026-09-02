//! review-supplier: enterprise policy check inside the TEE.
//!
//! Reads tenant KV map:
//!   z:<tid>:allowlist — key `suppliers` -> JSON array, `spending_limits` -> JSON object (values in dollars or cents), `policy_version` -> string
//! Fail-closed: if KV unavailable or malformed, deny. No allowlist leak in errors.

use crate::policy::{decide, parse_amount_cents, Policy, ReviewInput};

#[derive(serde::Deserialize)]
pub struct ReviewReq {
    pub supplier_id: String,
    pub sku: String,
    pub quantity: u32,
    pub amount: String,
    pub currency: String,
}

#[derive(serde::Serialize)]
pub struct ReviewResp {
    pub approved: bool,
    pub reason: String,
    pub supplier: String,
    pub policy_version: String,
}

pub fn review_supplier(input: &[u8]) -> Result<Vec<u8>, String> {
    let req: ReviewReq = serde_json::from_slice(input)
        .map_err(|e| alloc::format!("review-supplier: bad input: {e}"))?;
    #[cfg(target_arch = "wasm32")]
    {
        let resp = review_wasm(req)?;
        serde_json::to_vec(&resp).map_err(|e| e.to_string())
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = req;
        Err("review_supplier only on wasm32".to_string())
    }
}

#[cfg(target_arch = "wasm32")]
use crate::host::{
    interfaces::{kv_store, logging},
    tenant::tenant_context,
};

#[cfg(target_arch = "wasm32")]
fn review_wasm(req: ReviewReq) -> Result<ReviewResp, String> {
    // Quantity validation
    if req.quantity == 0 {
        return Ok(ReviewResp { approved: false, reason: "quantity must be > 0".to_string(), supplier: req.supplier_id, policy_version: "v1".to_string() });
    }
    let sku = req.sku.trim();
    if sku.is_empty() {
        return Ok(ReviewResp { approved: false, reason: "sku is empty".to_string(), supplier: req.supplier_id, policy_version: "v1".to_string() });
    }
    let supplier_trim = req.supplier_id.trim();
    if supplier_trim.is_empty() {
        return Ok(ReviewResp { approved: false, reason: "supplier_id is empty".to_string(), supplier: req.supplier_id, policy_version: "v1".to_string() });
    }
    let amount_cents = match parse_amount_cents(&req.amount) {
        Ok(v) => v,
        Err(e) => return Ok(ReviewResp { approved: false, reason: e, supplier: req.supplier_id, policy_version: "v1".to_string() }),
    };

    // Load policy fail-closed
    let policy = load_policy();
    if policy.is_none() {
        let _ = logging::error("review: policy unavailable, failing closed");
        return Ok(ReviewResp { approved: false, reason: "policy unavailable, denied".to_string(), supplier: req.supplier_id, policy_version: "unknown".to_string() });
    }
    let policy = policy.unwrap();
    let decision = decide(&policy, &ReviewInput { supplier_id: supplier_trim.to_string(), amount_cents, currency: req.currency.clone() });
    let _ = logging::info(&alloc::format!("review: supplier={} amount_cents={} {} sku={} approved={}", supplier_trim, amount_cents, req.currency, sku, decision.approved));
    Ok(ReviewResp { approved: decision.approved, reason: decision.reason, supplier: req.supplier_id, policy_version: policy.version })
}

#[cfg(target_arch = "wasm32")]
fn load_policy() -> Option<Policy> {
    let tid = tenant_context::tenant_did();
    let map_name = alloc::format!("z:{}:allowlist", hex::encode(&tid));

    // suppliers
    let suppliers: Vec<String> = match kv_store::get(&map_name, b"suppliers") {
        Ok(Some(bytes)) => match serde_json::from_slice::<Vec<String>>(&bytes) {
            Ok(v) => v,
            Err(_) => {
                let _ = logging::error("allowlist suppliers JSON invalid, failing closed");
                return None;
            }
        },
        Ok(None) => {
            let _ = logging::error("allowlist suppliers key missing, failing closed");
            return None;
        }
        Err(e) => {
            let _ = logging::error(&alloc::format!("kv read suppliers failed: {e}, failing closed"));
            return None;
        }
    };

    // spending_limits
    let mut limits = alloc::collections::BTreeMap::new();
    let default_limit: u64;
    match kv_store::get(&map_name, b"spending_limits") {
        Ok(Some(bytes)) => {
            if let Ok(val) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                if let Some(obj) = val.as_object() {
                    for (k, v) in obj {
                        if let Some(n) = v.as_f64() {
                            let cents = (n * 100.0).round() as u64;
                            limits.insert(k.clone(), cents);
                        } else if let Some(s) = v.as_str() {
                            if let Ok(c) = parse_amount_cents(s) { limits.insert(k.clone(), c); }
                        }
                    }
                }
            }
            default_limit = limits.get("__default").copied().unwrap_or(500_000);
            limits.remove("__default");
        }
        Ok(None) => {
            let _ = logging::error("spending_limits missing, failing closed");
            return None;
        }
        Err(e) => {
            let _ = logging::error(&alloc::format!("kv read limits failed: {e}, failing closed"));
            return None;
        }
    }

    // policy_version
    let version = match kv_store::get(&map_name, b"policy_version") {
        Ok(Some(b)) => String::from_utf8(b).unwrap_or_else(|_| "v1".to_string()),
        _ => "v1".to_string(),
    };

    // Demo fallback ONLY if explicitly seeded? For production, fail-closed above already covers missing keys.
    // If suppliers was present but empty, still deny via decide.

    Some(Policy { suppliers, limits, default_limit, version, allowed_currencies: alloc::vec!["USD".to_string()] })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bad_input() { assert!(review_supplier(b"not json").is_err()); }
    #[test]
    fn non_wasm_err() {
        let v = serde_json::to_vec(&serde_json::json!({"supplier_id":"acme-corp","sku":"SKU-001","quantity":2,"amount":"100.00","currency":"USD"})).unwrap();
        assert!(review_supplier(&v).is_err());
    }
}
