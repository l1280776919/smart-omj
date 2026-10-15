"""Reviewed pixel boxes for local visits. Never randomly split neighboring frames."""
import hashlib
import json
from pathlib import Path
import shutil
from PIL import Image

ROOT = Path(__file__).resolve().parent
CLASSES = ['explore', 'summon', 'soul_entry', 'explore_marker', 'soul_title',
           'orochi_entry', 'orochi_title', 'layer1_option', 'layer1_selected',
           'challenge', 'ready', 'prepare_marker', 'auto_battle', 'battle_controls',
           'victory', 'reward', 'continue', 'back', 'other_layer_selected', 'network_error']
# x1,y1,x2,y2 in the 1920x1080 source captures; these are training annotations,
# never runtime click coordinates. Runtime actions use predicted boxes.
BOXES = {
    'explore': [956,195,1078,340], 'summon': [1548,250,1678,397],
    'soul_entry': [831,349,885,602], 'explore_marker': [1581,252,1909,308],
    'soul_title': [229,28,328,84], 'orochi_entry': [375,172,511,521],
    'orochi_title': [1387,174,1706,254], 'layer1_option': [142,170,286,217],
    'layer1_selected': [137,153,357,225], 'challenge': [1677,833,1901,1056],
    'ready': [1644,778,1891,1014], 'prepare_marker': [773,1027,1144,1078],
    'auto_battle': [20,927,163,1070], 'battle_controls': [329,4,512,94],
    'victory': [1032,143,1500,438], 'reward': [744,593,1170,951],
    'continue': [830,1015,1089,1073], 'back': [9,8,122,110],
    'other_layer_selected': [137,312,358,379],
    'network_error': [642,380,1279,700],
}
PAGES = {
    'court': ['explore','summon'],
    'explore': ['soul_entry','explore_marker','back'],
    'menu': ['soul_title','orochi_entry','back'],
    'layer1': ['soul_title','orochi_title','layer1_selected','challenge','back'],
    'layer2': ['soul_title','orochi_title','layer1_option','other_layer_selected','challenge','back'],
    'prepare': ['ready','prepare_marker'],
    'battle': ['auto_battle','battle_controls'],
    'victory': ['victory'],
    'reward': ['reward','continue'],
    'unknown': [],
    'network': ['network_error'],
}
# Explicitly named visits are atomic split groups. Names from early acquisition
# were provisional; page below records the visually reviewed ground truth.
VISITS = [
    ('train-court-01','court','train',None),
    ('train-menu-02','explore','train',None),
    ('train-menu-01','menu','train',None),
    ('train-layer-01','layer1','train',None),
    ('train-layer2-01','layer2','train',None),
    ('train-battle-01','prepare','train',[3,8,15,20,30,34]),
    ('train-combat-01','battle','train',list(range(0,15))),
    ('train-combat-01','victory','train',[16,17]),
    ('train-combat-01','reward','train',[24,27,30,33,36,39]),
    ('train-explore-02','unknown','train',None),
    ('val-explore-01','unknown','train',None), # same popup visit: keep in train
    ('val-explore-02','explore','val',None),
    ('val-menu-01','menu','val',None),
    ('val-layer2-01','layer2','val',None),
    ('val-layer1-01','network','train',None),
    ('val-prepare-01','network','train',None),
    ('train-return-01','layer1','val',None),
]

def main():
    out = ROOT/'dataset'
    records, hashes, groups = [], {}, {}
    for visit, page, split, indices in VISITS:
        assert groups.setdefault(visit,split) == split, f'Visit leakage: {visit}'
        files = sorted((ROOT/'data'/visit).glob('*.png'))
        assert files, f'Missing {visit}'
        if indices is not None:
            files = [f for f in files if int(f.stem) in indices]
        for f in files:
            digest = hashlib.sha256(f.read_bytes()).hexdigest()
            assert hashes.setdefault(digest,split) == split, f'Exact image leakage: {f}'
            w,h = Image.open(f).size
            assert (w,h) == (1920,1080)
            name = visit+'-'+f.stem
            target = out/'images'/split/(name+'.png')
            target.parent.mkdir(parents=True,exist_ok=True)
            shutil.copy2(f,target)
            labels = out/'labels'/split/(name+'.txt')
            labels.parent.mkdir(parents=True,exist_ok=True)
            lines=[]
            for label in PAGES[page]:
                x1,y1,x2,y2 = BOXES[label]
                lines.append(f'{CLASSES.index(label)} {(x1+x2)/2/w:.7f} {(y1+y2)/2/h:.7f} {(x2-x1)/w:.7f} {(y2-y1)/h:.7f}')
            labels.write_text('\n'.join(lines),encoding='utf-8')
            records.append(dict(file=str(target.relative_to(out)),visit=visit,page=page,split=split,sha256=digest))
    (out/'data.yaml').write_text(f'path: {out.as_posix()}\ntrain: images/train\nval: images/val\nnames: '+json.dumps(CLASSES)+'\n',encoding='utf-8')
    (out/'manifest.json').write_text(json.dumps(records,indent=2),encoding='utf-8')
    print({s:sum(r['split']==s for r in records) for s in ['train','val']})

if __name__ == '__main__':
    main()
