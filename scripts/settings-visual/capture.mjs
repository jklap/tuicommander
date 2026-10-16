import {spawn} from 'node:child_process';
import {mkdir,readFile,writeFile,unlink} from 'node:fs/promises';
const source=new URL('./',import.meta.url).pathname;
const dir=process.cwd()+'/.tmp/settings-visual/'+(process.env.VISUAL_RUN??'capture')+'/';
await mkdir(dir,{recursive:true});
const test=process.env.VISUAL_TEST; const failures=[];
await mkdir(dir+'chrome-profile',{recursive:true});await mkdir(dir+'png',{recursive:true});
await unlink(dir+'chrome-profile/DevToolsActivePort').catch(()=>{});
const chrome=spawn('/Users/stefano.straus/.agent-browser/browsers/chrome-149.0.7827.54/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing',['--headless=new','--no-first-run','--disable-background-networking','--disable-component-update','--disable-sync','--remote-debugging-port=0','--user-data-dir='+dir+'chrome-profile','about:blank'],{stdio:['ignore','ignore','pipe']});
let ws;
try {
let port;for(let i=0;i<100;i++){try{port=(await readFile(dir+'chrome-profile/DevToolsActivePort','utf8')).split('\n')[0];break;}catch{await new Promise(r=>setTimeout(r,100));}}
const targets=await(await fetch(`http://127.0.0.1:${port}/json`)).json();ws=new WebSocket(targets.find(t=>t.type==='page').webSocketDebuggerUrl);await new Promise(r=>ws.addEventListener('open',r,{once:true}));let seq=0;const pending=new Map();let errors=[],network=[];
ws.addEventListener('message',e=>{const x=JSON.parse(e.data);if(x.id){const p=pending.get(x.id);pending.delete(x.id);x.error?p.reject(new Error(JSON.stringify(x.error))):p.resolve(x.result);}else if(x.method==='Runtime.exceptionThrown')errors.push(x.params.exceptionDetails.exception?.description??x.params.exceptionDetails.text);else if(x.method==='Runtime.consoleAPICalled'&&x.params.type==='error')errors.push(x.params.args.map(a=>a.value??a.description).join(' '));else if(x.method==='Fetch.requestPaused'){const p=x.params;if(p.request.url.startsWith('http://127.0.0.1:15387/')||p.request.url.startsWith('data:'))send('Fetch.continueRequest',{requestId:p.requestId});else{network.push(p.request.url);send('Fetch.failRequest',{requestId:p.requestId,errorReason:'BlockedByClient'});}}});
function send(method,params={}){return new Promise((resolve,reject)=>{const id=++seq;pending.set(id,{resolve,reject});ws.send(JSON.stringify({id,method,params}));});}
async function ev(expression){const r=await send('Runtime.evaluate',{expression,awaitPromise:true,returnByValue:true});if(r.exceptionDetails)throw new Error(r.exceptionDetails.exception?.description??r.exceptionDetails.text);return r.result.value;}
await send('Page.enable');await send('Runtime.enable');await send('Fetch.enable',{patterns:[{urlPattern:'*'}]});
const tabs=test==='settings_text_contrast_on_themed_surfaces'?['general','terminal','keyboard-shortcuts','ai-chat','plugins','repo:/workspace/fixture-project']:test==='settings_agent_initials_contrast'?['agents']:test==='settings_appearance_legend_light_text_contrast'?['appearance']:test==='settings_help_badges_share_label_line'?['general','notifications','github','agents','repo:/workspace/fixture-project']:['general','appearance','notifications','terminal','keyboard-shortcuts','github','agents','ai-chat','dictation','smart-prompts','mcp','remote-access','remote-machines','telegram','plugins','repo:/workspace/fixture-project'];
const results=[];
for(const width of [1280,390])for(const theme of ['dark','light'])for(const tab of tabs){errors=[];network=[];await send('Emulation.setDeviceMetricsOverride',{width,height:800,deviceScaleFactor:1,mobile:false});await send('Page.navigate',{url:`http://127.0.0.1:15387/scripts/settings-visual/index.html?theme=${theme}&tab=${encodeURIComponent(tab)}`});await new Promise(r=>setTimeout(r,700));
for(let ready=0;ready<100;ready++){if(await ev("!!document.querySelector('[data-settings-content]')"))break;await new Promise(r=>setTimeout(r,100));}
let metrics;try{metrics=await ev(`(()=>{const content=document.querySelector('[data-settings-content]');return {title:document.title,text:document.body.innerText,content:content?{clientWidth:content.clientWidth,scrollWidth:content.scrollWidth,clientHeight:content.clientHeight,scrollHeight:content.scrollHeight}:null,calls:window.auditCalls}})()`);}catch(e){metrics={evaluationError:e.message};}
const name=tab.split(':')[0]+'-'+theme+'-'+width;const png=[];
if(metrics.content){const count=Math.ceil(metrics.content.scrollHeight/Math.max(metrics.content.clientHeight-40,1));for(let i=0;i<count;i++){await ev(`document.querySelector('[data-settings-content]').scrollTop=${i*(metrics.content.clientHeight-40)}`);await new Promise(r=>setTimeout(r,40));const p=dir+'png/'+name+'-'+i+'.png';const shot=await send('Page.captureScreenshot',{format:'png'});await writeFile(p,Buffer.from(shot.data,'base64'));png.push(p);}}
else{const p=dir+'png/'+name+'-error.png';await writeFile(p,Buffer.from((await send('Page.captureScreenshot',{format:'png'})).data,'base64'));png.push(p);}
const details=metrics.content?await ev(await readFile(source+'inspect.js','utf8')):null;
if(!metrics.content||errors.length)throw new Error('Settings did not mount cleanly: '+JSON.stringify({tab,errors,metrics}));
if(test && details){let bad=[]; if(test==='settings_text_contrast_on_themed_surfaces')bad=details.contrast.filter(x=>!['?','#42','G','P','C','X','D','A','O','E'].includes(x.text)); if(test==='settings_agent_initials_contrast')bad=details.contrast.filter(x=>['G','P'].includes(x.text)); if(test==='settings_appearance_legend_light_text_contrast')bad=details.contrast.filter(x=>['?','#42'].includes(x.text)); if(test==='settings_help_badges_share_label_line')bad=details.badges.filter(x=>x.ownLine); if(bad.length)failures.push({tab,theme,width,bad});}
results.push({tab,theme,width,errors:[...errors],blockedNetwork:[...network],metrics,details,png});await writeFile(dir+'results-'+width+'-'+theme+'.json',JSON.stringify(results,null,2));console.log(name,errors.length,metrics.content?JSON.stringify(metrics.content):'NO CONTENT');if(errors.length&&!process.argv.includes('--all'))console.log(errors);
}
if(test){await writeFile(dir+'assertions.json',JSON.stringify({test,failures},null,2)); console.log(test, failures.length?'FAIL':'PASS',JSON.stringify(failures)); if(failures.length)process.exitCode=1;}
}finally{ws?.close();chrome.kill('SIGTERM');}
