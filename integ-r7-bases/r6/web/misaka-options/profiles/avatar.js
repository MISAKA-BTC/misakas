/* misaka-identicon:v1: synchronous, non-cryptographic visual identifier ONLY.
 * Never use the pattern as an ownership/authentication proof. No storage/network. */
export function avatarNetwork(config = globalThis.MISAKA_CONFIG) {
  return `${config?.NETWORK_NAME || 'unconfigured'}:eip155:${String(config?.CHAIN_ID || 'unknown').toLowerCase()}`;
}
function hash128(seed) {
  let a=1779033703,b=3144134277,c=1013904242,d=2773480762;
  for (let i=0;i<seed.length;i++) {
    const k=seed.charCodeAt(i);
    a=b^Math.imul(a^k,597399067); b=c^Math.imul(b^k,2869860233);
    c=d^Math.imul(c^k,951274213); d=a^Math.imul(d^k,2716044179);
  }
  a=Math.imul(c^(a>>>18),597399067); b=Math.imul(d^(b>>>22),2869860233);
  c=Math.imul(a^(c>>>17),951274213); d=Math.imul(b^(d>>>19),2716044179);
  return [(a^b^c^d)>>>0,(b^a)>>>0,(c^a)>>>0,(d^a)>>>0];
}
export function avatarSvg(identity = 'misaka:local-device', {namespace='evm',network=avatarNetwork()} = {}) {
  let [a,b,c,d]=hash128(`misaka-identicon:v1:${namespace}:${network}:${String(identity).trim().toLowerCase()}`);
  const random=()=>{
    a>>>=0; b>>>=0; c>>>=0; d>>>=0;
    let t=(a+b)|0; a=b^(b>>>9); b=(c+(c<<3))|0; c=(c<<21)|(c>>>11);
    d=(d+1)|0; t=(t+d)|0; c=(c+t)|0; return (t>>>0)/4294967296;
  };
  const hue=Math.floor(random()*360);
  let path = '';
  for (let y = 0; y < 5; y++) for (let x = 0; x < 3; x++) {
    if (random() > 0.5) {
      path += `M${x+1} ${y+1}h1v1h-1z`;
      if (x !== 2) path += `M${5-x} ${y+1}h1v1h-1z`;
    }
  }
  return `<svg viewBox="0 0 7 7" aria-hidden="true" shape-rendering="crispEdges" xmlns="http://www.w3.org/2000/svg"><rect width="7" height="7" fill="hsl(${hue},35%,94%)"/><path fill="hsl(${hue},58%,37%)" d="${path || 'M3 3h1v1h-1z'}"/></svg>`;
}
