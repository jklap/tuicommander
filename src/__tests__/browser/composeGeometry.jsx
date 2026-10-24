import { render } from 'solid-js/web';
import { ComposePanel } from '../../components/ComposePanel/ComposePanel';
import '../../global.css';
render(()=><section id="geometry"><ComposePanel isOpen={()=>true} initialText={()=>'Draft'} onClose={()=>{}} onSend={()=>{}} onEnqueue={()=>{}} canEnqueue={()=>true} queuedCount={()=>1} onClearQueue={()=>{}} onLoadQueue={async()=>[{id:1,text:'Queued command',kind:'user_command'}]} onRemoveQueued={()=>{}} pinned={()=>true} onTogglePin={()=>{}} onDismiss={()=>{}} focusRequest={()=>0}/></section>,document.getElementById('root'));
requestAnimationFrame(()=>{
 const badge=document.querySelector('[aria-expanded]');
 badge.click();
 const ready=()=>{
  if(!document.querySelector('[title="Remove from queue"]')){requestAnimationFrame(ready);return;}
  window.controlGeometryReady=true;
 };
 requestAnimationFrame(ready);
});
