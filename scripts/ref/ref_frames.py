#!/usr/bin/env python3
"""参考视频 → 关键帧 + 拼图(复刻工作流 层 1 的通用模板, 与船/mod 无关)。

用法:
  python ref_frames.py --bv <BV号> --out <目录> [--frames 48] [--qn 80]
  python ref_frames.py --video <本地视频> --out <目录> [--frames 48]
产物: <out>/video.mp4, <out>/info.json(标题/简介/时长, 简介里常有公开网盘链接), <out>/frames/f_XXX_<秒>.jpg,
      <out>/contact_sheet.jpg(带时间戳的拼图), <out>/frames.csv
抽帧策略: ffmpeg 场景变化检测(scene>阈值)取候选, 再按时间均匀补齐/裁剪到 --frames 张, 避免只拿到片头。
B 站说明: 网页接口(view)常对机房 IP 返回 412; player/pagelist + player/playurl(platform=html5) 通常可用。
         下不到时用浏览器播放并截图, 再用 --video 或直接把截图放进 frames/ 后加 --sheet-only 出拼图。
只用于分析参考, 不再分发视频。
"""
import argparse, json, os, re, subprocess, sys, time, urllib.request

UA = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126 Safari/537.36"

def get(url, ref="https://www.bilibili.com/"):
    req = urllib.request.Request(url, headers={"User-Agent": UA, "Referer": ref})
    with urllib.request.urlopen(req, timeout=30) as r:
        return r.read()

def bili(bv, out, qn):
    pl = json.loads(get(f"https://api.bilibili.com/x/player/pagelist?bvid={bv}"))
    p = pl["data"][0]
    info = {"bvid": bv, "cid": p["cid"], "part": p["part"], "duration": p["duration"], "dimension": p.get("dimension")}
    try:  # 简介/标题(可能 412, 失败不致命)
        v = json.loads(get(f"https://api.bilibili.com/x/web-interface/view?bvid={bv}"))["data"]
        info.update(title=v.get("title"), desc=v.get("desc"), owner=v.get("owner", {}).get("name"))
    except Exception as e:
        info["view_error"] = str(e)
    def playurl():
        pu = json.loads(get(f"https://api.bilibili.com/x/player/playurl?bvid={bv}&cid={p['cid']}&qn={qn}&fnval=1&platform=html5&high_quality=1"))
        d = pu["data"]; info["quality"] = d.get("quality")
        return d["durl"][0]["url"], d["durl"][0].get("size")
    url, size = playurl()
    dst = os.path.join(out, "video.mp4")
    # CDN 连接常被中断: HTTP/1.1 + 断点续传循环, 直到大小对上
    for attempt in range(60):
        have = os.path.getsize(dst) if os.path.exists(dst) else 0
        if size and have >= size:
            break
        if attempt:
            time.sleep(2)
            try:
                url, _ = playurl()  # 每次重取地址, 可能换到能续传的镜像
            except Exception:
                pass
        subprocess.run(["curl", "-sfL", "--http1.1", "--speed-limit", "20000", "--speed-time", "20", "-C", "-", "-A", UA, "-e", "https://www.bilibili.com/", "-o", dst, url])
        if not size:
            break
    if size and os.path.getsize(dst) < size:
        print(f"警告: 下载不完整 {os.path.getsize(dst)}/{size}", file=sys.stderr)
    info["size"] = size
    json.dump(info, open(os.path.join(out, "info.json"), "w"), ensure_ascii=False, indent=2)
    return dst

def duration(path):
    return float(subprocess.check_output(["ffprobe", "-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0", path]).decode().strip())

def frames(video, out, n, thr):
    fd = os.path.join(out, "frames"); os.makedirs(fd, exist_ok=True)
    for f in os.listdir(fd):
        os.remove(os.path.join(fd, f))
    dur = duration(video)
    r = subprocess.run(["ffmpeg", "-hide_banner", "-i", video, "-vf", f"select='gt(scene,{thr})',showinfo", "-an", "-f", "null", "-"], capture_output=True, text=True)
    ts = [float(m) for m in re.findall(r"pts_time:([0-9.]+)", r.stderr)]
    # 场景切点 + 均匀网格, 合并后按最小间隔去重, 再均匀抽 n 个
    grid = [dur * (i + 0.5) / n for i in range(n)]
    cand = sorted(set([round(t + 0.4, 2) for t in ts if t + 0.4 < dur] + grid))
    gap = dur / (n * 2.5); keep = []
    for t in cand:
        if not keep or t - keep[-1] >= gap:
            keep.append(t)
    if len(keep) > n:
        keep = [keep[int(i * len(keep) / n)] for i in range(n)]
    rows = []
    for i, t in enumerate(keep):
        fn = os.path.join(fd, f"f_{i:03d}_{t:07.2f}s.jpg")
        subprocess.run(["ffmpeg", "-hide_banner", "-loglevel", "error", "-ss", str(t), "-i", video, "-frames:v", "1", "-q:v", "2", fn], check=True)
        rows.append((i, t, os.path.basename(fn)))
    with open(os.path.join(out, "frames.csv"), "w") as f:
        f.write("idx,time_s,file,notes\n")
        for i, t, fn in rows:
            f.write(f"{i},{t:.2f},{fn},\n")
    return fd

def sheet(fd, out, cols=6, w=480):
    from PIL import Image, ImageDraw
    fs = sorted(x for x in os.listdir(fd) if x.lower().endswith((".jpg", ".png")))
    ims = [Image.open(os.path.join(fd, x)).convert("RGB") for x in fs]
    if not ims:
        sys.exit("没有帧")
    h = int(ims[0].height * w / ims[0].width); rows = (len(ims) + cols - 1) // cols
    S = Image.new("RGB", (cols * w, rows * (h + 18)), (16, 16, 16)); d = ImageDraw.Draw(S)
    for i, (im, name) in enumerate(zip(ims, fs)):
        x, y = (i % cols) * w, (i // cols) * (h + 18)
        S.paste(im.resize((w, h)), (x, y + 18)); d.text((x + 4, y + 3), name, fill=(255, 255, 0))
    p = os.path.join(out, "contact_sheet.jpg"); S.save(p, quality=88); return p

if __name__ == "__main__":
    a = argparse.ArgumentParser()
    a.add_argument("--bv"); a.add_argument("--video"); a.add_argument("--out", required=True)
    a.add_argument("--frames", type=int, default=48); a.add_argument("--qn", type=int, default=80)
    a.add_argument("--scene", type=float, default=0.3); a.add_argument("--sheet-only", action="store_true")
    o = a.parse_args(); os.makedirs(o.out, exist_ok=True)
    if not o.sheet_only:
        v = o.video or bili(o.bv, o.out, o.qn)
        frames(v, o.out, o.frames, o.scene)
    print(sheet(os.path.join(o.out, "frames"), o.out))
