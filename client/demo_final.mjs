import { T3nClient, TenantClient, setEnvironment, loadWasmComponent, eth_get_address, metamask_sign, createEthAuthInput, getNodeUrl, getContractVersion } from "@terminal3/t3n-sdk";
setEnvironment("sandbox");
const key = process.env.T3N_API_KEY;
if (!key) { console.error("Set T3N_API_KEY env var"); process.exit(1); }
const wasmComponent = await loadWasmComponent();
const addr = eth_get_address(key);
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
const t3n = new T3nClient({ trustAnchor, wasmComponent, handlers:{EthSign: metamask_sign(addr, undefined, key)}});
await t3n.handshake();
const did = await t3n.authenticate(createEthAuthInput(addr));
const tenantDid = did.value;
const scriptName = `z:${tenantDid.slice("did:t3n:".length)}:procurement-agent`;
const ver = await getContractVersion(getNodeUrl(), scriptName);
console.log("Contract", scriptName, "ver", ver);

async function call(func, inputObj){
  const res = await t3n.execute({ contract_id: scriptName, contract_version: ver, function_name: func, input: inputObj });
  // res is string JSON or object
  let parsed = typeof res === "string" ? JSON.parse(res) : res;
  // SDK may wrap? Check shape
  if (parsed && typeof parsed === "object" && parsed.value) parsed = parsed.value;
  console.log(`\n${func} input=${JSON.stringify(inputObj)} ->`, JSON.stringify(parsed, null, 2));
  return parsed;
}

await call("review-supplier", { supplier_id:"acme-corp", sku:"SKU-001", quantity:2, amount:"1200.00", currency:"USD"});
await call("review-supplier", { supplier_id:"evil-corp", sku:"SKU-001", quantity:1, amount:"100.00", currency:"USD"});
await call("review-supplier", { supplier_id:"acme-corp", sku:"SKU-999", quantity:1, amount:"99999.00", currency:"USD"});
// create-po via tenant (will demonstrate placeholder flow but without user profile, will show placeholder error handling)
// This should succeed to httpbin call but with placeholder fields unresolved (since no user profile delegation)
// However our contract uses placeholders that will be resolved host-side; without delegation it may fail with placeholder error - we catch it.

try {
  await call("create-po", { supplier_id:"acme-corp", sku:"SKU-001", quantity:5, amount:"1200.00", currency:"USD"});
} catch(e){
  console.log("create-po error (expected without user delegation):", e.message.slice(0,800));
  // try to show underlying
  console.log(e);
}
