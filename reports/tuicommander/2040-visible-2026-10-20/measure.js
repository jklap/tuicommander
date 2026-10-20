const config=window.__desk2040Config;
const delay=ms=>new Promise(r=>setTimeout(r,ms));
const terminal=()=>window.__TUIC__.terminals().find(t=>t.sessionId===config.session);
for(let i=0;i<100&&!terminal();i++)await delay(50);
if(!terminal())throw Error('Session not adopted');
document.querySelector(`[data-tab-id="${terminal().id}"]`).click();
await delay(2500);
const canvas=[...document.querySelectorAll('canvas[tabindex]')].find(c=>c.getBoundingClientRect().width>0);
if(!canvas)throw Error('No visible terminal canvas');
const input=[...document.querySelectorAll('input[aria-hidden=true]')].find(e=>e.parentElement.getBoundingClientRect().width>0);
if(!input)throw Error('Terminal keyboard input not found');
const w=window.__TAURI__.window.getCurrentWindow();
const windowState={visibility:document.visibilityState,focus:document.hasFocus(),nativeFocus:await w.isFocused(),visible:await w.isVisible(),position:await w.outerPosition(),size:await w.outerSize()};
const pending=new Map(),keys=[],rafs=[],timers=[],ipc=[],paintCalls=[];
const originalInvoke=window.__TAURI_INTERNALS__.invoke;
window.__TAURI_INTERNALS__.invoke=function(command,args,...rest){
 const entry={command,args,at:performance.now()};
 if(command==='write_pty'&&args.sessionId===config.session)ipc.push(entry);
 const promise=originalInvoke.call(this,command,args,...rest);
 if(command==='write_pty'&&args.sessionId===config.session)promise.then(()=>entry.completed_at=performance.now(),e=>entry.error=String(e));
 return promise;
};
const originalFill=CanvasRenderingContext2D.prototype.fillText;
CanvasRenderingContext2D.prototype.fillText=function(text,...rest){
 const begin=performance.now(),result=originalFill.call(this,text,...rest),end=performance.now();
 if(this.canvas===canvas){
  paintCalls.push({at:end,duration_ms:end-begin});
  for(const ch of String(text)){const sample=pending.get(ch);if(sample){sample.fillText_ms=end-sample.dispatch_at;sample.paint_at=end;pending.delete(ch);requestAnimationFrame(()=>requestAnimationFrame(()=>{sample.two_raf_ms=performance.now()-sample.dispatch_at}));}}
 }
 return result;
};
let animation,last=performance.now(),prev=last;
const beat=at=>{rafs.push({at,gap_ms:at-last});last=at;animation=requestAnimationFrame(beat)};animation=requestAnimationFrame(beat);
const timer=setInterval(()=>{const now=performance.now();timers.push({at:now,gap_ms:now-prev});prev=now},100);
const start=performance.now(),visibility=[{at:start,state:document.visibilityState}];
const changed=()=>visibility.push({at:performance.now(),state:document.visibilityState});document.addEventListener('visibilitychange',changed);
try{
 input.focus({preventScroll:true});
 for(let i=0;i<config.samples;i++){
  const key=String.fromCodePoint(0x2460+i),event=new KeyboardEvent('keydown',{key,bubbles:true,cancelable:true});
  const sample={key,dispatch_at:performance.now(),isTrusted:event.isTrusted,target:input.outerHTML};keys.push(sample);pending.set(key,sample);
  sample.dispatch_result=input.dispatchEvent(event);sample.returned_at=performance.now();sample.defaultPrevented=event.defaultPrevented;
  await delay(150);
 }
 await delay(1000);
 return {session:config.session,wall_time_origin_ms:performance.timeOrigin,started_at:start,ended_at:performance.now(),windowState,visibility,userAgent:navigator.userAgent,dpr:devicePixelRatio,canvas:{width:canvas.width,height:canvas.height,rect:canvas.getBoundingClientRect().toJSON()},keys,ipc,rafs,timers,fillText_calls:paintCalls.length,claim:'Untrusted JS keydown. fillText completion is a canvas submission proxy, not compositor presentation. Socket observation includes observer overhead.'};
}finally{
 clearInterval(timer);cancelAnimationFrame(animation);document.removeEventListener('visibilitychange',changed);
 CanvasRenderingContext2D.prototype.fillText=originalFill;window.__TAURI_INTERNALS__.invoke=originalInvoke;
}
