# z-tenant-procurement — Confidential B2B Procurement Agent

TEE contract for Terminal 3 Network that enforces supplier allowlist + per-supplier spending limits inside the enclave, then creates purchase orders where payment PII is substituted host-side via `http-with-placeholders`.

## Functions

| Function | What it does |
|---|---|
| `review-supplier` | Checks supplier against allowlist + amount against per-supplier limit (in cents, with currency check). Fail-closed if policy KV is missing. Uses shared `policy.rs` logic. |
| `create-po` | Re-validates policy (same allowlist/limit check) then POSTs to supplier API via `http-with-placeholders`. Payment fields `{{profile.payment_method}}`, `{{profile.payment_account_ref}}`, `{{profile.full_name}}`, `{{profile.email}}` are resolved host-side — plaintext never enters WASM/agent/logs. |

Both functions validate `quantity > 0`, non-empty `sku`/`supplier_id`, amount parsing (cents, max 2 decimals), and currency (`USD` only).

## KV Maps

- `z:<tid>:allowlist` — keys: `suppliers` (JSON array), `spending_limits` (JSON object `{supplier: limit_dollars}` or cents string, plus `__default`), `policy_version` (string, default `v1`)
- `z:<tid>:secrets` — key `supplier_api_key` (optional Bearer token)

Policy lives in KV; adding a supplier = one `map-entry-set`, no redeploy.

## Host capabilities

```json
{ "host_capabilities": ["kv_store", "logging", "tenant_context", "http-with-placeholders"] }
```

## Building

```bash
rustup target add wasm32-wasip2
cargo build --target wasm32-wasip2 --release
# output: target/wasm32-wasip2/release/z_tenant_procurement.wasm ( ~220KB, lto, strip)
cargo test # native tests for policy.rs pure logic
```

## Security notes

- Denied responses do NOT leak allowlist contents.
- Fail-closed: missing/invalid KV → deny.
- Amounts in cents (u64), no f64.
- `create-po` enforces policy before any egress.

