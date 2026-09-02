//! create-po: creates a purchase order via outbound HTTP with placeholders.
//!
//! Payment/banking PII is NEVER passed as contract input. The contract templates
//! `{{profile.<field>}}` markers and the host's http-with-placeholders resolves
//! them from the calling user's profile inside the TEE at dispatch time.
//! Enforces allowlist + spending limit via shared policy (fail-closed).

use crate::policy::{decide, parse_amount_cents, Policy, ReviewInput};

#[derive(serde::Deserialize)]
pub struct CreatePoReq {
    pub supplier_id: String,
    pub sku: String,
    pub quantity: u32,
    pub amount: String,
    pub currency: String,
}

#[derive(serde::Serialize)]
pub struct CreatePoResp {
    pub po_id: String,
    pub status: String,
    pub supplier_id: String,
}

const SUPPLIER_API_BASE: &str = "https://httpbin.org";

pub fn create_po(input: &[u8]) -> Result<Vec<u8>, String> {
    let req: CreatePoReq = serde_json::from_slice(input)
        .map_err(|e| alloc::format!("create-po: bad input: {e}"))?;
    #[cfg(target_arch = "wasm32")]
    {
        let resp = create_po_wasm(req)?;
        serde_json::to_vec(&resp).map_err(|e| e.to_string())
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = req;
        Err("create_po only on wasm32".to_string())
    }
}

#[cfg(target_arch = "wasm32")]
use crate::host::{
    interfaces::{http_with_placeholders as hwp, kv_store, logging},
    tenant::tenant_context,
};

#[cfg(target_arch = "wasm32")]
fn create_po_wasm(req: CreatePoReq) -> Result<CreatePoResp, String> {
    use serde_json::json;

    // Input validation
    let supplier_trim = req.supplier_id.trim().to_string();
    if supplier_trim.is_empty() || req.sku.trim().is_empty() {
        return Err("supplier_id and sku are required".to_string());
    }
    if req.quantity == 0 {
        return Err("quantity must be > 0".to_string());
    }
    let amount_cents = parse_amount_cents(&req.amount).map_err(|e| e)?;


    // Policy enforcement — must pass allowlist + limits before any egress
    let policy = load_policy_for_po().ok_or_else(|| "policy unavailable, denied".to_string())?;
    let decision = decide(&policy, &ReviewInput { supplier_id: supplier_trim.clone(), amount_cents, currency: req.currency.clone() });
    if !decision.approved {
        let _ = logging::info(&alloc::format!("create-po denied by policy: {}", decision.reason));
        return Err(alloc::format!("policy denied: {}", decision.reason));
    }

    // Optional: read supplier API key from secrets map
    let tid = tenant_context::tenant_did();
    let secrets_map = alloc::format!("z:{}:secrets", hex::encode(&tid));
    let api_key: Option<String> = match kv_store::get(&secrets_map, b"supplier_api_key") {
        Ok(Some(b)) => String::from_utf8(b).ok(),
        _ => None,
    };

    let _ = logging::info(&alloc::format!("create-po: supplier={} sku={} qty={} amount_cents={} {}", supplier_trim, req.sku, req.quantity, amount_cents, req.currency));

    let po_body = json!({
        "supplier_id": supplier_trim,
        "sku": req.sku,
        "quantity": req.quantity,
        "amount": req.amount,
        "currency": req.currency,
        "payment": {
            "method": "{{profile.payment_method}}",
            "account_ref": "{{profile.payment_account_ref}}",
            "holder_name": "{{profile.full_name}}",
            "holder_email": "{{profile.email}}"
        },
        "requested_by": "{{profile.email}}",
        "notes": "PO created via T3N Confidential Procurement Agent"
    });

    let mut headers = alloc::vec![("Accept".to_string(), "application/json".to_string())];
    if let Some(k) = api_key {
        headers.push(("Authorization".to_string(), alloc::format!("Bearer {k}")));
    }

    let url = alloc::format!("{SUPPLIER_API_BASE}/post");

    let resp = hwp::call(&hwp::Request { method: hwp::Verb::Post, url, headers: Some(headers), payload: Some(serde_json::to_vec(&po_body).map_err(|e| e.to_string())?) })
        .map_err(|e| alloc::format!("supplier PO call failed: {}", fmt_hwp_err(e)))?;

    if resp.code != 200 && resp.code != 201 {
        let body = String::from_utf8_lossy(&resp.payload);
        let _ = logging::error(&alloc::format!("supplier API HTTP {}: {}", resp.code, body));
        return Err(alloc::format!("supplier API failed: HTTP {}", resp.code));
    }

    let json: serde_json::Value = serde_json::from_slice(&resp.payload).map_err(|e| e.to_string())?;
    // Derive PO id from supplier response if available, else deterministic hash
    let po_id = if let Some(id) = json.get("json").and_then(|j| j.get("po_id")).and_then(|v| v.as_str()) {
        id.to_string()
    } else {
        // fallback: deterministic from supplier+sku+hash of request
        let hash = {
            let mut h: u64 = 1469598103934665603;
            for b in alloc::format!("{}:{}:{}:{}", supplier_trim, req.sku, req.quantity, amount_cents).bytes() {
                h ^= b as u64; h = h.wrapping_mul(1099511628211);
            }
            alloc::format!("{:016x}", h)
        };
        alloc::format!("PO-{}-{}-{}", supplier_trim, req.sku, &hash[..8])
    };

    let _ = logging::info(&alloc::format!("PO created: {} for supplier {}", po_id, supplier_trim));

    Ok(CreatePoResp { po_id, status: "submitted".to_string(), supplier_id: supplier_trim })
}

