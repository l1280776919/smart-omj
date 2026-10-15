import { invoke } from '@tauri-apps/api/core';
export interface ButtonClass { id:string; name:string; anchor:[number,number] }
export interface ButtonBox { label:string; bounds:[number,number,number,number] }
export interface ButtonSample { id:string; name:string; batch:string; split:'train'|'val'; reviewed:boolean; width:number; height:number; boxes:ButtonBox[] }
export interface ButtonDataset { classes:ButtonClass[]; samples:ButtonSample[] }
export interface ButtonResult {
  model_directory?:string;
  items:(ButtonBox & {name:string;confidence:number;point:[number,number]})[];
  classes:ButtonClass[]; dimensions:[number,number]; compute:{provider:string;fallback_reason:string|null};
  timings:{inference_ms:number;total_ms:number};
}
export interface ButtonStatus {
  dataset:ButtonDataset; available:boolean; root:string; model_classes:ButtonClass[];
  job:null|{status:string;log:string;progress:null|{stage:string;progress:number;error?:string;device_name?:string;epoch?:number;epochs?:number};
    result:null|{map50:number;map50_95:number;per_class:(ButtonClass & {map50:number})[];counts:Record<string,{train:number;val:number}>}};
}
export interface ButtonJob {
  id:string; status:string; config:null|{epochs:number;imgsz:number;patience:number;device:string;class_ids:string[]};
  classes:ButtonClass[]; result:NonNullable<ButtonStatus['job']>['result']; lineage:unknown;
}
export interface ButtonHistory {jobs:ButtonJob[];active:{source_job?:string;directory?:string}|null;active_directory:string;active_classes:ButtonClass[]}
export const buttons = {
  prelabelDrafts:(samples:ButtonSample[])=>invoke<number>('button_prelabel_drafts',{samples}),
  history:()=>invoke<ButtonHistory>('button_history'),
  status:()=>invoke<ButtonStatus>('button_status'),
  import:(dataUrl:string,name:string,batch:string)=>invoke<string>('button_import',{dataUrl,name,batch}),
  importScene:(index:number)=>invoke<string>('button_import_scene',{index}),
  sample:(id:string)=>invoke<{sample:ButtonSample;data_url:string}>('button_sample',{id}),
  save:(sample:ButtonSample,classes:ButtonClass[])=>invoke<void>('button_save',{sample,classes}),
  partition:(updates:{id:string;split:'train'|'val';batch:string}[])=>invoke<void>('button_partition',{updates}),
  pruneClasses:()=>invoke<number>('button_prune_classes'),
  editClass:(id:string,replacement:ButtonClass|null)=>invoke<void>('button_edit_class',{id,replacement}),
  delete:(id:string)=>invoke<void>('button_delete',{id}),
  start:(epochs:number,device:string,classIds?:string[],imgsz=640,patience=15)=>invoke<void>('button_start',{epochs,device,classIds,imgsz,patience}),
  cancel:()=>invoke<void>('button_cancel'),
  activate:(jobId?:string)=>invoke<void>('button_activate',{jobId}),
  recognize:(dataUrl:string,threshold:number,jobId?:string)=>invoke<ButtonResult>('button_recognize',{dataUrl,threshold,jobId}),
};
