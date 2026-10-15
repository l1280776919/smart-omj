// @vitest-environment jsdom
import { mount,flushPromises } from '@vue/test-utils';
import { createPinia,setActivePinia } from 'pinia';
import { expect,it,vi,beforeEach } from 'vitest';
import ButtonStudio from './ButtonStudio.vue';
import { buttons,type ButtonStatus,type ButtonSample } from '../services/buttons';
import { useAppStore } from '../stores/app';
vi.mock('vue-router',()=>({onBeforeRouteLeave:vi.fn()}));
vi.mock('../services/buttons',()=>({buttons:{status:vi.fn(),sample:vi.fn(),save:vi.fn(),recognize:vi.fn(),partition:vi.fn(),editClass:vi.fn(),pruneClasses:vi.fn(),start:vi.fn()}}));
beforeEach(()=>vi.clearAllMocks());
const png='data:image/png;base64,AA==';
function setup(sharedBatch=false){
  const pinia=createPinia();setActivePinia(pinia);useAppStore().connected=true;
  const sample:ButtonSample={id:'one',name:'capture.png',batch:'visit-a',split:'train',reviewed:false,width:1920,height:1080,boxes:[]};
  const status:ButtonStatus={available:true,root:'test',model_classes:[],job:null,dataset:{classes:[{id:'challenge',name:'挑战',anchor:[.5,.5]}],samples:[sample]}};
  if(sharedBatch){sample.reviewed=true;sample.boxes=[{label:'challenge',bounds:[10,20,50,60]}];status.dataset.samples.push({...sample,id:'two',split:'val'});}
  vi.mocked(buttons.status).mockImplementation(async()=>JSON.parse(JSON.stringify(status)));
  vi.mocked(buttons.sample).mockImplementation(async()=>({sample:JSON.parse(JSON.stringify(sample)),data_url:png}));
  vi.mocked(buttons.save).mockResolvedValue();
  return mount(ButtonStudio,{global:{plugins:[pinia],stubs:{ButtonCanvas:true}}});
}
it('loads independent annotations and requires explicit negative review before saving empty labels',async()=>{
  const page=setup();await flushPromises();
  const button=(text:string)=>page.findAll('button').find(b=>b.text()===text)!;
  await page.find('.button-sample-list button').trigger('click');await flushPromises();
  expect(page.text()).toContain('1920 × 1080');
  await button('保存标注').trigger('click');await flushPromises();
  expect(page.text()).toContain('请标注按钮，或明确勾选');expect(buttons.save).not.toHaveBeenCalled();
  await page.find('[role=checkbox]').trigger('click');
  await button('保存标注').trigger('click');await flushPromises();
  expect(buttons.save).toHaveBeenCalledWith(expect.objectContaining({reviewed:true,boxes:[]}),expect.any(Array));
  const payload=vi.mocked(buttons.save).mock.calls[0]![0];expect(payload).not.toHaveProperty('scene');
  page.unmount();
});
it('prelabels remain editable and unsaved, without a scene association',async()=>{
  const page=setup();await flushPromises();await page.find('.button-sample-list button').trigger('click');await flushPromises();
  vi.mocked(buttons.recognize).mockResolvedValue({items:[{label:'challenge',name:'挑战',bounds:[1600,800,1800,1000],point:[1700,900],confidence:.9}],classes:[{id:'challenge',name:'挑战',anchor:[.5,.5]}],dimensions:[1920,1080],compute:{provider:'CUDA',fallback_reason:null},timings:{inference_ms:2,total_ms:10}});
  await page.findAll('button').find(b=>b.text()==='模型预标注')!.trigger('click');await flushPromises();
  expect(page.text()).toContain('未保存');expect(page.text()).toContain('本图标注 · 1');
  expect(page.text()).not.toContain('所属场景');
  page.unmount();
});

