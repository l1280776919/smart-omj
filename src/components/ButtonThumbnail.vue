<script setup lang="ts">
import { onMounted,onUnmounted,ref } from 'vue';
import { buttons } from '../services/buttons';
const props=defineProps<{id:string;name:string}>();
const root=ref<HTMLElement>(),url=ref(''),failed=ref(false);
let observer:IntersectionObserver|undefined,disposed=false;
async function load(){
  observer?.disconnect();
  try{const result=await buttons.sample(props.id);if(!disposed)url.value=result.data_url;}
  catch{if(!disposed)failed.value=true;}
}
onMounted(()=>{
  if(typeof IntersectionObserver==='undefined'){void load();return;}
  observer=new IntersectionObserver(entries=>{if(entries.some(e=>e.isIntersecting))void load();},{rootMargin:'100px'});
  if(root.value)observer.observe(root.value);
});
onUnmounted(()=>{disposed=true;observer?.disconnect();});
</script>
<template><span ref="root" class="button-thumbnail"><img v-if="url" :src="url" :alt="name"/><span v-else>{{ failed?'预览不可用':'加载预览…' }}</span></span></template>
<style scoped>
.button-thumbnail{display:flex;width:128px;height:72px;align-items:center;justify-content:center;background:#edf2f8;border-radius:5px;overflow:hidden;flex-shrink:0;color:#718096;font-size:11px}.button-thumbnail img{display:block;width:100%;height:100%;object-fit:contain}
</style>
