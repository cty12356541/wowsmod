"""Run the material shader GPU regression in a headless Edge or Chrome.

Start the Vite source server at http://127.0.0.1:5173/ with
`npm --prefix preview_app/web run dev`. For --scenes, serve a preview project
on http://127.0.0.1:8960/ so Vite can forward /api requests to it.
"""

import argparse
import json
from pathlib import Path

import cdp


HERE = Path(__file__).resolve().parent
DEFAULT_OUTPUT = HERE.parents[1] / 'out' / 'verification' / 'material-gpu.json'

SCENE_CHECK = """(async () => {
  const rows = [];
  const base = { quality: 'high', w: 384, h: 256, env: 'studio', light: 'night', look: 'game',
    waterline: false, hl: false, wm: false, sim: false, t: 0, lod: 0, data: true };
  for (const channel of ['albedo', 'normal', 'wire', 'mask', 'emissive', 'final']) {
    const a = await __render({...base, channel});
    const b = await __render({...base, channel, gl: JSON.stringify({bloomS: 0})});
    if (!a.ok || !b.ok) throw new Error(JSON.stringify({channel, a, b}));
    const equal = a.dataURL === b.dataURL;
    rows.push({channel, defaultEqualsBloomOff: equal,
      ok: channel === 'final' ? true : equal});
  }
  return rows;
})()"""


def main():
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument(
        '--url', default='http://127.0.0.1:5173/', help='Vite source server URL',
    )
    parser.add_argument(
        '--scenes', action='store_true',
        help='verify that diagnostic scene renders are identical with Bloom off',
    )
    parser.add_argument(
        '--output', type=Path, default=DEFAULT_OUTPUT,
        help='JSON report path (default: out/verification/material-gpu.json in the repository)',
    )
    args = parser.parse_args()
    output = args.output.resolve()

    browser = cdp.find_browser()
    if not browser:
        raise RuntimeError('No Edge or Chrome browser found')

    proc, userdir, ws = cdp.launch(browser)
    try:
        client = cdp.CDP(ws)
        cdp.open_page(client, args.url.rstrip('/') + '/?render=wait', 640, 480)
        client.wait_console('WM_RENDER_READY', 60)
        result = cdp.ev(client, (HERE / 'verify_material_gpu.js').read_text(encoding='utf-8'))
        if args.scenes:
            result['scenes'] = cdp.ev(client, SCENE_CHECK)
            result['ok'] = result['ok'] and all(row['ok'] for row in result['scenes'])

        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_text(json.dumps(result, ensure_ascii=False, indent=2), encoding='utf-8')
        summary = {
            'ok': result['ok'],
            'numeric_cases': len(result['rows']),
            'failures': [row['name'] for row in result['rows'] if not row['ok']],
            'shaderErrors': result['shaderErrors'],
            'scenes': result.get('scenes'),
            'output': str(output),
        }
        print(json.dumps(summary, ensure_ascii=False))
        return 0 if result['ok'] else 1
    finally:
        cdp.close(proc, userdir)


if __name__ == '__main__':
    raise SystemExit(main())