it('shows precise missing-data states and sends only explicitly selected image IDs',async()=>{
  const page=setup();await flushPromises();
  await page.findAll('[role=tab]').find(t=>t.text()==='数据集与类别')!.trigger('click');
  expect(page.text()).toContain('缺已审核标注');expect(page.text()).not.toContain('需补充');
  vi.mocked(buttons.partition).mockResolvedValue();
  const checkboxes=page.findAll('[role=checkbox]');
  const choice=checkboxes.find(c=>c.attributes('aria-label')==='选择 capture.png')!;
  await choice.trigger('click');await flushPromises();
  await page.findAll('button').find(b=>b.text()==='选中图片设为验证集')!.trigger('click');await flushPromises();
  expect(buttons.partition).toHaveBeenCalledWith([{id:'one',batch:'visit-a',split:'val'}]);
  expect(buttons.save).not.toHaveBeenCalled();page.unmount();
});
it('opens explicit category delete confirmation before any dataset mutation',async()=>{
  const page=setup();await flushPromises();
  await page.findAll('[role=tab]').find(t=>t.text()==='数据集与类别')!.trigger('click');
  await page.findAll('button').find(b=>b.text()==='删除')!.trigger('click');await flushPromises();
  expect(buttons.editClass).not.toHaveBeenCalled();
  expect(document.body.textContent).toContain('确认删除类别及标注');
  page.unmount();
});
it('changes the visible image partition without saving or discarding pending boxes',async()=>{
  const page=setup();await flushPromises();
  await page.find('.button-sample-list button').trigger('click');await flushPromises();
  page.findComponent({name:'ButtonCanvas'}).vm.$emit('change',[{label:'challenge',bounds:[10,20,50,60]}]);
  await flushPromises();
  vi.mocked(buttons.partition).mockResolvedValue();
  await page.find('.image-partition').findAll('button').find(b=>b.text()==='验证集')!.trigger('click');await flushPromises();
  expect(buttons.partition).toHaveBeenCalledWith([{id:'one',batch:'visit-a',split:'val'}]);
  expect(buttons.save).not.toHaveBeenCalled();
  expect(page.text()).toContain('本图标注 · 1');expect(page.text()).toContain('修改尚未保存');
  await page.findAll('button').find(b=>b.text()==='保存标注')!.trigger('click');await flushPromises();
  expect(buttons.save).toHaveBeenCalledWith(expect.objectContaining({split:'val',reviewed:true,boxes:[{label:'challenge',bounds:[10,20,50,60]}]}),expect.any(Array));
  page.unmount();
});
it('opens a dataset thumbnail in the large annotation view',async()=>{
  const page=setup();await flushPromises();
  await page.findAll('[role=tab]').find(t=>t.text()==='数据集与类别')!.trigger('click');await flushPromises();
  expect(page.find('.preview-image img').attributes('src')).toBe(png);
  await page.find('.preview-image').trigger('click');await flushPromises();
  expect(page.find('.image-partition').exists()).toBe(true);
  expect(page.findComponent({name:'ButtonCanvas'}).props('image')).toBe(png);
  page.unmount();
});
it('allows training when reviewed train and validation images share a source batch',async()=>{
  const page=setup(true);await flushPromises();
  await page.findAll('[role=tab]').find(t=>t.text()==='训练模型')!.trigger('click');await flushPromises();
  expect(page.text()).toContain('允许同批图片分别用于训练和验证');
  const start=page.findAll('button').find(b=>b.text().includes('开始训练'))!;
  expect(start.exists()).toBe(true);expect(start.attributes('disabled')).toBeUndefined();
  page.unmount();
});

it('passes selected training configuration without changing annotations',async()=>{
 const page=setup(true);await flushPromises();
 await page.findAll('[role=tab]').find(t=>t.text()==='训练模型')!.trigger('click');
 vi.mocked(buttons.start).mockResolvedValue();
 await page.findAll('button').find(b=>b.text()==='开始训练')!.trigger('click');await flushPromises();
 expect(buttons.start).toHaveBeenCalledWith(50,'auto',['challenge'],640,15);
 expect(buttons.save).not.toHaveBeenCalled();
 page.unmount();
});
