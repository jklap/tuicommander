import { createSignal, Show } from 'solid-js';
import { render } from 'solid-js/web';
import { Transcript } from '../../components/AIChatPanel/Transcript';
import { LastPromptBar } from '../../components/Terminal/LastPromptBar';
import { ViewModeToggle } from '../../components/Terminal/TerminalChatView';
import { terminalsStore } from '../../stores/terminals';
import { Select } from '../../components/ui';
import terminal from '../../components/Terminal/Terminal.module.css';
import chat from '../../components/Terminal/TerminalChatView.module.css';
import prompt from '../../components/Terminal/LastPromptBar.module.css';
import '../../global.css';

const id = terminalsStore.add({sessionId:'geometry',cwd:null,name:'Geometry',fontSize:14,awaitingInput:null,agentType:'claude',agentSessionId:'geometry-agent'});
const [context, setContext] = createSignal(true);
const mode = () => terminalsStore.get(id)?.viewMode ?? 'cli';
const entries = [{id:'message',kind:'user',text:'The conversation starts here and must remain clear of the view switch.'}];

render(() => <section id="geometry" style="padding:16px;background:var(--bg-secondary);height:500px">
  <div class={terminal.wrapper}>
    <ViewModeToggle terminalId={id}/>
    <Show when={context()}><LastPromptBar intent={()=>'Checking shared terminal controls with a long context preview that must truncate before the CLI and Chat switch at the right edge of the header.'} ptyDescription={()=>null} prompt={()=>null}/></Show>
    <Show when={mode() === 'chat'} fallback={<div data-content class={terminal.content}>CLI output starts here.</div>}>
      <div data-content class={chat.chatView}><Transcript entries={()=>entries} busy={()=>false} emptyMessage="Empty" terminalAgent="claude"/></div>
    </Show>
  </div>
  <Select aria-label="Reference status"><option>Ready</option></Select>
</section>,document.getElementById('root'));
document.querySelector('[role="tablist"]').setAttribute('data-form-control','');
window.setTerminalGeometry = (mode, context) => {
  setContext(context);
  terminalsStore.setViewMode(id, mode);
};

// Catches: Chat adds its own switch row, Context text overlaps the switch,
// or removing that row lets no-Context chat content run beneath the controls.
// Run in real Chrome through agent-browser; jsdom cannot measure layout.
window.assertTerminalViewAlignment = () => {
  const failures = [];
  const results = [];
  for (const hasContext of [true, false]) {
    const modes = [];
    for (const mode of ['cli', 'chat']) {
      window.setTerminalGeometry(mode, hasContext);
      const toggle = document.querySelector('[data-terminal-view-toggle]').getBoundingClientRect();
      const content = document.querySelector('[data-content]').getBoundingClientRect();
      const bar = document.querySelector(`.${prompt.bar}`)?.getBoundingClientRect();
      if (bar) {
        const text = document.querySelector(`.${prompt.preview}`).getBoundingClientRect();
        const centre = (rect) => rect.top + rect.height / 2;
        if (Math.abs(centre(toggle) - centre(text)) > 0.5) failures.push(`${mode}: switch is not centred on Context text`);
        if (toggle.bottom > bar.bottom - 1) failures.push(`${mode}: switch crosses Context divider`);
        if (text.right > toggle.left) failures.push(`${mode}: Context text runs under switch`);
        if (Math.abs(toggle.top - bar.top - 2) > 0.5) failures.push(`${mode}: switch has a separate row above Context`);
      } else if (content.top < toggle.bottom) failures.push(`${mode}: content runs under switch without Context`);
      if (toggle.height !== 22) failures.push(`${mode}: switch lost shared desktop toolbar height`);
      modes.push({mode,top:toggle.top,left:toggle.left,contentTop:content.top,contextTop:bar?.top});
    }
    const [cli, chat] = modes;
    for (const key of ['top', 'left', 'contentTop', 'contextTop']) {
      if (cli[key] !== chat[key]) failures.push(`context=${hasContext}: ${key} changes between modes (${cli[key]} / ${chat[key]})`);
    }
    results.push({context:hasContext,modes});
  }
  window.setTerminalGeometry('cli', true);
  if (failures.length) throw new Error(failures.join('; '));
  return results;
};
window.controlGeometryReady=true;
