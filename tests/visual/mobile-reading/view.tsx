import { render } from "solid-js/web";
import { OutputView } from "../../../src/mobile/components/OutputView";
import "../../../src/mobile/mobile.css";

window.fetch = async () => new Response(JSON.stringify({ lines: [], screen: [], total_lines: 0 }));
document.body.style.cssText = "margin:0;background:#151515;color:#ddd";
document.getElementById("root")!.style.cssText = "height:100vh;display:flex;flex-direction:column";
render(() => <OutputView sessionId="recorded-coordinator" />, document.getElementById("root")!);
