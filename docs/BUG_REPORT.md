# Bug Report — T3N Sandbox (2026-09-02)

All bugs were hit while building the B2B Procurement Agent against `@terminal3/t3n-sdk@5.7.0` on sandbox (`cn-api.sg.testnet.t3n.terminal3.io`).

## 1. Trust manifest rtmr1 allowlist mismatch — blocks `fetchTrustedManifest()` (CRITICAL)

- **Manifest URL**: `https://cn-api.sg.testnet.t3n.terminal3.io/api/trust-manifest` (also serves sandbox)
- **Returned manifest** (2026-09-02 snapshot):

```json
{
  "cluster": "testnet",
  "version": 1787800421,
  "peer_ids": ["QmPk4AtbFore74fJoP4CoS9Q96TvRvoQWR4VmkYtkBLmwz", "QmQBh7... ", "QmSGy7..."],
  "rtmr3_allowlist": ["+XO6nLsfqnTkX0VcNk9AaXAu79ErxURODtjuGOIF8Sk7OQYq3PVVsMG8jzDEeNJQ"],
  "signed_at": "2026-08-27T03:13:41Z",
  "signature": "387384a9186bd06ab8ce8e2fbb7055ae96202eac4ce57c68a771fba8421be2006dacc087b6549d5919b42db163368da39d693444c4c1e771c2a9281394128e36"
}
```

Missing key: `rtmr1_allowlist`. SDK 5.7 validates that the anchor contains both `rtmr1_allowlist` and `rtmr3_allowlist`; `fetchTrustedManifest()` throws:

```
Error: Trust manifest at https://cn-api.sg.testnet.t3n.terminal3.io/api/trust-manifest is malformed.
```

- **DKG attestation failure when patching**: Manually constructing `{ expected_peer_ids, rtmr3_allowlist: ["+XO6..."], rtmr1_allowlist: ["+XO6..."] }` then handshaking yields:

```
DKG attestation verification failed for https://cn-api.sg.testnet.t3n.terminal3.io: 0/3 quotes valid
(peer ...: RTMR1 not in allowlist, RTMR1 kP0XBuMMdrW4..., RTMR3 +XO6...; ...)
Pinned RTMR1 allow-list: [+XO6...], RTMR3 allow-list: [+XO6...]
```

Live TEE RTMR1 is `kP0XBuMMdrW4...`, not `+XO6...`. Correct `rtmr1_allowlist` should contain `kP0XBuMMdrW4...` (full value was truncated in logs; available from quote dump).

- **Impact**: No fresh checkout can call `fetchTrustedManifest("sandbox"|"testnet")` on 5.7.0 — matches Superteam comment "latest ADK version hasn't been compatible with testnet" (Marcin Adam, 1d ago). Status page shows green.

- **Workaround used for this submission**: `new T3nClient({ trustAnchor:{unsafe_trust_server:true}, ... })` skips attestation and successfully handshakes+authenticates ( DID `did:t3n:0a46b0c38cab1691220abede5d65e633385b6732`, 20B credits). Documented in `client/*.mjs` with TODO to replace once manifest is refreshed. This is the DM's recommended local-mock fallback alternative; `unsafe` is strictly more honest than a full mock because the contract still runs in the real TEE.

- **Expected fix**: Publish a fresh manifest containing both `rtmr1_allowlist: ["kP0XBuMMdrW4..."]` and `rtmr3_allowlist: ["+XO6..."]` signed with the operator key, or temporarily publish an SDK 5.7.1 that treats a missing `rtmr1_allowlist` as non-fatal when the node only advertises rtmr3.

## 2. `TenantContractsNamespace.execute` validates canonical name as tail (LOW)

Calling `tenant.contracts.execute({ contract:"procurement-agent", ... })` or with full `z:<tid>:procurement-agent` throws:

```
Tenant name tail must match /^[a-zA-Z0-9_-][a-zA-Z0-9_.-]{0,127}$/
```

Stack shows `canonicalTenantName` being applied to the `contract` argument, which then applies `validateTail` to the whole canonical string (contains colons). The working path is `t3n.execute({ contract_id: "z:...:procurement-agent", contract_version, function_name, input })` directly on the `T3nClient`. Tenant SDK's contract helper should either accept the canonical ID or validate only the tail segment.

Workaround: use `t3n.execute` for reads; `tenant.contracts.register` works fine.

## 3. Sandbox vs testnet aliasing confusion (INFO)

- Sandbox signup at `https://terminal3.io/products/agent-developer-kit` tells you to `setEnvironment("sandbox")`, but `NODE_URLS["sandbox"] === NODE_URLS["testnet"] === "https://cn-api.sg.testnet.t3n.terminal3.io"`. Both fetch the same testnet manifest and hit bug #1. Docs at `docs.terminal3.io/.../get-started/quickstart` still show `setEnvironment("testnet")`. Unify the naming or make sandbox resolve to a separate cluster when ready.

## 4. `http-with-placeholders` egress requires user grant even for tenant self-test (EXPECTED)

`create-po` correctly fails with `egress denied for host httpbin.org` when called via tenant without a user delegation granting that host. This is the intended TEE policy (host allowlist is per user grant, not per tenant). Not a bug, but worth calling out for new builders — the walkthrough buries the grant step. For a self-contained demo we kept the failure as documented behavior; an agent with a `userClient` grant would succeed.

## Environment

- SDK: `@terminal3/t3n-sdk@5.7.0` (verified via `npm view`), Node 22, Rust 1.88, `wasm32-wasip2`
- Tenant: `did:t3n:0a46b0c38cab1691220abede5d65e633385b6732` (Germany, hermes.agent.grade@gmail.com), package `z-tenant-procurement@0.1.0`, contract id 864
- Screenshots and `demo_final.mjs` output captured 2026-09-02 (see Google Doc).

Reported to: Superteam listing comment, Telegram @wardumb DM (will send DID), and this file.
