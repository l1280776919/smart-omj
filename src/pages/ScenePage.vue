<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue';
import { NAlert, NButton, NInputNumber, NProgress, NTag, NSelect } from 'naive-ui';
import ButtonStudio from '../components/ButtonStudio.vue';
import { useAppStore } from '../stores/app';
import { scenes, sceneName, type SceneStatus, type Recognition, type Samples } from '../services/scenes';

const store = useAppStore();
const status = ref<SceneStatus|null>(null), result = ref<Recognition|null>(null);
const samples = ref<Samples|null>(null), reviewPage = ref(0);
const tab = ref('recognize'), interval = ref(.5), epochs = ref(30), busy = ref(false), error = ref(''), notice = ref('');
const device = ref('auto');
const deviceOptions = [
  { label: '自动（优先 GPU）', value: 'auto' },
  { label: 'GPU / CUDA', value: 'cuda' },
  { label: 'CPU', value: 'cpu' }
];

const running = computed(() => status.value?.job?.status === 'running');
const disabled = computed(() => busy.value || !store.connected);
const activeClasses = computed(() => Object.values(status.value?.model?.classes ?? {}));

let timer: ReturnType<typeof setTimeout>|undefined;
let disposed = false;

async function refresh() {
  if (!store.connected) return;
  status.value = await scenes.status();
}

async function poll() {
  try {
    await refresh();
  } catch (e) {
    error.value = String(e);
  } finally {
    if (!disposed) timer = setTimeout(poll, 1500);
  }
}

onMounted(poll);
onUnmounted(() => {
  disposed = true;
  clearTimeout(timer);
});

async function perform(action: () => Promise<unknown>, message = '') {
  busy.value = true;
  error.value = '';
  notice.value = '';
  try {
    await action();
    await refresh();
    notice.value = message;
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = false;
  }
}

async function review(page = 0) {
  await perform(async () => {
    samples.value = await scenes.samples(page);
    reviewPage.value = page;
  });
}

async function exclude(index: number, value: boolean) {
  await perform(async () => {
    await scenes.exclude(index, value);
    samples.value = await scenes.samples(reviewPage.value);
  });
}

async function recognize() {
  result.value = null;
  await perform(async () => {
    result.value = await scenes.recognize(store.config!.serial);
  });
}

async function openFolder(kind: string) {
  await perform(() => scenes.open(kind));
}

async function start(action: string) {
  samples.value = null;
  await perform(() => scenes.start(action, interval.value, epochs.value, device.value));
}
</script>

