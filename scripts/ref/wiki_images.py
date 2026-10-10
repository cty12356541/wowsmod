#!/usr/bin/env python3
"""从 MediaWiki 站点(如 BWIKI)按文件名下载参考图, 记录来源与 sha256(素材只本地用, 不进仓库)。

python scripts/ref/wiki_images.py --api https://wiki.biligame.com/sr/api.php --out <目录> 文件名1 文件名2 ...
python scripts/ref/wiki_images.py --api ... --out <目录> --prefix 知更鸟 --min-size 1000   # 按前缀列出并下载宽或高 ≥ min-size 的图
输出 <目录>/sources.json: 文件名 → {url, descriptionurl, width, height, sha256, fetched}。已存在且哈希一致的跳过(可续传)。
"""
import argparse, hashlib, json, os, time, urllib.parse, urllib.request

UA = {'User-Agent': 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/129.0 Safari/537.36'}

def get(url):
    return urllib.request.urlopen(urllib.request.Request(url, headers=UA), timeout=60).read()

def api(base, **q):
    q['format'] = 'json'
    return json.loads(get(base + '?' + urllib.parse.urlencode(q)))

def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('--api'); ap.add_argument('--out', required=True)
    ap.add_argument('--prefix'); ap.add_argument('--min-size', type=int, default=0)
    ap.add_argument('--from-sources', help='直接按另一台机器生成的 sources.json 里的 url 下载(站点 API 被 WAF 拦时用; 图片 CDN 通常可达)')
    ap.add_argument('names', nargs='*')
    a = ap.parse_args()
    os.makedirs(a.out, exist_ok=True)
    infos = []
    if a.from_sources:
        for n, v in json.load(open(a.from_sources, encoding='utf-8')).items():
            infos.append({'name': n, 'url': v['url'], 'descriptionurl': v.get('descriptionurl'), 'width': v['width'], 'height': v['height'], 'expect': v['sha256']})
    if a.prefix:
        d = api(a.api, action='query', list='allimages', aiprefix=a.prefix, ailimit=500, aiprop='url|size')
        infos += [i for i in d['query']['allimages'] if max(i['width'], i['height']) >= a.min_size]
    for n in a.names:
        d = api(a.api, action='query', titles='File:' + n, prop='imageinfo', iiprop='url|size')
        for p in d['query']['pages'].values():
            if 'imageinfo' in p:
                ii = p['imageinfo'][0]; ii['name'] = n; infos.append(ii)
            else:
                print('not found:', n)
    sp = os.path.join(a.out, 'sources.json')
    src = json.load(open(sp, encoding='utf-8')) if os.path.exists(sp) else {}
    for i in infos:
        n = i['name']; path = os.path.join(a.out, n)
        if os.path.exists(path) and n in src and hashlib.sha256(open(path, 'rb').read()).hexdigest() == src[n]['sha256']:
            continue
        b = get(i['url'])
        if i.get('expect') and hashlib.sha256(b).hexdigest() != i['expect']:
            print('sha256 不一致:', n)
        open(path, 'wb').write(b)
        src[n] = {'url': i['url'], 'descriptionurl': i.get('descriptionurl'), 'width': i['width'], 'height': i['height'],
                  'sha256': hashlib.sha256(b).hexdigest(), 'fetched': time.strftime('%Y-%m-%d %H:%M')}
        print(f"{n}  {i['width']}x{i['height']}  {len(b)} B")
    json.dump(src, open(sp, 'w', encoding='utf-8'), ensure_ascii=False, indent=1)
    print(json.dumps({'files': len(src), 'out': a.out}, ensure_ascii=False))

if __name__ == '__main__':
    main()
