<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { NButton, NForm, NFormItem, NInputNumber, NSelect, NCheckbox, NDatePicker, NTag } from 'naive-ui';
import { useAppStore } from '../stores/app';
import { desktop } from '../services/desktop';
import VisionObserver from '../components/VisionObserver.vue';
import type { Config } from '../types';

const store = useAppStore();
const draft = ref<Config | null>(null);
const acknowledged = ref(false);
const priority = ref(50);
const dueAt = ref<number | null>(null);
const repeating = ref(false);
const intervalMinutes = ref<number | null>(60);
const totalRuns = ref<number | null>(2);

const invalidRepeat = computed(() => repeating.value && (intervalMinutes.value == null || totalRuns.value == null));
const queue = computed(() => store.snapshot?.queue);
const statusLabels: Record<string, string> = {
  pending: '等待',
  running: '运行中',
  completed: '完成',
  failed: '失败',
  stopped: '已停止',
  interrupted: '已中断'
};

const statusTagType: Record<string, 'default' | 'info' | 'success' | 'warning' | 'error'> = {
  pending: 'info',
  running: 'warning',
  completed: 'success',
  failed: 'error',
  stopped: 'default',
  interrupted: 'error'
};

async function enqueue() {
  if (draft.value) {
    await store.perform(
      () => desktop.enqueue(
        clone(draft.value!),
        priority.value,
        dueAt.value ?? Date.now(),
        repeating.value ? { interval_minutes: intervalMinutes.value!, total_runs: totalRuns.value! } : null
      ),
      '已加入队列'
    );
  }
}

async function toggleQueue() {
  await store.perform(() => desktop.setQueueEnabled(!queue.value?.enabled));
}

async function removeEntry(id: string) {
  await store.perform(() => desktop.removeQueueEntry(id));
}

const clone = (value: Config): Config => JSON.parse(JSON.stringify(value));

watch(() => JSON.stringify(store.config), () => {
  if (store.config) draft.value = clone(store.config);
}, { immediate: true });

const disabled = computed(() => !store.connected || store.running || store.busy);
const dirty = computed(() => JSON.stringify(draft.value) !== JSON.stringify(store.config));
const layers = computed(() => store.limits?.layers.map(value => ({
  label: value === 11 ? `魂十一 · 悲鸣（需已选层数并锁定阵容）` : value === 1 ? `第 1 层 · 视觉闭环已就绪` : `第 ${value} 层 · 尚未训练`,
  value,
  disabled: ![1,11].includes(value)
})) ?? []);

async function save() {
  if (draft.value) await store.save(clone(draft.value));
}

async function start() {
  if (dirty.value && draft.value) {
    if (!await store.save(clone(draft.value))) return;
  }
  await store.perform(() => desktop.start(), '御魂任务已启动');
}
</script>

