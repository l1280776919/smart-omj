"""Train the yard/explore pilot and export ONNX without changing the live detector."""
import csv
import json
from pathlib import Path

import numpy as np
from PIL import Image, ImageOps
import torch
from ultralytics import YOLO

ROOT = Path(__file__).resolve().parent


def main():
    torch.set_num_threads(8)
    device = 0 if torch.cuda.is_available() else 'cpu'
    source = ROOT / 'scene-materials-0.5s'
    dataset = ROOT / 'scene-dataset-pilot'
    dataset.mkdir(exist_ok=False)
    records = list(csv.DictReader((source / 'manifest.csv').open(encoding='utf-8-sig')))
    counts = {}
    for label in sorted({r['label'] for r in records}):
        items = [r for r in records if r['label'] == label]
        boundary = int(len(items) * 0.75)
        for index, row in enumerate(items):
            # Exclude four samples on each side of the temporal split.
            split = 'train' if index < boundary - 4 else 'val' if index >= boundary + 4 else 'excluded'
            row['split'] = split
            if split == 'excluded':
                continue
            target = dataset / split / label / Path(row['image']).name
            target.parent.mkdir(parents=True, exist_ok=True)
            with Image.open(source / row['image']) as img:
                ImageOps.pad(img.convert('RGB'), (224, 224), method=Image.Resampling.BILINEAR,
                             color=(114, 114, 114)).save(target, quality=95)
            counts[f'{split}/{label}'] = counts.get(f'{split}/{label}', 0) + 1
    (dataset / 'split_manifest.json').write_text(json.dumps(records, ensure_ascii=False, indent=2), encoding='utf-8')
    print('DATASET', counts, flush=True)
    model = YOLO('yolo11n-cls.pt')
    model.train(data=str(dataset), epochs=30, imgsz=224, batch=16, workers=0, device=device,
                project=str(ROOT / 'runs'), name='scenes-pilot', seed=42, patience=10,
                optimizer='AdamW', lr0=0.001, freeze=10, plots=True,
                auto_augment=None, erasing=0, fliplr=0, flipud=0, scale=0)
    best = YOLO(str(model.trainer.best))
    metrics = best.val(data=str(dataset), imgsz=224, batch=16, workers=0, device=device)
    exported = best.export(format='onnx', imgsz=224, dynamic=False, simplify=False,
                           opset=17, batch=1, device='cpu')
    import onnxruntime as ort
    session = ort.InferenceSession(str(exported), providers=['CPUExecutionProvider'])
    sample = next((dataset / 'val').rglob('*.jpg'))
    with Image.open(sample) as img:
        tensor = np.asarray(img.convert('RGB'), dtype=np.float32).transpose(2, 0, 1)[None] / 255.0
    output = session.run(None, {session.get_inputs()[0].name: tensor})[0]
    with torch.no_grad():
        reference = best.model.cpu().eval()(torch.from_numpy(tensor))
        if isinstance(reference, (tuple, list)):
            reference = reference[0]
        error = float(np.max(np.abs(output - reference.numpy())))
    if not np.isfinite(output).all() or error > 1e-4:
        raise RuntimeError(f'ONNX parity check failed: {error}')
    report = dict(classes=best.names, input_size=224,
                  preprocessing='RGB; aspect-preserving bilinear resize to fit 224x224; centered padding RGB(114,114,114); float32 NCHW /255; no further normalization',
                  dataset_counts=counts, top1=float(metrics.top1), top5=float(metrics.top5),
                  onnx_max_abs_error=error, onnx=str(exported),
                  limitation='Pilot only: temporal holdout from the same video per class, not independent validation. Only yard/explore; other scenes forced into these classes.')
    (Path(exported).parent / 'scene_model.json').write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding='utf-8')
    print('RESULT', json.dumps(report, ensure_ascii=False), flush=True)


if __name__ == '__main__':
    main()
