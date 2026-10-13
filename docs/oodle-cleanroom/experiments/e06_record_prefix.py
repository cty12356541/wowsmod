# E06: longest common prefix (LCP) of each record vs its gold share; dump heads.
import collections
from cr import blocks, records
lcp = collections.Counter(); ex = collections.defaultdict(list)
for m, b, g in blocks():
    for qi, go, i, t, d, s in records(b, g):
        k = 0
        while k < min(len(d), len(s)) and d[k] == s[k]: k += 1
        lcp[(t, i, min(k, 9))] += 1
        if len(ex[(t, i)]) < 4: ex[(t, i)].append((m["codec"], m["id"], len(d), len(s), d[:20].hex(), s[:10].hex()))
for k in sorted(lcp): print("(top6, rec_idx, min(LCP,9))", k, lcp[k])
for k, v in ex.items():
    for e in v: print(k, e)
