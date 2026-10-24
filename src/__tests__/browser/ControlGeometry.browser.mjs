import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { mkdir, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { it } from 'node:test';
import { createServer } from 'vite';
import solid from 'vite-plugin-solid';

// Real browser layout, not happy-dom's zero-size boxes. Screenshots clip only the fixture element.
it('catches shared control geometry drifting at desktop and touch widths', {timeout: 60000}, async () => {
  const directory = resolve(process.env.TUIC_GEOMETRY_EVIDENCE ?? '.tmp/control-geometry');
  await mkdir(directory, {recursive: true});
  const server = await createServer({configFile:false, plugins:[solid()], cacheDir:resolve('.tmp/control-geometry-vite'), logLevel:'error', server:{host:'127.0.0.1',port:0}});
  let chrome, socket;
  try {
    await server.listen();
    chrome = spawn(process.env.TUIC_TEST_CHROME, ['--headless=new', '--disable-gpu', '--no-first-run', '--remote-debugging-port=0', `--user-data-dir=${directory}/chrome`, 'about:blank'], {stdio:['ignore','ignore','pipe']});
    const endpoint = await new Promise((ok, fail) => {
      let stderr = '';
      chrome.stderr.on('data', data => {stderr += data; const match = stderr.match(/DevTools listening on (ws:\/\/\S+)/); if(match) ok(match[1]);});
      chrome.once('error', fail);
      chrome.once('exit', code => fail(new Error(`Chrome exited ${code}: ${stderr}`)));
    });
    socket = new WebSocket(endpoint);
    await new Promise((ok, fail) => {socket.addEventListener('open',ok,{once:true});socket.addEventListener('error',fail,{once:true});});
    let sequence=0;
    const pending = new Map();
    socket.addEventListener('close',()=>{for(const request of pending.values()){clearTimeout(request.timer);request.reject(new Error('Browser disconnected'));}pending.clear();});
    socket.addEventListener('message', event => {const message=JSON.parse(event.data);const request=pending.get(message.id);if(request){clearTimeout(request.timer);pending.delete(message.id);message.error ? request.reject(new Error(JSON.stringify(message.error))) : request.resolve(message.result);}});
    const send = (method, params={}, sessionId) => new Promise((resolve,reject) => {const id=++sequence;const timer=setTimeout(()=>{pending.delete(id);reject(new Error(`CDP ${method} did not respond`));},20000);pending.set(id,{resolve,reject,timer});socket.send(JSON.stringify({id,method,params,sessionId}));});
    const {targetId} = await send('Target.createTarget',{url:'about:blank'});
    const {sessionId} = await send('Target.attachToTarget',{targetId,flatten:true});
    const evaluate = async expression => {const result=await send('Runtime.evaluate',{expression,awaitPromise:true,returnByValue:true},sessionId);if(result.exceptionDetails)throw new Error(JSON.stringify(result.exceptionDetails));return result.result.value;};
    await send('Page.enable',{},sessionId);
    const measurements=[];
    for(const width of [1400,390]) {
      await send('Emulation.setDeviceMetricsOverride',{width,height:900,deviceScaleFactor:1,mobile:false},sessionId);
      const loaded = new Promise((resolve,reject) => {const timer=setTimeout(()=>reject(new Error('Page load event did not arrive')),20000);const listener = event => {const message=JSON.parse(event.data);if(message.method==='Page.loadEventFired' && message.sessionId===sessionId){clearTimeout(timer);socket.removeEventListener('message',listener);resolve();}};socket.addEventListener('message',listener);});
      await send('Page.navigate',{url:`http://127.0.0.1:${server.httpServer.address().port}/src/__tests__/browser/${process.env.TUIC_GEOMETRY_FIXTURE ?? "controlGeometry"}.html`},sessionId);
      await loaded;
      await evaluate(`new Promise((resolve,reject)=>{const stop=setTimeout(()=>reject(new Error('Fixture did not mount')),15000);const check=()=>{if(window.controlGeometryReady){clearTimeout(stop);requestAnimationFrame(()=>requestAnimationFrame(resolve));}else requestAnimationFrame(check);};check();})`);
      const result=await evaluate(`({rows:document.querySelector('[data-row]')?[...document.querySelectorAll('[data-row]')].map(row=>({size:row.dataset.row,heights:[...row.children].map(element=>({tag:element.tagName,name:element.getAttribute('aria-label')||element.getAttribute('title')||element.textContent?.trim(),height:element.getBoundingClientRect().height}))})):[{size:document.querySelector('#geometry').dataset.controlSize??'default',heights:[...document.querySelectorAll('#geometry [data-form-control]:not(textarea)')].map(element=>({tag:element.tagName,name:element.getAttribute('aria-label')||element.getAttribute('title')||element.textContent?.trim(),height:element.getBoundingClientRect().height}))}],clip:(()=>{const r=document.querySelector('#geometry').getBoundingClientRect();return {x:r.x,y:r.y,width:r.width,height:r.height,scale:1};})()})`);
      const stretch = await evaluate(`[...document.querySelectorAll('[data-stretch-case]')].map(row=>({container:row.dataset.stretchCase,controls:[...row.querySelectorAll('button')].map(button=>({height:button.getBoundingClientRect().height,width:button.getBoundingClientRect().width,radius:getComputedStyle(button).borderRadius}))}))`);
      for (const row of stretch) {
        assert.ok(row.controls.length >= 2, 'Stretch cases must exercise multiple button sizes');
        for (const control of row.controls) {
          assert.equal(control.height, Math.max(Number(row.container), width <= 600 ? 44 : 36), JSON.stringify({width,row}));
          assert.ok(control.width >= (width <= 600 ? 44 : 36), JSON.stringify({width,row}));
          assert.equal(control.radius, '0px', 'Stretch actions must have square edges');
        }
      }
      const joined = await evaluate(`[...document.querySelectorAll('[data-joined-case]')].map(group=>({size:group.dataset.joinedCase,overflow:getComputedStyle(group).overflow,controls:[...group.querySelectorAll('button')].map(button=>{const r=button.getBoundingClientRect(),s=getComputedStyle(button);return {left:r.left,right:r.right,height:r.height,start:s.borderStartStartRadius,end:s.borderStartEndRadius,startBorder:s.borderInlineStartWidth,endBorder:s.borderInlineEndWidth};})}))`);
      for (const group of joined) {
        assert.notEqual(group.overflow,'hidden','Group must not clip focus or dropdown');
        assert.ok(group.controls.length > 0, 'Joined group must mount real buttons');
        for (const [index,control] of group.controls.entries()) {
          assert.equal(control.height, width<=600?44:group.size==='compact'?22:36);
          if(index>0) {
            assert.equal(control.left,group.controls[index-1].right,'Connected edges must meet');
            assert.equal(control.start,'0px');
            assert.equal(control.startBorder,'1px','Join must have exactly one divider');
          } else assert.notEqual(control.start,'0px','First action retains outer radius');
          if(index<group.controls.length-1) {assert.equal(control.end,'0px');assert.equal(control.endBorder,'0px');}
          else assert.notEqual(control.end,'0px','Last action retains outer radius');
        }
      }
      const textarea = await evaluate('window.textareaGeometry ? window.textareaGeometry() : null');
      if(textarea) {
        const minimum=width<=600?44:36, maximum=width<=600?132:108;
        assert.equal(textarea.cleared.height,minimum,'Cleared auto-grow must shrink to shared target');
        assert.equal(textarea.long.height,maximum,'Long content must respect shared cap');
        assert.ok(textarea.long.scroll>textarea.long.client,'Long content must remain scrollable');
        assert.equal(textarea.long.overflow,'auto');
        assert.ok(textarea.narrow.height>textarea.wide.height,'Width-only wrapping must resize without input');
        for(const name of ['initial','replaced'])assert.ok(textarea[name].height>minimum && textarea[name].height<maximum,JSON.stringify({name,textarea}));
      }
      await send('Input.dispatchMouseEvent',{type:'mouseMoved',x:0,y:0},sessionId);
      const states = await evaluate('window.controlStateGeometry ? window.controlStateGeometry() : null');
      if(states) {
        for(const control of states.invalid)assert.equal(control.border,'rgb(200, 10, 20)','Native invalid field must use error token');
        for(const control of states.focused) {assert.equal(control.border,'rgb(200, 10, 20)');assert.equal(control.outline,'rgb(10, 20, 200)');assert.equal(control.outlineStyle,'solid');}
        for(const control of states.valid)assert.equal(control.border,'rgb(80, 80, 80)','False invalid state must reset border');
        assert.equal(states.off.border,'rgb(80, 80, 80)');assert.equal(states.on.border,'rgb(10, 200, 20)');
        assert.equal(states.disabled.border,'rgb(10, 200, 20)');assert.equal(states.selection.border,'rgb(10, 20, 200)','Ordinary selected actions retain accent');
        for(const [selector,expected] of [['[data-remove-toggle]','rgb(200, 10, 20)'],['[data-disabled-toggle]','rgb(10, 200, 20)']]) {
          const box=await evaluate(`(()=>{const r=document.querySelector('${selector}').getBoundingClientRect();return{x:r.x+r.width/2,y:r.y+r.height/2};})()`);
          await send('Input.dispatchMouseEvent',{type:'mouseMoved',...box},sessionId);
          const color=await evaluate(`getComputedStyle(document.querySelector('${selector}')).borderColor`);
          assert.equal(color,expected,'Enabled removal hover only must use danger token');
        }
      }
      result.containment = [];
      measurements.push({width,...result,stretch,joined,textarea,states});
      const image=await send('Page.captureScreenshot',{format:'png',clip:result.clip},sessionId);
      const path=resolve(directory,`controls-${width}.png`);
      await writeFile(path,Buffer.from(image.data,'base64'));
      console.log(path);
      // Catches: platform-specific desktop height wins over wrapped touch rows.
      if (process.env.TUIC_GEOMETRY_FIXTURE === 'toolbarGeometry') {
        for (const platform of ['linux', 'windows']) {
          await evaluate(`document.documentElement.className='platform-${platform}'; new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)))`);
          const bounds = await evaluate(`(()=>{const toolbar=document.querySelector('#toolbar'); const box=toolbar.getBoundingClientRect(); const buttons=[...toolbar.querySelectorAll('button')]; return {toolbarBottom:box.bottom,actionsBottom:Math.max(...buttons.map(button=>button.getBoundingClientRect().bottom)),actions:buttons.length};})()`);
          result.containment.push({platform,...bounds});
          console.log('CONTAINMENT',JSON.stringify({width,platform,...bounds}));
          const capture = await send('Page.captureScreenshot',{format:'png',clip:(await evaluate(`(()=>{const box=document.querySelector('#geometry').getBoundingClientRect(); return {x:box.x,y:box.y,width:box.width,height:Math.max(box.height,100),scale:1};})()`))},sessionId);
          const platformPath=resolve(directory,`controls-${width}-${platform}.png`);
          await writeFile(platformPath,Buffer.from(capture.data,'base64'));
          console.log(platformPath);
        }
        await evaluate(`document.documentElement.className=''`);
      }
    }
    await writeFile(resolve(directory,'measurements.json'),JSON.stringify(measurements,null,2));
    if (process.env.TUIC_GEOMETRY_FIXTURE === 'toolbarGeometry') {
      for (const result of measurements) assert.equal(result.containment.length, 2, 'Both platform containment cases must execute');
    }
    for (const result of measurements) for (const bounds of result.containment) {
      assert.ok(bounds.actions >= 2, 'Containment must measure actual mounted toolbar actions');
      assert.ok(bounds.actionsBottom <= bounds.toolbarBottom + 1, `${bounds.platform} toolbar must contain actions at ${result.width}px: ${JSON.stringify(bounds)}`);
    }
    for(const result of measurements)for(const row of result.rows) {
      assert.ok(row.heights.length >= 2, 'The mounted fixture must contain multiple real controls');
      for(const element of row.heights)assert.equal(element.height,result.width<=600?44:row.size==='compact'?22:36,JSON.stringify({width:result.width,row}));
    }
  } finally {
    socket?.close();
    if(chrome && chrome.exitCode === null) {chrome.kill();await new Promise(resolve=>chrome.once('exit',resolve));}
    await server.close();
  }
});
