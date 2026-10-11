const $ = (id) => document.getElementById(id);
let current;
const status = (text, error = false) => { $('status').textContent = text; $('status').dataset.error = String(error); };
async function call(message) {
  const response = await chrome.runtime.sendMessage(message);
  if (!response?.ok) throw new Error(response?.error ?? 'Extension worker unavailable.');
  return response.result;
}
const node = (tag, text, className) => {
  const element = document.createElement(tag); if (text) element.textContent = text; if (className) element.className = className; return element;
};
function renderErrors(draft, errors, selection) {
  $('errors').replaceChildren();
  if (!errors.length) return;
  const section = node('details', undefined, 'card'); section.open = true;
  const badge = node('span', String(errors.length), 'badge'); badge.setAttribute('aria-label', `${errors.length} page errors`);
  const summary = node('summary', 'Errors '); summary.append(badge);
  const attach = document.createElement('input'); attach.type = 'checkbox'; attach.checked = draft.attachErrors === true;
  attach.onchange = () => call({kind:'attach_errors',tabId:draft.tabId,attach:attach.checked}).then(render).catch((error)=>status(error.message,true));
  const label = node('label', ' Attach to feedback'); label.prepend(attach);
  const list = node('pre', errors.map((error) => `[${error.kind}] ${error.message}${error.source ? `\n  at ${error.source}${error.line === undefined ? '' : `:${error.line}${error.column === undefined ? '' : `:${error.column}`}`}` : ''}`).join('\n'));
  const only = node('button', 'Send errors only', 'primary'); only.disabled = !selection;
  only.onclick = async () => {
    only.disabled = true;
    try { const result = await call({kind:'send',tabId:draft.tabId,errorsOnly:true}); render(result.draft); status(result.outcome.typed ? 'Errors delivered now.' : 'Errors queued for the next idle window.'); }
    catch (error) { status(error.message,true); await refresh(); }
  };
  const clear = node('button', 'Clear errors'); clear.onclick = () => call({kind:'clear_errors',tabId:draft.tabId}).then(render).catch((error)=>status(error.message,true));
  section.append(summary, list, label, only, clear);
  $('errors').append(section);
}
function render(draft) {
  if (!draft) return;
  current = draft;
  $('repo').textContent = draft.repoPath ?? 'Open Design Mode from TUICommander to select a repository.';
  $('connection').textContent = draft.connection;
  $('servers').replaceChildren(node('h2','Dev servers'));
  for (const server of draft.servers?.servers ?? []) {
    const open=node('button',`${server.url} · ${server.source}${server.pid?' · pid '+server.pid:''}`);
    open.onclick=()=>call({kind:'open_server',url:server.url}).catch((error)=>status(error.message,true));$('servers').append(open);
  }
  if (draft.servers?.scanNotice) $('servers').append(node('p',draft.servers.scanNotice,'muted'));
  if (!draft.servers?.servers?.length) $('servers').append(node('p','No running dev server found. Use the address bar or start your configured Run Script.','muted'));
  const refreshServers=node('button','Refresh servers and agents');refreshServers.onclick=()=>call({kind:'refresh'}).then(render).catch((error)=>status(error.message,true));$('servers').append(refreshServers);
  if(draft.servers?.canStart) {const start=node('button','Start dev server');start.onclick=()=>call({kind:'start_server'}).then(render).catch((error)=>status(error.message,true));$('servers').append(start);}
  const selection = draft.selectedSessionId ?? '';
  if (draft.agentNotice) status(draft.agentNotice,true);
  $('agent').replaceChildren();
  for (const agent of draft.agents ?? []) { const option=node('option',agent.label.slice(0,120)); option.value=agent.sessionId; option.title=agent.label; $('agent').append(option); }
  if (!draft.agents?.length) { const reason = draft.connection.startsWith('Connected') ? 'No live agents in this repository' : 'Connection unavailable — see reason above'; const option=node('option',reason); option.value=''; $('agent').append(option); }
  if (!selection && draft.agents?.length) { const option=node('option','Choose another agent'); option.value=''; $('agent').prepend(option); }
  $('agent').value=selection;
  const errors = draft.errors ?? [];
  $('send').disabled = !selection || !(draft.picks.some((pick)=>pick.comment.trim()) || (draft.attachErrors && errors.length));
  renderErrors(draft, errors, selection);
  const focused = document.activeElement;
  // A comment save updates state without stealing the user's focus/caret.
  if (focused instanceof HTMLTextAreaElement && focused.closest('#cards')) return;
  $('cards').replaceChildren();
  if (!draft.picks.length) $('cards').append(node('p','Pick an element on your page, then add a comment.','muted'));
  for (const pick of draft.picks) {
    const card = node('article',undefined,'card');
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
