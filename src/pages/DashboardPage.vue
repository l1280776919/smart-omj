<script setup lang="ts">
import { computed } from 'vue';
import { NButton, NTag, NEmpty } from 'naive-ui';
import { useAppStore } from '../stores/app';

const store = useAppStore();
const run = computed(() => store.snapshot?.run);

const labels: Record<string, string> = {
  running: '正在运行',
  stopping: '正在停止',
  completed: '已完成',
  stopped: '已停止',
  failed: '运行失败',
  interrupted: '上次运行中断'
};

const statusType = computed(() => {
  if (store.running) return 'warning';
  if (run.value?.status === 'completed') return 'success';
  if (run.value?.status === 'failed') return 'error';
  return 'default';
});
</script>

<template>
  <div class="page-heading">
    <div class="page-heading-left">
      <h1>运行概览</h1>
      <p>监控阴阳师御魂自动化任务进度，查看实时运行状态与诊断结果。</p>
    </div>
    <div class="actions">
      <RouterLink v-if="!store.running" to="/tasks">
        <NButton type="primary">配置并启动任务</NButton>
      </RouterLink>
      <NButton v-else type="error" @click="store.stop">
        停止当前任务
      </NButton>
    </div>
  </div>

  <div class="stats">
    <div class="stat">
      <small>
        <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2"><rect x="5" y="2" width="14" height="20" rx="2"/><line x1="12" y1="18" x2="12.01" y2="18"/></svg>
        当前连接设备
      </small>
      <strong>{{ store.config?.serial || '尚未选择' }}</strong>
      <RouterLink to="/devices" class="muted" style="margin-top: 6px; display: inline-flex; align-items: center; gap: 4px;">
        {{ store.config?.serial ? '切换设备 →' : '前往连接设备 →' }}
      </RouterLink>
    </div>

    <div class="stat">
      <small>
        <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="10"/><polyline points="12 6 12 12 16 14"/></svg>
        已完成轮数
      </small>
      <strong>
        {{ run?.completed_rounds ?? 0 }}
        <em>/ {{ store.config?.soul.rounds ?? '—' }} 轮</em>
      </strong>
      <span class="muted" style="margin-top: 6px; font-size: 11.5px;">
        限制时长: {{ store.config?.soul.max_minutes ? `${store.config.soul.max_minutes} 分钟` : '未设定' }}
      </span>
    </div>

    <div class="stat">
      <small>
        <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2"><activity x1="2" y1="12" x2="6" y2="12"/><polyline points="22 12 18 12 15 21 9 3 6 12 2 12"/></svg>
        引擎运行状态
      </small>
      <strong>{{ labels[run?.status ?? ''] || '等待任务' }}</strong>
      <div style="margin-top: 6px;">
        <NTag :type="statusType" size="small" round>
          {{ store.running ? '任务活跃中' : (run?.status ? labels[run.status] : '空闲中') }}
        </NTag>
      </div>
    </div>
  </div>

  <div class="two-columns">
    <!-- Left: Running Monitor / Mission Control -->
    <section class="panel">
      <div class="panel-heading">
        <h2>
          <svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="2" style="color: var(--primary)"><polygon points="5 3 19 12 5 21 5 3"/></svg>
          任务控制中心
        </h2>
        <NTag v-if="run" size="small" type="info">御魂 · 单人循环</NTag>
      </div>

      <template v-if="run">
        <div style="display: flex; align-items: center; justify-content: space-between; margin-bottom: 12px;">
          <span style="font-size: 13px; font-weight: 500; color: var(--text-secondary);">
            当前阶段：<strong style="color: var(--primary)">{{ run.stage }}</strong>
          </span>
          <span class="muted" style="font-size: 11.5px;">
            开始时间：{{ new Date(run.started_at_ms).toLocaleTimeString() }}
          </span>
        </div>

        <div class="run-message">
          {{ run.message || '任务进行中，引擎正在观察画面与匹配界面...' }}
        </div>

        <div v-if="run.failure_screenshot" style="margin: 12px 0;">
          <p class="muted" style="margin-bottom: 4px;">异常失败快照：</p>
          <span class="path">{{ run.failure_screenshot }}</span>
        </div>

        <div class="actions" style="margin-top: 20px;">
          <NButton v-if="store.running" type="error" @click="store.stop">停止运行</NButton>
          <RouterLink v-else to="/tasks">
            <NButton type="primary">配置并启动</NButton>
          </RouterLink>
          <RouterLink to="/logs">
            <NButton>查看运行日志</NButton>
          </RouterLink>
        </div>
      </template>

      <NEmpty v-else description="暂无正在运行的任务记录" style="padding: 24px 0;">
        <template #extra>
          <div class="actions" style="justify-content: center; margin-top: 10px;">
            <RouterLink to="/tasks">
              <NButton type="primary">前往配置并启动</NButton>
            </RouterLink>
            <RouterLink to="/devices">
              <NButton>连接第一台设备</NButton>
            </RouterLink>
          </div>
        </template>
      </NEmpty>
    </section>

    <!-- Right: Pre-flight Checklist -->
    <section class="panel">
      <div class="panel-heading">
        <h2>
          <svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="2" style="color: var(--primary)"><path d="M22 11.08V12a10 10 0 1 1-5.93-9.14"/><polyline points="22 4 12 14.01 9 11.01"/></svg>
          运行前就绪检查
        </h2>
        <NTag size="small" :bordered="false" type="info">快速指引</NTag>
      </div>

      <div class="checklist">
        <div class="checklist-item">
          <span class="checklist-num">1</span>
          <div>
            <strong>保持游戏界面完整</strong>
            <p class="muted" style="margin: 2px 0 0;">分辨率支持自动适配，确保无遮挡并处于前台窗口。</p>
          </div>
        </div>

        <div class="checklist-item">
          <span class="checklist-num">2</span>
          <div>
            <strong>进入目标御魂挑战页</strong>
            <p class="muted" style="margin: 2px 0 0;">手动导航到对应御魂层数的单人挑战界面。</p>
          </div>
        </div>

        <div class="checklist-item">
          <span class="checklist-num">3</span>
          <div>
            <strong>锁定阵容与自动战斗</strong>
            <p class="muted" style="margin: 2px 0 0;">在游戏中开启阵容锁定，并将战斗模式切换为自动。</p>
          </div>
        </div>

        <div class="checklist-item">
          <span class="checklist-num">4</span>
          <div>
            <strong>运行设备诊断核验</strong>
            <p class="muted" style="margin: 2px 0 0;">在设备管理页确认模板及按钮识别置信度正常。</p>
          </div>
        </div>
      </div>

      <div style="margin-top: 16px; padding: 12px 14px; background: #fffbeb; border-radius: 8px; border: 1px solid #fde68a;">
        <p style="font-size: 11.5px; color: #92400e; margin: 0; line-height: 1.6;">
          <strong>提示：</strong>当前御魂流程为实验版，体力购买、自动换狗粮和多人组队暂未开放。遇到异常会自动停止并保留现场快照。
        </p>
      </div>
    </section>
  </div>
</template>
