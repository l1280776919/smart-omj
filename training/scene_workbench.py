"""Desktop scene workbench worker. Every job has isolated, recoverable outputs."""
import argparse
import json
from pathlib import Path
import time


def atomic(path, value):
    temporary = path.with_suffix('.tmp')
    temporary.write_text(json.dumps(value, ensure_ascii=False, indent=2), encoding='utf-8')
    temporary.replace(path)


def split_samples(rows):
    result = []
    independent = True
    for label in sorted({r['label'] for r in rows}):
        items = [dict(r) for r in rows if r['label'] == label]
        batches = sorted({r['batch'] for r in items})
        if len(batches) >= 2:
            held = set(batches[-max(1, round(len(batches) * .2)):])
            for row in items:
                row['split'] = 'val' if row['batch'] in held else 'train'
        else:
            independent = False
            items.sort(key=lambda r: r['seconds'])
            # Short clips still need two validation frames beyond the 4s gap.
            boundary = min(items[int(len(items) * .75)]['seconds'], items[-2]['seconds'] - 2) if len(items) >= 2 else items[0]['seconds']
            for row in items:
                t = row['seconds']
                row['split'] = 'train' if t < boundary - 2 else 'val' if t >= boundary + 2 else 'excluded'
        for split in ['train', 'val']:
            if sum(r['split'] == split for r in items) < 2:
                raise ValueError(f'{label} 的 {split} 样本不足，请补充视频或减小抽帧间隔')
        result.extend(items)
    return result, independent


