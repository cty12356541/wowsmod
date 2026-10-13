# E08: which records start with 8 verbatim gold bytes? group by (quantum_idx==0, rec_idx).
import collections
from cr import blocks, records
c = collections.Counter()
for m, b, g in blocks():
    for qi, go, i, t, d, s in records(b, g):
        c[(qi == 0, i, d[:8] == s[:8])] += 1
for k in sorted(c): print("(first quantum, rec_idx, starts with 8 gold bytes)", k, c[k])
