# E04: inside an 8c0a quantum payload (F+1 bytes after the 5-byte quantum header), test hypothesis
# H4: payload = concatenation of records, each = 3-byte BE header H, then (H & MASK) bytes.
import os, json, collections, sys
R = os.environ.get("OODLE_CORPUS", r"D:\WoWSModding\oodle_corpus")
MASK = int(sys.argv[1], 16) if len(sys.argv) > 1 else 0x3ffff
meta = [json.loads(l) for l in open(os.path.join(R, "meta.jsonl"))]
res = collections.Counter(); nrec = collections.Counter(); top = collections.Counter(); ex = []
for m in meta:
    p = os.path.join(R, m["codec"], "%06d" % m["id"])
    b = open(p + ".blk", "rb").read()
    if b[:2].hex() != "8c0a": continue
    pos = 0
    while pos < len(b):
        F = int.from_bytes(b[pos+2:pos+5], "big"); pl = b[pos+5:pos+6+F]; pos += 6 + F
        q, k, tops = 0, 0, []
        while q + 3 <= len(pl):
            H = int.from_bytes(pl[q:q+3], "big"); tops.append(H >> 18); q += 3 + (H & MASK); k += 1
        good = q == len(pl); res[good] += 1
        if good: nrec[k] += 1; top[tuple(tops)] += 1
        elif len(ex) < 8: ex.append((m["codec"], m["id"], len(pl), pl[:16].hex()))
print("exact cover:", res); print("records:", sorted(nrec.items())); print("top-bit patterns:", top.most_common(12))
for e in ex: print(e)