<template>
  <div class="scene-heading" :class="{ compact: tab === 'buttons' }">
    <div class="page-heading-left">
      <h1>场景识别与训练工作台</h1>
      <p>从游戏录像中抽帧采集、标注训练本地 YOLO 模型，并实时进行场景分类与按钮检测。</p>
    </div>
    <NTag :type="status?.model ? 'info' : 'default'" round>
      {{ status?.model ? `${activeClasses.length} 个分类场景 · 模型已就绪` : '等待模型就绪' }}
    </NTag>
  </div>

  <div class="scene-tabs" role="tablist" aria-label="场景工作台">
    <button
      role="tab"
      :aria-selected="tab === 'recognize'"
      :class="{ active: tab === 'recognize' }"
      @click="tab = 'recognize'"
    >
      场景识别
      <span>实机验证</span>
    </button>
    <button
      role="tab"
      :aria-selected="tab === 'train'"
      :class="{ active: tab === 'train' }"
      @click="tab = 'train'"
    >
      素材与训练
      <span v-if="running">处理中</span>
      <span v-else>模型构建</span>
    </button>
    <button
      role="tab"
      :aria-selected="tab === 'buttons'"
      :class="{ active: tab === 'buttons' }"
      @click="tab = 'buttons'"
    >
      按钮识别与坐标
      <span>独立检测</span>
    </button>
  </div>

  <NAlert v-if="error" type="error" closable @close="error = ''" class="feedback">{{ error }}</NAlert>
  <NAlert v-if="notice" type="success" closable @close="notice = ''" class="feedback">{{ notice }}</NAlert>

  <!-- Tab 1: Scene Recognition -->
  <div v-if="tab === 'recognize'" class="scene-recognition">
    <section class="scene-screen panel">
      <div class="panel-heading">
        <div>
          <h2>模拟器实时画面</h2>
          <p class="panel-desc">{{ store.config?.serial || '尚未连接或选择设备' }}</p>
        </div>
        <NButton
          type="primary"
          :loading="busy"
          :disabled="disabled || store.running || !store.config?.serial || !status?.model"
          @click="recognize"
        >
          截图并识别
        </NButton>
      </div>

      <div class="scene-canvas">
        <img v-if="result" :src="result.data_url" alt="本次场景识别使用的模拟器截图"/>
        <div v-else class="scene-empty">
          <div class="viewfinder">⌗</div>
          <h3>当前未截取画面</h3>
          <p>连接模拟器后，点击右上角「截图并识别」核对模型分类结果。</p>
          <RouterLink v-if="!store.config?.serial" to="/devices" style="color: var(--primary); font-weight: 500;">
            前往连接设备 →
          </RouterLink>
        </div>
      </div>
      <div class="scene-caption">
        <span>画面保持原生比例</span>
        <span>Rust ONNX 本地快速推理</span>
      </div>
    </section>

    <aside class="scene-inspector panel">
      <h2>模型分类预测</h2>
      <template v-if="result">
        <div class="prediction-name">
          {{ result.prediction.uncertain ? '暂不确定' : sceneName(result.prediction.class) }}
        </div>
        <p class="muted" style="margin-top: 4px;">{{ result.prediction.elapsed_ms.toFixed(0) }} ms · 推理耗时</p>
        <div style="margin: 8px 0;">
          <NTag v-if="result.prediction.compute" :type="result.prediction.compute.provider === 'CUDA' ? 'success' : 'warning'" size="small" round>
            {{ result.prediction.compute.provider === 'CUDA' ? 'GPU · CUDA' : 'CPU 推理' }}
          </NTag>
        </div>
        <p v-if="result.prediction.compute?.fallback_reason" class="scene-hint">
          GPU 未启用：{{ result.prediction.compute.fallback_reason }}
        </p>

        <div style="margin-top: 18px;">
          <div v-for="score in result.prediction.scores" :key="score.class" class="scene-score">
            <div>
              <span>{{ sceneName(score.class) }}</span>
              <strong>{{ (score.confidence * 100).toFixed(1) }}%</strong>
            </div>
            <NProgress type="line" :percentage="score.confidence * 100" :show-indicator="false" :height="6" color="#2563eb"/>
          </div>
        </div>
      </template>

      <div v-else class="scene-result-empty">
        <span>—</span>
        <p class="muted">识别后在此显示场景置信度</p>
      </div>

      <div class="model-summary">
        <small style="font-weight: 600; color: var(--text-main);">当前模型支持场景：</small>
        <div class="scene-tags">
          <NTag v-for="name in activeClasses" :key="name" :bordered="false" size="small" type="info">
            {{ sceneName(name) }}
          </NTag>
        </div>
        <p class="muted" style="font-size: 11.5px; margin-top: 10px;">仅针对列出场景训练。其他未知画面需结合截图综合判断。</p>
      </div>
    </aside>
  </div>

  <!-- Tab 2: Materials & Training -->
  <div v-else-if="tab === 'train'">
    <div class="scene-workflow">
      <span :class="{ done: status?.classes.length }"><b>1</b>放入录像</span>
      <i>→</i>
      <span :class="{ done: status?.prepared }"><b>2</b>抽帧与检查</span>
      <i>→</i>
      <span :class="{ done: status?.job?.result }"><b>3</b>训练与评估</span>
      <i>→</i>
      <span><b>4</b>启用并验证</span>
    </div>

    <NAlert v-if="status && !status.available" type="warning" class="feedback">
      此电脑未找到训练环境。识别功能仍可使用；训练需要项目 training 目录及其 Python 虚拟环境。
    </NAlert>

    <div class="scene-training-grid">
      <section class="panel">
        <div class="panel-heading">
          <div>
            <h2>场景素材录像</h2>
            <p class="panel-desc">按场景文件夹存放 MP4、MOV、MKV 录像</p>
          </div>
          <NButton :disabled="disabled || !status?.available" @click="openFolder('videos')">
            打开录像目录
          </NButton>
        </div>

        <div v-if="!status?.classes.length" class="scene-tip">
          在 videos 目录下新建场景文件夹并将录像放入后刷新。训练分类器至少需要两个场景。
        </div>

        <div v-for="item in status?.classes" :key="item.name" class="scene-material-row">
          <div class="scene-file-icon">{{ sceneName(item.name).slice(0, 1) }}</div>
          <div class="scene-material-info">
            <strong>{{ sceneName(item.name) }}</strong>
            <small>{{ item.name }} · {{ item.videos.length }} 段录像</small>
          </div>
          <span>{{ status?.prepared?.classes[item.name] ?? '—' }} <small>张</small></span>
        </div>

        <div class="scene-form-row">
          <label for="sample-interval">
            抽帧间隔
            <small>每隔多少秒保存一张图片素材</small>
          </label>
          <NInputNumber
            id="sample-interval"
            v-model:value="interval"
            :min=".1"
            :max="30"
            :step=".5"
            :disabled="running"
            :show-button="true"
          >
            <template #suffix>秒</template>
          </NInputNumber>
        </div>

        <div class="actions">
          <NButton
            type="primary"
            :disabled="disabled || running || !status?.available || !status?.classes.some(c => c.videos.length)"
            @click="start('prepare')"
          >
            {{ status?.prepared ? '重新抽取素材' : '抽取图片素材' }}
          </NButton>
          <NButton :disabled="disabled || !status?.prepared || running" @click="review()">
            检查图片
          </NButton>
          <NButton tertiary :disabled="disabled" @click="perform(refresh)">
            刷新
          </NButton>
        </div>
      </section>

      <section class="panel">
        <div class="panel-heading">
          <div>
            <h2>分类器模型训练</h2>
            <p class="panel-desc">自动划分验证集并导出高精度轻量模型</p>
          </div>
          <NTag :bordered="false" type="info">YOLO-Cls</NTag>
        </div>

        <div class="scene-form-row">
          <label for="train-device">计算加速设备</label>
          <NSelect
            id="train-device"
            v-model:value="device"
            :options="deviceOptions"
            :disabled="running"
            style="width: 200px"
          />
        </div>

        <div class="scene-form-row">
          <label for="train-epochs">
            训练轮数
            <small>默认 30 轮，支持早停策略</small>
          </label>
          <NInputNumber
            id="train-epochs"
            v-model:value="epochs"
            :min="1"
            :max="300"
            :precision="0"
            :disabled="running"
          />
        </div>

        <div class="actions scene-train-actions">
          <NButton
            type="primary"
            :disabled="disabled || running || !status?.available || !status?.prepared || Object.keys(status.prepared.classes).length < 2"
            @click="start('train')"
          >
            开始训练
          </NButton>
          <NButton v-if="running" :disabled="disabled" @click="perform(scenes.cancel, '任务已取消，当前模型保持不变')">
            取消任务
          </NButton>
        </div>

        <div v-if="status?.job" class="scene-job" aria-live="polite">
          <div class="scene-job-title">
            <strong>
              {{ { running: status.job.progress?.stage ?? '启动中', completed: '处理完成', failed: '处理失败', cancelled: '已取消', interrupted: '上次任务已中断' }[status.job.status] }}
            </strong>
            <span v-if="status.job.progress?.epoch">{{ status.job.progress.epoch }} / {{ status.job.progress.epochs }} 轮</span>
          </div>

          <NProgress
            type="line"
            :percentage="Math.round((status.job.progress?.progress ?? 0) * 100)"
            :status="status.job.status === 'failed' ? 'error' : 'default'"
            color="#2563eb"
          />

          <p v-if="status.job.progress?.error" class="scene-error">{{ status.job.progress.error }}</p>

          <div v-if="status.job.result && status.job.status === 'completed'" class="scene-evaluation">
            <strong>{{ (status.job.result.top1 * 100).toFixed(1) }}<small>% 验证准确率</small></strong>
            <p>{{ status.job.result.limitation }}</p>
            <NButton type="primary" :disabled="disabled" @click="perform(scenes.activate, '新模型已启用，可以切换到场景识别验证')">
              启用此模型
            </NButton>
          </div>

          <details style="margin: 12px 0;">
            <summary style="cursor: pointer; font-size: 12px; color: var(--primary);">查看详细运行日志</summary>
            <pre>{{ status.job.log || '等待输出…' }}</pre>
          </details>
          <NButton text size="small" :disabled="disabled" @click="openFolder('result')">
            打开任务输出目录 ↗
          </NButton>
        </div>
      </section>
    </div>

    <!-- Review Section -->
    <section v-if="samples" class="panel scene-review">
      <div class="panel-heading">
        <div>
          <h2>素材质量审核</h2>
          <p class="panel-desc">排除过渡帧或黑屏图片，原图保持完好</p>
        </div>
        <NButton tertiary size="small" @click="samples = null">收起</NButton>
      </div>

      <div class="scene-gallery">
        <article v-for="item in samples.items" :key="item.index" :class="{ excluded: item.excluded }">
          <img v-if="item.thumbnail" :src="item.thumbnail" :alt="`${sceneName(item.label)} ${item.seconds} 秒截图`"/>
          <div v-else class="scene-tip">无缩略图</div>
          <div>
            <span>{{ sceneName(item.label) }} · {{ item.seconds.toFixed(1) }}s</span>
            <NButton size="tiny" :disabled="disabled || running" @click="exclude(item.index, !item.excluded)">
              {{ item.excluded ? '恢复' : '排除' }}
            </NButton>
          </div>
        </article>
      </div>

      <div class="actions scene-review-pages">
        <NButton :disabled="disabled || reviewPage === 0" @click="review(reviewPage - 1)">上一页</NButton>
        <span>{{ reviewPage + 1 }} / {{ Math.max(1, Math.ceil(samples.total / 12)) }} · 共 {{ samples.total }} 张</span>
        <NButton :disabled="disabled || (reviewPage + 1) * 12 >= samples.total" @click="review(reviewPage + 1)">下一页</NButton>
        <NButton text :disabled="disabled" @click="openFolder('prepared')">打开图片目录</NButton>
      </div>
    </section>

    <footer class="scene-storage">
      <span>本地素材目录:</span>
      <code>{{ status?.root ?? '连接桌面应用后显示' }}</code>
    </footer>
  </div>

  <!-- Tab 3: Buttons Studio -->
  <ButtonStudio v-show="tab === 'buttons'"/>
