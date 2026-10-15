// @vitest-environment jsdom
import {mount,flushPromises} from '@vue/test-utils';
import {beforeEach,expect,it,vi} from 'vitest';
import {NSelect,NInputNumber} from 'naive-ui';
import ButtonModelReview from './ButtonModelReview.vue';
import {buttons,type ButtonDataset,type ButtonHistory,type ButtonResult} from '../services/buttons';
vi.mock('../services/buttons',()=>({buttons:{history:vi.fn(),sample:vi.fn(),recognize:vi.fn(),activate:vi.fn(),prelabelDrafts:vi.fn()}}));
const classes=[{id:'soulbtn',name:'御魂副本',anchor:[.5,.5] as [number,number]}];
const dataset:ButtonDataset={classes,samples:[{id:'one',name:'explore.png',batch:'real-source',split:'val',reviewed:true,width:100,height:100,boxes:[{label:'soulbtn',bounds:[10,10,40,40]}]}]};
const prediction:ButtonResult={model_directory:'candidate/export',classes,items:[{label:'soulbtn',name:'御魂副本',bounds:[10,10,40,40],point:[25,25],confidence:.9}],dimensions:[100,100],compute:{provider:'CUDA',fallback_reason:null},timings:{inference_ms:1,total_ms:2}};
beforeEach(()=>{
 vi.clearAllMocks();
 vi.mocked(buttons.history).mockResolvedValue({active:null,active_directory:'active-model',active_classes:classes,jobs:[{id:'candidate',status:'completed',config:null,classes,lineage:null,result:{map50:.9,map50_95:.8,counts:{soulbtn:{train:5,val:4}},per_class:[{...classes[0]!,map50:.9}]}}]} satisfies ButtonHistory);
 vi.mocked(buttons.sample).mockResolvedValue({sample:dataset.samples[0]!,data_url:'data:image/png;base64,AA=='});
 vi.mocked(buttons.recognize).mockResolvedValue(prediction);
});
function setup(data=dataset){return mount(ButtonModelReview,{props:{dataset:JSON.parse(JSON.stringify(data)),checked:['one'],dirty:false,disabled:false},global:{stubs:{ButtonCanvas:true}}});}
it('tests a historical candidate without activation and invalidates acceptance after threshold changes',async()=>{
 const page=setup();await flushPromises();
 page.findAllComponents(NSelect)[0]!.vm.$emit('update:value','candidate');await flushPromises();
 const activate=()=>page.findAll('button').find(b=>b.text()==='启用已验收的候选模型')!;
 expect(activate().attributes('disabled')).toBeDefined();
 await page.findAll('button').find(b=>b.text()==='批量识别验收')!.trigger('click');await flushPromises();
 expect(buttons.recognize).toHaveBeenCalledWith('data:image/png;base64,AA==',.5,'candidate');
 expect(buttons.activate).not.toHaveBeenCalled();
 expect(page.text()).toContain('点击落框 1 / 1');
 expect(activate().attributes('disabled')).toBeUndefined();
 page.findComponent(NInputNumber).vm.$emit('update:value',.7);await flushPromises();
 expect(activate().attributes('disabled')).toBeDefined();page.unmount();
});
it('prelabels only selected empty pending images and keeps them unreviewed',async()=>{
 const data=JSON.parse(JSON.stringify(dataset)) as ButtonDataset;data.samples[0]!.reviewed=false;data.samples[0]!.boxes=[];
 data.samples.push({...dataset.samples[0]!,id:'reviewed'});
 const page=setup(data);await page.setProps({checked:['one','reviewed']});await flushPromises();
 vi.mocked(buttons.prelabelDrafts).mockResolvedValue(1);
 await page.findAll('button').find(b=>b.text()==='勾选空白图片预标注')!.trigger('click');await flushPromises();
 expect(buttons.prelabelDrafts).toHaveBeenCalledWith([expect.objectContaining({id:'one',reviewed:false,batch:'real-source',boxes:[{label:'soulbtn',bounds:[10,10,40,40]}]})]);page.unmount();
});
