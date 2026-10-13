# E03: hypothesis H3: block = sequence of quanta; each quantum = 2-byte hdr + 3-byte BE field F,
# payload F+1 bytes (so quantum = F+6 bytes), each quantum decodes to min(0x40000, remaining) bytes.
import os, json, collections
R = os.environ.get("OODLE_CORPUS", r"D:\WoWSModding\oodle_corpus")
meta = [json.loads(l) for l in open(os.path.join(R, "meta.jsonl"))]
ok = collections.Counter(); bad = []
fbits = collections.Counter()
for m in meta:
    p = os.path.join(R, m["codec"], "%06d" % m["id"])
    b = open(p + ".blk", "rb").read()
    pos, out, hs = 0, m["dest_len"], []
    good = True
    while pos < len(b):
        h = b[pos:pos+2].hex(); hs.append(h)
        if h[0:2] == "cc":
            n = min(out, 0x40000); pos += 2 + n; out -= n; continue
        if pos + 5 > len(b): good = False; break
        F = int.from_bytes(b[pos+2:pos+5], "big"); fbits[F >> 18] += 1
        pos += 6 + (F & 0x3ffff); out -= min(out, 0x40000)
    good = good and pos == len(b) and out == 0
    ok[(b[:2].hex(), good)] += 1
    if not good and len(bad) < 6: bad.append((m["codec"], m["id"], len(b), m["dest_len"], hs[:5], b[:12].hex()))
print(ok); print("F>>18:", fbits)
for x in bad: print(x)
