<script setup lang="ts">
import { computed, ref } from 'vue';
import { NAlert, NButton, NCheckbox, NTag } from 'naive-ui';
import { useAppStore } from '../stores/app';
import { desktop } from '../services/desktop';
import type { ImportPreview } from '../types';

const store = useAppStore();
const preview = ref<ImportPreview | null>(null);
const content = ref('');
const filename = ref('');
const acknowledged = ref(false);
const backup = ref('');

const disabled = computed(() => !store.connected || store.busy || store.running || !!store.snapshot?.queue.enabled);
const stale = computed(() => !!preview.value && JSON.stringify(preview.value.base) !== JSON.stringify(store.config));

async function selectFile(event: Event) {
  const input = event.target as HTMLInputElement;
  const file = input.files?.[0];
  input.value = '';
  preview.value = null;
  content.value = '';
  acknowledged.value = false;
  backup.value = '';
  filename.value = '';
  if (!file) return;

  await store.perform(async () => {
    if (file.size > 1024 * 1024) throw new Error('配置文件不能超过1 MiB');
    const source = new TextDecoder('utf-8', { fatal: true, ignoreBOM: true }).decode(await file.arrayBuffer());
    const report = await desktop.previewImport(source);
    content.value = source;
    filename.value = file.name;
    preview.value = report;
  });
}

async function applyImport() {
  if (!preview.value || !acknowledged.value || stale.value || preview.value.errors.length) return;
  await store.perform(async () => {
    backup.value = await desktop.importConfig(content.value, JSON.parse(JSON.stringify(preview.value!.base)));
    const latest = await desktop.snapshot();
    store.apply(latest.snapshot);
    preview.value = null;
    content.value = '';
    acknowledged.value = false;
  }, '配置已导入并备份，未启动任务');
}
</script>

<template>
  <div class="page-heading">
    <div class="page-heading-left">
      <h1>应用与数据设置</h1>
      <p>管理本地引擎运行环境、数据目录，并支持导入旧版 OAS 自动化配置文件。</p>
    </div>
  </div>

  <section class="panel">
    <div class="panel-heading">
      <div>
        <h2>本地引擎运行环境</h2>
        <p class="panel-desc">原生跨平台底层环境与本地存储路径</p>
      </div>
      <NTag type="success" round>Rust / Tokio 引擎就绪</NTag>
    </div>

    <div style="display: grid; grid-template-columns: repeat(auto-fit, minmax(240px, 1fr)); gap: 12px; margin-top: 10px;">
      <div class="device-row" style="margin: 0;">
        <span style="color: var(--text-muted); font-size: 13px;">配置架构版本</span>
        <strong>{{ store.config?.schema_version ?? '—' }}</strong>
      </div>
      <div class="device-row" style="margin: 0;">
        <span style="color: var(--text-muted); font-size: 13px;">ADB 程序来源</span>
        <span class="path">{{ store.snapshot?.adb_source || '未连接' }}</span>
      </div>
      <div class="device-row" style="margin: 0; grid-column: 1 / -1;">
        <span style="color: var(--text-muted); font-size: 13px;">本地数据目录</span>
        <span class="path">{{ store.snapshot?.data_directory || '未连接' }}</span>
      </div>
    </div>
  </section>

  <section class="panel">
    <div class="panel-heading">
      <div>
        <h2>导入旧版 OAS 配置</h2>
        <p class="panel-desc">自动解析旧配置 JSON 并将设备号、御魂层数和运行时长映射到新架构。导入前会自动备份。</p>
      </div>
    </div>

    <div class="import-card">
      <label class="import-file">
        <svg viewBox="0 0 24 24" width="22" height="22" fill="none" stroke="currentColor" stroke-width="2" style="color: var(--primary)"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><polyline points="17 8 12 3 7 8"/><line x1="12" y1="3" x2="12" y2="15"/></svg>
        <span>选择旧配置文件 (JSON)</span>
        <input type="file" accept=".json,application/json" :disabled="disabled" @change="selectFile" />
      </label>
      <span class="muted" style="font-size: 11.5px;">支持不超过 1 MiB 的旧版设置文件。已有运行队列不随导入更改。</span>
    </div>

    <template v-if="preview">
      <div style="margin-top: 18px; padding: 12px 14px; background: var(--bg-subtle); border-radius: var(--radius-md); border: 1px solid var(--border-subtle);">
        <span class="path" style="font-weight: 600;">{{ filename }}</span>
      </div>

      <div style="margin: 12px 0;">
        <NAlert v-for="warning in preview.warnings" :key="warning" type="warning" class="feedback">{{ warning }}</NAlert>
        <NAlert v-for="error in preview.errors" :key="error" type="error" class="feedback">{{ error }}</NAlert>
        <NAlert v-if="stale" type="warning" class="feedback">当前配置已变化，请重新选择文件生成预览。</NAlert>
      </div>

      <div class="import-mappings">
        <article v-for="mapping in preview.mappings" :key="mapping.target">
          <strong>{{ mapping.target }}</strong>
          <p>{{ JSON.stringify(mapping.before) }} → {{ JSON.stringify(mapping.after) }}</p>
          <small class="path">来源：{{ mapping.source }}</small>
        </article>
      </div>

      <details v-if="preview.ignored.length" style="margin: 14px 0; font-size: 12.5px;">
        <summary style="cursor: pointer; color: var(--primary); font-weight: 500;">
          未迁移字段（{{ preview.ignored.length }} 项）
        </summary>
        <ul class="ignored-fields" style="background: var(--bg-subtle); padding: 12px 24px; border-radius: 8px; margin-top: 8px;">
          <li v-for="path in preview.ignored" :key="path">{{ path }}</li>
        </ul>
      </details>

      <div style="margin: 18px 0;">
        <NCheckbox v-model:checked="acknowledged" :disabled="disabled || stale || !!preview.errors.length">
          已核对映射和未迁移项，确认使用单人流程并备份导入
        </NCheckbox>
      </div>

      <div>
        <NButton
          type="primary"
          :disabled="disabled || stale || !acknowledged || !!preview.errors.length"
          @click="applyImport"
        >
          备份并导入
        </NButton>
      </div>
    </template>

    <NAlert v-if="backup" type="success" style="margin-top: 16px;">
      <p style="margin: 0 0 4px; font-weight: 600;">备份已成功保存：</p>
      <p class="path" style="margin: 0;">{{ backup }}</p>
    </NAlert>
  </section>
</template>

<style scoped>
.import-card {
  padding: 20px;
  background: var(--bg-subtle);
  border: 1px dashed var(--border-color);
  border-radius: var(--radius-md);
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 8px;
  text-align: center;
}

.import-file {
  display: inline-flex;
  align-items: center;
  gap: 10px;
  cursor: pointer;
  padding: 8px 16px;
  background: white;
  border: 1px solid var(--border-color);
  border-radius: var(--radius-sm);
  font-weight: 500;
  font-size: 13px;
  transition: all 0.15s ease;
}

.import-file:hover {
  border-color: var(--primary);
  color: var(--primary);
}

.import-mappings {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(min(100%, 240px), 1fr));
  gap: 12px;
  margin: 18px 0;
}

.import-mappings article {
  border: 1px solid var(--border-color);
  background: white;
  border-radius: var(--radius-md);
  padding: 14px;
  overflow-wrap: anywhere;
}

.import-mappings article strong {
  color: var(--primary);
  font-size: 13px;
}

.ignored-fields {
  max-height: 200px;
  overflow: auto;
  overflow-wrap: anywhere;
  font-size: 12px;
  line-height: 1.8;
  color: var(--text-muted);
}
</style>
