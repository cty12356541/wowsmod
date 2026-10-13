# E14: run the E13 parse over every 8c0a record whose literal and command streams are both stored.
import collections, sys
from cr import blocks, records, stream
import e13_full_parse as P
res = collections.Counter(); rows = []
for m, b, g in blocks():
    for qi, go, i, t, d, s in records(b, g):
        q = 8 if go == 0 else 0
        T0 = d[q] >> 4
        _, _, _, q2 = stream(d, q)
        T1 = d[q2] >> 4 if q2 < len(d) else -1
        if T0 or T1: res["entropy"] += 1; continue
        r = P.analyse(b, g, qi, i, verbose=False)
        key = r if r == "OK" else r.split(":")[0][:40]; res[key] += 1
        if r == "OK": rows.append((m["codec"], m["id"], qi, i, P.LAST.copy()))
        elif res[key] <= 3: print("FAIL", m["codec"], m["id"], qi, i, r[:200])
print(res)
for r in rows[:60]: print(r)
