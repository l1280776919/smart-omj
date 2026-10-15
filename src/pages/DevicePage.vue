<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { NButton, NInput, NInputNumber, NTag, NEmpty, NSwitch } from 'naive-ui';
import { useAppStore } from '../stores/app';
import { desktop } from '../services/desktop';
import type { Device } from '../types';

const store = useAppStore();
const endpoint = ref('');
const manual = ref(false);
const x = ref(640);
const y = ref(360);

const disabled = computed(() => !store.connected || store.running || store.busy);
const strategyLabels: Record<string, string> = { stretch: '比例映射', anchored: '边缘锚定', letterbox: '黑边裁剪' };

watch(() => [store.screen?.width, store.screen?.height].join('x'), () => {
  if (store.screen) {
    x.value = Math.floor(store.screen.width / 2);
    y.value = Math.floor(store.screen.height / 2);
  }
});

watch(() => store.config?.endpoint, value => {
  if (value) endpoint.value = value;
}, { immediate: true });

async function connect() {
  await store.perform(async () => {
    await desktop.connect(endpoint.value);
    store.devices = await desktop.devices();
  }, '模拟器已连接，请选择设备');
}

async function select(device: Device) {
  if (store.config) {
    store.screen = null;
    store.diagnosis = null;
    await store.save({ ...store.config, endpoint: endpoint.value, serial: device.serial });
  }
}

async function diagnose() {
  if (!store.config?.serial) {
    store.error = '请先选择设备';
    return;
  }
  await store.perform(async () => {
    store.diagnosis = await desktop.diagnose(store.config!.serial);
  });
}

async function tap() {
  if (!store.config?.serial || !store.screen) return;
  const dimensions: [number, number] = [store.screen.width, store.screen.height];
  await store.perform(async () => {
    store.screen = await desktop.tap(store.config!.serial, x.value, y.value, dimensions);
    store.diagnosis = null;
  });
}

async function clickElement(element: string) {
  if (store.config?.serial) await store.perform(async () => {
    const result = await desktop.clickElement(store.config!.serial, element);
    store.screen = result.screenshot;
    store.diagnosis = result.diagnosis;
    store.notice = result.verification === 'unconfirmed'
      ? '点击后画面未定位，请核对新截图；未自动重试'
      : result.target_disappeared
        ? '点击后目标已消失，请核对截图中的界面'
        : '点击后目标仍存在，请核对截图；未自动重试';
  });
}
</script>

