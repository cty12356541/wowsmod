# E09: full dump of one record (streams + remainder) next to gold, for manual inspection.
import sys
from cr import META, load, records, stream
cid, idx, rec = sys.argv[1], int(sys.argv[2]), int(sys.argv[3])
gl = int(sys.argv[4]) if len(sys.argv) > 4 else 512
m = [x for x in META if x["codec"] == cid and x["id"] == idx][0]
b, g = load(m)
for qi, go, i, t, d, s in records(b, g):
    if qi != 0 or i != rec: continue
    q = 8 if go == 0 else 0
    print("record len", len(d), "top6", t, "verbatim", d[:q].hex())
    for k in range(2):
        T, A, p, q = stream(d, q); print("stream", k, "type", T, "declen", A, "len", len(p)); print("  ", p.hex())
    print("remainder @", q, len(d) - q); print("  ", d[q:].hex())
    print("gold share", len(s))
    for o in range(0, min(gl, len(s)), 32): print("  %06x" % o, s[o:o+32].hex())
