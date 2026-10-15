import { invoke } from '@tauri-apps/api/core';
export interface Compute { provider: string; fallback_reason: string | null }
export interface ModelInfo { classes: Record<string,string>; top1: number; limitation: string; input_size: number; device?: string; device_name?: string }
export interface SceneStatus {
  root: string; available: boolean; classes: {name:string;videos:string[]}[];
  prepared: null | {images:number;classes:Record<string,number>;interval:number};
  model: ModelInfo | null;
  job: null | {action:string; status:string; directory:string; log:string; result:ModelInfo|null;
    progress:null|{stage:string;progress:number;epoch?:number;epochs?:number;error?:string;device?:string;device_name?:string}};
}
export interface Recognition { data_url:string;prediction:{compute?:Compute;class:string;confidence:number;uncertain:boolean;elapsed_ms:number;scores:{class:string;confidence:number}[]} }
export interface Samples { total:number;items:{index:number;label:string;seconds:number;thumbnail:string|null;excluded:boolean}[] }
export const scenes = {
  samples: (page:number) => invoke<Samples>('scene_samples',{page}),
  exclude: (index:number,excluded:boolean) => invoke<void>('scene_set_excluded',{index,excluded}),
  status: () => invoke<SceneStatus>('scene_status'),
  start: (action:string,interval:number,epochs:number,device='auto') => invoke<void>('scene_start',{action,interval,epochs,device}),
  cancel: () => invoke<void>('scene_cancel'),
  activate: () => invoke<void>('scene_activate'),
  open: (kind:string) => invoke<void>('scene_open_folder',{kind}),
  recognize: (serial:string) => invoke<Recognition>('scene_recognize',{serial}),
};
export const sceneName = (name:string) => ({yard:'庭院',explore:'探索',mitama:'御魂入口',orochi:'八岐大蛇',ready:'战斗准备',fight:'战斗中',fail:'挑战失败',results:'战斗结算',battle:'战斗',victory:'胜利',defeat:'失败',loading:'加载'}[name] ?? name);
