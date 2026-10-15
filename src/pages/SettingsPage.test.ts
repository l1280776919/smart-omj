// @vitest-environment jsdom
import { mount, flushPromises } from '@vue/test-utils';
import { createPinia, setActivePinia } from 'pinia';
import { beforeEach, expect, it, vi } from 'vitest';
import { NCheckbox } from 'naive-ui';
import SettingsPage from './SettingsPage.vue';
import { desktop } from '../services/desktop';
import { useAppStore } from '../stores/app';
import type { Config, ImportPreview, Snapshot } from '../types';
vi.mock('../services/desktop', () => ({ desktop: { previewImport: vi.fn(), importConfig: vi.fn(), snapshot: vi.fn() } }));
const base: Config = { schema_version: 1, serial: 'emulator-5554', endpoint: '127.0.0.1:16384', soul: { rounds: 10, max_minutes: 30, stage_timeout_seconds: 180, layer: 11 } };
const imported: Config = { ...base, soul: { ...base.soul, rounds: 40 } };
function snapshot(config = base, sequence = 0): Snapshot {
  return { config, sequence, run: null, queue: { enabled: false, entries: [] }, logs: [], adb_source: 'adb', data_directory: 'data' };
}
const report: ImportPreview = { base, config: imported, mappings: [{ source: '/orochi/orochi_config/limit_count', target: '/soul/rounds', before: 10, after: 40 }], warnings: ['仅支持单人'], errors: [], ignored: ['/orochi/scheduler/enable'] };
beforeEach(() => { vi.resetAllMocks(); });
function setup() {
  const pinia = createPinia(); setActivePinia(pinia);
  const store = useAppStore(); store.connected = true; store.apply(snapshot());
  const page = mount(SettingsPage, { global: { plugins: [pinia] } });
  return { store, page };
}
async function choose(page: ReturnType<typeof setup>['page'], size = 20) {
  const input = page.get('input[type=file]');
  const bytes = new TextEncoder().encode('\uFEFF{"orochi":{}}\n');
  Object.defineProperty(input.element, 'files', { value: [{ name: 'old.json', size, arrayBuffer: async () => bytes.buffer }], configurable: true });
  await input.trigger('change'); await flushPromises();
}
it('previews before explicit confirmation, preserves BOM, imports and refreshes state', async () => {
  vi.mocked(desktop.previewImport).mockResolvedValue(report);
  vi.mocked(desktop.importConfig).mockResolvedValue('data/config/imports/test');
  vi.mocked(desktop.snapshot).mockResolvedValue({ snapshot: snapshot(imported, 1), limits: {} as never });
  const { page, store } = setup(); await choose(page);
  expect(desktop.previewImport).toHaveBeenCalledWith('\uFEFF{"orochi":{}}\n');
  expect(desktop.importConfig).not.toHaveBeenCalled();
  expect(page.text()).toContain('/orochi/scheduler/enable');
  const apply = page.findAll('button').find(button => button.text() === '备份并导入')!;
  expect(apply.attributes('disabled')).toBeDefined();
  page.findComponent(NCheckbox).vm.$emit('update:checked', true); await flushPromises();
  await apply.trigger('click'); await flushPromises();
  expect(desktop.importConfig).toHaveBeenCalledWith('\uFEFF{"orochi":{}}\n', base);
  expect(store.config?.soul.rounds).toBe(40);
  expect(page.text()).toContain('data/config/imports/test'); page.unmount();
});
it('invalidates stale preview and blocks selection while queue is enabled', async () => {
  vi.mocked(desktop.previewImport).mockResolvedValue(report);
  const { page, store } = setup(); await choose(page);
  store.apply(snapshot(imported, 1)); await flushPromises();
  expect(page.text()).toContain('当前配置已变化');
  expect(page.findAll('button').find(button => button.text() === '备份并导入')?.attributes('disabled')).toBeDefined();
  store.apply({ ...snapshot(imported, 2), queue: { enabled: true, entries: [] } }); await flushPromises();
  expect(page.get('input[type=file]').attributes('disabled')).toBeDefined(); page.unmount();
});
it('rejects oversized files before IPC and prevents applying incompatible fields', async () => {
  const { page, store } = setup(); await choose(page, 1024 * 1024 + 1);
  expect(desktop.previewImport).not.toHaveBeenCalled(); expect(store.error).toContain('1 MiB');
  vi.mocked(desktop.previewImport).mockResolvedValue({ ...report, errors: ['不支持无限次数'] });
  await choose(page);
  expect(page.text()).toContain('不支持无限次数');
  expect(page.findAll('button').find(button => button.text() === '备份并导入')?.attributes('disabled')).toBeDefined(); page.unmount();
});
