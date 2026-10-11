import {addPick, editPick, deletePick, draftForTab, recipient, deliverDraft} from './cards.js';
import {reviewAgents} from './reviewAgents.js';
import {connectNative, nativeRequest} from './native.js';
import {HEARTBEAT_MS, heartbeatReply} from './heartbeat.js';
let connection = 'Connecting…';
let bound;
let agents = [];
let servers = {servers:[],canStart:false};
let lastRefresh = 0;
let lastMode;
let armed;
async function refreshConnection() {
  if (Date.now() - lastRefresh < 5000) return;
  lastRefresh = Date.now();
  try {
    const hello = await request('hello');
    agents = reviewAgents(await request('list_agents'), bound.repoPath);
    connection = `Connected · ${hello.endpoint}`;
    try {
      const modes=await request('mode_status');
      const mode=modes.find((mode)=>mode.repoPath===bound.repoPath);
      if(mode?.status==='stopped' && lastMode!=='stopped') {
        for(const tab of await chrome.tabs.query({})) if(tab.id) chrome.tabs.sendMessage(tab.id,{kind:'stop'}).catch(()=>{});
      }
      lastMode=mode?.status;
      // TUIC is the authority on which session started Design Mode; binding.json is only the install-time copy.
      armed=mode?.status==='armed'?mode.sessionId:undefined;
    } catch(error) { connection=`${connection} · Status unavailable: ${error.message}`; }
    try { servers=await request('list_dev_servers'); }
    catch(error) { servers={servers:[],canStart:false,scanNotice:error.message}; }
  } catch (error) { agents = []; connection = error.message; }
}
let reviewedTab;
let beating = false;
async function beat() {
  if (beating) return;
  beating = true;
  try {
    const reply = await request('heartbeat');
    await heartbeatReply(reply, chrome.storage.session, chrome.tabs, reviewedTab);
  } catch { /* The backend expires the mode when beats stop. Retry reload on the next beat. */ }
  finally { beating = false; }
}
setInterval(beat, HEARTBEAT_MS);
let serial = Promise.resolve();
const binding = fetch(chrome.runtime.getURL('binding.json')).then((response) => {
  if (!response.ok) throw new Error('Open Design Mode from TUICommander to bind this profile.');
  return response.json();
}).then((value) => { bound = value; return value; });
// Observe rejection even when no panel is open.
binding.catch((error) => { connection = error.message; });
chrome.sidePanel.setPanelBehavior({openPanelOnActionClick: true});
const notify = () => chrome.runtime.sendMessage({kind: 'changed'}).catch(() => {});
async function request(action, fields = {}) {
  const config = await binding;
  connectNative((error) => { connection = error; notify(); });
  return nativeRequest({action, repoPath: config.repoPath, version: '2.0.0', ...fields});
}
async function drafts() { return (await chrome.storage.local.get('drafts')).drafts ?? {}; }
async function mutate(tabId, change) {
  const all = await drafts();
  all[String(tabId)] = change(draftForTab(all, tabId));
  await chrome.storage.local.set({drafts: all}); notify();
}
async function activeTab() {
  const [tab] = await chrome.tabs.query({active: true, lastFocusedWindow: true});
  if (!tab?.id) throw new Error('Select a web page first.');
  return tab;
}
async function snapshot(tabId) {
  reviewedTab = tabId;
  await chrome.storage.session.set({reviewedTab: tabId});
  const all = await drafts();
  const draft = draftForTab(all, tabId);
  const selection = connection.startsWith('Connected') ? recipient(draft, agents, armed ?? bound?.sessionId) : {};
  if (Object.keys(selection).some((key) => draft[key] !== selection[key])) { all[String(tabId)] = {...draft,...selection}; await chrome.storage.local.set({drafts:all}); }
  return {tabId, ...draft, ...selection, repoPath: bound?.repoPath, connection, agents, servers, armedSessionId: armed ?? bound?.sessionId};
}
async function capture(message, sender) {
  if (sender.frameId !== 0) return {captureError: 'Frame screenshot omitted; DOM evidence retained.'};
  const tab = await chrome.tabs.get(sender.tab.id);
  if (!tab.active || tab.url !== message.documentUrl) return {captureError: 'Page changed before capture.'};
  const r = message.raw.rect;
  if (![r.x,r.y,r.width,r.height].every(Number.isFinite) || r.width <= 0 || r.height <= 0) return {};
  try {
    const data = await chrome.tabs.captureVisibleTab(tab.windowId, {format: 'png'});
    const current = await chrome.tabs.get(tab.id);
    if (!current.active || current.url !== tab.url) return {captureError: 'Page changed during capture.'};
    const bitmap = await createImageBitmap(await (await fetch(data)).blob());
    const ratio = bitmap.width / message.viewport.width;
    const x = Math.max(0, r.x * ratio), y = Math.max(0, r.y * ratio);
    const width = Math.min(bitmap.width - x, r.width * ratio), height = Math.min(bitmap.height - y, r.height * ratio);
    if (width < 1 || height < 1) { bitmap.close(); return {}; }
    const scale = Math.min(1, 800 / width, 600 / height);
    const canvas = new OffscreenCanvas(Math.max(1, Math.round(width * scale)), Math.max(1, Math.round(height * scale)));
    canvas.getContext('2d').drawImage(bitmap,x,y,width,height,0,0,canvas.width,canvas.height); bitmap.close();
    const blob = await canvas.convertToBlob({type:'image/png'});
    if (blob.size > 128 * 1024) return {captureError:'Screenshot too large; DOM evidence retained.'};
    const bytes = new Uint8Array(await blob.arrayBuffer());
    let binary = ''; for (const byte of bytes) binary += String.fromCharCode(byte);
    return {image: `data:image/png;base64,${btoa(binary)}`};
  } catch (error) { return {captureError: `Screenshot unavailable: ${error.message}`}; }
}
async function handle(message, sender) {
  if (message.kind === 'pick') {
    if (!sender.tab?.id || !sender.documentId || sender.url !== message.documentUrl || message.raw?.url !== sender.url) throw new Error('Pick document changed.');
    if (JSON.stringify(message.raw).length > 32768) throw new Error('Pick is too large.');
    if (!draftForTab(await drafts(), sender.tab.id).multi) await chrome.tabs.sendMessage(sender.tab.id, {kind:'stop'});
    const pick = {id: crypto.randomUUID(), raw: message.raw, documentId: sender.documentId, ...await capture(message, sender)};
    try { pick.evidence = await request('pick', {raw: message.raw}); connection = 'Connected'; }
    catch (error) { connection = error.message; }
    await mutate(sender.tab.id, (draft) => addPick(draft, pick));
    return snapshot(sender.tab.id);
  }
  // Content scripts never get a path to sends, host configuration or draft edits.
  if (sender.tab || sender.url !== chrome.runtime.getURL('panel.html')) return;
  const tab = await activeTab();
  if (message.kind === 'snapshot') { await refreshConnection(); return snapshot(tab.id); }
  if (message.kind === 'arm') {
    await mutate(tab.id, (draft) => ({...draft, multi: message.multi === true}));
    await chrome.tabs.sendMessage(tab.id, {kind:'arm',multi:message.multi === true});
    return snapshot(tab.id);
  }
  if (message.kind === 'open_server') {
    const fresh=await request('list_dev_servers');
    if(!fresh.servers.some((server)=>server.url===message.url)) throw new Error('That server is no longer available. Refresh the list.');
    await chrome.tabs.update(tab.id,{url:message.url});
  }
  if (message.kind === 'start_server') { await request('start_dev_server');lastRefresh=0;await refreshConnection(); }
  if (message.kind === 'refresh') { lastRefresh=0;await refreshConnection(); }
  if (message.kind === 'send') {
    const draft=draftForTab(await drafts(),message.tabId);
    const outcome=await deliverDraft(draft,(batch)=>request('send',batch),
      (value)=>mutate(message.tabId,()=>value),crypto.randomUUID());
    return {draft:await snapshot(message.tabId),outcome};
  }
  if (message.kind === 'stop') await chrome.tabs.sendMessage(tab.id,{kind:'stop'});
  if (message.kind === 'edit') await mutate(message.tabId, (draft) => editPick(draft,message.id,message.comment));
  if (message.kind === 'select') await mutate(message.tabId, (draft) => ({...draft,selectedSessionId:message.sessionId,selectionInitialized:true,agentNotice:''}));
  if (message.kind === 'delete') await mutate(message.tabId, (draft) => deletePick(draft,message.id));
  return snapshot(tab.id);
}
chrome.runtime.onMessage.addListener((message,sender,reply) => {
  if (message.kind === 'changed') return;
  // One queue prevents frame picks, comment edits and deletes losing updates.
  const operation = serial.then(() => handle(message,sender));
  serial = operation.catch(() => {});
  operation.then((result) => reply({ok:true,result}), (error) => reply({ok:false,error:error.message}));
  return true;
});
chrome.tabs.onActivated.addListener(notify);
chrome.tabs.onUpdated.addListener((_id,change) => { if (change.status === 'complete') notify(); });
chrome.tabs.onRemoved.addListener((tabId) => {
  serial = serial.then(async () => { const all=await drafts(); delete all[String(tabId)]; await chrome.storage.local.set({drafts:all}); }).catch(() => {});
});
