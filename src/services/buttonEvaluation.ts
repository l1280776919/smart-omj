import type { ButtonBox, ButtonResult } from './buttons';
export function boxIou(a:number[], b:number[]) {
  const intersection=Math.max(0,Math.min(a[2]!,b[2]!)-Math.max(a[0]!,b[0]!))*Math.max(0,Math.min(a[3]!,b[3]!)-Math.max(a[1]!,b[1]!));
  const union=(a[2]!-a[0]!)*(a[3]!-a[1]!)+(b[2]!-b[0]!)*(b[3]!-b[1]!)-intersection;
  return union>0?intersection/union:0;
}
export function evaluateButtons(truth:ButtonBox[], result:ButtonResult) {
  const labels=new Set(result.classes.map(c=>c.id));
  const targets=truth.filter(b=>labels.has(b.label));
  const used=new Set<number>();let hits=0,clicks=0;
  for(const detection of [...result.items].sort((a,b)=>b.confidence-a.confidence)) {
    let match=-1,overlap=.5;
    targets.forEach((target,i)=>{const score=boxIou(target.bounds,detection.bounds);if(!used.has(i)&&target.label===detection.label&&score>=overlap){match=i;overlap=score;}});
    if(match<0)continue;
    used.add(match);hits++;
    const [x1,y1,x2,y2]=targets[match]!.bounds,[x,y]=detection.point;
    if(x>=x1&&x<=x2&&y>=y1&&y<=y2)clicks++;
  }
  return {hits,misses:targets.length-hits,falsePositives:result.items.length-hits,clicks,targets:targets.length,excluded:truth.length-targets.length};
}