</template>

<style scoped>
.scene-heading {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 20px;
  margin-bottom: 24px;
}
.scene-heading.compact {
  margin-bottom: 16px;
}

.scene-tabs {
  display: flex;
  gap: 12px;
  background: var(--bg-surface);
  padding: 6px;
  border-radius: var(--radius-md);
  border: 1px solid var(--border-color);
  margin-bottom: 24px;
}

.scene-tabs button {
  border: 0;
  border-radius: var(--radius-sm);
  background: transparent;
  padding: 10px 20px;
  cursor: pointer;
  font-size: 14px;
  font-weight: 500;
  color: var(--text-secondary);
  display: flex;
  align-items: center;
  gap: 8px;
  transition: all 0.15s ease;
}

.scene-tabs button:hover {
  color: var(--text-main);
  background: var(--bg-hover);
}

.scene-tabs button.active {
  background: var(--primary-subtle);
  color: var(--primary);
  font-weight: 600;
}

.scene-tabs span {
  font-size: 11px;
  font-weight: 400;
  color: var(--text-muted);
}
.scene-tabs button.active span {
  color: var(--primary);
  opacity: 0.8;
}

.scene-recognition {
  display: grid;
  grid-template-columns: minmax(0, 1fr) 300px;
  gap: 20px;
}