<template>
  <div class="page-heading">
    <div class="page-heading-left">
      <h1>设备管理与画面诊断</h1>
      <p>管理本机模拟器 ADB 连接，实时抓取屏幕画面并执行模板视觉定位。</p>
    </div>
    <div class="actions">
      <NButton :disabled="disabled" @click="store.refreshDevices">
        <template #icon>
          <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2"><path d="M21.5 2v6h-6M21.34 15.57a10 10 0 1 1-.57-8.38l5.67-5.67"/></svg>
        </template>
        刷新设备列表
      </NButton>
    </div>
  </div>

  <section class="panel">
    <div class="panel-heading">
      <div>
        <h2>模拟器连接</h2>
        <p class="panel-desc">支持本机 MuMu、雷电等模拟器。默认端口 MuMu12: 16384，MuMu6: 7555。</p>
      </div>
    </div>

    <div style="display: flex; gap: 12px; align-items: center; flex-wrap: wrap; margin-bottom: 16px;">
      <NInput
        v-model:value="endpoint"
        aria-label="ADB 地址"
        placeholder="127.0.0.1:16384"
        style="max-width: 320px;"
        :disabled="disabled"
      />
      <NButton type="primary" :disabled="disabled || !endpoint" @click="connect">
        连接模拟器
      </NButton>
      <div style="display: flex; gap: 6px; align-items: center;">
        <span class="muted" style="font-size: 11.5px;">常用端口:</span>
        <NButton size="tiny" tertiary @click="endpoint = '127.0.0.1:16384'">MuMu 12</NButton>
        <NButton size="tiny" tertiary @click="endpoint = '127.0.0.1:7555'">MuMu 6</NButton>
        <NButton size="tiny" tertiary @click="endpoint = '127.0.0.1:5555'">雷电/标准</NButton>
      </div>
    </div>

    <div v-if="store.devices.length" style="display: flex; flex-direction: column; gap: 8px;">
      <div
        v-for="device in store.devices"
        :key="device.serial"
        class="device-row"
        :style="device.serial === store.config?.serial ? 'border-color: var(--primary); background: var(--primary-subtle);' : ''"
      >
        <div style="display: flex; align-items: center; gap: 12px;">
          <div style="width: 36px; height: 36px; border-radius: 8px; background: white; border: 1px solid var(--border-color); display: grid; place-items: center; color: var(--primary); flex-shrink: 0;">
            <svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="2"><rect x="5" y="2" width="14" height="20" rx="2"/><line x1="12" y1="18" x2="12.01" y2="18"/></svg>
          </div>
          <div>
            <strong>{{ device.serial }}</strong>
            <small>{{ device.description || 'ADB 通讯设备' }}</small>
          </div>
        </div>
        <div class="actions">
          <NTag :type="device.status === 'device' ? 'success' : 'warning'" size="small" round>
            {{ device.status === 'device' ? '在线' : device.status }}
          </NTag>
          <NButton
            size="small"
            :type="device.serial === store.config?.serial ? 'primary' : 'default'"
            :disabled="disabled || device.status !== 'device'"
            @click="select(device)"
          >
            {{ device.serial === store.config?.serial ? '当前选用' : '选择设备' }}
          </NButton>
        </div>
      </div>
    </div>

    <NEmpty
      v-else
      description="未检测到已连接的 ADB 设备。请检查模拟器是否已启动并在设置中开启 ADB。"
      style="padding: 24px 0;"
    />
  </section>

  <section class="panel">
    <div class="panel-heading">
      <div>
        <h2>屏幕监视与识别诊断</h2>
        <p class="panel-desc">
          当前设备：<strong style="color: var(--primary)">{{ store.config?.serial || '未选择' }}</strong>
          <template v-if="store.screen"> · {{ store.screen.width }} × {{ store.screen.height }} px</template>
        </p>
      </div>
      <div class="actions">
        <NButton :disabled="disabled || !store.config?.serial" @click="store.capture">
          获取最新截图
        </NButton>
        <NButton type="primary" secondary :disabled="disabled || !store.config?.serial" @click="diagnose">
          运行模板诊断
        </NButton>
      </div>
    </div>

    <div class="screenshot-frame">
      <img v-if="store.screen" :src="store.screen.data_url" alt="模拟器当前屏幕截图" />
      <p v-else>
        暂无屏幕截图。点击「获取最新截图」查看模拟器实时画面。<br/>
        <span style="font-size: 11.5px; opacity: 0.7;">任务运行时设备将由引擎独占控制。</span>
      </p>
    </div>

    <div style="margin-top: 18px; padding: 14px 16px; background: var(--bg-subtle); border-radius: var(--radius-md); border: 1px solid var(--border-subtle);">
      <div style="display: flex; align-items: center; justify-content: space-between; flex-wrap: wrap; gap: 12px;">
        <div style="display: flex; align-items: center; gap: 10px;">
          <NSwitch v-model:value="manual" :disabled="disabled" />
          <span style="font-size: 13px; font-weight: 500;">手动像素点击调试</span>
          <span class="muted" style="font-size: 11.5px;">（基于模拟器原生坐标系）</span>
        </div>
        <div v-if="manual" class="actions">
          <span style="font-size: 12px; color: var(--text-muted)">X:</span>
          <NInputNumber
            v-model:value="x"
            :min="0"
            :max="(store.screen?.width ?? 1) - 1"
            :precision="0"
            aria-label="横坐标"
            style="width: 110px"
          />
          <span style="font-size: 12px; color: var(--text-muted)">Y:</span>
          <NInputNumber
            v-model:value="y"
            :min="0"
            :max="(store.screen?.height ?? 1) - 1"
            :precision="0"
            aria-label="纵坐标"
            style="width: 110px"
          />
          <NButton
            type="primary"
            size="small"
            :disabled="disabled || !store.screen || !store.config?.serial"
            @click="tap"
          >
            点击并重新截图
          </NButton>
        </div>
      </div>
    </div>
  </section>

  <section v-if="store.diagnosis" class="panel">
    <div class="panel-heading">
      <div>
        <h2>诊断结果</h2>
        <p class="panel-desc">匹配器：{{ store.diagnosis.matcher }} · 耗时 {{ store.diagnosis.elapsed_ms }} ms</p>
      </div>
      <NTag :type="store.diagnosis.alignment_confident ? 'success' : 'error'" round>
        {{ store.diagnosis.alignment_confident ? '画面已定位' : '未正确定位（禁止点击）' }}
      </NTag>
    </div>

    <div style="padding: 10px 14px; background: var(--bg-subtle); border-radius: 6px; font-size: 12.5px; margin-bottom: 16px; border: 1px solid var(--border-subtle);">
      视口适配：{{ store.diagnosis.viewport.source_size.join(' × ') }} → {{ store.diagnosis.viewport.working_size.join(' × ') }} · {{ strategyLabels[store.diagnosis.viewport.strategy] || store.diagnosis.viewport.strategy }} · 旋转 {{ store.diagnosis.viewport.rotation }}°
    </div>

    <div style="display: flex; flex-direction: column; gap: 8px;">
      <div v-for="match in store.diagnosis.matches" :key="match.id" class="device-row">
        <div>
          <strong style="font-size: 13.5px;">{{ match.id }}</strong>
          <small>坐标: [{{ match.bounds.join(', ') }}]</small>
        </div>
        <div class="actions">
          <NTag :type="match.found ? 'success' : 'default'" size="small">
            {{ match.found ? '已匹配' : '未匹配' }} ({{ match.score.toFixed(3) }})
          </NTag>
          <NButton
            v-if="manual && ['challenge','ready','locked'].includes(match.id)"
            size="small"
            :disabled="disabled || !match.found"
            @click="clickElement(match.id)"
          >
            识别后点击并验证
          </NButton>
        </div>
      </div>
    </div>

    <p v-if="store.diagnosis.ocr" style="margin-top: 16px; font-size: 12px; color: var(--text-muted); background: var(--bg-subtle); padding: 8px 12px; border-radius: 6px;">
      OCR 文本识别：{{ store.diagnosis.ocr }}
    </p>
  </section>
</template>
