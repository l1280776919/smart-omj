// @vitest-environment jsdom
import { mount } from '@vue/test-utils';
import { expect,it,vi } from 'vitest';
import ButtonCanvas from './ButtonCanvas.vue';
it('maps a drag on a scaled and offset canvas into original image coordinates',async()=>{
  vi.stubGlobal('DOMPoint',class {
    constructor(public x:number,public y:number){}
    matrixTransform(){return {x:(this.x-100)*2,y:(this.y-50)*2};}
  });
  const page=mount(ButtonCanvas,{props:{image:'data:image/png;base64,AA==',width:1920,height:1080,boxes:[],classes:[{id:'challenge',name:'挑战',anchor:[.5,.5]}],selected:-1,drawLabel:'challenge',zoom:1}});
  const svg=page.find('svg').element as SVGSVGElement;
  svg.getScreenCTM=()=>({inverse:()=>({})}) as unknown as DOMMatrix;
  svg.setPointerCapture=()=>{};
  svg.dispatchEvent(new MouseEvent('pointerdown',{clientX:200,clientY:150,button:0,bubbles:true}));
  svg.dispatchEvent(new MouseEvent('pointermove',{clientX:400,clientY:300,bubbles:true}));
  svg.dispatchEvent(new MouseEvent('pointerup',{bubbles:true}));
  expect(page.emitted('change')?.[0]?.[0]).toEqual([{label:'challenge',bounds:[200,200,600,500]}]);
  page.unmount();vi.unstubAllGlobals();
});
