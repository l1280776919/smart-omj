import {expect,it} from 'vitest';
import {evaluateButtons} from './buttonEvaluation';
import type {ButtonResult,ButtonBox} from './buttons';
const box:ButtonBox={label:'soulbtn',bounds:[10,10,30,30]};
const result:ButtonResult={classes:[{id:'soulbtn',name:'御魂副本',anchor:[.5,.5]}],items:[],dimensions:[100,100],compute:{provider:'CUDA',fallback_reason:null},timings:{inference_ms:1,total_ms:1}};
it('matches each target once, measures missed targets and wrong click anchors',()=>{
 const prediction={...box,name:'御魂副本',confidence:.9,point:[50,50] as [number,number]};
 expect(evaluateButtons([box,{...box,bounds:[60,60,90,90]}],{...result,items:[prediction,{...prediction,confidence:.8}]})).toEqual({hits:1,misses:1,falsePositives:1,clicks:0,targets:2,excluded:0});
});
it('does not match another class and explicitly excludes unsupported targets',()=>{
 expect(evaluateButtons([box,{...box,label:'layer1'}],{...result,items:[{...box,label:'back',name:'返回',confidence:.9,point:[20,20]}]})).toEqual({hits:0,misses:1,falsePositives:1,clicks:0,targets:1,excluded:1});
});
it('includes correct boundary clicks and counts predictions on negative frames',()=>{
 const predicted={...result,items:[{...box,name:'御魂副本',confidence:.9,point:[10,30] as [number,number]}]};
 expect(evaluateButtons([box],predicted).clicks).toBe(1);
 expect(evaluateButtons([],predicted).falsePositives).toBe(1);
});
