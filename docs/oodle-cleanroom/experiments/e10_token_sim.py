# E10: test token hypothesis H10 on a record whose two streams are stored (type 0):
#   stream0 = literal bytes (raw), stream1 = one byte per command.
#   command byte c >= 0x10 (provisional): bit7 = reuse previous offset, bits3..6 = match length,
#   bits0..2 = literal count. Commands < 0x10 are "special"; we print the gold context there.
# For new-offset commands we list all offsets <= 64K consistent with the gold bytes.
import sys
from cr import META, load, records, stream
cid, idx, rec = sys.argv[1], int(sys.argv[2]), int(sys.argv[3])
m = [x for x in META if x["codec"] == cid and x["id"] == idx][0]
b, g = load(m)
for qi, go, i, t, d, s in records(b, g):
    if qi != 0 or i != rec: continue
    q = 8 if go == 0 else 0
    _, _, lits, q = stream(d, q); _, _, cmds, q = stream(d, q)
    pos, lp, last = go + q if False else go + (8 if go == 0 else 0), 0, None
    for n, c in enumerate(cmds):
        if c < 0x10:
            print("#%d special %02x at gold+%x lits_used=%d ; gold ahead:" % (n, c, pos - go, lp), g[pos:pos+24].hex()); 
            # skip: assume it is a long match; find longest match over small offsets
            best = max(((k, o) for o in range(1, min(pos, 65536)+1) for k in [next((j for j in range(0, 1<<17) if pos+j >= len(g) or g[pos+j] != g[pos+j-o]), 0)]), default=(0,0))
            print("     longest match: len %d off %d" % best); pos += best[0]; last = best[1]; continue
        ll, ml, rep = c & 7, (c >> 3) & 15, c >> 7
        ok_l = g[pos:pos+ll] == lits[lp:lp+ll]; pos += ll; lp += ll
        if rep:
            ok = last is not None and all(g[pos+j] == g[pos+j-last] for j in range(ml))
            print("#%d %02x lit%d(%s) rep ml%d off%s -> %s" % (n, c, ll, ok_l, ml, last, ok))
        else:
            cands = [o for o in range(1, min(pos, 65536)+1) if all(g[pos+j] == g[pos+j-o] for j in range(ml))]
            print("#%d %02x lit%d(%s) NEW ml%d cands" % (n, c, ll, ok_l, ml), cands[:12], len(cands))
            last = cands[0] if len(cands) == 1 else (cands[0] if cands else None)
        pos += ml
    print("end pos", hex(pos - go), "lits used", lp, "of", len(lits), "remainder", d[q:].hex())
