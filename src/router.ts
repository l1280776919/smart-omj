import { createRouter, createWebHashHistory } from 'vue-router';
import ScenePage from './pages/ScenePage.vue';
import DashboardPage from './pages/DashboardPage.vue';
import DevicePage from './pages/DevicePage.vue';
import TasksPage from './pages/TasksPage.vue';
import LogsPage from './pages/LogsPage.vue';
import SettingsPage from './pages/SettingsPage.vue';

export const router = createRouter({ history: createWebHashHistory(), routes: [
  { path: '/scenes', component: ScenePage, meta: { title: '场景训练' } },
  { path: '/', component: DashboardPage, meta: { title: '运行概览' } },
  { path: '/devices', component: DevicePage, meta: { title: '设备管理' } },
  { path: '/tasks', component: TasksPage, meta: { title: '任务配置' } },
  { path: '/logs', component: LogsPage, meta: { title: '运行日志' } },
  { path: '/settings', component: SettingsPage, meta: { title: '应用设置' } },
] });
