import { computed, ref } from 'vue';
import { defineStore } from 'pinia';
import { desktop } from '../services/desktop';
import type { Config, Device, Diagnosis, Limits, Screenshot, Snapshot } from '../types';

export const useAppStore = defineStore('application', () => {
  const snapshot = ref<Snapshot | null>(null);
  const limits = ref<Limits | null>(null);
  const devices = ref<Device[]>([]);
  const screen = ref<Screenshot | null>(null);
  const diagnosis = ref<Diagnosis | null>(null);
  const error = ref(''); const notice = ref(''); const busy = ref(false); const connected = ref(false);
  const running = computed(() => ['running', 'stopping'].includes(snapshot.value?.run?.status ?? ''));
  const config = computed(() => snapshot.value?.config ?? null);
  function apply(next: Snapshot) {
    if (snapshot.value && next.sequence <= snapshot.value.sequence) return;
    snapshot.value = { ...next, logs: next.logs.slice(-400) };
  }
  async function perform<T>(job: () => Promise<T>, message = ''): Promise<T | undefined> {
    if (busy.value) return;
    busy.value = true; error.value = ''; notice.value = '';
    try { const value = await job(); notice.value = message; return value; }
    catch (cause) { error.value = cause instanceof Error ? cause.message : String(cause); }
    finally { busy.value = false; }
  }
  let unlisten: (() => void) | undefined;
  async function initialize() {
    await perform(async () => {
      unlisten?.(); unlisten = undefined; snapshot.value = null;
      // Subscribe first. Sequence ordering prevents the query response overwriting a newer event.
      unlisten = await desktop.subscribe(apply);
      try {
        const initial = await desktop.snapshot(); limits.value = initial.limits; apply(initial.snapshot); connected.value = true;
      } catch (cause) { unlisten(); unlisten = undefined; connected.value = false; throw cause; }
    });
  }
  function dispose() { unlisten?.(); unlisten = undefined; }
  async function refreshDevices() { await perform(async () => { devices.value = await desktop.devices(); }); }
  async function save(value: Config) {
    return perform(async () => { await desktop.save(value); const latest = await desktop.snapshot(); apply(latest.snapshot); return true; }, '配置已保存');
  }
  async function capture() {
    const serial = config.value?.serial; if (!serial) { error.value = '请先选择并保存设备'; return; }
    await perform(async () => { screen.value = await desktop.capture(serial); diagnosis.value = null; });
  }
  async function stop() {
    // Stop must remain available even while another request is pending.
    try { await desktop.stop(); } catch (cause) { error.value = String(cause); }
  }
  return { snapshot, limits, devices, screen, diagnosis, error, notice, busy, connected, running, config,
    apply, perform, initialize, dispose, refreshDevices, save, capture, stop };
});
