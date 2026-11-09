#!/usr/bin/env python3
"""Collect the results of tools/figma-export.js and write assets/.

The export runs through the Figma MCP `use_figma` tool, so its JSON results
end up in tool output. Pass files (or a Claude Code session transcript,
.jsonl) that contain those results:

    tools/import-figma.py RESULT.json [MORE.json | transcript.jsonl ...]

Writes:
    assets/parts/<hash>.svg         one file per unique layer, as exported
    assets/animations/<name>.json   one timeline per animation and edge
and prints the hashes already known (to pass as KNOWN on the next export).
"""
import json
import re
import sys
import unicodedata
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PARTS = ROOT / "assets" / "parts"
ANIMS = ROOT / "assets" / "animations"


def results_in(path):
    text = Path(path).read_text()
    if path.endswith(".jsonl"):
        for line in text.splitlines():
            try:
                entry = json.loads(line)
            except json.JSONDecodeError:
                continue
            content = entry.get("message", {}).get("content")
            if not isinstance(content, list):
                continue
            for block in content:
                if block.get("type") != "tool_result":
                    continue
                inner = block.get("content")
                texts = [inner] if isinstance(inner, str) else [
                    c.get("text", "") for c in inner or [] if isinstance(c, dict)
                ]
                for t in texts:
                    yield from parse(t)
    else:
        yield from parse(text)


def parse(text):
    text = text.strip()
    if not text.startswith("{") or '"frames"' not in text or '"svgs"' not in text:
        return
    try:
        data = json.loads(text)
    except json.JSONDecodeError:
        return
    # Only the hash-keyed format of the current exporter.
    if all(re.fullmatch(r"[0-9a-f]{16}", k) for k in data.get("svgs", {})):
        yield data


def slug(name):
    """'Done – morning – Left edge' -> 'done-morning-left'."""
    name = re.sub(r"\(.*?\)", "", name).replace(" edge", "")
    name = unicodedata.normalize("NFKD", name).encode("ascii", "ignore").decode()
    return re.sub(r"[^a-z0-9]+", "-", name.lower()).strip("-")


def main(paths):
    PARTS.mkdir(parents=True, exist_ok=True)
    ANIMS.mkdir(parents=True, exist_ok=True)
    frames = {}
    for path in paths:
        for data in results_in(path):
            for h, svg in data["svgs"].items():
                (PARTS / f"{h}.svg").write_text(svg + "\n")
            for f in data["frames"]:
                frames[f["id"]] = f
            for w in data.get("warnings", []):
                print("warning:", w, file=sys.stderr)
    for f in frames.values():
        anim = {
            "name": f["name"],
            "figma_node": f["id"],
            "width": f["w"],
            "height": f["h"],
            "duration": f["duration"],
            "nodes": f["nodes"],
        }
        out = ANIMS / f"{slug(f['name'])}.json"
        out.write_text(json.dumps(anim, ensure_ascii=False, separators=(",", ":")) + "\n")
    used = {n["svg"] for f in frames.values() for n in f["nodes"] if "svg" in n}
    for p in PARTS.glob("*.svg"):
        if p.stem not in used:
            p.unlink()
    missing = sorted({n["svg"] for f in frames.values() for n in f["nodes"] if "svg" in n} - {p.stem for p in PARTS.glob("*.svg")})
    print(f"{len(frames)} animations, {len(list(PARTS.glob('*.svg')))} parts", file=sys.stderr)
    if missing:
        print("missing parts:", missing, file=sys.stderr)
    print(json.dumps(sorted(p.stem for p in PARTS.glob("*.svg"))))


if __name__ == "__main__":
    main(sys.argv[1:])
