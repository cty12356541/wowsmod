#!/usr/bin/env python3
"""增量、分块、校验、可续传的目录镜像(两台机器之间只能逐个搬小文件时用)。

发送端:  python syncpack.py pack  <源目录> <发件箱> [--remote 对端manifest.json] [--rules rules.txt] [--chunk 2]
接收端:  python syncpack.py check <收件箱>               # 列出缺失/损坏的块(续传: 只重传这些)
         python syncpack.py apply <收件箱> <目标目录>     # 校验全部块 → 解包 → 逐文件校验 → 写 <目标>/.sync/manifest.json
         python syncpack.py manifest <目录> [--rules ..]  # 只生成清单(首次对齐/核对)

规则文件: 每行 `+ glob` 包含 / `- glob` 排除, 按顺序最后匹配者生效; 另有 `max_mb N` 限制单文件大小。
块: 变更的小文件按 --chunk(MB)打成 tar.gz; 超过块大小的文件单独切片。plan.json 记录每块 sha256 与每文件 sha256。
"""
import argparse, fnmatch, hashlib, io, json, os, sys, tarfile, time

DEFAULT_RULES = """
+ *
- .sync/*
- */.git/*
- */target/*
- */.wowsmod/cache/*
max_mb 50
"""

def sha(path, bs=1 << 20):
    h = hashlib.sha256()
    with open(path, 'rb') as f:
        for b in iter(lambda: f.read(bs), b''):
            h.update(b)
    return h.hexdigest()

def load_rules(p):
    txt = open(p, encoding='utf-8').read() if p else DEFAULT_RULES
    rules, max_mb = [], None
    for ln in txt.splitlines():
        ln = ln.strip()
        if not ln or ln.startswith('#'):
            continue
        if ln.startswith('max_mb'):
            max_mb = float(ln.split()[1]); continue
        rules.append((ln[0] == '+', ln[1:].strip()))
    return rules, max_mb

def included(rel, rules):
    ok = False
    for inc, g in rules:
        if fnmatch.fnmatch(rel, g) or fnmatch.fnmatch('/' + rel, g):
            ok = inc
    return ok

def manifest(root, rules_path=None, old=None):
    rules, max_mb = load_rules(rules_path)
    out, skipped = {}, []
    old = old or {}
    for d, dirs, files in os.walk(root):
        dirs.sort()
        for fn in sorted(files):
            p = os.path.join(d, fn)
            rel = os.path.relpath(p, root).replace(os.sep, '/')
            if rel.startswith('.sync/') or not included(rel, rules):
                continue
            st = os.stat(p)
            if max_mb and st.st_size > max_mb * 1e6:
                skipped.append(rel); continue
            o = old.get(rel)
            if o and o['size'] == st.st_size and abs(o.get('mtime', 0) - st.st_mtime) < 1e-3:
                h = o['sha256']  # 大小和时间没变就复用旧哈希
            else:
                h = sha(p)
            out[rel] = {'size': st.st_size, 'sha256': h, 'mtime': st.st_mtime}
    return out, skipped

def cmd_manifest(a):
    m, sk = manifest(a.root, a.rules)
    os.makedirs(os.path.join(a.root, '.sync'), exist_ok=True)
    json.dump(m, open(os.path.join(a.root, '.sync', 'manifest.json'), 'w', encoding='utf-8'), ensure_ascii=False, indent=0)
    print(json.dumps({'files': len(m), 'bytes': sum(v['size'] for v in m.values()), 'skipped_too_big': sk}, ensure_ascii=False))

