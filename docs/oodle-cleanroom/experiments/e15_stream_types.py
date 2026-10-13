# E15: distribution of sub-stream type nibbles (stream0 = literals, stream1 = commands) and record flag.
import collections
from cr import blocks, records, stream
c = collections.Counter(); ratio = collections.defaultdict(list)
for m, b, g in blocks():
    for qi, go, i, t, d, s in records(b, g):
        if len(d) == len(s) and d == s: c["verbatim record (len == share, bytes == gold)"] += 1; continue
        q = 8 if go == 0 else 0
        try:
            T0, A0, p0, q0 = stream(d, q); T1, A1, p1, q1 = stream(d, q0)
        except IndexError: c[("short", t, d[:12].hex())] += 1; continue
        pass
        c[(t, T0, T1)] += 1
        for T, A, p in ((T0, A0, p0), (T1, A1, p1)):
            if T: ratio[T].append(len(p) / max(A, 1))
print("(record top6, lit-stream type, cmd-stream type): count")
for k, v in sorted(c.items(), key=lambda x: -x[1]): print(k, v)
for T, r in sorted(ratio.items()): print("type", T, "n", len(r), "mean payload/declen %.3f max %.3f" % (sum(r)/len(r), max(r)))
