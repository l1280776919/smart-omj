"""Independent button detector training. Dataset is an immutable job snapshot."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import time


def validate_dataset(dataset, root):
    classes = dataset['classes']
    ids = [c['id'] for c in classes]
    if not ids or len(ids) != len(set(ids)):
        raise ValueError('请先添加按钮类别')
    rows = [r for r in dataset['samples'] if r['reviewed']]
    hashes = {}
    counts = {key: dict(train=0, val=0) for key in ids}
    for row in rows:
        split = row['split']
        if split not in ('train', 'val'):
            raise ValueError('数据集划分无效')
        image = root / 'images' / (row['id'] + '.png')
        digest = hashlib.sha256(image.read_bytes()).hexdigest()
        if hashes.setdefault(digest, split) != split:
            raise ValueError('训练集和验证集存在相同图片，请移除重复素材')
        for box in row['boxes']:
            x1, y1, x2, y2 = box['bounds']
            if box['label'] not in ids or not (0 <= x1 < x2 <= row['width'] and 0 <= y1 < y2 <= row['height']):
                raise ValueError('标注类别或边界无效')
            counts[box['label']][split] += 1
    for label, count in counts.items():
        if not count['train'] or not count['val']:
            raise ValueError(f'{label} 缺少训练或验证标注，请补充并审核素材')
    return rows, counts


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--root', type=Path, required=True)
    parser.add_argument('--job', type=Path, required=True)
    parser.add_argument('--epochs', type=int, default=50)
    parser.add_argument('--patience', type=int, default=15)
    parser.add_argument('--imgsz', type=int, choices=[640, 960, 1280], default=640)
    parser.add_argument('--device', choices=['auto', 'cuda', 'cpu'], default='auto')
    args = parser.parse_args()
    from scene_workbench import atomic
    compute = {}
    def status(stage, progress, **extra):
        atomic(args.job/'progress.json', dict(stage=stage, progress=progress, **compute, **extra))
    try:
        if not 1 <= args.epochs <= 300 or not 1 <= args.patience <= 100:
            raise ValueError('训练轮数无效')
        dataset = json.loads((args.job/'annotations.json').read_text(encoding='utf-8'))
        rows, counts = validate_dataset(dataset, args.root)
        import torch
        from ultralytics import YOLO
        device = 0 if args.device != 'cpu' and torch.cuda.is_available() else 'cpu'
        if args.device == 'cuda' and device == 'cpu':
            raise ValueError('CUDA 不可用，请检查 GPU 训练环境')
        compute.update(device='cuda:0' if device == 0 else 'cpu', device_name=torch.cuda.get_device_name(0) if device == 0 else 'CPU')
        ids = [c['id'] for c in dataset['classes']]
        for row in rows:
            images = args.job/'dataset/images'/row['split']
            labels = args.job/'dataset/labels'/row['split']
            images.mkdir(parents=True, exist_ok=True)
            labels.mkdir(parents=True, exist_ok=True)
            shutil.copy2(args.root/'images'/f"{row['id']}.png", images/f"{row['id']}.png")
            lines = []
            for box in row['boxes']:
                x1, y1, x2, y2 = box['bounds']; w, h = row['width'], row['height']
                lines.append(f'{ids.index(box["label"])} {(x1+x2)/2/w} {(y1+y2)/2/h} {(x2-x1)/w} {(y2-y1)/h}')
            (labels/f"{row['id']}.txt").write_text('\n'.join(lines), encoding='utf-8')
        import yaml
        config = args.job/'dataset/data.yaml'
        config.write_text(yaml.safe_dump(dict(path=str(config.parent.resolve()), train='images/train', val='images/val', names=ids)), encoding='utf-8')
        status('加载训练模型', 0, counts=counts)
        weights = Path(__file__).resolve().parents[1]/'yolo11n.pt'
        model = YOLO(str(weights) if weights.is_file() else 'yolo11n.pt')
        def epoch(trainer):
            status('训练中', (trainer.epoch+1)/args.epochs*.9, epoch=trainer.epoch+1, epochs=args.epochs,
                   metrics={k:float(v) for k,v in trainer.metrics.items()})
        model.add_callback('on_fit_epoch_end', epoch)
        model.train(data=str(config), epochs=args.epochs, imgsz=args.imgsz, batch=8, workers=0, device=device,
                    project=str(args.job), name='model', seed=42, patience=args.patience, optimizer='AdamW', lr0=.001,
                    mosaic=0, fliplr=0, flipud=0, degrees=0, scale=.1, translate=.05, plots=True)
        status('验证与导出', .93)
        best = YOLO(str(model.trainer.best))
        metrics = best.val(data=str(config), imgsz=args.imgsz, workers=0, device=device)
        exported = best.export(format='onnx', imgsz=args.imgsz, opset=17, simplify=False, dynamic=False, nms=False, batch=1, device='cpu')
        # Compare the portable ONNX model with the exported PyTorch detector.
        import numpy as np
        import onnxruntime as ort
        from PIL import Image, ImageOps
        sample = next(r for r in rows if r['split']=='val')
        with Image.open(args.root/'images'/f"{sample['id']}.png") as im:
            array = np.asarray(ImageOps.pad(im.convert('RGB'), (args.imgsz,args.imgsz), color=(114,114,114)), dtype=np.float32).transpose(2,0,1)[None]/255
        session = ort.InferenceSession(str(exported), providers=['CPUExecutionProvider'])
        output = session.run(None, {session.get_inputs()[0].name:array})[0]
        with torch.no_grad():
            ref = best.model.cpu().float().eval()(torch.from_numpy(array))
            if isinstance(ref, (tuple,list)): ref=ref[0]
            error = float(np.max(np.abs(ref.numpy()-output)))
        if not np.isfinite(output).all() or not np.allclose(ref.numpy(), output, atol=.02, rtol=1e-3):
            raise ValueError(f'ONNX 一致性检查失败：{error}')
        dest = args.job/'export'; dest.mkdir()
        shutil.copy2(exported, dest/'soul1.onnx')
        atomic(dest/'model.json', dict(input_size=args.imgsz, classes=ids, confidence=.5, nms_iou=.45))
        atomic(dest/'buttons.json', dataset['classes'])
        per_class = [{**c, 'map50':float(metrics.box.ap50[i])} for i,index in enumerate(metrics.box.ap_class_index) for c in [dataset['classes'][int(index)]]]
        report = dict(config=vars(args) | {"root":str(args.root), "job":str(args.job)}, actual_epochs=model.trainer.epoch+1, directory=str(dest), classes=dataset['classes'], counts=counts, map50=float(metrics.box.map50),
                      map50_95=float(metrics.box.map), per_class=per_class, onnx_max_abs_error=error, **compute, created_at=int(time.time()))
        atomic(args.job/'result.json', report)
        status('训练完成', 1)
    except Exception as exc:
        status('失败', 0, error=str(exc))
        raise


if __name__ == '__main__':
    main()
