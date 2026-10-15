<script setup lang="ts">
import {computed,onMounted,onUnmounted,ref,watch} from 'vue';
import {NAlert,NButton,NInputNumber,NSelect} from 'naive-ui';
import {buttons,type ButtonDataset,type ButtonHistory,type ButtonResult,type ButtonSample} from '../services/buttons';
import {evaluateButtons} from '../services/buttonEvaluation';
import ButtonCanvas from './ButtonCanvas.vue';
const props=defineProps<{dataset:ButtonDataset;checked:string[];dirty:boolean;disabled:boolean}>();
const emit=defineEmits<{annotate:[id:string];activated:[];candidate:[id:string|undefined]}>();
const history=ref<ButtonHistory|null>(null),candidate=ref<string>('active'),threshold=ref(.5),error=ref(''),busy=ref(false),cancelled=ref(false),scope=ref('val');
const rows=ref<{sample:ButtonSample;result:ButtonResult;score:ReturnType<typeof evaluateButtons>}[]>([]);
const selected=ref(-1),image=ref(''),report=ref(''),notice=ref('');
const options=computed(()=>[{label:'当前启用模型',value:'active'},...(history.value?.jobs.filter(j=>j.status==='completed')??[]).map(j=>({label:`${j.id.slice(0,8)} · ${j.classes.length} 类 · mAP50 ${j.result?(j.result.map50*100).toFixed(1):'—'}%`,value:j.id}))]);
const job=computed(()=>history.value?.jobs.find(j=>j.id===candidate.value));
const modelClasses=computed(()=>candidate.value==='active'?history.value?.active_classes??[]:job.value?.classes??[]);
const missing=computed(()=>props.dataset.classes.filter(c=>!modelClasses.value.some(m=>m.id===c.id)));
const current=computed(()=>rows.value[selected.value]);
const totals=computed(()=>rows.value.reduce((s,r)=>({hits:s.hits+r.score.hits,misses:s.misses+r.score.misses,falsePositives:s.falsePositives+r.score.falsePositives,clicks:s.clicks+r.score.clicks}),{hits:0,misses:0,falsePositives:0,clicks:0}));
let disposed=false;
onUnmounted(()=>{disposed=true;cancelled.value=true;});
async function refresh(){try{history.value=await buttons.history();}catch(e){error.value=String(e);}}
onMounted(()=>{emit('candidate',undefined);void refresh();});
watch(threshold,()=>{report.value='';});
watch(()=>JSON.stringify(props.dataset),()=>{report.value='';});
function changeModel(id:string){candidate.value=id;rows.value=[];selected.value=-1;report.value='';emit('candidate',id==='active'?undefined:id);}
async function inspect(index:number){selected.value=index;image.value='';const id=rows.value[index]!.sample.id;try{const data=await buttons.sample(id);if(selected.value===index && rows.value[index]?.sample.id===id)image.value=data.data_url;}catch(e){error.value=String(e);}}
async function run(){
  busy.value=true;cancelled.value=false;error.value='';rows.value=[];report.value='';selected.value=-1;
  const selection=JSON.parse(JSON.stringify(props.dataset.samples.filter(s=>s.reviewed&&(scope.value==='val'?s.split==='val':props.checked.includes(s.id))))) as ButtonSample[];
  const started=new Date().toISOString(),model=candidate.value==='active'?{directory:history.value?.active_directory}: {job_id:candidate.value},confidence=threshold.value;
  try{
    if(!selection.length)throw new Error('没有符合条件的已审核图片，请先审核或在数据集勾选图片。');
    let modelDirectory:string|undefined;
    for(const sample of selection){
      if(cancelled.value||disposed)break;
      const data=await buttons.sample(sample.id);
      const result=await buttons.recognize(data.data_url,confidence,candidate.value==='active'?undefined:candidate.value);
      if(modelDirectory && result.model_directory!==modelDirectory)throw new Error('启用模型在验收途中发生变化，请重新验收。');
      modelDirectory=result.model_directory;
      rows.value.push({sample,result,score:evaluateButtons(sample.boxes,result)});
    }
    report.value=JSON.stringify({started,model,threshold:confidence,iou:.5,scope:scope.value,cancelled:cancelled.value,planned:selection.length,totals:totals.value,rows:rows.value},null,2);
    if(rows.value.length&&!disposed)await inspect(0);
  }catch(e){error.value=String(e);}finally{busy.value=false;}
}
async function prelabelSelected(){
  busy.value=true;error.value='';cancelled.value=false;
  const selection=props.dataset.samples.filter(s=>props.checked.includes(s.id)&&!s.reviewed&&!s.boxes.length);
  const drafts:ButtonSample[]=[];
  try{
    if(!selection.length)throw new Error('请在数据集中勾选尚未审核且没有标注框的图片。');
    for(const sample of selection){
      if(cancelled.value||disposed)break;
      const data=await buttons.sample(sample.id);
      const prediction=await buttons.recognize(data.data_url,threshold.value,candidate.value==='active'?undefined:candidate.value);
      drafts.push({...sample,reviewed:false,boxes:prediction.items.filter(b=>props.dataset.classes.some(c=>c.id===b.label)).map(b=>({label:b.label,bounds:b.bounds}))});
    }
    if(drafts.length){await buttons.prelabelDrafts(drafts);emit('activated');}
    notice.value=`已保存 ${drafts.length} 张预标注草稿，仍需逐张视觉审核。已有框或已审核的图片已跳过。`;
  }catch(e){error.value=String(e);}finally{busy.value=false;}
}
function download(){const url=URL.createObjectURL(new Blob([report.value],{type:'application/json'}));const a=document.createElement('a');a.href=url;a.download=`button-acceptance-${Date.now()}.json`;a.click();setTimeout(()=>URL.revokeObjectURL(url),1000);}
async function activate(){busy.value=true;error.value='';try{await buttons.activate(candidate.value);await refresh();notice.value='已启用所选历史模型，旧启用记录已备份。';emit('activated');}catch(e){error.value=String(e);}finally{busy.value=false;}}
</script>
<template>
<section class="panel model-review">
  <div class="review-heading"><div><h3>候选模型与批量验收</h3><p>先比较历史模型，再决定启用。下方单图识别也使用此处选定模型。</p></div><NButton :disabled="busy" @click="refresh">刷新模型历史</NButton></div>
  <NAlert v-if="error" type="error">{{error}}</NAlert><NAlert v-if="notice" type="success">{{notice}}</NAlert>
  <details v-if="history?.jobs.length"><summary>所有训练任务（{{history.jobs.length}}）</summary><table class="button-table"><thead><tr><th>任务</th><th>状态</th><th>输入</th><th>mAP50</th></tr></thead><tbody><tr v-for="run in history.jobs" :key="run.id"><td>{{run.id.slice(0,8)}}</td><td>{{({completed:'完成',failed:'失败',running:'训练中',cancelled:'已取消',interrupted:'已中断'} as Record<string,string>)[run.status] || run.status}}</td><td>{{run.config?.imgsz ?? '未记录'}}</td><td>{{run.result ? (run.result.map50*100).toFixed(1)+'%' : '—'}}</td></tr></tbody></table></details>
  <label>测试模型<NSelect :value="candidate" :options="options" :disabled="busy" @update:value="changeModel" /></label>
  <p class="muted">当前启用：{{history?.active?.source_job || history?.active_directory || '读取中…'}}</p>
  <p>所选模型：{{modelClasses.map(c=>c.name).join('、') || '尚无可用类别'}}</p>
  <NAlert v-if="missing.length" type="warning">此模型不包含：{{missing.map(c=>c.name).join('、')}}。这些类别不计入验收指标，也无法被此模型识别。</NAlert>
  <div v-if="job" class="run-details">
    <p>任务 {{job.id}} · mAP50–95 {{job.result ? (job.result.map50_95*100).toFixed(1)+'%' : '未知'}}</p>
    <p v-if="job.config">{{job.config.imgsz}} px · 最多 {{job.config.epochs}} 轮 · 早停 {{job.config.patience}} 轮 · {{job.config.device}}</p><p v-else>历史任务未记录结构化训练参数。</p>
    <details v-if="job.lineage"><summary>派生模型来源</summary><pre>{{JSON.stringify(job.lineage,null,2)}}</pre></details>
    <table v-if="job.result" class="button-table"><thead><tr><th>类别</th><th>训练框</th><th>验证框</th><th>mAP50</th></tr></thead><tbody><tr v-for="c in job.classes" :key="c.id"><td>{{c.name}}</td><td>{{job.result.counts[c.id]?.train ?? '—'}}</td><td>{{job.result.counts[c.id]?.val ?? '—'}}</td><td>{{((job.result.per_class.find(p=>p.id===c.id)?.map50 ?? 0)*100).toFixed(1)}}%</td></tr></tbody></table>
  </div>
  <div class="review-controls"><label>验收图片<NSelect v-model:value="scope" :disabled="busy" :options="[{label:'全部已审核验证图片',value:'val'},{label:'数据集中勾选的已审核图片',value:'checked'}]" /></label><label>置信度<NInputNumber v-model:value="threshold" :min=".05" :max="1" :step=".05" :disabled="busy" /></label><NButton :disabled="disabled || busy || dirty || !modelClasses.length" @click="run">批量识别验收</NButton><NButton v-if="busy" @click="cancelled=true">停止后续图片</NButton><NButton :disabled="disabled || busy || dirty || !checked.length || !modelClasses.length" @click="prelabelSelected">勾选空白图片预标注</NButton><NButton :disabled="!report || busy" @click="download">导出验收报告</NButton></div>
  <p>同类别 IoU ≥ 0.5 一对一匹配；点击点按模型锚点计算。仅评估模型包含的类别。近似画面可能使验证分数偏高，仍需逐图检查。</p>
  <p aria-live="polite">已检查 {{rows.length}} 张 · 命中 {{totals.hits}} · 漏检 {{totals.misses}} · 误检 {{totals.falsePositives}} · 点击落框 {{totals.clicks}} / {{totals.hits}}</p>
  <div class="review-list"><button v-for="(row,index) in rows" :key="row.sample.id" :class="{selected:index===selected}" @click="inspect(index)">{{row.sample.name}} · 漏 {{row.score.misses}} / 误 {{row.score.falsePositives}} / 点击 {{row.score.clicks}}:{{row.score.hits}}</button></div>
  <template v-if="current && image"><div class="review-heading"><h4>{{current.sample.name}}</h4><NButton :disabled="dirty || busy" @click="emit('annotate',current.sample.id)">返回此图修正标注</NButton></div><div class="review-images"><div><p>人工标注（含未参与模型的类别）</p><ButtonCanvas :image="image" :width="current.sample.width" :height="current.sample.height" :boxes="current.sample.boxes" :classes="dataset.classes" :selected="-1" :zoom="1" readonly /></div><div><p>识别框与点击锚点 · {{current.result.compute.provider}}</p><ButtonCanvas :image="image" :width="current.sample.width" :height="current.sample.height" :boxes="current.result.items" :classes="current.result.classes" :selected="-1" :zoom="1" readonly /></div></div><table class="button-table"><thead><tr><th>识别类别</th><th>置信度</th><th>原图点击坐标</th></tr></thead><tbody><tr v-for="(d,index) in current.result.items" :key="index"><td>{{d.name}} · {{d.label}}</td><td>{{(d.confidence*100).toFixed(1)}}%</td><td>{{d.point[0]}}, {{d.point[1]}}</td></tr></tbody></table></template>
  <NAlert v-if="candidate!=='active'" type="info">启用会切换自动化使用的按钮模型。请检查误检、漏检和点击位置；可从历史列表重新启用旧任务。</NAlert>
  <NButton v-if="candidate!=='active'" :disabled="disabled || busy || dirty || cancelled || !report || rows.length===0" @click="activate">启用已验收的候选模型</NButton>
</section>
</template>
<style scoped>
.model-review{display:grid;gap:12px}.model-review p{margin:0;font-size:12px;overflow-wrap:anywhere}.review-heading,.review-controls{display:flex;gap:14px;align-items:center;justify-content:space-between;flex-wrap:wrap}.review-heading h3{margin:0 0 6px}.review-controls label{min-width:180px}.review-list{display:flex;flex-wrap:wrap;gap:6px;max-height:190px;overflow:auto}.review-list button{background:var(--bg-subtle);color:inherit;border:1px solid var(--border-color);padding:7px;cursor:pointer}.review-list button:hover{background:var(--bg-hover)}.review-list button:focus-visible{outline:2px solid var(--primary);outline-offset:2px}.review-list .selected{border-color:var(--primary)}.review-images{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:12px}.review-images>div{min-width:0}.run-details pre{max-height:180px;overflow:auto}@media(max-width:1000px){.review-images{grid-template-columns:1fr}}
</style>