<template>
  <div class="page-heading">
    <div class="page-heading-left">
      <h1>任务配置与调度</h1>
      <p>配置御魂单人循环参数，设定轮数与时长上限，支持预约排队与周期执行。</p>
    </div>
    <div class="actions">
      <NButton v-if="store.running" type="error" @click="store.stop">
        停止正在运行的任务
      </NButton>
    </div>
  </div>

  <div style="margin-bottom: 20px; padding: 12px 16px; background: #eff6ff; border-radius: var(--radius-md); border: 1px solid #bfdbfe; display: flex; align-items: center; gap: 12px;">
    <svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="#2563eb" stroke-width="2" style="flex-shrink: 0"><circle cx="12" cy="12" r="10"/><line x1="12" y1="16" x2="12" y2="12"/><line x1="12" y1="8" x2="12.01" y2="8"/></svg>
    <span style="font-size: 13px; color: #1e40af; line-height: 1.5;">
      支持<strong>壹层和魂十一·悲鸣</strong>单人循环；悲鸣请先停在已选中悲鸣的挑战页。请预先配置好式神阵容并在游戏内开启「锁定阵容」与「自动战斗」。
    </span>
  </div>

  <!-- Primary Task Configuration Panel -->
  <section class="panel">
    <div class="panel-heading">
      <div>
        <h2>御魂单人循环配置</h2>
        <p class="panel-desc">
          运行设备：<strong style="color: var(--primary)">{{ store.config?.serial || '请先在设备页选择设备' }}</strong>
          <span class="muted">（达到轮数或最大分钟后引擎会自动停止）</span>
        </p>
      </div>
      <NTag :type="store.config?.serial ? 'info' : 'warning'" size="small">
        {{ store.config?.serial ? '设备就绪' : '未选择设备' }}
      </NTag>
    </div>

    <NForm v-if="draft && store.limits" label-placement="top">
      <div class="form-grid">
        <NFormItem label="目标御魂层数">
          <NSelect v-model:value="draft.soul.layer" :options="layers" :disabled="disabled"/>
        </NFormItem>
        <NFormItem label="最多完成轮数">
          <NInputNumber
            v-model:value="draft.soul.rounds"
            :min="store.limits.rounds[0]"
            :max="store.limits.rounds[1]"
            :precision="0"
            :disabled="disabled"
          >
            <template #suffix>轮</template>
          </NInputNumber>
        </NFormItem>
        <NFormItem label="最多运行时间">
          <NInputNumber
            v-model:value="draft.soul.max_minutes"
            :min="store.limits.max_minutes[0]"
            :max="store.limits.max_minutes[1]"
            :precision="0"
            :disabled="disabled"
          >
            <template #suffix>分钟</template>
          </NInputNumber>
        </NFormItem>
        <NFormItem label="每阶段超时时间">
          <NInputNumber
            v-model:value="draft.soul.stage_timeout_seconds"
            :min="store.limits.stage_timeout_seconds[0]"
            :max="store.limits.stage_timeout_seconds[1]"
            :precision="0"
            :disabled="disabled"
          >
            <template #suffix>秒</template>
          </NInputNumber>
        </NFormItem>
      </div>
    </NForm>

    <!-- Confirmation & Action Footer Bar -->
    <div style="margin-top: 18px; padding: 16px 20px; background: var(--bg-subtle); border-radius: var(--radius-md); border: 1px solid var(--border-subtle); display: flex; align-items: center; justify-content: space-between; flex-wrap: wrap; gap: 16px;">
      <NCheckbox v-model:checked="acknowledged" :disabled="disabled">
        <span style="font-size: 13.5px; font-weight: 500;">已配置并锁定阵容、开启自动战斗，确认所选目标层数</span>
      </NCheckbox>

      <div class="actions">
        <NButton :disabled="disabled || !dirty || !draft" @click="save">
          保存配置
        </NButton>
        <NButton
          type="primary"
          :disabled="disabled || !acknowledged || !store.config?.serial"
          @click="start"
        >
          保存并开始
        </NButton>
        <NButton v-if="store.running" type="error" @click="store.stop">
          停止任务
        </NButton>
      </div>
    </div>
  </section>

  <!-- Vision Observer -->
  <VisionObserver />

  <!-- Task Queue Section -->
  <section class="panel" style="margin-top: 24px;">
    <div class="panel-heading">
      <div>
        <h2>
          <span>任务队列调度</span>
        </h2>
        <p class="panel-desc">
          队列状态：<strong>{{ queue?.enabled ? '已启用' : '已暂停' }}</strong> · 支持指定优先级与预约时间
        </p>
      </div>
      <div class="actions">
        <NButton
          :type="queue?.enabled ? 'warning' : 'primary'"
          :disabled="!store.connected || store.busy || (!queue?.enabled && !acknowledged)"
          @click="toggleQueue"
        >
          {{ queue?.enabled ? '暂停队列' : '启用队列' }}
        </NButton>
        <NButton v-if="store.running" type="error" @click="store.stop">
          停止并暂停队列
        </NButton>
      </div>
    </div>

    <!-- Queue Form -->
    <div style="padding: 16px; background: var(--bg-subtle); border-radius: var(--radius-md); border: 1px solid var(--border-subtle); margin-bottom: 20px;">
      <h3 style="font-size: 14px; font-weight: 600; margin: 0 0 12px; color: var(--text-main);">添加当前配置到队列</h3>
      <div class="form-grid">
        <NFormItem label="优先级（0–100，数值越大越先执行）">
          <NInputNumber v-model:value="priority" :min="0" :max="100" :precision="0" :disabled="store.busy"/>
        </NFormItem>
        <NFormItem label="预约时间（留空表示立即到期执行）">
          <NDatePicker v-model:value="dueAt" type="datetime" clearable :disabled="store.busy"/>
        </NFormItem>
      </div>

      <div style="margin: 10px 0;">
        <NCheckbox v-model:checked="repeating" :disabled="store.busy">
          <span style="font-weight: 500;">周期重复执行</span>
        </NCheckbox>
      </div>

      <div v-if="repeating" class="form-grid" style="margin-top: 12px;">
        <NFormItem label="每次成功结束后等待（分钟）">
          <NInputNumber v-model:value="intervalMinutes" :min="1" :max="10080" :precision="0" :disabled="store.busy"/>
        </NFormItem>
        <NFormItem label="总执行次数（含首次）">
          <NInputNumber v-model:value="totalRuns" :min="2" :max="100" :precision="0" :disabled="store.busy"/>
        </NFormItem>
      </div>

      <div style="display: flex; justify-content: flex-end; margin-top: 12px;">
        <NButton
          type="primary"
          secondary
          :disabled="!store.connected || store.busy || !acknowledged || !draft?.serial || priority == null || invalidRepeat"
          @click="enqueue"
        >
          当前配置加入队列
        </NButton>
      </div>
    </div>

    <!-- Queue List -->
    <h3 style="font-size: 14px; font-weight: 600; margin: 18px 0 12px; color: var(--text-main);">当前队列任务 ({{ queue?.entries.length ?? 0 }})</h3>
    <div v-if="!queue?.entries.length" class="muted" style="padding: 16px 0; text-align: center;">
      尚无队列任务。在上方配置参数后点击「当前配置加入队列」。
    </div>

    <div v-else style="display: flex; flex-direction: column; gap: 10px;">
      <article
        v-for="entry in queue?.entries"
        :key="entry.id"
        style="padding: 16px; background: white; border: 1px solid var(--border-color); border-radius: var(--radius-md); box-shadow: var(--shadow-xs); display: flex; justify-content: space-between; align-items: flex-start; gap: 16px; overflow-wrap: anywhere;"
      >
        <div style="flex: 1;">
          <div style="display: flex; align-items: center; gap: 10px; margin-bottom: 6px;">
            <NTag :type="statusTagType[entry.status] || 'default'" size="small" round>
              {{ statusLabels[entry.status] ?? entry.status }}
            </NTag>
            <strong style="font-size: 14px; color: var(--text-main);">
              御魂 {{ entry.config.soul.layer }} 层 · {{ entry.config.soul.rounds }} 轮
            </strong>
            <span class="muted" style="font-size: 12px;">
              设备: {{ entry.config.serial }} · 优先级 {{ entry.priority }}
            </span>
          </div>

          <p class="muted" style="margin: 4px 0; font-size: 12px;">
            预约时间：{{ new Date(entry.due_at_ms).toLocaleString() }}
            <template v-if="entry.repeat">
              · 成功次数 {{ entry.completed_runs }} / {{ entry.repeat.total_runs }} (间隔 {{ entry.repeat.interval_minutes }} 分钟)
            </template>
          </p>

          <p v-if="entry.last_run" class="muted" style="margin: 4px 0; font-size: 12px;">
            上次结果：{{ statusLabels[entry.last_run.status] ?? entry.last_run.status }} · 已完成 {{ entry.last_run.completed_rounds }} 轮 · {{ entry.last_run.ended_at_ms ? new Date(entry.last_run.ended_at_ms).toLocaleString() : '未正常结束' }}
          </p>
          <p v-if="entry.message" style="color: var(--warning); margin: 4px 0; font-size: 12px;">
            {{ entry.message }}
          </p>
        </div>

        <NButton size="small" type="error" tertiary :disabled="store.busy || entry.status === 'running'" @click="removeEntry(entry.id)">
          移除
        </NButton>
      </article>
    </div>
  </section>
</template>
