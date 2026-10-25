import { render } from 'solid-js/web';
import { MobileComposerMenu } from '../../mobile/components/MobileComposerMenu';
import { CommandInput } from '../../mobile/components/CommandInput';
import { Select } from '../../components/ui';
import '../../global.css';
let queued=0;
render(() => <section id="geometry" style="padding:16px;background:var(--bg-secondary)">
  <Select aria-label="Reference status"><option>Ready</option></Select>
  <div id="menu-case" style="position:relative;margin-top:300px"><MobileComposerMenu onAttach={()=>{}} onPark={()=>{}} onQueue={()=>{queued++;}} onStop={()=>{}} onCommands={()=>{}}/></div>
  <div id="terminal-case"><CommandInput sessionId="geometry-only"/></div>
</section>, document.getElementById('root'));
const trigger=document.querySelector('#menu-case button');
trigger.click();
if(trigger.getAttribute('aria-expanded')!=='true')throw new Error('Menu did not open');
document.querySelector('#menu-case [role="menuitem"]').focus();
document.dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true}));
if(document.activeElement!==trigger || document.querySelector('#menu-case [role="menu"]'))throw new Error('Escape must close and restore trigger focus');
trigger.click();
[...document.querySelectorAll('#menu-case [role="menuitem"]')].find(item=>item.textContent.trim()==='Queue').click();
if(queued!==1 || trigger.getAttribute('aria-expanded')!=='false')throw new Error('Queue must invoke once and dismiss');
trigger.click();
// Catches: shared trigger width eats the command editor's original 114px budget.
const form=document.querySelector('#terminal-case > div');
const input=form.querySelector('textarea');
const send=form.querySelector('button[aria-label="Send"]');
const menu=form.querySelector('button[aria-label="More composer actions"]');
const composer=input.parentElement;
const px=(element,property)=>parseFloat(getComputedStyle(element)[property])||0;
// Reserve the existing 22px mic even when Chromium offers no speech constructor.
const budget=menu.getBoundingClientRect().width+px(input,'paddingLeft')+px(input,'paddingRight')+px(composer,'paddingRight')+22+px(form,'gap')+px(send,'marginLeft')+send.getBoundingClientRect().width;
if(budget>114)throw new Error(`Command input horizontal budget exceeded: ${budget}`);
window.controlGeometryReady=true;
