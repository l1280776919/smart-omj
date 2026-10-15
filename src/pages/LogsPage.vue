<script setup lang="ts">
import { computed, ref } from 'vue';
import { NInput, NSelect, NVirtualList, NEmpty } from 'naive-ui';
import { useAppStore } from '../stores/app';

const store = useAppStore();
const level = ref('all');
const search = ref('');

const logs = computed(() =>
  (store.snapshot?.logs ?? [])
    .filter(log => (level.value === 'all' || log.level === level.value) && log.message.toLowerCase().includes(search.value.toLowerCase()))
    .slice()
    .reverse()
);

const options = [
  { label: '全部等级', value: 'all' },
  { label: '调试 (DEBUG)', value: 'debug' },
  { label: '信息 (INFO)', value: 'info' },
  { label: '错误 (ERROR)', value: 'error' }
];
</script>

<template>
  <div class="page-heading">
    <div class="page-heading-left">
      <h1>运行日志监控</h1>
      <p>实时显示最近 400 条引擎事件记录。支持多级别过滤与全文检索。</p>
    </div>
    <div v-if="store.snapshot?.data_directory" class="path" style="font-size: 11px;">
      {{ store.snapshot.data_directory }}\logs\engine.jsonl
    </div>
  </div>

  <section class="panel">
    <div class="panel-heading">
      <div style="display: flex; gap: 12px; align-items: center; flex-wrap: wrap; width: 100%;">
        <NSelect
          v-model:value="level"
          :options="options"
          style="width: 150px"
          aria-label="日志等级"
        />
        <NInput
          v-model:value="search"
          placeholder="搜索日志事件或关键词..."
          aria-label="搜索日志"
          clearable
          style="max-width: 360px"
        />
        <span class="muted" style="margin-left: auto; font-size: 12px;">
          筛选结果：<strong style="color: var(--text-main)">{{ logs.length }}</strong> 条
        </span>
      </div>
    </div>

    <div style="border: 1px solid var(--border-color); border-radius: var(--radius-md); overflow: hidden; background: #fafbfc;">
      <NVirtualList
        v-if="logs.length"
        :items="logs"
        key-field="sequence"
        :item-size="52"
        style="height: 520px"
      >
        <template #default="{ item }">
          <div class="log-row" :class="item.level">
            <time>{{ new Date(item.timestamp_ms).toLocaleTimeString() }}</time>
            <span>{{ item.level }}</span>
            <div :title="item.message" style="overflow-x: auto; white-space: nowrap;">
              <span>{{ item.message }}</span>
              <small v-if="item.device_id" class="muted"> · {{ item.device_id }}</small>
            </div>
          </div>
        </template>
      </NVirtualList>

      <NEmpty
        v-else
        description="没有符合当前筛选条件的日志记录"
        style="padding: 60px 0;"
      />
    </div>
  </section>
</template>
