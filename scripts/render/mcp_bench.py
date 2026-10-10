"""wowsmod MCP 出图计时: 起 `wowsmod mcp`(stdio JSON-RPC), 调 wowsmod_render 若干次, 打印每次耗时。
用法: python mcp_bench.py <wowsmod.exe> <项目> [次数=3]
"""
import json, subprocess, sys, time, os, tempfile

exe, proj = sys.argv[1], sys.argv[2]
n = int(sys.argv[3]) if len(sys.argv) > 3 else 3
env = dict(os.environ)
env.pop("WOWSMOD_PROJECT", None)
p = subprocess.Popen([exe, "mcp"], stdin=subprocess.PIPE, stdout=subprocess.PIPE, cwd=tempfile.gettempdir(), env=env)
seq = 0


def call(method, params=None, notify=False):
    global seq
    msg = {"jsonrpc": "2.0", "method": method}
    if params is not None:
        msg["params"] = params
    if not notify:
        seq += 1
        msg["id"] = seq
    p.stdin.write((json.dumps(msg) + "\n").encode())
    p.stdin.flush()
    if notify:
        return None
    while True:
        line = p.stdout.readline()
        if not line:
            raise SystemExit("mcp closed")
        r = json.loads(line)
        if r.get("id") == seq:
            return r


call("initialize", {"protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": {"name": "bench", "version": "1"}})
call("notifications/initialized", {}, notify=True)
res = []
for i in range(n):
    t = time.time()
    r = call("tools/call", {"name": "wowsmod_render", "arguments": {"project": proj, "compare": True, "query": "view=fq&compare=split&base=base&w=1600&h=900"}})
    dt = time.time() - t
    c = r.get("result", {}).get("content", [])
    img = [x for x in c if x.get("type") == "image"]
    txt = [x.get("text", "") for x in c if x.get("type") == "text"]
    info = {}
    try:
        info = json.loads(txt[0]) if txt else {}
    except Exception:
        pass
    print(json.dumps({"i": i, "seconds": round(dt, 3), "image_bytes": len(img[0]["data"]) * 3 // 4 if img else 0, "isError": r.get("result", {}).get("isError"), "warmed_now": info.get("warmed_now"), "text": "" if img else (txt[0][:300] if txt else str(r)[:300])}, ensure_ascii=False))
    res.append(dt)
p.stdin.close()
p.wait(timeout=10)
print(json.dumps({"mcp_alive_after": True, "times": [round(x, 3) for x in res]}))
