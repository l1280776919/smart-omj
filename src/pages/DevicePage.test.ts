// @vitest-environment jsdom
import { flushPromises, mount } from '@vue/test-utils';
import { createPinia, setActivePinia } from 'pinia';
import { nextTick } from 'vue';
import { NInputNumber, NSwitch } from 'naive-ui';
import { expect, it, vi } from 'vitest';
import { desktop } from '../services/desktop';
import { useAppStore } from '../stores/app';
import DevicePage from './DevicePage.vue';
vi.mock('../services/desktop', () => ({ desktop: { tap: vi.fn() } }));

it('uses native screenshot bounds and sends the preview size with manual clicks', async () => {
  const pinia = createPinia(); setActivePinia(pinia); const store = useAppStore();
  store.connected = true;
  store.apply({ sequence: 0, config: { schema_version: 1, serial: 'emulator-5554', endpoint: '127.0.0.1:16384', soul: { rounds: 1, max_minutes: 1, stage_timeout_seconds: 30, layer: 11 } }, run: null, logs: [], queue: { enabled: false, entries: [] }, adb_source: 'adb', data_directory: 'data' });
  const page = mount(DevicePage, { global: { plugins: [pinia] } });
  store.screen = { width: 1920, height: 1080, data_url: '' };
  page.findComponent(NSwitch).vm.$emit('update:value', true); await nextTick();
  let coordinates = page.findAllComponents(NInputNumber);
  expect(coordinates.map(input => input.props('max'))).toEqual([1919,1079]);
  coordinates[0]!.vm.$emit('update:value', 1800); coordinates[1]!.vm.$emit('update:value', 900);
  await nextTick(); vi.mocked(desktop.tap).mockResolvedValue(store.screen);
  await page.findAll('button').find(button => button.text() === '点击并重新截图')!.trigger('click');
  await flushPromises();
  expect(desktop.tap).toHaveBeenCalledWith('emulator-5554',1800,900,[1920,1080]);
  store.screen = { width: 720, height: 1280, data_url: '' }; await nextTick();
  coordinates = page.findAllComponents(NInputNumber);
  expect(coordinates.map(input => input.props('max'))).toEqual([719,1279]);
  expect(coordinates.map(input => input.props('value'))).toEqual([360,640]);
  page.unmount();
});
