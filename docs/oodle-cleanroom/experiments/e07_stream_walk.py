# E07: walk sub-stream headers inside records (skipping 8 verbatim bytes where present, see E08).
# H7: nibble T==0 -> 3-byte header, low 20 bits = N, N bytes follow.
#     T!=0 -> 5-byte header T(4)|A(18)|B(18), B bytes follow.
from cr import blocks, records, stream
shown = 0
for m, b, g in blocks():
    for qi, go, i, t, d, s in records(b, g):
        q = 8 if d[:8] == s[:8] and go == 0 else 0
        out = []
        for _ in range(3):
            if q >= len(d): break
            T, A, p, q2 = stream(d, q); out.append((q, T, A, len(p))); q = q2
        print(m["codec"], m["id"], "q", qi, "rec", i, "t", t, "len", len(d), out)
        shown += 1
    if shown > 30: break