def cmd_pack(a):
    cache = os.path.join(a.root, '.sync', 'local_manifest.json')
    old = json.load(open(cache, encoding='utf-8')) if os.path.exists(cache) else {}
    m, sk = manifest(a.root, a.rules, old)
    os.makedirs(os.path.dirname(cache), exist_ok=True)
    json.dump(m, open(cache, 'w', encoding='utf-8'), ensure_ascii=False)
    remote = json.load(open(a.remote, encoding='utf-8')) if a.remote else {}
    changed = [r for r, v in m.items() if remote.get(r, {}).get('sha256') != v['sha256']]
    chunk = int(a.chunk * 1024 * 1024)
    os.makedirs(a.outbox, exist_ok=True)
    for f in os.listdir(a.outbox):
        os.remove(os.path.join(a.outbox, f))
    parts, cur, cur_n, k = [], None, 0, 0
    def close():
        nonlocal cur, cur_n, k
        if cur is None:
            return
        cur[1].close()
        data = cur[0].getvalue()
        name = f'c{k:04d}.tgz'; k += 1
        open(os.path.join(a.outbox, name), 'wb').write(data)
        parts.append({'name': name, 'kind': 'tar', 'sha256': hashlib.sha256(data).hexdigest(), 'size': len(data)})
        cur, cur_n = None, 0
    big = []
    for r in changed:
        sz = m[r]['size']
        if sz > chunk // 2:
            big.append(r); continue
        if cur is not None and cur_n + sz > chunk:
            close()
        if cur is None:
            buf = io.BytesIO(); cur = (buf, tarfile.open(fileobj=buf, mode='w:gz', format=tarfile.PAX_FORMAT))
        cur[1].add(os.path.join(a.root, r), arcname=r); cur_n += sz
    close()
    for r in big:
        with open(os.path.join(a.root, r), 'rb') as f:
            i = 0
            while True:
                b = f.read(chunk)
                if not b:
                    break
                name = f'c{k:04d}.part'; k += 1
                open(os.path.join(a.outbox, name), 'wb').write(b)
                parts.append({'name': name, 'kind': 'slice', 'file': r, 'index': i, 'sha256': hashlib.sha256(b).hexdigest(), 'size': len(b)})
                i += 1
    plan = {'created': time.strftime('%Y-%m-%d %H:%M:%S'), 'source': os.path.abspath(a.root), 'files': {r: m[r] for r in changed},
            'chunks': parts, 'total_files': len(m), 'skipped_too_big': sk}
    json.dump(plan, open(os.path.join(a.outbox, 'plan.json'), 'w', encoding='utf-8'), ensure_ascii=False, indent=0)
    print(json.dumps({'changed_files': len(changed), 'chunks': len(parts), 'bytes': sum(p['size'] for p in parts), 'skipped_too_big': sk, 'outbox': a.outbox}, ensure_ascii=False))

def bad_chunks(inbox):
    plan = json.load(open(os.path.join(inbox, 'plan.json'), encoding='utf-8'))
    bad = []
    for c in plan['chunks']:
        p = os.path.join(inbox, c['name'])
        if not os.path.exists(p) or os.path.getsize(p) != c['size'] or sha(p) != c['sha256']:
            bad.append(c['name'])
    return plan, bad

def cmd_check(a):
    plan, bad = bad_chunks(a.inbox)
    print(json.dumps({'chunks': len(plan['chunks']), 'missing_or_bad': bad}, ensure_ascii=False))
    sys.exit(1 if bad else 0)

def cmd_apply(a):
    plan, bad = bad_chunks(a.inbox)
    if bad:
        print(json.dumps({'ok': False, 'missing_or_bad': bad}, ensure_ascii=False)); sys.exit(1)
    dest = a.dest
    os.makedirs(dest, exist_ok=True)
    for c in plan['chunks']:
        if c['kind'] == 'tar':
            with tarfile.open(os.path.join(a.inbox, c['name'])) as t:
                for mem in t.getmembers():
                    if mem.name.startswith('/') or '..' in mem.name.split('/'):
                        raise SystemExit('unsafe path ' + mem.name)
                try:
                    t.extractall(dest, filter='data')
                except TypeError:
                    t.extractall(dest)
    slices = {}
    for c in plan['chunks']:
        if c['kind'] == 'slice':
            slices.setdefault(c['file'], []).append(c)
    for f, cs in slices.items():
        out = os.path.join(dest, f); os.makedirs(os.path.dirname(out), exist_ok=True)
        with open(out + '.tmp', 'wb') as w:
            for c in sorted(cs, key=lambda x: x['index']):
                w.write(open(os.path.join(a.inbox, c['name']), 'rb').read())
        os.replace(out + '.tmp', out)
    wrong = [r for r, v in plan['files'].items() if sha(os.path.join(dest, r)) != v['sha256']]
    mp = os.path.join(dest, '.sync', 'manifest.json')
    os.makedirs(os.path.dirname(mp), exist_ok=True)
    m = json.load(open(mp, encoding='utf-8')) if os.path.exists(mp) else {}
    for r, v in plan['files'].items():
        if r not in wrong:
            m[r] = v
    json.dump(m, open(mp, 'w', encoding='utf-8'), ensure_ascii=False, indent=0)
    print(json.dumps({'ok': not wrong, 'applied_files': len(plan['files']) - len(wrong), 'hash_mismatch': wrong, 'manifest': mp}, ensure_ascii=False))
    sys.exit(1 if wrong else 0)

def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sp = ap.add_subparsers(dest='cmd', required=True)
    p = sp.add_parser('pack'); p.add_argument('root'); p.add_argument('outbox'); p.add_argument('--remote'); p.add_argument('--rules'); p.add_argument('--chunk', type=float, default=2.0); p.set_defaults(f=cmd_pack)
    p = sp.add_parser('check'); p.add_argument('inbox'); p.set_defaults(f=cmd_check)
    p = sp.add_parser('apply'); p.add_argument('inbox'); p.add_argument('dest'); p.set_defaults(f=cmd_apply)
    p = sp.add_parser('manifest'); p.add_argument('root'); p.add_argument('--rules'); p.set_defaults(f=cmd_manifest)
    a = ap.parse_args(); a.f(a)

if __name__ == '__main__':
    main()
