<script setup lang="ts">
import {onUnmounted,ref} from 'vue';
import {NAlert,NButton,NCheckbox,NInputNumber} from 'naive-ui';
import {buttons} from '../services/buttons';
const props=defineProps<{disabled:boolean}>();const emit=defineEmits<{imported:[]}>();
const input=ref<HTMLInputElement>(),file=ref<File>(),interval=ref(5),start=ref(0),limit=ref(24),busy=ref(false),error=ref(''),notice=ref('');
const frames=ref<{seconds:number;url:string;selected:boolean;imported:boolean}[]>([]);
let cancelled=false,disposed=false,video:HTMLVideoElement|undefined,url='';
function cleanup(){if(video){video.pause();video.removeAttribute('src');video.load();video=undefined;}if(url){URL.revokeObjectURL(url);url='';}}
onUnmounted(()=>{disposed=true;cancelled=true;cleanup();});
function choose(e:Event){const selected=(e.target as HTMLInputElement).files?.[0];if(selected){file.value=selected;frames.value=[];notice.value='';} (e.target as HTMLInputElement).value='';}
function event(target:HTMLVideoElement,name:string){return new Promise<void>((resolve,reject)=>{const done=()=>{clearTimeout(timer);target.removeEventListener(name,success);target.removeEventListener('error',failure);};const success=()=>{done();resolve();};const failure=()=>{done();reject(new Error('录像解码失败，请使用系统支持的 MP4/H.264 文件。'));};const timer=setTimeout(()=>{done();reject(new Error('录像读取超时，请缩短录像或换一种编码。'));},15000);target.addEventListener(name,success,{once:true});target.addEventListener('error',failure,{once:true});});}
async function sample(){
 if(!file.value)return;busy.value=true;error.value='';frames.value=[];cancelled=false;
 try{
  if(!Number.isFinite(start.value)||start.value<0||!Number.isFinite(interval.value)||interval.value<1||interval.value>600||!Number.isInteger(limit.value)||limit.value<1||limit.value>60)throw new Error('请填写有效的起始时间、间隔和候选帧数量。');
  cleanup();video=document.createElement('video');video.muted=true;video.preload='auto';url=URL.createObjectURL(file.value);
  const loaded=event(video,'loadeddata');video.src=url;await loaded;
  if(!Number.isFinite(video.duration)||start.value>=video.duration)throw new Error('起始时间超出录像范围。');
  const canvas=document.createElement('canvas');canvas.width=video.videoWidth;canvas.height=video.videoHeight;const context=canvas.getContext('2d')!;
  for(let i=0;i<limit.value;i++){
   if(cancelled||disposed)break;const seconds=start.value+i*interval.value;if(seconds>=video.duration)break;
   if(Math.abs(video.currentTime-seconds)>.001){const seeked=event(video,'seeked');video.currentTime=seconds;await seeked;}
   context.drawImage(video,0,0);frames.value.push({seconds,url:canvas.toDataURL('image/png'),selected:false,imported:false});
   await new Promise(resolve=>setTimeout(resolve,0));
  }
  notice.value=`已生成 ${frames.value.length} 张候选帧，请选择清晰且有差异的画面。`;
 }catch(e){error.value=String(e);}finally{cleanup();busy.value=false;}
}
async function importFrames(){if(!file.value)return;busy.value=true;error.value='';cancelled=false;let count=0;
 try{
  const source=`video:${file.value.name}:${file.value.size}:${file.value.lastModified}`;
  if(source.length>200)throw new Error('录像文件名过长，请缩短文件名后重试。');
  for(const frame of frames.value.filter(f=>f.selected&&!f.imported)){
   if(cancelled||disposed)break;
   await buttons.import(frame.url,`${file.value.name} @ ${frame.seconds.toFixed(3)}s`,source);frame.imported=true;frame.selected=false;count++;
  }
  notice.value=`已导入 ${count} 张原分辨率图片，均为待审核；原录像保留。`;
 }catch(e){error.value=String(e);}finally{emit('imported');busy.value=false;}
}
</script>
<template><details class="panel video-sampler"><summary>从录像抽帧并挑选素材</summary><div class="video-controls"><input ref="input" type="file" accept="video/*" hidden @change="choose"/><NButton :disabled="disabled || busy" @click="input?.click()">选择本地录像</NButton><span>{{file?.name || '尚未选择录像'}}</span><label>起始秒<NInputNumber v-model:value="start" :min="0" :disabled="busy" /></label><label>间隔秒<NInputNumber v-model:value="interval" :min="1" :max="600" :disabled="busy" /></label><label>最多候选帧<NInputNumber v-model:value="limit" :min="1" :max="60" :precision="0" :disabled="busy" /></label><NButton :disabled="disabled || busy || !file" @click="sample">生成预览</NButton><NButton v-if="busy" @click="cancelled=true">停止后续处理</NButton></div><p>保留原始分辨率和真实录像来源，不自动全选相邻帧。来源相同的图片仍可分别划分训练与验证集。</p><NAlert v-if="error" type="error">{{error}}</NAlert><p aria-live="polite">{{notice}}</p><div class="video-frames"><label v-for="frame in frames" :key="frame.seconds"><img :src="frame.url" :alt="`${frame.seconds} 秒画面`"/><NCheckbox v-model:checked="frame.selected" :disabled="busy || frame.imported">{{frame.seconds.toFixed(2)}} 秒 {{frame.imported?'已导入':''}}</NCheckbox></label></div><NButton :disabled="disabled || busy || !frames.some(f=>f.selected&&!f.imported)" @click="importFrames">导入勾选帧</NButton></details></template>
<style scoped>.video-sampler summary{cursor:pointer;font-weight:600}.video-controls{display:flex;align-items:end;flex-wrap:wrap;gap:12px;margin-top:16px}.video-controls label{width:130px}.video-frames{display:grid;grid-template-columns:repeat(auto-fill,minmax(220px,1fr));gap:12px;max-height:550px;overflow:auto;margin:12px 0}.video-frames img{width:100%;display:block}.video-sampler p{font-size:12px;color:var(--text-secondary)}</style>
