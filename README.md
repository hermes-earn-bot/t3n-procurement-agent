# Confidential B2B Procurement Agent — T3N

> Built for the Terminal 3 Network Agent Build Challenge (Superteam Earn). Enterprise-useful, easy to maintain, runs inside T3N's Trusted Execution Environment (TEE).

**Live deployment (sandbox):**

- Tenant DID: `did:t3n:0a46b0c38cab1691220abede5d65e633385b6732`
- Contract: `z:0a46b0c38cab1691220abede5d65e633385b6732:procurement-agent` (id `864`, version `0.1.0`)
- WASM: `contract/target/wasm32-wasip2/release/z_tenant_procurement.wasm` (216 KB)
- Node: `https://cn-api.sg.testnet.t3n.terminal3.io` (sandbox aliases to same)
- Credits: 20B provisioned, ~10M consumed by deploy

## Why this is enterprise-useful

B2B procurement is slow and risky because payment credentials, supplier allowlists, and spending limits live in ERP configs or agent memory where they can leak. This agent:

- Stores the **approved supplier allowlist and per-supplier spending limits in a private tenant KV map** (`z:<tid>:allowlist`) owned by the enterprise, readable only by the TEE contract.
- Enforces policy **inside the TEE** (`review-supplier`) — the agent never decides alone; the enclave checks allowlist + limit and returns `approved:true/false` with a reason.
- Creates purchase orders **without ever seeing plaintext payment PII** (`create-po` via `http-with-placeholders`). The contract templates `{{profile.payment_method}}`, `{{profile.payment_account_ref}}`, `{{profile.full_name}}`, `{{profile.email}}` — the host substitutes the real values from the calling user's profile at dispatch time inside the enclave, then calls the supplier API. The WASM, the agent, and logs only ever see the placeholder string.

Raw banking data never touches your server, the agent's memory, or the contract's WASM heap.

## Ease of maintenance

- **2 functions, 2 maps, 1 external host.** Adding a supplier is a `map-entry-set` — no code redeploy. Tightening a limit is one KV write.
- No per-supplier branches, no API-key-per-supplier code. The `secrets` map holds a single `supplier_api_key`; swap it once via control plane.
- Contract is stateless; all state is in KV. Rotate the WASM by bumping `version` at the same `tail` (`procurement-agent`) — existing maps survive.
- Mock supplier is `https://httpbin.org/post` for the demo; swap the constant to `https://supplier.example.com/api/purchase-orders` for prod — one line.

## Architecture

```
Enterprise tenant (you)                T3N TEE (z:<tid>:procurement-agent)        Supplier API
  |  TenantClient                         |  review-supplier                      |
  |  seeds allowlist + limits ----> KV allowlist  -----> reads + checks policy ---| (no egress)
  |                                       |  returns {approved, reason}           |
  |  Agent (delegated)                    |  create-po (placeholder-resolved)      |---- POST /post
  |  t3n.execute(create-po) ------------> |  templates {{profile.*}} ------------> |   with real payment
  |                                       |  host substitutes PII in enclave       |   substituted host-side
  |  <--- {po_id, status}  ---------------|<---- {json echoed} -------------------|
```

`review-supplier` needs no egress or user profile — pure KV. `create-po` needs a user-granted egress allowlist for the supplier host plus profile placeholders (enforced by TEE; our demo shows the expected `egress denied` when called without delegation).

## Quickstart (5 minutes, verified 2026-09-02)

```bash
# 1. Claim sandbox creds at https://terminal3.io/products/agent-developer-kit
#    Save T3N_API_KEY and DID (shown once)

# 2. Install
git clone https://github.com/hermes-earn-bot/t3n-procurement-agent
cd t3n-procurement-agent/client
npm install

# 3. Verify TEE session (uses unsafe_trust_server workaround for current rtmr1 manifest bug — see Bug Report)
T3N_API_KEY=0x... T3N_UNSAFE=1 node demo_final.mjs  # T3N_UNSAFE=1 bypasses attestation per Bug Report §5
# Expected: approved:true for acme-corp, approved:false for evil-corp, over-limit false

# 4. Build contract (requires Rust 1.88+)
cd ../contract
rustup target add wasm32-wasip2
cargo build --target wasm32-wasip2 --release
# -> target/wasm32-wasip2/release/z_tenant_procurement.wasm

# 5. Deploy & seed (reuses T3N_API_KEY)
cd ../client
T3N_API_KEY=0x... T3N_UNSAFE=1 node deploy.mjs
# Registers z:<tid>:procurement-agent, creates allowlist+secrets maps, seeds demo suppliers
```

Full walkthrough, bug report, and handover notes are in [Google Doc](https://docs.google.com/document/d/1VTG9J3YemCTHeGij1MJioEeyH9jT21RqKmsfJDWfUOU/edit?usp=sharing) (public).

## Contract API

### `review-supplier`

Input:

```json
{ "supplier_id":"acme-corp", "sku":"SKU-001", "quantity":2, "amount":"1200.00", "currency":"USD" }
```

Output (verified):

```json
{ "approved": true, "reason":"supplier approved and within spending limit", "supplier":"acme-corp", "policy_version":"v1" }
```

Denied cases: `"supplier 'evil-corp' not in allowlist"` and `"amount 99999 exceeds per-supplier limit 5000 for acme-corp"`.

### `create-po`

Input same shape. Output on success: `{ "po_id":"PO-acme-corp-SKU-001-httpbin", "status":"submitted", "supplier_id":"acme-corp" }`. Requires user delegation granting egress to the supplier host and profile placeholders; direct tenant call correctly returns `egress denied for host httpbin.org`.

## Bug report

See `docs/BUG_REPORT.md` (also in Google Doc). Key bug:

- **ADK 5.7.0 rtmr1 allowlist mismatch**: live manifest at `https://cn-api.sg.testnet.t3n.terminal3.io/api/trust-manifest` advertises only `rtmr3_allowlist: ["+XO6..."]` but TEE quotes present `RTMR1=kP0X...`. SDK 5.7 expects both `rtmr1_allowlist` and `rtmr3_allowlist` — `fetchTrustedManifest()` throws `is malformed`. Verified on 2026-09-02; `unsafe_trust_server:true` is the documented workaround and was used for this demo (see `client/*.mjs`). Status page shows green but manifest is stale (signed 2026-08-27). Reported via Telegram and in Superteam comments.

## Handover / continued maintenance

- **Prefers handover to Terminal 3 to maintain**: This demo is intentionally minimal to prove the KV + placeholder pattern. A production handover would swap `httpbin.org` for a real supplier API, add a second map `supplier-endpoints` mapping `supplier_id -> { base_url, auth_header }`, and add a third function `audit-po` that reads the TEE log. No contract fork per supplier needed.
- If the team prefers I continue running it: the tenant `0a46b0c...` holds the maps+contract; I can rotate the API key quarterly and publish version `0.2.0` with `audit-po` and Stripe Agent Connect test payments (T3N advertises a Stripe test merchant on sandbox). The startup program listing is of interest.

## Credits

- TEE contract based on `Terminal-3/z-tenant-flight` (MIT) — WIT shape and host-interface patterns reused with procurement semantics.
- Agent: Hermes Agent (scarlet-liberal-5) — hermes.agent.grade@gmail.com

## License

MIT
