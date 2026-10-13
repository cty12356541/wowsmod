# Shared helpers for black-box experiments (framing established in E03/E04/E05).
import os, json
R = os.environ.get("OODLE_CORPUS", r"D:\WoWSModding\oodle_corpus")
META = [json.loads(l) for l in open(os.path.join(R, "meta.jsonl"))]
def load(m):
    p = os.path.join(R, m["codec"], "%06d" % m["id"])
    return open(p + ".blk", "rb").read(), open(p + ".gold", "rb").read()
def blocks(prefix="8c0a"):
    for m in META:
        b, g = load(m)
        if b[:2].hex() == prefix: yield m, b, g
def records(b, g):
    """yield (quantum_idx, gold_offset_of_record, rec_idx, top6, data, gold_share)"""
    pos, gpos, qi = 0, 0, 0
    while pos < len(b):
        F = int.from_bytes(b[pos+2:pos+5], "big"); pl = b[pos+5:pos+6+F]; pos += 6 + F
        qout = min(0x40000, len(g) - gpos); q = 0; i = 0
        while q < len(pl):
            H = int.from_bytes(pl[q:q+3], "big"); L = H & 0x3ffff
            yield qi, gpos + i*0x20000, i, H >> 18, pl[q+3:q+3+L], g[gpos+i*0x20000:gpos+min(qout,(i+1)*0x20000)]
            q += 3 + L; i += 1
        gpos += qout; qi += 1
def stream(d, q):
    """E07 header: returns (type, decoded_len_or_None, payload_bytes, next_q)"""
    T = d[q] >> 4
    if T == 0:
        N = int.from_bytes(d[q:q+3], "big") & 0xfffff; return 0, N, d[q+3:q+3+N], q+3+N
    v = int.from_bytes(d[q:q+5], "big"); A = (v >> 18) & 0x3ffff; B = v & 0x3ffff
    return T, A, d[q+5:q+5+B], q+5+B
