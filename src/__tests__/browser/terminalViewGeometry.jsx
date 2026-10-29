import { render } from 'solid-js/web';
import { LastPromptBar } from '../../components/Terminal/LastPromptBar';
import { ViewModeToggle } from '../../components/Terminal/TerminalChatView';
import { terminalsStore } from '../../stores/terminals';
import { Select } from '../../components/ui';
import terminal from '../../components/Terminal/Terminal.module.css';
import prompt from '../../components/Terminal/LastPromptBar.module.css';
import '../../global.css';

const id=terminalsStore.add({sessionId:'geometry',cwd:null,name:'Geometry',fontSize:14,awaitingInput:null,agentType:'claude'});

render(() => <section id="geometry" style="padding:16px;background:var(--bg-secondary)">
  <div class={terminal.wrapper}>
    <ViewModeToggle terminalId={id}/><LastPromptBar intent={()=>"Checking shared terminal controls"} ptyDescription={()=>null} prompt={()=>null}/>
  </div>
  <Select aria-label="Reference status"><option>Ready</option></Select>
</section>,document.getElementById('root'));
document.querySelector('[role="tablist"]').setAttribute('data-form-control','');

// Catches: the view switch crosses the Context divider or jumps on mode changes.
// Run through agent-browser eval on this Vite fixture; jsdom cannot measure layout.
window.assertTerminalViewAlignment = () => {
  const toggle = document.querySelector('[data-terminal-view-toggle]');
  terminalsStore.setViewMode(id, 'cli');
  const cli = toggle.getBoundingClientRect();
  const text = document.querySelector(`.${prompt.preview}`).getBoundingClientRect();
  const bar = document.querySelector(`.${prompt.bar}`).getBoundingClientRect();
  const centre = (rect) => rect.top + rect.height / 2;
  const failures = [];
  if (Math.abs(centre(cli) - centre(text)) > 0.5) failures.push('Switch is not centred on Context text');
  if (cli.bottom > bar.bottom - 1) failures.push('Switch crosses the Context divider');
  if (cli.height !== 22) failures.push('Switch lost shared desktop toolbar height');
  terminalsStore.setViewMode(id, 'chat');
  const chat = toggle.getBoundingClientRect();
  if (cli.top !== chat.top || cli.left !== chat.left) failures.push('Switch moves between CLI and Chat');
  terminalsStore.setViewMode(id, 'cli');
  if (failures.length) throw new Error(failures.join('; '));
  return {textCentre:centre(text),switchCentre:centre(cli),divider:bar.bottom - 1,height:cli.height};
};
window.controlGeometryReady=true;
