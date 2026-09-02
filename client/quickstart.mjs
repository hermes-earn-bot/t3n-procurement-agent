import { T3nClient, setEnvironment, loadWasmComponent, eth_get_address, metamask_sign, createEthAuthInput, getNodeUrl } from "@terminal3/t3n-sdk";
setEnvironment("sandbox");
const key = process.env.T3N_API_KEY;
if(!key){ console.error("Set T3N_API_KEY"); process.exit(1); }
const wasmComponent = await loadWasmComponent();
const addr = eth_get_address(key);
console.log("Node", getNodeUrl());
const t3n = new T3nClient({ trustAnchor:{unsafe_trust_server:true}, wasmComponent, handlers:{EthSign: metamask_sign(addr, undefined, key)}});
await t3n.handshake();
const did = await t3n.authenticate(createEthAuthInput(addr));
console.log("Connected as", did.value);
console.log("Balance", await t3n.getUsage());
