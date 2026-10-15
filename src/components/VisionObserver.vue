<script setup lang="ts">
import { computed, onUnmounted, ref, watch } from 'vue';
import { NButton, NAlert, NTag } from 'naive-ui';
import { useAppStore } from '../stores/app';
import { desktop } from '../services/desktop';
import type { VisualPreview } from '../types';

const store = useAppStore();
const preview = ref<VisualPreview | null>(null);
const observing = ref(false);
const loading = ref(false);
const error = ref('');

let timer: ReturnType<typeof setTimeout> | undefined;
let disposed = false;

const suggestion = computed(() => {
  const value = preview.value?.suggested;
  return value && typeof value === 'object' ? `点击 ${value.Tap.target} → 等待 ${value.Tap.expected.join(' / ')}` : '等待新画面';
});

function stop() {
  observing.value = false;
  if (timer) clearTimeout(timer);
}

async function refresh() {
  if (!store.config?.serial || store.running || loading.value || disposed) return;
  loading.value = true;
  error.value = '';
  try {
    preview.value = await desktop.observeSoul(store.config.serial);
  } catch (e) {
    error.value = String(e);
    stop();
  } finally {
    loading.value = false;
  }
  if (observing.value && !disposed) {
    timer = setTimeout(refresh, 600);
  }
}

function toggle() {
  if (observing.value) stop();
  else {
    observing.value = true;
    void refresh();
  }
}

watch(() => store.running, value => {
  if (value) stop();
});

onUnmounted(() => {
  disposed = true;
  stop();
});
</script>

<template>
  <section class="panel" style="margin-top: 24px;">
    <div class="panel-heading">
      <div>
        <h2>
          <span>视觉闭环观察</span>
        </h2>
        <p class="panel-desc">实时截取模拟器画面，使用 YOLO 目标检测模型识别按钮并给出建议动作（不执行实际点击）。</p>
      </div>
      <div class="actions">
        <NButton
          :disabled="!store.connected || !store.config?.serial || store.running || loading || observing"
          @click="refresh"
        >
          单次观察
        </NButton>
        <NButton
          :type="observing ? 'warning' : 'primary'"
          :disabled="!store.connected || !store.config?.serial || store.running"
          @click="toggle"
        >
          {{ observing ? '停止连续观察' : '启动连续观察' }}
        </NButton>
      </div>
    </div>

    <NAlert v-if="error" type="error" style="margin-top: 12px; margin-bottom: 12px;">{{ error }}</NAlert>

    <template v-if="preview">
      <div style="display: flex; align-items: center; justify-content: space-between; flex-wrap: wrap; gap: 10px; margin-bottom: 12px; padding: 10px 14px; background: var(--bg-subtle); border-radius: var(--radius-md); border: 1px solid var(--border-subtle);">
        <div style="display: flex; align-items: center; gap: 8px;">
          <NTag type="info" size="small" round>{{ preview.page_label }}</NTag>
          <span style="font-weight: 600; font-size: 13.5px; color: var(--text-main);">{{ suggestion }}</span>
        </div>
        <div style="font-size: 11.5px; color: var(--text-muted);">
          {{ preview.reason }}
          <template v-if="preview.observation.compute">
            · 推理设备: <strong>{{ preview.observation.compute.provider }}</strong>
          </template>
        </div>
      </div>

      <div style="background: #0b1120; border-radius: var(--radius-md); overflow: hidden; border: 1px solid #1e293b; box-shadow: inset 0 2px 4px rgba(0,0,0,0.4);">
        <svg
          style="width: 100%; display: block;"
          :viewBox="`0 0 ${preview.screenshot.width} ${preview.screenshot.height}`"
          role="img"
          aria-label="游戏截图与检测框"
        >
          <image
            :href="preview.screenshot.data_url"
            :width="preview.screenshot.width"
            :height="preview.screenshot.height"
          />
          <g v-for="(d, i) in preview.observation.detections" :key="i">
            <rect
              :x="d.bounds[0]"
              :y="d.bounds[1]"
              :width="d.bounds[2] - d.bounds[0]"
              :height="d.bounds[3] - d.bounds[1]"
              fill="rgba(72, 239, 176, 0.12)"
              stroke="#10b981"
              stroke-width="3"
            />
            <rect
              :x="d.bounds[0]"
              :y="Math.max(0, d.bounds[1] - 28)"
              :width="d.class.length * 14 + 65"
              height="26"
              fill="#064e3b"
              rx="4"
            />
            <text
              :x="d.bounds[0] + 6"
              :y="Math.max(18, d.bounds[1] - 10)"
              fill="#ecfdf5"
              font-size="14"
              font-weight="600"
              font-family="sans-serif"
            >
              {{ d.class }} {{ Math.round(d.confidence * 100) }}%
            </text>
          </g>
        </svg>
      </div>

      <div style="display: flex; gap: 16px; margin-top: 10px; font-size: 11.5px; color: var(--text-muted); flex-wrap: wrap;">
        <span>截图: {{ preview.observation.timings.screenshot_ms.toFixed(0) }} ms</span>
        <span>识别合计: {{ (preview.observation.timings.recognition_ms ?? 0).toFixed(0) }} ms</span>
        <span>场景: {{ (preview.observation.timings.scene_ms ?? 0).toFixed(0) }} ms</span>
        <span>模板: {{ (preview.observation.timings.template_ms ?? 0).toFixed(0) }} ms</span>
        <span>预处理: {{ preview.observation.timings.preprocess_ms.toFixed(0) }} ms</span>
        <span>模型推理: {{ preview.observation.timings.inference_ms.toFixed(0) }} ms</span>
        <span><strong>总耗时: {{ preview.observation.timings.total_ms.toFixed(0) }} ms</strong></span>
      </div>
    </template>
  </section>
</template>
