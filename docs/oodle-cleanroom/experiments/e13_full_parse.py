# E13: full parse hypothesis for records whose literal & command streams are stored (type 0).
#  record (after optional 8 verbatim bytes) = S_lit, S_cmd, u16 X, u16 N, N x u16 offsets, U (unknown), LEN
#  LEN = one entry per special command (cmd < 0x10): byte b<0xfc -> b+91 ; b>=0xfc -> 4*u16le+(b-0xfc)+343
#  start of LEN chosen so that exactly (#specials) entries end at record end.
#  H13b: for special 0x02 the length is 62 smaller than the 0x01 formula.
#  special 0x01: offset = next from N-table ; special 0x02: offset searched (unknown source) and printed.
import sys
from cr import META, load, records, stream
LAST = {}
SPECIAL_BELOW = 3  # H13d (from E15/hydra 12 rec 1): only command bytes 0x00..0x02 are special
import os
INIT_LAST = int(os.environ.get('INIT_LAST', '8'))  # H13c: initial reuse-offset before any new offset
def u16(d, o): return d[o] | d[o+1] << 8
def parse_len(t, s):
    out = []
    while s < len(t):
        b = t[s]
        if b < 0xfc: out.append(b + 91); s += 1
        else:
            if s + 3 > len(t): return None
            out.append(4 * u16(t, s+1) + (b - 0xfc) + 343); s += 3
    return out
def analyse(b, g, qsel, rec, verbose=True):
    for qi, go, i, t, d, s in records(b, g):
        if qi != qsel or i != rec: continue
        q = 8 if go == 0 else 0
        T0, _, lits, q = stream(d, q); T1, _, cmds, q = stream(d, q)
        if T0 or T1: return "entropy-coded streams (types %d,%d)" % (T0, T1)
        if q + 4 > len(d): return "short record"
        X, N = u16(d, q), u16(d, q+2)
        if q + 4 + 2*N > len(d): return "N table overruns record (X=%d N=%d)" % (X, N)
        offs = [u16(d, q+4+2*k) for k in range(N)]
        tail = d[q+4+2*N:]; nsp = sum(c < SPECIAL_BELOW for c in cmds)
        starts = [st for st in range(len(tail)+1) if (lambda L: L is not None and len(L) == nsp)(parse_len(tail, st))]
        if not starts: return "no LEN start fits"
        fails = []
        for st in starts:
            try: r = sim(st, tail, X, N, offs, cmds, lits, go, s, g, verbose)
            except (IndexError, TypeError) as e: r = "EXC " + type(e).__name__
            if r == "OK": return r
            fails.append((st, r))
        return "no start works: %s" % fails[:4]
def sim(st, tail, X, N, offs, cmds, lits, go, s, g, verbose):
        L = parse_len(tail, st); U = tail[:st]
        pos, lp, last, oi, li = go + (8 if go == 0 else 0), 0, INIT_LAST, 0, 0
        need02 = []
        for n, c in enumerate(cmds):
            if c < SPECIAL_BELOW:
                l = L[li] - (62 if c == 2 else 0); li += 1
                if c == 1: off = offs[oi]; oi += 1
                else:
                    cands = [o for o in range(1, pos+1) if g[pos:pos+l] == g[pos-o:pos-o+l]]
                    need02.append((c, hex(pos-go), l, cands[:5], len(cands))); off = cands[0] if cands else 1
                if off > pos or pos + l > go + len(s) or any(g[pos+j] != g[pos+j-off] for j in range(l)):
                    if verbose: print("  diag: special", n, hex(c), "entry->len", l, "at", hex(pos-go), "LEN", L, "remaining", go+len(s)-pos)
                    return "FAIL at cmd %d special %02x len %d off %d pos %x" % (n, c, l, off, pos-go)
                pos += l; last = off; continue
            ll, ml, rep = c & 7, (c >> 3) & 15, c >> 7
            if g[pos:pos+ll] != lits[lp:lp+ll]: return "FAIL lits at cmd %d" % n
            pos += ll; lp += ll
            if not rep: off = offs[oi]; oi += 1; last = off
            if any(g[pos+j] != g[pos+j-last] for j in range(ml)): return "FAIL match at cmd %d %02x off %d" % (n, c, last)
            pos += ml
        trail = g[pos:go+len(s)] == lits[lp:]
        res = "OK" if trail and oi == N else "end mismatch"
        LAST.clear(); LAST.update(X=X, N=N, ncmd=len(cmds), nsp=len(L), U=U.hex(), far=[(x[1], x[3][:3]) for x in need02], ndist=len(set(offs)))
        if verbose: print("X", X, "N", N, "U", U.hex(), "LEN", L, "02-offsets", need02, "offs used", oi, "/", N, "trailing lits ok", trail)
        return res
if __name__ == "__main__":
    cid, idx = sys.argv[1], int(sys.argv[2]); qsel, rec = int(sys.argv[3]), int(sys.argv[4])
    m = [x for x in META if x["codec"] == cid and x["id"] == idx][0]
    b, g = load(m); print(analyse(b, g, qsel, rec))
