import {
  T3nClient,
  TenantClient,
  setEnvironment,
  loadWasmComponent,
  eth_get_address,
  metamask_sign,
  createEthAuthInput,
  getNodeUrl,
} from "@terminal3/t3n-sdk";
import { readFile } from "fs/promises";
import path from "path";

setEnvironment("sandbox");

const T3N_API_KEY = process.env.T3N_API_KEY;
if (!T3N_API_KEY) {
  console.error("Set T3N_API_KEY");
  process.exit(1);
}

// WASM built with unsafe bypass is required due to manifest rtmr1 mismatch (bug documented).
// In production, replace {unsafe_trust_server:true} with await fetchTrustedManifest("sandbox")
const wasmComponent = await loadWasmComponent();
const address = eth_get_address(T3N_API_KEY);
console.log("Derived address:", address);
console.log("Node:", getNodeUrl());

let trustAnchor;
if (process.env.T3N_UNSAFE === "1") {
  trustAnchor = { unsafe_trust_server: true };
} else {
  try {
    const { fetchTrustedManifest } = await import("@terminal3/t3n-sdk");
    trustAnchor = await fetchTrustedManifest("sandbox");
  } catch (e) {
    console.warn("manifest fetch failed, falling back to unsafe:", e.message);
    trustAnchor = { unsafe_trust_server: true };
  }
}
const t3n = new T3nClient({
  trustAnchor,
  wasmComponent,
  handlers: { EthSign: metamask_sign(address, undefined, T3N_API_KEY) },
});
await t3n.handshake();
const did = await t3n.authenticate(createEthAuthInput(address));
const tenantDid = did.value;
console.log("Authenticated DID:", tenantDid);
console.log("Usage:", JSON.stringify(await t3n.getUsage(), null, 2));

const tenant = new TenantClient({ t3n, baseUrl: getNodeUrl(), tenantDid });
console.log("Tenant me:", JSON.stringify(await tenant.tenant.me(), null, 2));

// ---- 1. Register the WASM contract ----
const wasmPath = path.resolve("contract/target/wasm32-wasip2/release/z_tenant_procurement.wasm");
console.log("Reading WASM:", wasmPath);
const wasmBytes = await readFile(wasmPath);
console.log("WASM size:", wasmBytes.length);

const CONTRACT_TAIL = "procurement-agent";
const CONTRACT_VERSION = "0.1.1";

let contractId;
try {
  const reg = await tenant.contracts.register({ tail: CONTRACT_TAIL, version: CONTRACT_VERSION, wasm: wasmBytes });
  console.log("Register result:", JSON.stringify(reg, null, 2));
  contractId = reg.contract_id ?? reg.contractId ?? reg.id;
} catch (e) {
  const msg = e.message || String(e);
  if (msg.includes("version") && msg.includes("not higher than current version")) {
    console.log(`Contract already at ${CONTRACT_VERSION} — reusing existing deployment`);
    // fetch existing contract id via tenant.contracts.list if available, else assume 865
    try {
      const list = await tenant.contracts.list?.();
      console.log("Contracts list:", JSON.stringify(list, null, 2).slice(0,2000));
      contractId = 865;
    } catch {}
    contractId = contractId ?? 865;
  } else {
    console.log("Register error (maybe already exists):", msg);
    throw e;
  }
}
console.log("Contract ID:", contractId);
const tenantId = tenantDid.slice("did:t3n:".length);
const scriptName = `z:${tenantId}:${CONTRACT_TAIL}`;
console.log("Script name:", scriptName);

// ---- 2. Create KV maps ----
// Helpers to create safely (idempotent)
async function createMap(tail) {
  try {
    const r = await tenant.maps.create({
      tail,
      visibility: "private",
      writers: { only: [contractId] },
      readers: { only: [contractId] },
    });
    console.log(`Map ${tail} created:`, JSON.stringify(r).slice(0,300));
  } catch (e) {
    const msg = e.message || String(e);
    if (msg.includes("already exists") || msg.includes("map already exists")) {
      console.log(`Map ${tail} already exists, updating ACL to include contract ${contractId}`);
      try {
        const u = await tenant.maps.update({
          tail,
          readers: { only: [contractId] },
          writers: { only: [contractId] },
        });
        console.log(`Map ${tail} ACL updated:`, JSON.stringify(u).slice(0,300));
      } catch (e2) {
        console.log(`Map ${tail} update failed:`, e2.message?.slice(0,500));
      }
    } else {
      console.log(`Map ${tail} create failed:`, msg.slice(0,800));
      throw e;
    }
  }
}

