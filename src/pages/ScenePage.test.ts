// @vitest-environment jsdom
import { mount, flushPromises } from '@vue/test-utils';
import { createPinia, setActivePinia } from 'pinia';
import { expect, it, vi } from 'vitest';
import ScenePage from './ScenePage.vue';
import { useAppStore } from '../stores/app';
import { scenes, type SceneStatus } from '../services/scenes';
vi.mock('../services/scenes',()=>({sceneName:(v:string)=>v,scenes:{status:vi.fn(),start:vi.fn(),cancel:vi.fn(),activate:vi.fn(),samples:vi.fn(),exclude:vi.fn()}}));
it('starts preparation with chosen defaults and blocks duplicate jobs',async()=>{
  const pinia=createPinia();setActivePinia(pinia);useAppStore().connected=true;
  const status:SceneStatus={root:'training',available:true,classes:[{name:'yard',videos:['a.mp4']},{name:'explore',videos:['b.mp4']}],prepared:null,model:null,job:null};
  vi.mocked(scenes.status).mockImplementation(async()=>structuredClone(status));
  const page=mount(ScenePage,{global:{plugins:[pinia],stubs:{RouterLink:true}}});
  await flushPromises();await page.findAll('[role=tab]')[1]!.trigger('click');
  const button=(label:string)=>page.findAll('button').find(b=>b.text()===label)!;
  expect(button('开始训练').attributes('disabled')).toBeDefined();
  vi.mocked(scenes.start).mockImplementation(async()=>{status.job={action:'prepare',status:'running',directory:'job',log:'',progress:null,result:null};});
  await button('抽取图片素材').trigger('click');await flushPromises();
  expect(scenes.start).toHaveBeenCalledWith('prepare',.5,30,'auto');
  expect(button('抽取图片素材').attributes('disabled')).toBeDefined();
  expect(button('取消任务').exists()).toBe(true);
  page.unmount();
});
