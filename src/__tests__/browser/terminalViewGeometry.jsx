import { render } from 'solid-js/web';
import { ViewModeToggle } from '../../components/Terminal/TerminalChatView';
import { terminalsStore } from '../../stores/terminals';
import { Select } from '../../components/ui';
import '../../global.css';

const id=terminalsStore.add({sessionId:'geometry',cwd:null,name:'Geometry',fontSize:14,awaitingInput:null,agentType:'claude'});
terminalsStore.setViewMode(id,'chat');
render(() => <section id="geometry" style="padding:16px;background:var(--bg-secondary)">
  <ViewModeToggle terminalId={id}/><Select aria-label="Reference status"><option>Ready</option></Select>
</section>,document.getElementById('root'));
document.querySelector('[role="tablist"]').setAttribute('data-form-control','');
window.controlGeometryReady=true;
