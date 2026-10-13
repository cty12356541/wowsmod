# E05: relate records to output. H5a: record count = ceil(quantum_out / 0x20000).
# H5b: a record with top6==32 whose length equals its output share is a verbatim copy of gold.
import os, json, collections
R = os.environ.get("OODLE_CORPUS", r"D:\WoWSModding\oodle_corpus")
meta = [json.loads(l) for l in open(os.path.join(R, "meta.jsonl"))]
cnt = collections.Counter(); raw = collections.Counter(); ex = []
for m in meta:
    p = os.path.join(R, m["codec"], "%06d" % m["id"])
    b = open(p + ".blk", "rb").read()
    if b[:2].hex() != "8c0a": continue
    g = open(p + ".gold", "rb").read()
    pos, gpos = 0, 0
    while pos < len(b):
        F = int.from_bytes(b[pos+2:pos+5], "big"); pl = b[pos+5:pos+6+F]; pos += 6 + F
        qout = min(0x40000, len(g) - gpos); recs = []; q = 0
        while q < len(pl):
            H = int.from_bytes(pl[q:q+3], "big"); recs.append((H >> 18, pl[q+3:q+3+(H & 0x3ffff)])); q += 3 + (H & 0x3ffff)
        cnt[(len(recs), -(-qout // 0x20000))] += 1
        for i, (t, d) in enumerate(recs):
            share = g[gpos + i*0x20000: gpos + min(qout, (i+1)*0x20000)]
            raw[(t, len(d) == len(share), d == share)] += 1
            if t != 34 and len(ex) < 10: ex.append((m["codec"], m["id"], i, t, len(d), len(share), d[:12].hex(), share[:8].hex()))
        gpos += qout
print("(records, ceil(out/128K)):", cnt); print("(top6, len==share, bytes==share):", raw)
for e in ex: print(e)
