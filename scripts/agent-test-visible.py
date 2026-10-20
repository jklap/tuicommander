#!/usr/bin/env python3
"""Authorized macOS native regression, not part of the unattended test suite.

Observe the real agent-test window through its own Unix socket, then stop it.
Keep an unrelated application in the foreground throughout the run. See the
agent-test section in docs/guides/development-setup.md for prerequisites.
"""
import argparse
import base64
import json
import os
from pathlib import Path
import subprocess
import sys
import time
import uuid

ROOT = Path(__file__).resolve().parent.parent
TEST = "agent_test_window_renders_visible_without_focus"


def frontmost():
    script = 'ObjC.import("AppKit"); JSON.stringify({pid:$.NSWorkspace.sharedWorkspace.frontmostApplication.processIdentifier,name:$.NSWorkspace.sharedWorkspace.frontmostApplication.localizedName.js})'
    return json.loads(subprocess.check_output(
        ["osascript", "-l", "JavaScript", "-e", script], text=True))


def api(socket, path, body=None, method=None):
    command = ["curl", "--max-time", "10", "--silent", "--show-error",
               "--fail", "--unix-socket", socket, "-H", "content-type: application/json"]
    if method:
        command += ["-X", method]
    if body is not None:
        command += ["--data-binary", json.dumps(body)]
    return json.loads(subprocess.check_output(command + ["http://localhost" + path], text=True, stderr=subprocess.PIPE))


def evaluate(socket, script, evidence):
    # Identical eval logs coalesce without changing their ID. Correlate the
    # request itself, not a log ID or another observer's newest result.
    nonce = uuid.uuid4().hex
    wrapped = f"""
try {{
 const result = await (async () => {{ {script} }})();
 return {{nonce: {json.dumps(nonce)}, result}};
}} catch (error) {{
 return {{nonce: {json.dumps(nonce)}, error: String(error)}};
}}
"""
    reply = api(socket, "/debug/invoke_js", {"script": wrapped})
    if not reply.get("ok"):
        raise RuntimeError(f"native evaluation refused: {reply}")
    # Harness budget is larger than the ten-second paint observation below.
    deadline = time.monotonic() + 30
    while time.monotonic() < deadline:
        rows = api(socket, "/logs?source=eval_js&limit=20")
        for row in rows:
            try:
                response = json.loads(row["message"].split("\n---\n")[-1])
            except (ValueError, KeyError):
                continue
            if not isinstance(response, dict) or response.get("nonce") != nonce:
                continue
            (evidence / "eval.json").write_text(json.dumps(row, indent=2))
            if "error" in response:
                raise RuntimeError(f"native evaluation failed: {response['error']}")
            return response.get("result")
        time.sleep(.1)
    raise RuntimeError(f"native evaluation {nonce} did not return within the harness budget")


