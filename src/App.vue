<script setup lang="ts">
import { computed, h, onMounted, onUnmounted } from 'vue';
import { useRoute } from 'vue-router';
import { NConfigProvider, NButton, NAlert, NTag } from 'naive-ui';
import { useAppStore } from './stores/app';

const store = useAppStore();
const route = useRoute();

const navigation = [
  {
    path: '/',
    name: '运行概览',
    icon: () => h('svg', { viewBox: '0 0 24 24', width: '18', height: '18', fill: 'none', stroke: 'currentColor', 'stroke-width': '2', 'stroke-linecap': 'round', 'stroke-linejoin': 'round' }, [
      h('rect', { x: '3', y: '3', width: '7', height: '9', rx: '1' }),
      h('rect', { x: '14', y: '3', width: '7', height: '5', rx: '1' }),
      h('rect', { x: '14', y: '12', width: '7', height: '9', rx: '1' }),
      h('rect', { x: '3', y: '16', width: '7', height: '5', rx: '1' })
    ])
  },
  {
    path: '/devices',
    name: '设备管理',
    icon: () => h('svg', { viewBox: '0 0 24 24', width: '18', height: '18', fill: 'none', stroke: 'currentColor', 'stroke-width': '2', 'stroke-linecap': 'round', 'stroke-linejoin': 'round' }, [
      h('rect', { x: '5', y: '2', width: '14', height: '20', rx: '2', ry: '2' }),
      h('line', { x1: '12', y1: '18', x2: '12.01', y2: '18' })
    ])
  },
  {
    path: '/tasks',
    name: '任务配置',
    icon: () => h('svg', { viewBox: '0 0 24 24', width: '18', height: '18', fill: 'none', stroke: 'currentColor', 'stroke-width': '2', 'stroke-linecap': 'round', 'stroke-linejoin': 'round' }, [
      h('path', { d: 'M12 2v4' }),
      h('path', { d: 'M12 18v4' }),
      h('path', { d: 'M4.93 4.93l2.83 2.83' }),
      h('path', { d: 'M16.24 16.24l2.83 2.83' }),
      h('path', { d: 'M2 12h4' }),
      h('path', { d: 'M18 12h4' }),
      h('path', { d: 'M4.93 19.07l2.83-2.83' }),
      h('path', { d: 'M16.24 7.76l2.83-2.83' })
    ])
  },
  {
    path: '/scenes',
    name: '场景训练',
    icon: () => h('svg', { viewBox: '0 0 24 24', width: '18', height: '18', fill: 'none', stroke: 'currentColor', 'stroke-width': '2', 'stroke-linecap': 'round', 'stroke-linejoin': 'round' }, [
      h('path', { d: 'M2 12h20' }),
      h('path', { d: 'M20 12v6a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2v-6' }),
      h('path', { d: 'm12 2 8 8-8 8-8-8z' })
    ])
  },
  {
    path: '/logs',
    name: '运行日志',
    icon: () => h('svg', { viewBox: '0 0 24 24', width: '18', height: '18', fill: 'none', stroke: 'currentColor', 'stroke-width': '2', 'stroke-linecap': 'round', 'stroke-linejoin': 'round' }, [
      h('polyline', { points: '4 17 10 11 4 5' }),
      h('line', { x1: '12', y1: '19', x2: '20', y2: '19' })
    ])
  },
  {
    path: '/settings',
    name: '应用设置',
    icon: () => h('svg', { viewBox: '0 0 24 24', width: '18', height: '18', fill: 'none', stroke: 'currentColor', 'stroke-width': '2', 'stroke-linecap': 'round', 'stroke-linejoin': 'round' }, [
      h('circle', { cx: '12', cy: '12', r: '3' }),
      h('path', { d: 'M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z' })
    ])
  }
];

const themeOverrides = {
  common: {
    primaryColor: '#2563eb',
    primaryColorHover: '#1d4ed8',
    primaryColorPressed: '#1e40af',
    primaryColorSuppl: '#3b82f6',
    borderRadius: '8px',
    fontFamily: '-apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "PingFang SC", "Microsoft YaHei", sans-serif'
  },
  Button: {
    borderRadiusMedium: '8px',
    borderRadiusSmall: '6px'
  },
  Card: {
    borderRadius: '12px'
  }
};

const currentDeviceLabel = computed(() => store.config?.serial || '未选设备');

onMounted(() => store.initialize());
onUnmounted(() => store.dispose());
</script>

<template>
  <NConfigProvider :theme-overrides="themeOverrides">
    <div class="app-shell">
      <aside class="sidebar">
        <div class="brand">
          <span class="brand-mark">御</span>
          <div class="brand-text">
            <strong>SMART OMJ</strong>
            <small>阴阳师自动化引擎</small>
          </div>
        </div>

        <nav aria-label="主导航">
          <RouterLink
            v-for="item in navigation"
            :key="item.path"
            :to="item.path"
            :class="{ selected: route.path === item.path }"
          >
            <component :is="item.icon" class="nav-icon" />
            <span>{{ item.name }}</span>
          </RouterLink>
        </nav>

        <div class="sidebar-footer">
          <div class="status-row">
            <span class="connection-dot" :class="{ online: store.connected }"></span>
            <span>{{ store.connected ? 'Rust 核心引擎已连接' : '引擎未就绪' }}</span>
          </div>
          <small>v0.1 · 原生高性能架构</small>
        </div>
      </aside>

      <main class="workspace">
        <header class="topbar">
          <div class="topbar-title">
            <span>{{ route.meta.title || '自动化助手' }}</span>
          </div>
          <div class="topbar-actions">
            <RouterLink to="/devices" class="topbar-device-pill" title="点击切换或配置设备">
              <span class="connection-dot" :class="{ online: !!store.config?.serial }"></span>
              <span>设备: {{ currentDeviceLabel }}</span>
            </RouterLink>
            <NTag :type="store.running ? 'warning' : 'default'" size="small" round>
              {{ store.running ? '任务运行中' : '待机就绪' }}
            </NTag>
            <NButton v-if="store.running" size="small" type="error" @click="store.stop">
              停止任务
            </NButton>
          </div>
        </header>

        <div class="page-content">
          <NAlert v-if="store.error" type="error" closable @close="store.error = ''" class="feedback" role="alert">
            {{ store.error }}
          </NAlert>
          <NAlert v-if="store.notice" type="success" closable @close="store.notice = ''" class="feedback">
            {{ store.notice }}
          </NAlert>
          <NAlert v-if="!store.connected" type="info" class="feedback">
            启动桌面应用后可自动连接引擎。
            <NButton size="small" :loading="store.busy" @click="store.initialize" style="margin-left: 8px">重新连接</NButton>
          </NAlert>
          <RouterView />
        </div>
      </main>
    </div>
  </NConfigProvider>
</template>
