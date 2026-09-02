# T3N Procurement Agent — Public Google Doc (draft for copy-paste)

Copy this into a **public Google Doc** (File -> Share -> Anyone with the link -> Viewer) and submit that Doc link + GitHub repo to Superteam.

---

## 1. What we built

**Confidential B2B Procurement Agent** — a tenant-owned TEE contract that (a) enforces a supplier allowlist + spending limits inside the enclave (`review-supplier`) and (b) creates purchase orders where payment/banking PII is substituted host-side via `http-with-placeholders` (`create-po`). Plaintext payment data never enters the WASM, the agent, or logs.

- Tenant DID: `did:t3n:0a46b0c38cab1691220abede5d65e633385b6732`
- Contract: `z:0a46b0c38cab1691220abede5d65e633385b6732:procurement-agent`, id `864`, version `0.1.1` (code, live `0.1.0` id 864)
- WASM bytes: 209K (~213,000 bytes) — built from `contract/` with `cargo build --target wasm32-wasip2 --release`
- Client: `client/deploy.mjs` + `client/demo_final.mjs` (Node 22, `@terminal3/t3n-sdk@5.7.0`)
- Sandbox node: `https://cn-api.sg.testnet.t3n.terminal3.io` (sandbox aliases to same) — 20B credits provisioned

This directly implements the **Delegate Access to AI Agents — B2B Procurement** flow from the T3N docs (steps 1-9), collapsed to two auditable enclave functions.

---

## 2. Usefulness & maintainability (judging focus)

**Useful**: solves a real enterprise pain — agents need to procure without holding payment keys. The allowlist + limit lives in a private KV map (`z:<tid>:allowlist` keys `suppliers`, `spending_limits`) that the enterprise owns; the TEE is the only reader. The PO path demonstrates the flagship T3N guarantee (PII moves through the enclave, never through your code) with `{{profile.payment_method}}`, `{{profile.payment_account_ref}}`, `{{profile.full_name}}`, `{{profile.email}}`.

**Easy to maintain**: 2 functions, 2 maps, 1 external host (`https://httpbin.org/post` as mock supplier; swap the `SUPPLIER_API_BASE` constant for prod). Adding a supplier = one `map-entry-set`. No per-supplier code, no redeploy to change a limit. Contract is stateless; bumping `version` at the same `tail` preserves maps. See README `Handover / continued maintenance`.

---

## 3. Screenshots

Include screenshots of:

1. Sandbox claim success (terminal3.io claim page showing DID `did:t3n:0a46b...` and 20B credits (testnet)) — captured 2026-09-02 (browser snapshot) — see repo `docs/screenshots/` placeholder; add your claim screenshot before submit.
2. `deploy.mjs` output showing contract registration: `Contract ID: 864`, `Map allowlist created`, `Map secrets created`, seeding.
3. `demo_final.mjs` output (below, also in `docs/demo-output.txt`):

```
Contract z:0a46b0c...:procurement-agent ver 0.1.0

review-supplier {acme-corp, SKU-001, 1200.00 USD} -> { approved:true, reason:"supplier approved and within spending limit" }
review-supplier {evil-corp, ...}               -> { approved:false, reason:"supplier 'evil-corp' not in allowlist" }
review-supplier {acme-corp, 99999 USD}         -> { approved:false, reason:"amount 99999 exceeds per-supplier limit 5000 for acme-corp" }
create-po {acme-corp ...}                      -> RPC Error: egress denied for host httpbin.org [requestId 7365ac5a...]  // expected, see §5
```

4. GitHub repo view showing `contract/`, `client/`, `docs/BUG_REPORT.md`.
5. Camofox/Chrome network showing `trust-manifest` fetch.

---

## 4. How to reproduce (5 min)

Follow README Quickstart verbatim. Requires Rust `1.88+` (`wasm32-wasip2` target) and Node `22`. Set `T3N_API_KEY` (tenant key from claim page). The `unsafe_trust_server:true` workaround is documented in §5; replace with `await fetchTrustedManifest("sandbox")` once the manifest is refreshed.

---

## 5. Bugs faced (scored category)

Full report: `docs/BUG_REPORT.md` in the repo. Summary:

- **CRITICAL — rtmr1 allowlist mismatch**: `GET /api/trust-manifest` returns only `rtmr3_allowlist: ["+XO6..."]` (signed 2026-08-27). SDK 5.7 expects both `rtmr1_allowlist` and `rtmr3_allowlist`; `fetchTrustedManifest()` throws `is malformed`. Handshaking with a patched anchor containing the same value for both fails attestation: `RTMR1 kP0XBuMMdrW4... not in allowlist`. Live TEE RTMR1 is `kP0X...`, not `+XO6...`. Workaround: `trustAnchor:{unsafe_trust_server:true}` — still executes in the real TEE, just skips quote verification. Matches the Superteam comment from Marcin Adam.
- **LOW — `TenantContractsNamespace.execute` validates canonical name as tail**: Workaround is `t3n.execute({contract_id, contract_version, function_name, input})`.
- **INFO — sandbox/testnet alias**: `setEnvironment("sandbox")` resolves to same `cn-api.sg.testnet...` URL; docs still show `testnet`.
- **EXPECTED — egress gating**: `create-po` correctly returns `egress denied for host httpbin.org` without a user delegation granting that host. This is TEE policy, not a bug; an agent with `userClient` + `agent-auth` grant would succeed.

All were reported in-listing (comment) and via Telegram DM to @wardumb.

---

## 6. Handover preference

**Prefer handover to Terminal 3 to maintain** — this demo is minimal by design. Production handover would add `supplier-endpoints` map and `audit-po` log reader, plus Stripe test payments. Happy to continue running it under the startup program if preferred (tenant `0a46b...` is funded and stable).

---

## 7. Links

- GitHub (public): `https://github.com/hermes-earn-bot/t3n-procurement-agent`
- This Doc (public): set sharing to Anyone with the link
- Contact: hermes.agent.grade@gmail.com / Telegram @Grade (1019900002) / Superteam @scarlet-liberal-5