#[cfg(target_arch = "wasm32")]
fn load_policy_for_po() -> Option<Policy> {
    let tid = tenant_context::tenant_did();
    let map_name = alloc::format!("z:{}:allowlist", hex::encode(&tid));
    let suppliers: Vec<String> = match kv_store::get(&map_name, b"suppliers") {
        Ok(Some(bytes)) => serde_json::from_slice(&bytes).ok()?,
        _ => return None,
    };
    let mut limits = alloc::collections::BTreeMap::new();
    let default_limit: u64;
    match kv_store::get(&map_name, b"spending_limits") {
        Ok(Some(bytes)) => {
            if let Ok(val) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                if let Some(obj) = val.as_object() {
                    for (k, v) in obj {
                        if let Some(n) = v.as_f64() { limits.insert(k.clone(), (n*100.0).round() as u64); }
                        else if let Some(s) = v.as_str() { if let Ok(c)=parse_amount_cents(s){ limits.insert(k.clone(), c); } }
                    }
                }
            }
            default_limit = limits.get("__default").copied().unwrap_or(500_000);
            limits.remove("__default");
        }
        _ => return None,
    }
    let version = match kv_store::get(&map_name, b"policy_version") { Ok(Some(b)) => String::from_utf8(b).unwrap_or("v1".into()), _ => "v1".into() };
    Some(Policy { suppliers, limits, default_limit, version, allowed_currencies: alloc::vec!["USD".into()] })
}

#[cfg(target_arch = "wasm32")]
fn fmt_hwp_err(e: hwp::HttpError) -> String {
    match e {
        hwp::HttpError::EgressDenied(h) => alloc::format!("egress denied for host {h}"),
        hwp::HttpError::PlaceholderDenied(m) => alloc::format!("placeholder not permitted: {m}"),
        hwp::HttpError::PlaceholderUnknown(f) => alloc::format!("user profile missing field: {f}"),
        hwp::HttpError::PlaceholderNoUserContext => "no user context bound".to_string(),
        hwp::HttpError::UpstreamError(r) => alloc::format!("upstream: {r}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bad_input() { assert!(create_po(b"not json").is_err()); }
    #[test]
    fn non_wasm_err() {
        let v = serde_json::to_vec(&serde_json::json!({"supplier_id":"acme-corp","sku":"SKU-001","quantity":1,"amount":"100.00","currency":"USD"})).unwrap();
        assert!(create_po(&v).is_err());
    }
}
