# E02: dump header bytes of 8c0a blocks, sorted by size.
import os, json
R = os.environ.get("OODLE_CORPUS", r"D:\WoWSModding\oodle_corpus")
meta = [json.loads(l) for l in open(os.path.join(R, "meta.jsonl"))]
rows = []
for m in meta:
    p = os.path.join(R, m["codec"], "%06d" % m["id"])
    b = open(p + ".blk", "rb").read()
    if b[:2].hex() != "8c0a": continue
    rows.append((len(b), m["dest_len"], m["codec"], m["id"], b[2:14].hex()))
rows.sort()
for r in rows[:25] + rows[len(rows)//2:len(rows)//2+10] + rows[-10:]: print(r)
