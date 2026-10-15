"""Train a small local pilot; Python is used only for training/export, never at runtime."""
import argparse
import json
from pathlib import Path
import shutil
import torch
from ultralytics import YOLO

ROOT = Path(__file__).resolve().parents[1]

def main():
    p = argparse.ArgumentParser()
    p.add_argument('--epochs', type=int, default=100)
    p.add_argument('--device', default='auto')
    p.add_argument('--name', default='soul1-pilot')
    p.add_argument('--weights', default=str(ROOT / 'yolo11n.pt'))
    a = p.parse_args()
    torch.set_num_threads(8)
    model = YOLO(a.weights)
    model.train(data=str(ROOT/'training/dataset/data.yaml'), epochs=a.epochs,
                imgsz=640, batch=8, workers=0, device=(0 if torch.cuda.is_available() else 'cpu') if a.device == 'auto' else a.device, project=str(ROOT/'training/runs'),
                name=a.name, exist_ok=False, seed=42, deterministic=True, patience=30,
                optimizer='AdamW', lr0=0.002, warmup_epochs=3, freeze=10,
                mosaic=0, mixup=0, fliplr=0, flipud=0, degrees=0, translate=0.05,
                scale=0.15, hsv_h=0.01, hsv_s=0.15, hsv_v=0.15, plots=True)
    best = YOLO(str(model.trainer.best))
    exported = best.export(format='onnx', imgsz=640, dynamic=False, simplify=False,
                           opset=17, nms=False, batch=1, device='cpu')
    out = ROOT/'src-tauri/resources/detector'
    out.mkdir(parents=True, exist_ok=True)
    shutil.copy2(exported, out/'soul1.onnx')
    names = [best.names[i] for i in range(len(best.names))]
    (out/'model.json').write_text(json.dumps(dict(input_size=640, classes=names,
        confidence=0.70, nms_iou=0.45, supported_layers=[1],
        scope='Local pilot: MuMu 1920x1080, captured account/UI only; no exception samples yet'),
        ensure_ascii=False, indent=2), encoding='utf-8')
    for filename in ['onnxruntime.dll', 'onnxruntime_providers_shared.dll']:
        import onnxruntime
        shutil.copy2(Path(onnxruntime.__file__).parent/'capi'/filename, out/filename)
    print(f'Exported detector and runtime to {out}')

if __name__ == '__main__':
    main()
