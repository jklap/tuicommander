import { render } from 'solid-js/web';
import { PrDiffTab } from '../../components/PrDiffTab/PrDiffTab';
import '../../global.css';
const diff='diff --git a/a.ts b/a.ts\n--- a/a.ts\n+++ b/a.ts\n@@ -1 +1 @@\n-old\n+new';
render(()=><section style="width:min(700px,100%);background:var(--bg-secondary)"><PrDiffTab prNumber={1} prTitle="Geometry" diff={diff}/></section>,document.getElementById('root'));
requestAnimationFrame(()=>{
 const button=document.querySelector('button[title="Side-by-side"]');
 if(!button)throw new Error('Diff mode controls did not mount');
 const row=button.parentElement;row.id='geometry';row.dataset.row='compact';
 window.controlGeometryReady=true;
});
