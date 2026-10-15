import { beforeEach, describe, expect, it, vi } from 'vitest';
import { createPinia, setActivePinia } from 'pinia';
import { useAppStore } from './app';
import type { Config, Snapshot } from '../types';
import { desktop } from '../services/desktop';
vi.mock('../services/desktop', () => ({ desktop: { subscribe: vi.fn(), snapshot: vi.fn(), save: vi.fn(), stop: vi.fn() } }));
const config: Config = { schema_version: 1, serial: 'emulator-5554', endpoint: '127.0.0.1:16384', soul: { rounds: 5, max_minutes: 30, stage_timeout_seconds: 180, layer: 11 } };
function snapshot(sequence: number, status = 'running'): Snapshot {
  return { sequence, config, run: { run_id: 'one', device_id: config.serial, status, stage: '等待战斗', completed_rounds: 2, started_at_ms: 1, ended_at_ms: null, message: '', failure_screenshot: null }, logs: [], queue: { enabled: false, entries: [] }, adb_source: 'adb', data_directory: 'data' };
}
beforeEach(() => { setActivePinia(createPinia()); vi.resetAllMocks(); });
describe('authoritative engine projection', () => {
  it('ignores stale snapshots so stopping cannot revert to running', () => {
    const store = useAppStore(); store.apply(snapshot(3, 'stopping')); store.apply(snapshot(2));
    expect(store.snapshot?.run?.status).toBe('stopping'); expect(store.running).toBe(true);
    store.apply(snapshot(4, 'stopped')); expect(store.running).toBe(false);
  });
  it('subscribes before bootstrap and preserves events that race the query', async () => {
    vi.mocked(desktop.subscribe).mockImplementation(async receive => { receive(snapshot(5, 'completed')); return vi.fn<() => void>(); });
    vi.mocked(desktop.snapshot).mockResolvedValue({ snapshot: snapshot(4), limits: { rounds: [1,1000], max_minutes: [1,1440], stage_timeout_seconds: [30,600], layers: [10,11,12,13] } });
    const store = useAppStore(); await store.initialize();
    expect(store.snapshot?.sequence).toBe(5); expect(store.running).toBe(false); expect(store.connected).toBe(true);
  });
  it('keeps stop usable during pending operations and displays errors', async () => {
    vi.mocked(desktop.stop).mockRejectedValue('offline');
    const store = useAppStore(); store.busy = true; await store.stop();
    expect(desktop.stop).toHaveBeenCalledOnce(); expect(store.error).toBe('offline');
  });
  it('keeps the display log buffer bounded', () => {
    const store = useAppStore(); const next = snapshot(501);
    next.logs = Array.from({ length: 501 }, (_, sequence) => ({ sequence, timestamp_ms: 0, level: 'info', device_id: '', run_id: null, message: 'event' }));
    store.apply(next); expect(store.snapshot?.logs).toHaveLength(400); expect(store.snapshot?.logs[0]?.sequence).toBe(101);
  });
});