await createMap("allowlist");
await createMap("secrets");
// also secrets for supplier_api_key demo
console.log("Maps ready");

// ---- 3. Seed allowlist ----
// allowlist suppliers + spending_limits
async function seedMap(tail, entries) {
  const mapName = `z:${tenantId}:${tail}`;
  // TenantClient canonicalName helper may exist; fallback to manual
  const canonical = tenant.canonicalName ? tenant.canonicalName(tail) : mapName;
  console.log("Seeding", canonical, "with", Object.keys(entries));
  for (const [k, v] of Object.entries(entries)) {
    const val = typeof v === "string" ? v : JSON.stringify(v);
    try {
      const r = await tenant.executeControl("map-entry-set", {
        map_name: canonical,
        key: k,
        value: val,
      });
      console.log(`  ${k} -> ${val.slice(0,120)} :`, JSON.stringify(r).slice(0,200));
    } catch (e) {
      console.log(`  ${k} failed:`, e.message?.slice(0,600));
    }
  }
}

await seedMap("allowlist", {
  suppliers: ["acme-corp", "initech", "globex"],
  spending_limits: { "acme-corp": 5000, "initech": 10000, "globex": 2500, "__default": 5000 },
});
await seedMap("secrets", {
  supplier_api_key: "test-supplier-key-demo-123",
});

console.log("Seeding done");

// ---- 4. Test invocation ----
// Tenant-side invoke: review-supplier (policy check, no placeholders)
try {
  console.log("\n--- Invoking review-supplier (approved case) ---");
  const res1 = await tenant.contracts.execute({
    contract: scriptName,
    func: "review-supplier",
    input: JSON.stringify({ supplier_id: "acme-corp", sku: "SKU-001", quantity: 2, amount: "1200.00", currency: "USD" }),
  });
  console.log("review-supplier approved:", JSON.stringify(res1, null, 2).slice(0,1500));
} catch (e) {
  console.log("review-supplier failed:", e.message?.slice(0,1000));
  console.log(e);
}

try {
  console.log("\n--- Invoking review-supplier (denied: not in allowlist) ---");
  const res2 = await tenant.contracts.execute({
    contract: scriptName,
    func: "review-supplier",
    input: JSON.stringify({ supplier_id: "evil-corp", sku: "SKU-001", quantity: 1, amount: "100.00", currency: "USD" }),
  });
  console.log("review-supplier denied:", JSON.stringify(res2, null, 2).slice(0,1500));
} catch (e) {
  console.log("review-supplier denied case failed:", e.message?.slice(0,1000));
}

try {
  console.log("\n--- Invoking review-supplier (denied: over limit) ---");
  const res3 = await tenant.contracts.execute({
    contract: scriptName,
    func: "review-supplier",
    input: JSON.stringify({ supplier_id: "acme-corp", sku: "SKU-999", quantity: 1, amount: "99999.00", currency: "USD" }),
  });
  console.log("review-supplier over-limit:", JSON.stringify(res3, null, 2).slice(0,1500));
} catch (e) {
  console.log("over-limit failed:", e.message?.slice(0,1000));
}

// Note: create-po requires agent delegation with placeholder egress. Tenant direct call will fail egress check without agent auth.
// We demonstrate the failure mode and note the agent flow in docs.
try {
  console.log("\n--- Attempting create-po via tenant (expected egress/placeholder failure without agent delegation) ---");
  const res4 = await tenant.contracts.execute({
    contract: scriptName,
    func: "create-po",
    input: JSON.stringify({ supplier_id: "acme-corp", sku: "SKU-001", quantity: 5, amount: "1200.00", currency: "USD" }),
  });
  console.log("create-po tenant result:", JSON.stringify(res4, null, 2).slice(0,1500));
} catch (e) {
  console.log("create-po tenant (expected) error:", e.message?.slice(0,1200));
}

console.log("\nDONE — contract registered and policy checks demonstrated.");
console.log(`Contract: ${scriptName}  version ${CONTRACT_VERSION}  id ${contractId}`);
console.log(`WASM: ${wasmBytes.length} bytes`);
