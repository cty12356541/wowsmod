"""Exercise the real CLI and HTTP library against disposable, no-game fixtures.

The fixture tree is retained for local workbench inspection. No real client is
detected: every process receives an explicit nonexistent --game directory.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import socket
import subprocess
import time
import urllib.error
import urllib.request


def snapshot(root: Path) -> dict[str, str]:
    return {
        str(p.relative_to(root)): hashlib.sha256(p.read_bytes()).hexdigest()
        for p in sorted(root.rglob("*")) if p.is_file()
    }


def make_project(root: Path, name: str, ship: str, mode: str = "sdk") -> None:
    root.mkdir(parents=True)
    (root / "mod.toml").write_text(
        f'[mod]\nname = "{name}"\nship = "{ship}"\nversion = "0.1.0"\n'
        f'[install]\nmode = "{mode}"\n', encoding="utf-8"
    )
    (root / "assets" / "res_mods").mkdir(parents=True)
    (root / "assets" / "res_mods" / "fixture.txt").write_text(name, encoding="utf-8")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--exe", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    exe, output = args.exe.resolve(), args.out.resolve()
    output.mkdir(parents=True, exist_ok=False)
    projects = output / "projects"
    env = dict(os.environ, WOWSMOD_HOME=str(output / "home"))
    command = [str(exe), "--json", "--game", str(output / "no-game")]
    flags = subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0
    results: list[str] = []

    def run(*words: str) -> dict:
        completed = subprocess.run(command + list(words), env=env, capture_output=True,
                                   text=True, encoding="utf-8", timeout=60, creationflags=flags)
        if completed.returncode:
            raise AssertionError(f"CLI {words}: {completed.stdout}\n{completed.stderr}")
        data = json.loads(completed.stdout)
        assert data.get("ok") is True, data
        return data

    def check(condition: bool, label: str) -> None:
        if not condition:
            raise AssertionError(label)
        results.append(label)

    a, b, c = [projects / n for n in ("robin-a", "robin-b", "robin-c")]
    for root in (a, b, c):
        make_project(root, "same-pnf-name", "FSB510_Bourgogne")
    make_project(projects / "second-ship", "other-ship", "PASD001_Unknown")
    make_project(projects / "global", "global-ui", "", "loose")
    make_project(projects / "assets" / "pruned", "must-not-register", "FSB510_Bourgogne")
    before = snapshot(projects)
    a_entry = run("library", "add", str(a))["entry"]
    duplicate = run("library", "add", str(a / "."))["entry"]
    check(a_entry["id"] == duplicate["id"], "canonical duplicate preserves ID")
    scan = run("library", "scan", str(projects))
    check(len(scan["entries"]) == 5, "bounded scan prunes assets and finds five projects")
    listing = run("library", "list")["entries"]
    check(len({p["id"] for p in listing}) == 5, "same names have distinct identities")
    changed = run("library", "update", a_entry["id"], "--name", "知更鸟 · 展示方案",
                  "--favorite", "true", "--status", "review", "--tag", "知更鸟")["entry"]
    check(changed["favorite"] and changed["status"] == "review", "metadata edits persist")
    check(snapshot(projects) == before, "registration, listing and metadata preserve all project bytes")
    b_entry = next(p for p in listing if Path(p["path"]).name == b.name)
    removed = run("library", "remove", b_entry["id"])
    check(removed["entry"]["id"] == b_entry["id"] and b.is_dir(), "unregister retains source directory")
    run("library", "add", str(b))
    relocated = projects / "robin-a-moved"
    a.rename(relocated)
    moved = run("library", "relocate", a_entry["id"], str(relocated))["entry"]
    check(moved["id"] == a_entry["id"], "relocation preserves project ID")
    missing = projects / "missing"
    make_project(missing, "lost-project", "PFSB510_Bourgogne")
    missing_entry = run("library", "add", str(missing))["entry"]
    missing.rename(projects / "missing-moved-without-relocate")
    listing = run("library", "list")["entries"]
    check(next(p for p in listing if p["id"] == missing_entry["id"])["available"] is False,
          "missing source remains visible and unavailable")

    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        port = sock.getsockname()[1]
    base = f"http://127.0.0.1:{port}"

    def request(path: str, body: dict | None = None, origin: str | None = None):
        headers = {"Content-Type": "application/json"} if body is not None else {}
        if origin:
            headers["Origin"] = origin
        req = urllib.request.Request(base + path, headers=headers,
                                     data=None if body is None else json.dumps(body).encode())
        try:
            with urllib.request.urlopen(req, timeout=20) as response:
                return response.status, json.load(response)
        except urllib.error.HTTPError as error:
            return error.code, json.load(error)

    with (output / "server.log").open("wb") as log:
        process = subprocess.Popen(command + ["preview", "--library", "--no-open", "--port", str(port)],
                                   env=env, stdout=log, stderr=subprocess.STDOUT, creationflags=flags)
        try:
            deadline = time.monotonic() + 30
            while True:
                if process.poll() is not None:
                    raise AssertionError((output / "server.log").read_text(encoding="utf-8"))
                try:
                    status, data = request("/api/v1/library")
                    break
                except (OSError, urllib.error.URLError):
                    if time.monotonic() >= deadline:
                        raise
                    time.sleep(0.2)
            check(status == 200 and len(data["projects"]) == 6, "HTTP lists all projects without a game")
            status, opened = request(f'/api/v1/library/projects/{a_entry["id"]}/open', {})
            check(status == 200 and a_entry["id"] in str(opened), "open returns explicit project URL")
            status, _ = request("/api/v1/projects/does-not-exist/project")
            check(status == 404, "unknown project cannot reuse another session")
            status, _ = request(f'/api/v1/projects/{missing_entry["id"]}/project')
            check(status >= 400, "missing project returns actionable error")
            status, _ = request(f'/api/v1/library/projects/{a_entry["id"]}/update',
                                {"favorite": False}, "https://untrusted.example")
            check(status == 403, "foreign-origin mutation is rejected")
            after_rejection = run("library", "list")["entries"]
            check(next(p for p in after_rejection if p["id"] == a_entry["id"])["favorite"] is True,
                  "rejected mutation leaves catalog unchanged")
            for entry in (a_entry, b_entry):
                status, project = request(f'/api/v1/projects/{entry["id"]}/project')
                check(status == 200 and project["project"]["name"] == "same-pnf-name",
                      f'project session works without a model: {entry["id"]}')
                check(project["install"]["state"] != "installed", "same-name projects are not falsely installed")
            (output / "library.json").write_text(json.dumps(data, ensure_ascii=False, indent=2), encoding="utf-8")
        finally:
            process.terminate()
            try:
                process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=10)
    report = {"ok": True, "checks": results, "fixtures": str(output), "real_game_used": False}
    (output / "report.json").write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding="utf-8")
    print(json.dumps(report, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