def main():
    p = argparse.ArgumentParser()
    p.add_argument('action', choices=['prepare', 'train'])
    p.add_argument('--root', type=Path, required=True)
    p.add_argument('--job', type=Path, required=True)
    p.add_argument('--interval', type=float, default=.5)
    p.add_argument('--epochs', type=int, default=30)
    p.add_argument('--device', choices=['auto', 'cuda', 'cpu'], default='auto')
    a = p.parse_args()
    if not .1 <= a.interval <= 30 or not 1 <= a.epochs <= 300:
        p.error('Parameters outside supported range')
    a.job.mkdir(parents=True, exist_ok=True)
    compute = {}
    def status(stage, progress, **extra):
        atomic(a.job / 'progress.json', dict(stage=stage, progress=progress, **(compute | extra)))
    try:
        if a.action == 'prepare':
            import cv2
            from PIL import Image
            videos = sorted(v for v in (a.root / 'videos').glob('*/*') if v.suffix.lower() in {'.mp4','.mkv','.avi','.mov'})
            if not videos:
                raise ValueError('videos/场景名称/ 下没有视频')
            rows = []
            for n, video in enumerate(videos):
                label = video.parent.name
                cap = cv2.VideoCapture(str(video))
                try:
                    fps = cap.get(cv2.CAP_PROP_FPS)
                    total = cap.get(cv2.CAP_PROP_FRAME_COUNT)
                    if not cap.isOpened() or fps <= 0 or total <= 0:
                        raise ValueError(f'无法读取视频：{video.name}')
                    duration = total / fps
                    target = a.job / 'images' / label
                    target.mkdir(parents=True, exist_ok=True)
                    index = 0
                    while index * a.interval < duration:
                        seconds = index * a.interval
                        cap.set(cv2.CAP_PROP_POS_MSEC, seconds * 1000)
                        ok, frame = cap.read()
                        if not ok:
                            break
                        name = f'{n:03d}_{index:06d}.jpg'
                        img = Image.fromarray(cv2.cvtColor(frame, cv2.COLOR_BGR2RGB))
                        img.save(target / name, quality=95)
                        thumbs = a.job / 'thumbs' / label
                        thumbs.mkdir(parents=True, exist_ok=True)
                        img.thumbnail((320,180))
                        img.save(thumbs / (name + '.png'))
                        rows.append(dict(label=label, batch=str(video.relative_to(a.root)), seconds=seconds, image=str(target / name)))
                        index += 1
                        status('抽取图片', (n + min(seconds / duration, 1)) / len(videos), images=len(rows))
                    if index == 0:
                        raise ValueError(f'视频未抽出图片：{video.name}')
                finally:
                    cap.release()
            atomic(a.job / 'samples.json', rows)
            summary = dict(job=str(a.job), images=len(rows), classes={label:sum(r['label']==label for r in rows) for label in sorted({r['label'] for r in rows})}, interval=a.interval)
            atomic(a.root / 'scene-prepared.json', summary)
            status('素材已就绪', 1, **summary)
        else:
            import numpy as np
            import torch
            from PIL import Image, ImageOps
            from ultralytics import YOLO
            import onnxruntime as ort
            torch.set_num_threads(8)
            if a.device == 'cuda' and not torch.cuda.is_available():
                raise ValueError('CUDA 不可用，请安装 GPU 训练依赖并检查 NVIDIA 驱动')
            device = 0 if a.device != 'cpu' and torch.cuda.is_available() else 'cpu'
            compute.update(device='cuda:0' if device == 0 else 'cpu', device_name=torch.cuda.get_device_name(0) if device == 0 else 'CPU')
            print('COMPUTE', compute, flush=True)
            prepared = json.loads((a.root / 'scene-prepared.json').read_text(encoding='utf-8'))
            rows = json.loads((Path(prepared['job']) / 'samples.json').read_text(encoding='utf-8'))
            excluded_path = Path(prepared['job']) / 'excluded.json'
            excluded = set(json.loads(excluded_path.read_text(encoding='utf-8'))) if excluded_path.exists() else set()
            # Users may remove incorrect frames in the material folder before training.
            rows = [r for r in rows if Path(r['image']).is_file() and r['image'] not in excluded]
            if len({r['label'] for r in rows}) < 2:
                raise ValueError('训练至少需要两个场景类别')
            rows, independent = split_samples(rows)
            counts = {}
            for r in rows:
                if r['split'] == 'excluded':
                    continue
                dest = a.job / 'dataset' / r['split'] / r['label'] / Path(r['image']).name
                dest.parent.mkdir(parents=True, exist_ok=True)
                with Image.open(r['image']) as img:
                    ImageOps.pad(img.convert('RGB'), (224,224), method=Image.Resampling.BILINEAR, color=(114,114,114)).save(dest, quality=95)
                key = f"{r['split']}/{r['label']}"
                counts[key] = counts.get(key,0) + 1
            atomic(a.job / 'split.json', rows)
            status('加载训练模型', 0, counts=counts)
            weights = a.root.parent / 'yolo11n-cls.pt'
            model = YOLO(str(weights) if weights.exists() else 'yolo11n-cls.pt')
            def epoch(trainer):
                status('训练中', (trainer.epoch+1)/a.epochs*.9, epoch=trainer.epoch+1, epochs=a.epochs, metrics={k:float(v) for k,v in trainer.metrics.items()}, counts=counts)
            model.add_callback('on_fit_epoch_end', epoch)
            model.train(data=str(a.job/'dataset'), epochs=a.epochs, imgsz=224, batch=16, workers=0,
                        device=device, project=str(a.job), name='model',
                        seed=42, patience=10, optimizer='AdamW', lr0=.001, freeze=10, plots=True,
                        auto_augment=None, erasing=0, fliplr=0, flipud=0, scale=0)
            status('评估与导出', .93)
            best = YOLO(str(model.trainer.best))
            metrics = best.val(data=str(a.job/'dataset'), imgsz=224, workers=0, device=device)
            exported = best.export(format='onnx', imgsz=224, dynamic=False, simplify=False, opset=17, batch=1, device='cpu')
            sample = next((a.job/'dataset'/'val').rglob('*.jpg'))
            with Image.open(sample) as im:
                tensor = np.asarray(im.convert('RGB'), dtype=np.float32).transpose(2,0,1)[None]/255
            session = ort.InferenceSession(str(exported),providers=['CPUExecutionProvider'])
            output = session.run(None,{session.get_inputs()[0].name:tensor})[0]
            with torch.no_grad():
                reference = best.model.cpu().eval()(torch.from_numpy(tensor))
                if isinstance(reference,(tuple,list)): reference=reference[0]
                error=float(np.max(np.abs(reference.numpy()-output)))
            if not np.isfinite(output).all() or error>1e-4: raise ValueError('ONNX 输出一致性验证失败')
            report=dict(classes=best.names, input_size=224, top1=float(metrics.top1), counts=counts, **compute,
                        independent_validation=independent, onnx_max_abs_error=error,
                        preprocessing='RGB; fit 224x224 bilinear; centered padding 114; float32 NCHW /255',
                        limitation='按独立视频留出验证，仍需实机测试' if independent else '同一视频按时间留出验证，仅供初步检查；请补充独立录像',
                        onnx=str(exported), created_at=int(time.time()))
            atomic(Path(exported).parent/'scene_model.json', report)
            atomic(a.job/'result.json',report)
            status('训练完成',1,**report)
    except Exception as exc:
        status('失败',0,error=str(exc))
        raise


if __name__ == '__main__':
    main()