.scene-canvas {
  aspect-ratio: 16/9;
  background: #0b1120;
  border-radius: var(--radius-md);
  overflow: hidden;
  display: grid;
  place-items: center;
  border: 1px solid #1e293b;
}

.scene-canvas img {
  width: 100%;
  height: 100%;
  object-fit: contain;
}

.scene-empty {
  text-align: center;
  padding: 24px;
  color: #94a3b8;
}

.scene-empty h3 {
  font-size: 16px;
  font-weight: 600;
  margin: 12px 0 6px;
  color: #f1f5f9;
}

.scene-empty p {
  font-size: 12.5px;
  color: #94a3b8;
  margin: 0 0 12px;
}

.viewfinder {
  font-size: 40px;
  color: #3b82f6;
  line-height: 1;
}

.scene-caption {
  display: flex;
  justify-content: space-between;
  margin-top: 12px;
  font-size: 12px;
  color: var(--text-muted);
}

.scene-result-empty {
  padding: 30px 0;
  text-align: center;
}
.scene-result-empty > span {
  font-size: 40px;
  color: var(--text-faint);
}

.prediction-name {
  font-size: 28px;
  font-weight: 700;
  color: var(--primary);
  margin-top: 12px;
  line-height: 1.2;
}

.scene-score {
  margin: 14px 0;
}
.scene-score > div:first-child {
  display: flex;
  justify-content: space-between;
  margin-bottom: 6px;
  font-size: 12.5px;
}

