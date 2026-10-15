<script setup lang="ts">
import { ref } from 'vue';
import type { ButtonBox, ButtonClass } from '../services/buttons';
const props=defineProps<{image:string;width:number;height:number;boxes:ButtonBox[];classes:ButtonClass[];selected:number;drawLabel?:string;readonly?:boolean;zoom:number}>();
const emit=defineEmits<{select:[index:number];change:[boxes:ButtonBox[]]}>();
const svg=ref<SVGSVGElement>();
const draft=ref<ButtonBox[]|null>(null);
let drag:{kind:'draw'|'move'|'resize';index:number;start:[number,number];original:ButtonBox[]}|null=null;
function point(event:PointerEvent):[number,number] {
  const matrix=svg.value?.getScreenCTM();
  if(!matrix)return [0,0];
  const p=new DOMPoint(event.clientX,event.clientY).matrixTransform(matrix.inverse());
  return [Math.max(0,Math.min(props.width,p.x)),Math.max(0,Math.min(props.height,p.y))];
}
function begin(event:PointerEvent,index=-1,resize=false){
  if(props.readonly){if(index>=0)emit('select',index);return;}
  if(event.button!==0)return;
  if(index<0&&!props.drawLabel){emit('select',-1);return;}
  event.preventDefault();svg.value?.setPointerCapture(event.pointerId);
  const start=point(event);const original=props.boxes.map(b=>({label:b.label,bounds:[...b.bounds] as ButtonBox['bounds']}));
  if(index<0){index=original.length;original.push({label:props.drawLabel!,bounds:[...start,...start]});}
  drag={kind:resize?'resize':index===props.boxes.length?'draw':'move',index,start,original};
  draft.value=structuredClone(original);emit('select',index);
}
function move(event:PointerEvent){
  if(!drag)return;
  const p=point(event), {kind,index,start,original}=drag;
  const boxes=structuredClone(original);const [x1,y1,x2,y2]=original[index]!.bounds;
  if(kind==='draw')boxes[index]!.bounds=[Math.min(start[0],p[0]),Math.min(start[1],p[1]),Math.max(start[0],p[0]),Math.max(start[1],p[1])];
  else if(kind==='resize')boxes[index]!.bounds=[x1,y1,Math.max(x1+2,p[0]),Math.max(y1+2,p[1])].map((v,i)=>Math.min(v,i%2?props.height:props.width)) as ButtonBox['bounds'];
  else {const dx=Math.max(-x1,Math.min(props.width-x2,p[0]-start[0]));const dy=Math.max(-y1,Math.min(props.height-y2,p[1]-start[1]));boxes[index]!.bounds=[x1+dx,y1+dy,x2+dx,y2+dy];}
  draft.value=boxes;
}
function finish(){
  if(drag&&draft.value){emit('change',draft.value.filter(b=>b.bounds[2]-b.bounds[0]>=2&&b.bounds[3]-b.bounds[1]>=2));}
  drag=null;draft.value=null;
}
function cancel(){drag=null;draft.value=null;}
function anchor(box:ButtonBox){const a=props.classes.find(c=>c.id===box.label)?.anchor??[.5,.5];return [box.bounds[0]+(box.bounds[2]-box.bounds[0])*a[0]!,box.bounds[1]+(box.bounds[3]-box.bounds[1])*a[1]!];}
</script>
<template>
  <div class="button-canvas-scroll">
    <svg ref="svg" :viewBox="`0 0 ${width} ${height}`" :style="{width:`${zoom*100}%`}" class="button-canvas" role="img" aria-label="按钮标注画布，右侧可编辑精确坐标" @pointerdown="begin($event)" @pointermove="move" @pointerup="finish" @pointercancel="cancel">
      <image :href="image" :width="width" :height="height"/>
      <g v-for="(box,i) in draft??boxes" :key="i">
        <rect :x="box.bounds[0]" :y="box.bounds[1]" :width="box.bounds[2]-box.bounds[0]" :height="box.bounds[3]-box.bounds[1]" :class="{selected:i===selected}" class="annotation-box" @pointerdown.stop="begin($event,i)"/>
        <text :x="box.bounds[0]" :y="Math.max(26,box.bounds[1]-10)" class="annotation-label">{{ classes.find(c=>c.id===box.label)?.name??box.label }}</text>
        <path :d="`M${anchor(box)[0]!-10},${anchor(box)[1]}h20 M${anchor(box)[0]},${anchor(box)[1]!-10}v20`" stroke="#fff" stroke-width="2" vector-effect="non-scaling-stroke" pointer-events="none"/>
        <rect v-if="!readonly && i===selected" :x="box.bounds[2]-9" :y="box.bounds[3]-9" width="18" height="18" fill="white" stroke="#2878dc" class="resize-handle" @pointerdown.stop="begin($event,i,true)"/>
      </g>
    </svg>
  </div>
</template>
<style scoped>
.button-canvas-scroll{overflow:auto;background:#17212c;max-height:65vh;min-height:220px;border-radius:8px}.button-canvas{display:block;min-width:100%;touch-action:none;user-select:none}.annotation-box{fill:#2878dc12;stroke:#64b6ff;stroke-width:2;vector-effect:non-scaling-stroke;cursor:move}.annotation-box.selected{stroke:#ffcb69;fill:#ffcb6920;stroke-width:3}.annotation-label{font:24px sans-serif;fill:white;stroke:#152233;stroke-width:4px;paint-order:stroke;pointer-events:none}.resize-handle{cursor:nwse-resize}
</style>
