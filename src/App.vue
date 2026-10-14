<script setup lang="ts">
import { ref } from "vue";
import { invoke } from "@tauri-apps/api/core";

const status = ref("工程已初始化，OAS 引擎尚未接入。");
async function checkDesktop() {
  try {
    status.value = await invoke<string>("desktop_status");
  } catch {
    status.value = "请通过 pnpm tauri dev 启动桌面应用，再检查连接。";
  }
}
</script>

<template>
  <main>
    <p class="eyebrow">SMART OMJ · 开发起点</p>
    <h1>阴阳师一体化助手</h1>
    <p>统一管理自动化引擎、模拟器和任务。</p>
    <p role="status">{{ status }}</p>
    <button type="button" @click="checkDesktop">检查桌面连接</button>
  </main>
</template>

<style>
:root { font-family: "Segoe UI", "Microsoft YaHei", sans-serif; color: #24312b; background: #f5f6f2; font-synthesis: none; }
body { margin: 0; }
main { max-width: 760px; margin: 12vh auto; padding: 32px; }
h1 { font-size: clamp(28px, 5vw, 44px); font-weight: 600; }
p { line-height: 1.8; }
.eyebrow { font-size: 12px; letter-spacing: .16em; color: #617267; }
button { margin-top: 24px; padding: 12px 20px; border: 1px solid #254b3c; border-radius: 6px; background: #254b3c; color: white; cursor: pointer; font: inherit; }
button:focus-visible { outline: 3px solid #8cbca5; outline-offset: 4px; }
</style>