.model-summary {
  border-top: 1px solid var(--border-color);
  padding-top: 18px;
  margin-top: 20px;
}

.scene-tags {
  display: flex;
  gap: 6px;
  flex-wrap: wrap;
  margin-top: 8px;
}

.scene-tip {
  background: var(--primary-subtle);
  border: 1px solid var(--primary-border);
  padding: 12px 14px;
  font-size: 12px;
  line-height: 1.6;
  color: #1e40af;
  border-radius: var(--radius-md);
  margin: 14px 0;
}

.scene-workflow {
  display: flex;
  align-items: center;
  gap: 16px;
  padding: 0 0 20px;
  color: var(--text-muted);
  font-size: 13px;
}
.scene-workflow b {
  display: inline-grid;
  place-items: center;
  width: 22px;
  height: 22px;
  border-radius: 50%;
  background: var(--bg-hover);
  margin-right: 6px;
  font-weight: 600;
  font-size: 11px;
}
.scene-workflow .done {
  color: var(--primary);
  font-weight: 500;
}
.scene-workflow .done b {
  background: var(--primary-subtle);
  color: var(--primary);
}
.scene-workflow i {
  font-style: normal;
  color: var(--border-color);
}

.scene-training-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 20px;
}

.scene-material-row {
  display: flex;
  align-items: center;
  gap: 14px;
  padding: 12px 0;
  border-bottom: 1px solid var(--border-subtle);
}
.scene-file-icon {
  width: 36px;
  height: 36px;
  border-radius: 8px;
  background: var(--primary-subtle);
  color: var(--primary);
  display: grid;
  place-items: center;
  font-weight: 700;
  font-size: 16px;
}
.scene-material-info {
  flex: 1;
}
.scene-material-info strong {
  font-size: 13.5px;
  color: var(--text-main);
}
.scene-material-info small {
  display: block;
  margin-top: 2px;
  color: var(--text-muted);
  font-size: 11.5px;
}

.scene-form-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 16px;
  margin: 18px 0;
}
.scene-form-row label {
  font-size: 13px;
  color: var(--text-main);
}
.scene-form-row label small {
  display: block;
  font-size: 11.5px;
  color: var(--text-muted);
  margin-top: 2px;
}

.scene-job {
  border-top: 1px solid var(--border-color);
  padding-top: 16px;
  margin-top: 16px;
}
.scene-job-title {
  display: flex;
  justify-content: space-between;
  margin-bottom: 8px;
  font-size: 12.5px;
}
.scene-evaluation {
  margin: 16px 0;
  padding: 14px 16px;
  background: var(--bg-subtle);
  border-radius: var(--radius-md);
  border: 1px solid var(--border-subtle);
}
.scene-evaluation > strong {
  font-size: 24px;
  color: var(--primary);
}
.scene-evaluation small {
  font-size: 12px;
  color: var(--text-muted);
  margin-left: 6px;
}

.scene-gallery {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 14px;
}
.scene-gallery article {
  background: var(--bg-subtle);
  border-radius: var(--radius-md);
  overflow: hidden;
  border: 1px solid var(--border-subtle);
}
.scene-gallery img {
  width: 100%;
  aspect-ratio: 16/9;
  object-fit: contain;
  display: block;
  background: #000;
}
.scene-gallery article > div:last-child {
  padding: 8px 10px;
  display: flex;
  justify-content: space-between;
  align-items: center;
  font-size: 11.5px;
}
.scene-gallery .excluded img {
  opacity: 0.35;
}
.scene-gallery .excluded {
  border: 1px dashed var(--danger);
}

.scene-storage {
  display: flex;
  gap: 10px;
  align-items: center;
  font-size: 12px;
  color: var(--text-muted);
  margin-top: 20px;
  padding: 10px 14px;
  background: var(--bg-surface);
  border-radius: var(--radius-md);
  border: 1px solid var(--border-color);
}
.scene-storage code {
  font-family: var(--font-mono);
  background: var(--bg-subtle);
  padding: 2px 6px;
  border-radius: 4px;
}

@media (max-width: 1000px) {
  .scene-recognition, .scene-training-grid {
    grid-template-columns: 1fr;
  }
}
</style>
