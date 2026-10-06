// Host-owned bootstrap. The session and origin are replaced by Rust before navigation.
(() => {
  const sessionId = '__AIDE_SESSION__';
  const endpoint = '__AIDE_ENDPOINT__';
  const request = window.fetch.bind(window);
  let sequence = 0;
  let disposed = false;
  const listeners = new Set();
  let current = null;
  let pollRunning = false;

  const pending = new Set();
  async function rpc(method, params = {}) {
    if (disposed) throw new Error('SESSION_CLOSED');
    const requestId=String(++sequence);
    const controller = new AbortController(); pending.add(controller);
    const timer = setTimeout(() => controller.abort(), 6000);
    try {
      const response = await request(endpoint, {method:'POST', headers:{'Content-Type':'application/json'}, body:JSON.stringify({apiVersion:'1',sessionId,requestId,method,params}), signal:controller.signal});
      const result = await response.json();
      if(result.apiVersion!=='1'||result.sessionId!==sessionId||result.requestId!==requestId)throw new Error('INVALID_REPLY');
      if (!result.ok) throw Object.assign(new Error(result.error.message), result.error);
      return result;
    } finally { clearTimeout(timer); pending.delete(controller); }
  }
  const call = async (method, params) => (await rpc(method,params)).data;
  function accept(reply) {
    if(current && reply.instanceId===current.instanceId && reply.revision<current.revision)return false;
    const changed=!current||reply.instanceId!==current.instanceId||reply.revision!==current.revision;
    // Every event carries a full authoritative snapshot. Sequence gaps or host restarts
    // therefore resynchronize immediately without applying partial or stale deltas.
    current=reply;
    if(changed)listeners.forEach(fn=>{try{fn(reply.data);}catch{}});
    return changed;
  }
  async function sync() {
    if(pollRunning||disposed)return;pollRunning=true;
    try {const reply=await rpc('usage.get');if(Number.isSafeInteger(reply.sequence))accept(reply);}
    catch { /* Keep prior values until an authoritative snapshot is available. */ }
    finally {pollRunning=false;}
  }
  const aide = Object.freeze({
    accounts:Object.freeze({list:()=>call('accounts.list'), refresh:(params={})=>call('accounts.refresh',params)}),
    usage:Object.freeze({get:()=>call('usage.get'),onChanged:async listener=>{const reply=await rpc('usage.get');accept(reply);listeners.add(listener);listener(current.data);return()=>listeners.delete(listener);}}),
    window:Object.freeze({resize:(width,height)=>call('window.resize',{width,height}),drag:()=>call('window.drag'),hide:()=>call('window.hide')}),
    storage:Object.freeze({get:key=>call('storage.get',{key}),set:(key,value)=>call('storage.set',{key,value}),remove:key=>call('storage.remove',{key})}),
    app:Object.freeze({openSettings:()=>call('app.openSettings')}),
    theme:Object.freeze({ready:()=>call('theme.ready')})
  });
  Object.defineProperty(window,'aide',{value:aide,writable:false,configurable:false});
  // No child frames or workers are allowed by host CSP. Remove browser transports
  // which are not governed by connect-src (notably WebRTC) before theme code runs.
  for(const key of ['RTCPeerConnection','webkitRTCPeerConnection','WebTransport','Worker','SharedWorker']) {
    try {Object.defineProperty(window,key,{value:undefined,writable:false,configurable:false});}catch{}
  }
  window.addEventListener('contextmenu', event=>{event.preventDefault();void call('window.menu').catch(()=>{});},true);
  window.addEventListener('dblclick', event=>{event.preventDefault();event.stopImmediatePropagation();},true);
  window.addEventListener('error',()=>{void call('theme.failed').catch(()=>{});});
  window.addEventListener('unhandledrejection',()=>{void call('theme.failed').catch(()=>{});});
  const timer=setInterval(sync,1000);
  window.addEventListener('pagehide',()=>{disposed=true;clearInterval(timer);listeners.clear();pending.forEach(c=>c.abort());},{once:true});
})();
