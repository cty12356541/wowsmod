# E01: cc06 framing. Black-box: compare .blk bytes with .gold.
import os, json, collections
R = os.environ.get("OODLE_CORPUS", r"D:\WoWSModding\oodle_corpus")
meta = [json.loads(l) for l in open(os.path.join(R, "meta.jsonl"))]
res = collections.Counter(); ex = []
for m in meta:
    p = os.path.join(R, m["codec"], "%06d" % m["id"])
    b = open(p + ".blk", "rb").read()
    if b[:2].hex() != "cc06": continue
    g = open(p + ".gold", "rb").read()
    key = (len(b) - len(g), b[2:] == g, m["dest_len"] == len(g))
    res[key] += 1
    if len(ex) < 5: ex.append((m["codec"], m["id"], len(b), len(g), b[:8].hex(), g[:6].hex()))
print("(len(blk)-len(gold), blk[2:]==gold, dest_len==len(gold)) -> count")
for k, v in res.items(): print(k, v)
for e in ex: print(e)
