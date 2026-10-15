import { invoke, isTauri } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { VisualPreview, Bootstrap, RepeatSchedule, ImportPreview, Config, Device, Diagnosis, Screenshot, Snapshot, VerifiedClick } from '../types';

function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (!isTauri()) return Promise.reject(new Error('当前是浏览器预览。请使用 pnpm tauri dev 启动桌面应用。'));
  return invoke<T>(command, args);
}
export const desktop = {
  snapshot: () => call<Bootstrap>('get_snapshot'),
  previewImport: (content: string) => call<ImportPreview>('preview_oas_config', { content }),
  importConfig: (content: string, expectedBase: Config) => call<string>('import_oas_config', { content, expectedBase }),
  save: (config: Config) => call<void>('save_config', { config }),
  devices: () => call<Device[]>('list_devices'),
  connect: (endpoint: string) => call<string>('connect_device', { endpoint }),
  capture: (serial: string) => call<Screenshot>('capture_screen', { serial }),
  diagnose: (serial: string) => call<Diagnosis>('diagnose_screen', { serial }),
  observeSoul: (serial: string) => call<VisualPreview>('observe_soul', { serial }),
  clickElement: (serial: string, element: string) => call<VerifiedClick>('click_element', { serial, element }),
  tap: (serial: string, x: number, y: number, expectedSize: [number, number]) => call<Screenshot>('tap_device', { serial, x, y, expectedSize }),
  swipe: (serial: string, points: [number, number, number, number], durationMs: number, expectedSize: [number, number]) => call<Screenshot>('swipe_device', { serial, points, durationMs, expectedSize }),
  enqueue: (config: Config, priority: number, dueAtMs: number, repeat: RepeatSchedule | null = null) => call<void>('enqueue_task', { config, priority, dueAtMs, repeat }),
  setQueueEnabled: (enabled: boolean) => call<void>('set_queue_enabled', { enabled }),
  removeQueueEntry: (id: string) => call<void>('remove_queue_entry', { id }),
  start: () => call<string>('start_task'),
  stop: () => call<void>('stop_task'),
  subscribe: (receive: (snapshot: Snapshot) => void) => {
    if (!isTauri()) return Promise.reject(new Error('当前是浏览器预览。请使用 pnpm tauri dev 启动桌面应用。'));
    return listen<Snapshot>('engine:snapshot', event => receive(event.payload));
  },
};
