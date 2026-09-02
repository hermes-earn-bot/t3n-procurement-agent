//! Pure policy logic — no host I/O. Testable on native target.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

#[derive(Debug, Clone)]
pub struct Policy {
    pub suppliers: Vec<String>,
    pub limits: BTreeMap<String, u64>, // minor units (cents)
    pub default_limit: u64,
    pub version: String,
    pub allowed_currencies: Vec<String>,
}

impl Default for Policy {
    fn default() -> Self {
        let mut limits = BTreeMap::new();
        limits.insert("acme-corp".to_string(), 500_000); // $5000.00
        limits.insert("initech".to_string(), 1_000_000);
        limits.insert("globex".to_string(), 250_000);
        Self {
            suppliers: alloc::vec!["acme-corp".to_string(), "initech".to_string(), "globex".to_string()],
            limits,
            default_limit: 500_000,
            version: "v1".to_string(),
            allowed_currencies: alloc::vec!["USD".to_string()],
        }
    }
}

#[derive(Debug)]
pub struct ReviewInput {
    pub supplier_id: String,
    pub amount_cents: u64,
    pub currency: String,
}

#[derive(Debug, PartialEq)]
pub struct Decision {
    pub approved: bool,
    pub reason: String,
}

/// Parse "1250.00" -> 125000 cents. Rejects negative, empty, >2 decimals, non-numeric.
pub fn parse_amount_cents(s: &str) -> Result<u64, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("amount is empty".to_string());
    }
    if s.starts_with('-') {
        return Err("amount must be > 0".to_string());
    }
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() > 2 {
        return Err("amount must be numeric string, e.g. \"1250.00\"".to_string());
    }
    let dollars = parts[0];
    if dollars.is_empty() {
        return Err("amount must be numeric string, e.g. \"1250.00\"".to_string());
    }
    if !dollars.chars().all(|c| c.is_ascii_digit()) {
        return Err("amount must be numeric string, e.g. \"1250.00\"".to_string());
    }
    let cents_str = if parts.len() == 2 { parts[1] } else { "" };
    if cents_str.len() > 2 {
        return Err("amount has too many decimal places (max 2)".to_string());
    }
    if !cents_str.chars().all(|c| c.is_ascii_digit()) {
        return Err("amount must be numeric string, e.g. \"1250.00\"".to_string());
    }
    let d: u64 = dollars.parse().map_err(|_| "amount too large".to_string())?;
    let c: u64 = if cents_str.is_empty() {
        0
    } else if cents_str.len() == 1 {
        cents_str.parse::<u64>().unwrap() * 10
    } else {
        cents_str.parse::<u64>().unwrap()
    };
    let total = d.checked_mul(100).and_then(|v| v.checked_add(c)).ok_or("amount too large")?;
    if total == 0 {
        return Err("amount must be > 0".to_string());
    }
    Ok(total)
}

pub fn decide(policy: &Policy, req: &ReviewInput) -> Decision {
    let supplier = req.supplier_id.trim().to_string();
    if supplier.is_empty() {
        return Decision { approved: false, reason: "supplier_id is empty".to_string() };
    }
    // Currency check
    if !policy.allowed_currencies.is_empty() && !policy.allowed_currencies.iter().any(|c| c == &req.currency) {
        return Decision { approved: false, reason: alloc::format!("currency {} not allowed", req.currency) };
    }
    if !policy.suppliers.iter().any(|s| s == &supplier) {
        // Do NOT leak allowlist contents
        return Decision { approved: false, reason: alloc::format!("supplier '{}' not in allowlist", supplier) };
    }
    let limit = policy.limits.get(&supplier).copied().unwrap_or(policy.default_limit);
    if req.amount_cents > limit {
        return Decision {
            approved: false,
            reason: alloc::format!("amount {} exceeds per-supplier limit {} for {}", req.amount_cents, limit, supplier),
        };
    }
    Decision { approved: true, reason: "supplier approved and within spending limit".to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy() -> Policy { Policy::default() }

    #[test]
    fn parse_ok() {
        assert_eq!(parse_amount_cents("1250.00").unwrap(), 125000);
        assert_eq!(parse_amount_cents("0.01").unwrap(), 1);
        assert_eq!(parse_amount_cents("100").unwrap(), 10000);
        assert_eq!(parse_amount_cents("1.5").unwrap(), 150);
        assert_eq!(parse_amount_cents("  10.00  ").unwrap(), 1000);
    }
    #[test]
    fn parse_reject() {
        assert!(parse_amount_cents("").is_err());
        assert!(parse_amount_cents("-1").is_err());
        assert!(parse_amount_cents("0").is_err());
        assert!(parse_amount_cents("0.00").is_err());
        assert!(parse_amount_cents("1.234").is_err());
        assert!(parse_amount_cents("abc").is_err());
        assert!(parse_amount_cents("1..00").is_err());
    }
    #[test]
    fn allowlist_hit() {
        let p = policy();
        let d = decide(&p, &ReviewInput{ supplier_id:"acme-corp".into(), amount_cents:10000, currency:"USD".into() });
        assert!(d.approved);
    }
    #[test]
    fn allowlist_miss_no_leak() {
        let p = policy();
        let d = decide(&p, &ReviewInput{ supplier_id:"evil-corp".into(), amount_cents:10000, currency:"USD".into() });
        assert!(!d.approved);
        assert!(!d.reason.contains("acme-corp"));
        assert!(d.reason.contains("evil-corp"));
    }
    #[test]
    fn limit_boundary() {
        let p = policy();
        let at_limit = decide(&p, &ReviewInput{ supplier_id:"acme-corp".into(), amount_cents:500_000, currency:"USD".into() });
        assert!(at_limit.approved);
        let over = decide(&p, &ReviewInput{ supplier_id:"acme-corp".into(), amount_cents:500_001, currency:"USD".into() });
        assert!(!over.approved);
    }
    #[test]
    fn currency_reject() {
        let p = policy();
        let d = decide(&p, &ReviewInput{ supplier_id:"acme-corp".into(), amount_cents:10000, currency:"EUR".into() });
        assert!(!d.approved);
        assert!(d.reason.contains("currency"));
    }
    #[test]
    fn default_limit() {
        let p = policy();
        let d = decide(&p, &ReviewInput{ supplier_id:"globex".into(), amount_cents:300_000, currency:"USD".into() });
        assert!(!d.approved); // globex limit 250k
    }
}
