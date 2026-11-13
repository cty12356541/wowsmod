"""End-to-end apply/uninstall CLI smoke using a synthetic client only."""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import struct
import subprocess

from check_library import make_project, snapshot


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--exe", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    root = args.out.resolve()
    root.mkdir(parents=True, exist_ok=False)
    client = root / "fake-client"
    rm = client / "bin" / "123" / "res_mods"
    idx = client / "bin" / "123" / "idx"
    rm.mkdir(parents=True)
    idx.mkdir()
    (client / "res_packages").mkdir()
    # Empty IDX v40 fixture, matching wowsmod_formats::idx constants/header.
    (idx / "empty.idx").write_bytes(struct.pack("<II", 0x50465349, 0x02000000) + bytes(48))
    (rm / "fixture.txt").write_bytes(b"original foreign resource")
    (rm / "readme.txt").write_bytes(b"untouched fixture client")
    original = snapshot(rm)
    env = dict(os.environ, WOWSMOD_HOME=str(root / "home"))
    base = [str(args.exe.resolve()), "--json", "--game", str(client)]
    flags = subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0
    evidence: list[dict] = []

    def run(*words: str, success: bool = True) -> dict:
        result = subprocess.run(base + list(words), env=env, capture_output=True,
                                text=True, encoding="utf-8", timeout=60, creationflags=flags)
        response = json.loads(result.stdout)
        assert (result.returncode == 0) == success, (words, result.returncode, result.stdout, result.stderr)
        assert (response.get("ok") is True) == success, response
        evidence.append({"command": list(words), "exit": result.returncode, "response": response})
        return response

    a, b, c, other = [root / "projects" / name for name in ("A", "B", "C", "Other")]
    make_project(a, "SamePnF", "FSB510_Bourgogne")
    make_project(b, "SamePnF", "PFSB510")
    make_project(c, "DifferentPnF", "FSB510_Bourgogne")
    make_project(other, "OtherShip", "PASD001_Unknown")
    for project in (a, b, c, other):
        (project / "assets" / "res_mods" / "fixture.txt").write_text(project.name, encoding="utf-8")

    before = snapshot(root)
    first = run("apply", str(a), "--dry-run")
    assert first["dry_run"] is True and first["previous"] == []
    assert snapshot(root) == before, "initial dry run wrote files"
    run("apply", str(a))
    assert (rm / "fixture.txt").read_text() == "A"
    before = snapshot(root)
    plan = run("apply", str(b), "--dry-run")
    assert plan["previous"][0]["name"] == "SamePnF" and plan["build"] == "123"
    assert snapshot(root) == before, "replacement dry run wrote files"
    run("apply", str(b))
    assert (rm / "fixture.txt").read_text() == "B"
    before = snapshot(root)
    run("uninstall", str(a), success=False)
    assert snapshot(root) == before, "old same-name project removed active variant"
    run("apply", str(c))
    before = snapshot(root)
    run("uninstall", str(b), success=False)
    assert snapshot(root) == before, "stale managed manifest removed replacement resources"
    run("apply", str(other), success=False)
    assert snapshot(root) == before, "other-ship conflict altered current install"
    verified = run("installed", "--verify")
    assert "DifferentPnF" in json.dumps(verified), verified
    run("uninstall", str(c))
    assert snapshot(rm) == original, "uninstall did not restore original client bytes"
    report = {"ok": True, "real_game_used": False, "client_restored": True,
              "commands": len(evidence), "evidence": evidence}
    (root / "report.json").write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding="utf-8")
    print(json.dumps({k: v for k, v in report.items() if k != "evidence"}, ensure_ascii=False))


if __name__ == "__main__":
    main()
