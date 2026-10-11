const $ = (id) => document.getElementById(id);
let current;
const cards = new Map();
const status = (text, error = false) => { $('status').textContent = text; $('status').dataset.error = String(error); };
async function call(message) {
  const response = await chrome.runtime.sendMessage(message);
  if (!response?.ok) throw new Error(response?.error ?? 'Extension worker unavailable.');
  return response.result;
}
const node = (tag, text, className) => {
  const element = document.createElement(tag); if (text) element.textContent = text; if (className) element.className = className; return element;
};
function render(draft) {
  if (!draft) return;
  current = draft;
  $('repo').textContent = draft.repoPath ?? 'Open Design Mode from TUICommander to select a repository.';
  $('connection').textContent = draft.connection;
  $('servers-title').textContent=`Dev servers (${draft.servers?.servers?.length ?? 0})`;
  $('server-list').replaceChildren();
  for (const server of draft.servers?.servers ?? []) {
    const open=node('button',`${server.url} · ${server.source}${server.pid?' · pid '+server.pid:''}`);
    open.onclick=()=>call({kind:'open_server',url:server.url}).catch((error)=>status(error.message,true));$('server-list').append(open);
  }
  if (draft.servers?.scanNotice) $('server-list').append(node('p',draft.servers.scanNotice,'muted'));
  if (!draft.servers?.servers?.length) $('server-list').append(node('p','No running dev server found. Use the address bar or start your configured Run Script.','muted'));
  const refreshServers=node('button','Refresh servers and agents');refreshServers.onclick=()=>call({kind:'refresh'}).then(render).catch((error)=>status(error.message,true));$('server-list').append(refreshServers);
  if(draft.servers?.canStart) {const start=node('button','Start dev server');start.onclick=()=>call({kind:'start_server'}).then(render).catch((error)=>status(error.message,true));$('server-list').append(start);}
  const selection = draft.selectedSessionId ?? '';
  if (draft.agentNotice) status(draft.agentNotice,true);
  $('agent').replaceChildren();
  for (const agent of draft.agents ?? []) { const option=node('option',agent.label.slice(0,120)); option.value=agent.sessionId; option.title=agent.label; $('agent').append(option); }
  if (!draft.agents?.length) { const reason = draft.connection.startsWith('Connected') ? 'No live agents in this repository' : 'Connection unavailable — see reason above'; const option=node('option',reason); option.value=''; $('agent').append(option); }
  if (!selection && draft.agents?.length) { const option=node('option','Choose another agent'); option.value=''; $('agent').prepend(option); }
  $('agent').value=selection;
  $('send').disabled = !selection || !draft.picks.some((pick)=>pick.comment.trim());
  // Cards are keyed by pick id and never rebuilt or moved, so a comment being typed keeps its focus/caret
  // while later picks are appended. Picks only append or disappear, so the order is preserved.
  const ids = new Set(draft.picks.map((pick) => pick.id));
  for (const [id, card] of cards) if (!ids.has(id)) { card.remove(); cards.delete(id); }
  $('empty').hidden = draft.picks.length > 0;
  for (const pick of draft.picks) {
    if (cards.has(pick.id)) continue;
    const card = node('article',undefined,'card');
    cards.set(pick.id, card);
    card.append(node('h2',`${pick.raw.tagName} · ${(pick.raw.textContent ?? '').slice(0,80)}`),node('code',pick.raw.selector));
    if (pick.image) { const image=node('img'); image.src=pick.image; image.alt='Selected element'; card.append(image); }
    if (pick.captureError) card.append(node('p',pick.captureError,'muted'));
    const comment=node('textarea'); comment.placeholder='What should change?'; comment.setAttribute('aria-label',`Comment on ${pick.raw.selector}`); comment.value=pick.comment;
    comment.addEventListener('input',() => { call({kind:'edit',tabId:draft.tabId,id:pick.id,comment:comment.value}).catch((error)=>status(error.message,true)); });
    const details=node('details');details.append(node('summary','Element evidence'),node('pre',typeof pick.evidence==='string'?pick.evidence:JSON.stringify(pick.raw,null,2)));
    const remove=node('button','Delete pick');remove.onclick=() => call({kind:'delete',tabId:draft.tabId,id:pick.id}).then(render).catch((error)=>status(error.message,true));
    card.append(comment,details,remove);$('cards').append(card);
  }
}
async function refresh() { try { render(await call({kind:'snapshot'})); } catch(error) { status(error.message,true); } }
$('agent').onchange=() => { if(current) call({kind:'select',tabId:current.tabId,sessionId:$('agent').value}).then(render).catch((error)=>status(error.message,true)); };
$('send').onclick=async () => {
  if(!current) return;
  $('send').disabled=true;
  try { const result=await call({kind:'send',tabId:current.tabId});render(result.draft);status(result.outcome.typed?'Delivered now.':'Queued for the next idle window.'); }
  catch(error) { status(error.message,true);await refresh(); }
};
$('pick').onclick=() => call({kind:'arm'}).then(()=>status('Click one element. Escape cancels.')).catch((error)=>status(error.message,true));
$('multi').onclick=() => call({kind:'arm',multi:true}).then(()=>status('Pick elements. Stop or Escape ends picking.')).catch((error)=>status(error.message,true));
$('stop').onclick=() => call({kind:'stop'}).then(()=>status('Picking stopped.')).catch((error)=>status(error.message,true));
chrome.runtime.onMessage.addListener((message) => { if(message.kind==='changed') refresh(); });
refresh();
setInterval(refresh,5000);