def agent_test_window_renders_visible_without_focus(instance, port, evidence):
    if sys.platform != "darwin" or not (ROOT / ".git").is_file():
        raise RuntimeError("native observer requires an authorized macOS linked worktree")
    evidence = evidence.resolve()
    if not evidence.is_relative_to(Path.home() / "Gits"):
        raise ValueError("evidence directory must be under ~/Gits")
    evidence.mkdir(parents=True, exist_ok=False)
    variables = [f"AGENT_TEST_INSTANCE={instance}", f"AGENT_TEST_PORT={port}"]
    launch = json.loads(subprocess.check_output(
        ["make", "--no-print-directory", "agent-test", "AGENT_TEST_DRY_RUN=1", *variables],
        cwd=ROOT, text=True))
    (evidence / "launch.json").write_text(json.dumps(launch, indent=2))
    pid_file = Path(launch["pid_file"])
    if pid_file.exists():
        raise RuntimeError("selected agent-test instance already has a supervisor; not touching it")
    freeze = Path.home() / "Gits/.tmp/BUILD_FREEZE"
    if freeze.exists():
        raise RuntimeError("BUILD_FREEZE is present; do not launch a native build")
    before = frontmost()
    if not before.get("pid") or before["name"] == "tuicommander-agent-test":
        raise RuntimeError("an unrelated application must be in the foreground")
    (evidence / "front-before.json").write_text(json.dumps(before, indent=2))
    owned_pid = None
    sessions_before = set()
    drove_terminal = False
    cleanup = {}
    # mbx creates a Unix socket under TMPDIR: the worktree test wrapper's long
    # scratch path exceeds macOS SUN_LEN. Scope this short Gits path to the build.
    build_environment = {**os.environ, "TMPDIR": str(Path.home() / "Gits/.tmp") + "/"}
    with (evidence / "desktop.log").open("w") as log:
        child = subprocess.Popen(["make", "agent-test", *variables], cwd=ROOT,
                                 env=build_environment, stdout=log, stderr=subprocess.STDOUT)
        try:
            # Setup/build budget, not the behavior's paint deadline.
            deadline = time.monotonic() + 3600
            while time.monotonic() < deadline:
                if child.poll() is not None:
                    raise RuntimeError(f"agent-test launch exited {child.returncode}; see desktop.log")
                if pid_file.exists() and Path(launch["socket"]).exists():
                    owned_pid = int(pid_file.read_text())
                    executable = subprocess.check_output(
                        ["ps", "-p", str(owned_pid), "-o", "comm="], text=True).strip()
                    if executable != launch["binary"]:
                        owned_pid = None
                        raise RuntimeError("runtime PID is not this worktree's binary; refusing to drive it")
                    try:
                        api(launch["socket"], "/health")
                    except subprocess.CalledProcessError as error:
                        if error.returncode not in (7, 52, 56):
                            raise
                        time.sleep(.2)
                        continue
                    break
                time.sleep(.2)
            else:
                raise RuntimeError("native build/start did not finish within the setup budget")
            frontend_deadline = time.monotonic() + 60
            while time.monotonic() < frontend_deadline:
                try:
                    if evaluate(launch["socket"], """return Boolean(window.__TUIC__ && document.querySelector('button[title^="New Tab"]'));""", evidence):
                        break
                except (RuntimeError, ValueError) as error:
                    cleanup["frontend_setup_last_error"] = str(error)
                time.sleep(.2)
            else:
                raise RuntimeError("frontend controls did not reach the setup precondition")
            ready = frontmost()
            sessions_before = {row["session_id"] for row in api(launch["socket"], "/sessions")}
            if sessions_before:
                raise RuntimeError("observer requires an empty instance, not existing sessions")
            drove_terminal = True
            result = evaluate(launch["socket"], '''
const draws=[]; const original=CanvasRenderingContext2D.prototype.fillText;
CanvasRenderingContext2D.prototype.fillText=function(text,...args){
 if(draws.length<40)draws.push(String(text)); return original.call(this,text,...args);
};
try {
 const button=document.querySelector('button[title^="New Tab"]');
 if(!button)throw new Error('New Tab control missing');
 button.click();
 await new Promise(resolve=>{const start=performance.now();const check=()=>{
  if(draws.length>0||performance.now()-start>10000)resolve();else setTimeout(check,50);
 };check();});
 const w=window.__TAURI__.window.getCurrentWindow();
 return {visibility:document.visibilityState,focus:document.hasFocus(),
  nativeFocus:await w.isFocused(),visible:await w.isVisible(),
  position:await w.outerPosition(),size:await w.outerSize(),innerSize:await w.innerSize(),
  monitor:await window.__TAURI__.window.primaryMonitor(),draws,
  canvases:[...document.querySelectorAll('canvas')].map(c=>({
   width:c.width,height:c.height,rect:c.getBoundingClientRect().toJSON(),png:c.toDataURL()
  }))};
}finally{CanvasRenderingContext2D.prototype.fillText=original;}
''', evidence)
            after = frontmost()
            for index, canvas in enumerate(result["canvases"]):
                png = canvas.pop("png")
                if png.startswith("data:image/png;base64,"):
                    (evidence / f"terminal-{index}.png").write_bytes(base64.b64decode(png.split(",")[1]))
            result.update(owned_pid=owned_pid, front_before=before, front_ready=ready, front_after=after)
            failures = []
            if result["visibility"] != "visible" or not result["visible"]:
                failures.append("native window/document is hidden")
            if result["focus"] or result["nativeFocus"]:
                failures.append("agent-test took keyboard focus")
            if not any(c["rect"]["width"] > 0 and c["rect"]["height"] > 0 for c in result["canvases"]):
                failures.append("terminal canvas has no visible area")
            if not result["draws"]:
                failures.append("terminal did not paint real text within ten seconds")
            if ready["pid"] != before["pid"] or after["pid"] != before["pid"] or after["pid"] == owned_pid:
                failures.append("unrelated frontmost PID changed during launch/paint")
            monitor = result["monitor"]
            if monitor is None:
                failures.append("primary monitor geometry unavailable")
            else:
                scale = monitor["scaleFactor"]
                if result["innerSize"]["width"] / scale > 650 or result["innerSize"]["height"] / scale > 430:
                    failures.append("agent-test viewport is not compact")
                area = monitor["workArea"]
                for axis, size in [("x", "width"), ("y", "height")]:
                    gap = area["position"][axis] + area["size"][size] - result["position"][axis] - result["size"][size]
                    if not 0 <= gap <= 64 * scale:
                        failures.append(f"window is not at the primary work-area corner ({axis} gap={gap})")
            result["failures"] = failures
            (evidence / "observer.json").write_text(json.dumps(result, indent=2))
            if failures:
                raise AssertionError("; ".join(failures))
        finally:
            if owned_pid is not None:
                try:
                    if drove_terminal:
                        for row in api(launch["socket"], "/sessions"):
                            if row["session_id"] not in sessions_before:
                                api(launch["socket"], "/sessions/" + row["session_id"], method="DELETE")
                        cleanup["sessions_remaining"] = api(launch["socket"], "/sessions")
                except (OSError, ValueError, subprocess.SubprocessError) as error:
                    cleanup["session_cleanup_error"] = str(error)
                finally:
                    stop = subprocess.run(["make", "agent-test-stop", *variables], cwd=ROOT,
                                          text=True, capture_output=True)
                    cleanup.update(stop_exit=stop.returncode, stop_stdout=stop.stdout, stop_stderr=stop.stderr)
            if child.poll() is None and owned_pid is None:
                child.terminate()
            cleanup["supervisor_exit"] = child.wait(timeout=30)
            ps = subprocess.check_output(["ps", "-axo", "pid=,comm="], text=True)
            cleanup["worktree_processes_remaining"] = [line for line in ps.splitlines() if launch["binary"] in line]
            (evidence / "cleanup.json").write_text(json.dumps(cleanup, indent=2))
            if cleanup.get("stop_exit", 0) or cleanup.get("session_cleanup_error") or cleanup["worktree_processes_remaining"]:
                raise RuntimeError("owned instance cleanup failed; see cleanup.json")
    print(f"PASS {TEST}; evidence={evidence}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--instance", default="at-2090")
    parser.add_argument("--port", default="19883")
    parser.add_argument("--evidence-dir", type=Path, required=True)
    args = parser.parse_args()
    try:
        agent_test_window_renders_visible_without_focus(args.instance, args.port, args.evidence_dir)
    except (AssertionError, OSError, RuntimeError, ValueError, subprocess.SubprocessError) as error:
        print(f"FAIL {TEST}: {error}; evidence={args.evidence_dir}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
