# E11: with H10 tokens and H11 offset table (after the two streams: u16 X, u16 N, then N u16 LE
# distances consumed in order by every NEW-offset command, including special 0x01), find for each
# special command the set of lengths consistent with gold (the following command must then verify).
import sys
from cr import META, load, records, stream
cid, idx, rec = sys.argv[1], int(sys.argv[2]), int(sys.argv[3])
m = [x for x in META if x["codec"] == cid and x["id"] == idx][0]
b, g = load(m)
def u16(d, o): return d[o] | d[o+1] << 8
for qi, go, i, t, d, s in records(b, g):
    if qi != 0 or i != rec: continue
    q = 8 if go == 0 else 0
    _, _, lits, q = stream(d, q); _, _, cmds, q = stream(d, q)
    X, N = u16(d, q), u16(d, q+2); offs = [u16(d, q+4+2*k) for k in range(N)]
    tail = d[q+4+2*N:]
    print("X", X, "N", N, "offs", offs); print("tail", len(tail), tail.hex())
    def run(pos, lp, last, oi, n):
        """returns list of (n, pos, lp, last, oi) states after processing command n"""
        c = cmds[n]; res = []
        if c < 0x10:
            off = offs[oi] if c == 1 else last
            oi2 = oi + (c == 1)
            L = 0
            while pos + L < go + len(s) and g[pos+L] == g[pos+L-off]: L += 1
            return [(pos + l, lp, off, oi2, l) for l in range(1, L+1)]
        ll, ml, rep = c & 7, (c >> 3) & 15, c >> 7
        if g[pos:pos+ll] != lits[lp:lp+ll]: return []
        pos += ll; lp += ll
        off = last if rep else offs[oi]; oi += 0 if rep else 1
        if any(g[pos+j] != g[pos+j-off] for j in range(ml)): return []
        return [(pos + ml, lp, off, oi, None)]
    state = (go + q*0 + (8 if go == 0 else 0), 0, None, 0)
    for n in range(len(cmds)):
        outs = run(*state, n)
        if cmds[n] < 0x10:
            # keep lengths for which the next command verifies
            good = [o for o in outs if n+1 >= len(cmds) or run(*o[:4], n+1)]
            print("#%d special %02x off %d valid lengths %s" % (n, cmds[n], good[0][2] if good else -1, [o[4] for o in good][:6]))
            if not good: break
            state = good[0][:4]
        else:
            if not outs: print("#%d %02x FAIL" % (n, cmds[n])); break
            state = outs[0][:4]
    print("final pos", hex(state[0] - go), "lits used", state[1], "/", len(lits), "offs used", state[3], "/", N)
