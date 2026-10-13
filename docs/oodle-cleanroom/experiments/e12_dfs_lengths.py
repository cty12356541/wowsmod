# E12: like E11 but search over all special-command lengths (memoised DFS) so that the whole record
# reproduces its gold share exactly, with leftover literals appended at the end (H12).
import sys, functools
sys.setrecursionlimit(100000)
from cr import META, load, records, stream
def u16(d, o): return d[o] | d[o+1] << 8
def analyse(m, b, g, rec, verbose=True):
    for qi, go, i, t, d, s in records(b, g):
        if qi != 0 or i != rec: continue
        q = 8 if go == 0 else 0
        _, _, lits, q = stream(d, q); _, _, cmds, q = stream(d, q)
        X, N = u16(d, q), u16(d, q+2); offs = [u16(d, q+4+2*k) for k in range(N)]
        tail = d[q+4+2*N:]; end = go + len(s); start = go + (8 if go == 0 else 0)
        best = [0, None]
        @functools.lru_cache(None)
        def solve(n, pos, lp, last, oi):
            if n > best[0]: best[0] = n; best[1] = (pos - go, lp, last, oi)
            if n == len(cmds):
                return () if g[pos:end] == lits[lp:] and end - pos == len(lits) - lp else None
            c = cmds[n]
            if c < 0x10:
                if c == 1:
                    if oi >= N: return None
                    off, oi2 = offs[oi], oi + 1
                else: off, oi2 = last, oi
                if off is None or off > pos: return None
                L = 0
                while pos + L < end and g[pos+L] == g[pos+L-off]: L += 1
                for l in range(L, 0, -1):
                    r = solve(n+1, pos+l, lp, off, oi2)
                    if r is not None: return ((n, c, off, l),) + r
                return None
            ll, ml, rep = c & 7, (c >> 3) & 15, c >> 7
            if g[pos:pos+ll] != lits[lp:lp+ll]: return None
            pos += ll; lp += ll
            if rep: off = last
            else:
                if oi >= N: return None
                off = offs[oi]; oi += 1
            if off is None or off > pos or any(g[pos+j] != g[pos+j-off] for j in range(ml)): return None
            return solve(n+1, pos+ml, lp, off, oi)
        r = solve(0, start, 0, 8 if False else None, 0)
        if verbose:
            print("X", X, "N", N, "cmds", len(cmds), "lits", len(lits)); print("tail", len(tail), tail.hex())
            print("special solution:", r); print("deepest command reached", best)
        return X, N, cmds, tail, r
if __name__ == "__main__":
    cid, idx, rec = sys.argv[1], int(sys.argv[2]), int(sys.argv[3])
    m = [x for x in META if x["codec"] == cid and x["id"] == idx][0]
    b, g = load(m); analyse(m, b, g, rec)
