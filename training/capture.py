"""Local ADB dataset capture. Each invocation is a separately named capture visit."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import time

ROOT = Path(__file__).resolve().parents[1]

def main():
    p = argparse.ArgumentParser()
    p.add_argument('visit')
    p.add_argument('--serial', default='127.0.0.1:16384')
    p.add_argument('--count', type=int, default=5)
    p.add_argument('--interval', type=float, default=0.6)
    p.add_argument('--tap', type=int, nargs=2)
    p.add_argument('--delay', type=float, default=1.5)
    a = p.parse_args()
    adb = ROOT / 'src-tauri/resources/adb/adb.exe'
    def run(*args):
        return subprocess.run([str(adb), '-s', a.serial, *map(str, args)], check=True, capture_output=True, timeout=12).stdout
    out = ROOT / 'training/data' / a.visit
    out.mkdir(parents=True, exist_ok=False)
    if a.tap:
        run('shell', 'input', 'tap', *a.tap)
        time.sleep(a.delay)
    for i in range(a.count):
        start = time.perf_counter()
        raw = run('exec-out', 'screencap', '-p')
        name = f'{i:03}.png'
        (out / name).write_bytes(raw)
        with (out / 'capture.jsonl').open('a', encoding='utf-8') as f:
            f.write(json.dumps(dict(file=name, serial=a.serial, captured_at=time.time(), screenshot_ms=(time.perf_counter()-start)*1000, sha256=hashlib.sha256(raw).hexdigest()))+'\n')
        time.sleep(a.interval)
    print(out)

if __name__ == '__main__':
    main()
