"""Extract full-resolution scene images and review sheets from labeled videos."""
import argparse
import csv
import json
import subprocess
from pathlib import Path

from PIL import Image, ImageDraw


def main():
    root = Path(__file__).resolve().parent
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--input', type=Path, default=root / 'videos')
    parser.add_argument('--output', type=Path, default=root / 'scene-materials')
    parser.add_argument('--interval', type=float, default=2)
    args = parser.parse_args()
    if args.interval <= 0:
        parser.error('--interval must be positive')
    videos = sorted(p for p in args.input.rglob('*') if p.suffix.lower() in {'.mp4', '.mkv', '.mov', '.avi'})
    if not videos:
        parser.error('No videos found')
    args.output.mkdir(parents=True, exist_ok=False)
    review = args.output / 'previews'
    review.mkdir()
    records = []
    for number, video in enumerate(videos, 1):
        label = video.parent.name
        batch = f'{label}_{number:03d}'
        dest = args.output / 'images' / label
        dest.mkdir(parents=True, exist_ok=True)
        metadata = json.loads(subprocess.check_output([
            'ffprobe', '-v', 'error', '-show_entries', 'format=duration', '-of', 'json', str(video)
        ]))
        subprocess.run([
            'ffmpeg', '-hide_banner', '-loglevel', 'error', '-n', '-i', str(video),
            '-vf', f'select=isnan(prev_selected_t)+gte(t-prev_selected_t\\,{args.interval})',
            '-fps_mode', 'vfr', '-q:v', '2', str(dest / f'{batch}_%06d.jpg')
        ], check=True)
        frames = sorted(dest.glob(f'{batch}_*.jpg'))
        for index, frame in enumerate(frames):
            with Image.open(frame) as img:
                img.load()
                width, height = img.size
            records.append(dict(image=frame.relative_to(args.output).as_posix(), label=label,
                                source=str(video.resolve()), batch=batch,
                                sample_index=index, nominal_seconds=index * args.interval,
                                width=width, height=height))
        for offset in range(0, len(frames), 24):
            subset = frames[offset:offset + 24]
            sheet = Image.new('RGB', (1280, ((len(subset) + 3) // 4) * 205), '#202020')
            draw = ImageDraw.Draw(sheet)
            for i, frame in enumerate(subset):
                with Image.open(frame) as img:
                    img.thumbnail((320, 180))
                    x, y = (i % 4) * 320, (i // 4) * 205
                    sheet.paste(img, (x, y))
                    draw.text((x + 5, y + 183), frame.name, fill='white')
            sheet.save(review / f'{batch}_{offset // 24 + 1:02d}.jpg', quality=90)
        print(f'{label}: {len(frames)} images; duration={metadata["format"]["duration"]}s', flush=True)
    with (args.output / 'manifest.csv').open('w', encoding='utf-8-sig', newline='') as handle:
        writer = csv.DictWriter(handle, fieldnames=records[0].keys())
        writer.writeheader()
        writer.writerows(records)
    print(f'Verified {len(records)} images. Output: {args.output}')


if __name__ == '__main__':
    main()
