<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue';
import { onBeforeRouteLeave } from 'vue-router';
import { NAlert, NButton, NCheckbox, NInput, NInputNumber, NProgress, NSelect, NTag, NModal } from 'naive-ui';
import { useAppStore } from '../stores/app';
import { buttons as buttonApi, type ButtonClass, type ButtonSample, type ButtonStatus, type ButtonResult, type ButtonBox } from '../services/buttons';
import { desktop } from '../services/desktop';
import { scenes, type Samples } from '../services/scenes';
import ButtonCanvas from './ButtonCanvas.vue';
import ButtonThumbnail from './ButtonThumbnail.vue';
import ButtonModelReview from './ButtonModelReview.vue';
import ButtonVideoSampler from './ButtonVideoSampler.vue';

const store = useAppStore();
const mode = ref<'annotate'|'dataset'|'train'|'test'>('annotate');
const showLibrary = ref(false), showNewClass = ref(false), showCaptureSettings = ref(false);
const checked = ref<string[]>([]), editingMeta = ref(''), metaBatch = ref('');
const splitOptions = [{ label: '训练集', value: 'train' }, { label: '验证集', value: 'val' }];

const editClassId = ref(''), classDraft = ref<ButtonClass>({ id: '', name: '', anchor: [.5, .5] }), classModal = ref(false), classError = ref('');
const deleteClassId = ref(''), deleteModal = ref(false);
const deleteImpact = computed(() => {
  const rows = status.value?.dataset.samples.filter(s => s.boxes.some(b => b.label === deleteClassId.value)) ?? [];
  return {
    images: rows.length,
    boxes: rows.reduce((n, s) => n + s.boxes.filter(b => b.label === deleteClassId.value).length, 0)
  };
});

const status = ref<ButtonStatus|null>(null), sample = ref<ButtonSample|null>(null), classes = ref<ButtonClass[]>([]);
const image = ref(''), busy = ref(false), error = ref(''), notice = ref(''), saved = ref('');
const selected = ref(-1), selectedClass = ref(''), drawing = ref(false), zoom = ref(1), negative = ref(false);
const newId = ref(''), newName = ref('');
const batch = ref(`capture-${new Date().toISOString().replace(/[:.]/g, '-')}`);
const filter = ref('all'), query = ref('');
const excludedClasses = ref<string[]>([]), imgsz = ref(640), patience = ref(15);
const selectedCounts = computed(() => counts.value.filter(c => !excludedClasses.value.includes(c.id)));
const epochs = ref(50), device = ref('auto'), threshold = ref(.5);
const testJob = ref<string>();
const testImage = ref(''), testSize = ref<[number, number]>([1920, 1080]), result = ref<ButtonResult|null>(null), testSelected = ref(-1);
const materialInput = ref<HTMLInputElement>(), testInput = ref<HTMLInputElement>();
const picker = ref<Samples|null>(null), pickerPage = ref(0);

let timer: ReturnType<typeof setTimeout>|undefined, disposed = false;
const serialized = computed(() => JSON.stringify({ sample: sample.value, classes: classes.value, negative: negative.value }));
const dirty = computed(() => !!sample.value && serialized.value !== saved.value);

onBeforeRouteLeave(() => {
  if (dirty.value) {
    error.value = '标注有未保存修改，请先保存或放弃修改再离开训练页';
    return false;
  }
  return true;
});

const locked = computed(() => busy.value || !store.connected);
const running = computed(() => status.value?.job?.status === 'running');
const currentBox = computed(() => sample.value?.boxes[selected.value]);
const currentClass = computed(() => classes.value.find(c => c.id === selectedClass.value));
const classOptions = computed(() => classes.value.map(c => ({ label: `${c.name} (${c.id})`, value: c.id })));
const filtered = computed(() =>
  (status.value?.dataset.samples ?? []).filter(s =>
    (filter.value === 'all' || (filter.value === 'pending' ? !s.reviewed : s.reviewed)) &&
    s.name.toLowerCase().includes(query.value.toLowerCase())
  )
);

const counts = computed(() => (status.value?.dataset.classes ?? []).map(c => ({
  ...c,
  train: status.value!.dataset.samples.filter(s => s.reviewed && s.split === 'train').reduce((sum, s) => sum + s.boxes.filter(b => b.label === c.id).length, 0),
  val: status.value!.dataset.samples.filter(s => s.reviewed && s.split === 'val').reduce((sum, s) => sum + s.boxes.filter(b => b.label === c.id).length, 0),
})));

const conflicts = computed(() => {
  const groups = new Map<string, Set<string>>();
  for (const s of status.value?.dataset.samples ?? []) {
    if (!s.reviewed) continue;
    const splits = groups.get(s.batch) ?? new Set();
    splits.add(s.split);
    groups.set(s.batch, splits);
  }
  return [...groups].filter(([, splits]) => splits.size > 1).map(([batch]) => batch);
});

const unused = computed(() => (status.value?.dataset.classes ?? []).filter(c => !status.value?.dataset.samples.some(s => s.boxes.some(b => b.label === c.id))));
const ready = computed(() => selectedCounts.value.length > 0 && selectedCounts.value.every(c => c.train > 0 && c.val > 0));
const trainImages = computed(() => status.value?.dataset.samples.filter(s => s.reviewed && s.split === 'train').length ?? 0);
const valImages = computed(() => status.value?.dataset.samples.filter(s => s.reviewed && s.split === 'val').length ?? 0);

function countState(c: { train: number; val: number }) {
  return !c.train && !c.val ? '缺已审核标注' : !c.train ? '缺训练集' : !c.val ? '缺验证集' : '可训练';
}

const reviewed = computed(() => status.value?.dataset.samples.filter(s => s.reviewed).length ?? 0);

async function refresh() {
  if (store.connected) status.value = await buttonApi.status();
}

async function poll() {
  try {
    await refresh();
  } catch (e) {
    error.value = String(e);
  } finally {
    if (!disposed) timer = setTimeout(poll, 2000);
  }
}

onMounted(poll);
onUnmounted(() => {
  disposed = true;
  clearTimeout(timer);
});

async function perform(fn: () => Promise<void>, message = '') {
  busy.value = true;
  error.value = '';
  notice.value = '';
  try {
    await fn();
    await refresh();
    notice.value = message;
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = false;
  }
}

function clearDraft() {
  sample.value = null;
  image.value = '';
  selected.value = -1;
  saved.value = '';
}

async function load(id: string) {
  const value = await buttonApi.sample(id);
  sample.value = value.sample;
  image.value = value.data_url;
  classes.value = JSON.parse(JSON.stringify(status.value?.dataset.classes ?? []));
  selected.value = -1;
  negative.value = value.sample.reviewed && !value.sample.boxes.length;
  selectedClass.value = classes.value[0]?.id ?? '';
  drawing.value = false;
  zoom.value = 1;
  saved.value = serialized.value;
}

async function choose(id: string) {
  if (dirty.value) {
    error.value = '请先保存标注或点击「放弃修改」再切换图片';
    return;
  }
  await perform(async () => {
    await load(id);
    showLibrary.value = false;
  });
}

async function adjacent(delta: number) {
  const rows = status.value?.dataset.samples ?? [];
  const i = rows.findIndex(s => s.id === sample.value?.id);
  const next = rows[i + delta];
  if (next) await choose(next.id);
}

async function inspectSample(id: string) {
  await choose(id);
  if (sample.value?.id === id) mode.value = 'annotate';
}

async function setCurrentPartition(split: 'train' | 'val') {
  const current = sample.value;
  if (!current) return;
  await perform(async () => {
    await buttonApi.partition([{ id: current.id, batch: current.batch, split }]);
    current.split = split;
    const baseline = JSON.parse(saved.value);
    baseline.sample.split = split;
    saved.value = JSON.stringify(baseline);
  }, `本图已设为${split === 'train' ? '训练集' : '验证集'}；标注修改仍需单独保存`);
}

function newCapture() {
  batch.value = `capture-${new Date().toISOString().replace(/[:.]/g, '-')}`;
  notice.value = '已开始新采集批次，之后导入的图片使用新批次';
}

function toggleChecked(id: string, value: boolean) {
  checked.value = value ? [...new Set([...checked.value, id])] : checked.value.filter(v => v !== id);
}

async function syncAfterDatasetChange() {
  await refresh();
  checked.value = checked.value.filter(id => status.value?.dataset.samples.some(s => s.id === id));
  if (sample.value) await load(sample.value.id);
}

async function setPartition(ids: string[], split: 'train' | 'val') {
  if (dirty.value) {
    error.value = '请先保存或放弃当前标注，再修改数据集';
    return;
  }
  const updates = (status.value?.dataset.samples ?? []).filter(s => ids.includes(s.id)).map(s => ({ id: s.id, batch: s.batch, split }));
  await perform(async () => {
    await buttonApi.partition(updates);
    await syncAfterDatasetChange();
  }, `已将 ${updates.length} 张图片设为${split === 'train' ? '训练集' : '验证集'}，其他图片未更改`);
}

function editMeta(row: ButtonSample) {
  editingMeta.value = row.id;
  metaBatch.value = row.batch;
}

async function saveMeta() {
  await perform(async () => {
    const row = status.value?.dataset.samples.find(s => s.id === editingMeta.value);
    if (!row) throw new Error('图片已不存在，请刷新后重试');
    await buttonApi.partition([{ id: row.id, split: row.split, batch: metaBatch.value.trim() }]);
    editingMeta.value = '';
    await syncAfterDatasetChange();
  }, '已保存此图片的来源');
}

async function pruneClasses() {
  await perform(async () => {
    await buttonApi.pruneClasses();
    await syncAfterDatasetChange();
  }, '已清理没有任何标注框的类别');
}

function editClass(c: ButtonClass) {
  editClassId.value = c.id;
  classDraft.value = JSON.parse(JSON.stringify(c));
  classError.value = '';
  classModal.value = true;
}

async function saveClass() {
  busy.value = true;
  classError.value = '';
  try {
    await buttonApi.editClass(editClassId.value, JSON.parse(JSON.stringify(classDraft.value)));
    await syncAfterDatasetChange();
    classModal.value = false;
    notice.value = '类别已更新，所有已有标注同步使用新标识；重新训练后更新识别模型';
  } catch (e) {
    classError.value = String(e);
  } finally {
    busy.value = false;
  }
}

function askDeleteClass(id: string) {
  deleteClassId.value = id;
  classError.value = '';
  deleteModal.value = true;
}

async function deleteClass() {
  busy.value = true;
  classError.value = '';
  try {
    await buttonApi.editClass(deleteClassId.value, null);
    await syncAfterDatasetChange();
    deleteModal.value = false;
    notice.value = '类别及对应标注已删除，原图保留，受影响图片需要重新审核';
  } catch (e) {
    classError.value = String(e);
  } finally {
    busy.value = false;
  }
}

async function save(next = false) {
  if (!sample.value) return;
  if (!sample.value.boxes.length && !negative.value) {
    error.value = '请标注按钮，或明确勾选「此图没有目标按钮」';
    return;
  }
  await perform(async () => {
    sample.value!.reviewed = true;
    await buttonApi.save(JSON.parse(JSON.stringify(sample.value)), JSON.parse(JSON.stringify(classes.value)));
    saved.value = serialized.value;
    await refresh();
    if (next) {
      const nextSample = status.value?.dataset.samples.find(s => !s.reviewed && s.id !== sample.value?.id);
      if (nextSample) await load(nextSample.id);
    }
  }, '标注已保存');
}

function changeBoxes(boxes: ButtonBox[]) {
  if (sample.value) {
    sample.value.boxes = boxes;
    negative.value = false;
    if (selected.value >= boxes.length) selected.value = boxes.length - 1;
  }
}

function selectBox(index: number) {
  selected.value = index;
  if (sample.value?.boxes[index]) selectedClass.value = sample.value.boxes[index]!.label;
}

function addClass() {
  const id = newId.value.trim(), name = newName.value.trim();
  if (!/^[A-Za-z0-9_]{1,64}$/.test(id) || !name) {
    error.value = '请输入按钮名称和由英文字母、数字、下划线组成的唯一标识';
    return;
  }
  if (classes.value.some(c => c.id === id)) {
    error.value = '这个按钮标识已经存在，请直接选择';
    return;
  }
  classes.value.push({ id, name, anchor: [.5, .5] });
  selectedClass.value = id;
  newId.value = '';
  newName.value = '';
  drawing.value = true;
  showNewClass.value = false;
}

function setBound(index: number, value: number|null) {
  if (value !== null && currentBox.value) currentBox.value.bounds[index] = value;
}

async function toPng(file: File): Promise<{ url: string; width: number; height: number }> {
  if (file.size > 24 * 1024 * 1024) throw new Error('图片不得超过 24 MB');
  const source = URL.createObjectURL(file);
  try {
    const im = new Image();
    im.src = source;
    await im.decode();
    if (im.width > 8192 || im.height > 8192) throw new Error('图片边长不得超过 8192 像素');
    const canvas = document.createElement('canvas');
    canvas.width = im.width;
    canvas.height = im.height;
    canvas.getContext('2d')!.drawImage(im, 0, 0);
    return { url: canvas.toDataURL('image/png'), width: im.width, height: im.height };
  } finally {
    URL.revokeObjectURL(source);
  }
}

async function importFiles(event: Event) {
  const input = event.target as HTMLInputElement;
  const files = Array.from(input.files ?? []);
  input.value = '';
  await perform(async () => {
    let id = '';
    for (const file of files) {
      const png = await toPng(file);
      id = await buttonApi.import(png.url, file.name, batch.value);
    }
    await refresh();
    if (id) await load(id);
  }, '图片已导入，待标注');
}

async function capture(training: boolean) {
  await perform(async () => {
    const shot = await desktop.capture(store.config!.serial);
    if (training) {
      const id = await buttonApi.import(shot.data_url, `截图 ${new Date().toLocaleTimeString()}`, batch.value);
      await refresh();
      await load(id);
    } else {
      testImage.value = shot.data_url;
      testSize.value = [shot.width, shot.height];
      result.value = null;
      await recognize();
    }
  });
}

async function testFile(event: Event) {
  const input = event.target as HTMLInputElement;
  const file = input.files?.[0];
  input.value = '';
  if (!file) return;
  await perform(async () => {
    const png = await toPng(file);
    testImage.value = png.url;
    testSize.value = [png.width, png.height];
    result.value = null;
  });
}

async function recognize() {
  result.value = null;
  testSelected.value = -1;
  result.value = await buttonApi.recognize(testImage.value, threshold.value, testJob.value);
}

async function prelabel() {
  if (!sample.value) return;
  await perform(async () => {
    const prediction = await buttonApi.recognize(image.value, threshold.value);
    for (const c of prediction.classes) {
      if (prediction.items.some(b => b.label === c.id) && !classes.value.some(existing => existing.id === c.id)) {
        classes.value.push(c);
      }
    }
    const boxes = [...sample.value!.boxes];
    for (const item of prediction.items) {
      if (!boxes.some(b => b.label === item.label && b.bounds.every((v, i) => Math.abs(v - item.bounds[i]!) < 3))) {
        boxes.push({ label: item.label, bounds: item.bounds });
      }
    }
    changeBoxes(boxes);
    drawing.value = false;
  }, '预标注已加入，请检查所有按钮并修正漏标后保存');
}

async function openPicker(page = 0) {
  await perform(async () => {
    picker.value = await scenes.samples(page);
    pickerPage.value = page;
  });
}

async function importScene(index: number) {
  await perform(async () => {
    const id = await buttonApi.importScene(index);
    await refresh();
    await load(id);
    picker.value = null;
  }, '已复制素材，只保留来源批次，不关联场景');
}

async function toAnnotation() {
  await perform(async () => {
    const id = await buttonApi.import(testImage.value, `识别修正 ${new Date().toLocaleTimeString()}`, batch.value);
    await refresh();
    await load(id);
    if (result.value) {
      for (const c of result.value.classes) {
        if (result.value.items.some(b => b.label === c.id) && !classes.value.some(old => old.id === c.id)) {
          classes.value.push(c);
        }
      }
      changeBoxes(result.value.items.map(b => ({ label: b.label, bounds: b.bounds })));
    }
    mode.value = 'annotate';
  });
}

async function removeSample(id: string) {
  await perform(async () => {
    await buttonApi.delete(id);
    if (sample.value?.id === id) clearDraft();
    checked.value = checked.value.filter(v => v !== id);
  }, '素材已移出列表，原图保留');
}

async function startTraining() {
  await perform(() => buttonApi.start(epochs.value, device.value, selectedCounts.value.map(c => c.id), imgsz.value, patience.value), '训练已启动');
}

async function copyPoint(point: [number, number]) {
  await perform(async () => {
    await navigator.clipboard.writeText(JSON.stringify({ x: point[0], y: point[1] }));
  }, '原图坐标已复制');
}
</script>

<template>
  <section class="button-studio">
    <!-- Header Topline -->
    <div class="studio-header">
      <div>
        <h2>按钮识别与坐标</h2>
        <p class="muted">独立标注、训练与定位游戏按钮目标检测模型，输出高精度检测框与自适应点击坐标。</p>
      </div>
      <div class="actions">
        <NTag type="info" size="small" round>
          {{ status?.model_classes.length ?? 0 }} 个模型类别已启用
        </NTag>
      </div>
    </div>

    <!-- Step Navigation Bar -->
    <div class="studio-steps-container">
      <div class="button-steps" role="tablist" aria-label="按钮训练步骤">
        <button
          v-for="step in [
            { id: 'annotate', name: '标注图片' },
            { id: 'dataset', name: '数据集与类别' },
            { id: 'train', name: '训练模型' },
            { id: 'test', name: '识别测试' }
          ]"
          :key="step.id"
          role="tab"
          :aria-selected="mode === step.id"
          :class="{ active: mode === step.id }"
          @click="mode = step.id as typeof mode"
        >{{ step.name }}</button>
      </div>
      <div class="steps-progress-pill">
        <span>审核进度：</span>
        <strong>{{ reviewed }} / {{ status?.dataset.samples.length ?? 0 }}</strong>
        <span class="muted">张已审核</span>
      </div>
    </div>

    <NAlert v-if="!store.connected" type="info" class="button-feedback">
      请在桌面应用连接引擎后使用标注保存、模型训练与目标识别功能。
    </NAlert>
    <NAlert v-if="error" type="error" closable class="button-feedback" @close="error = ''">{{ error }}</NAlert>
    <NAlert v-if="notice" type="success" closable class="button-feedback" @close="notice = ''">{{ notice }}</NAlert>

    <input ref="materialInput" type="file" accept="image/png,image/jpeg,image/webp" multiple hidden @change="importFiles"/>
    <input ref="testInput" type="file" accept="image/png,image/jpeg,image/webp" hidden @change="testFile"/>

    <!-- ============================================== -->
    <!-- Mode 1: Annotate Image (Professional Studio)   -->
    <!-- ============================================== -->
    <div v-show="mode === 'annotate'" class="annotate-mode-wrapper">
      <!-- Capture Toolbar -->
      <div class="studio-toolbar panel">
        <div class="capture-actions">
          <NButton
            type="primary"
            secondary
            :disabled="locked || dirty || store.running || !store.config?.serial"
            @click="capture(true)"
          >
            <template #icon>
              <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2"><path d="M23 19a2 2 0 0 1-2 2H3a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h4l2-3h6l2 3h4a2 2 0 0 1 2 2z"/><circle cx="12" cy="13" r="4"/></svg>
            </template>
            模拟器截图
          </NButton>
          <NButton :disabled="locked || dirty" @click="materialInput?.click()">
            <template #icon>
              <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><polyline points="17 8 12 3 7 8"/><line x1="12" y1="3" x2="12" y2="15"/></svg>
            </template>
            导入图片
          </NButton>
          <NButton tertiary :disabled="locked || dirty" @click="openPicker()">
            从已有素材选择
          </NButton>
        </div>

        <NButton quaternary size="small" @click="showCaptureSettings = !showCaptureSettings">
          {{ showCaptureSettings ? '收起批次设置' : '采集批次设置 ⚙' }}
        </NButton>
      </div>

      <ButtonVideoSampler :disabled="locked || dirty" @imported="refresh" />

      <!-- Capture Settings Dropdown -->
      <div v-if="showCaptureSettings" class="capture-settings-card">
        <div style="flex: 1; min-width: 260px;">
          <label style="font-size: 12px; font-weight: 500; color: var(--text-secondary); display: block; margin-bottom: 4px;">
            新导入素材归属批次:
          </label>
          <NInput v-model:value="batch" aria-label="导入采集批次"/>
        </div>
        <NButton :disabled="busy" @click="newCapture">
          新建独立采集批次
        </NButton>
        <p class="muted" style="width: 100%; font-size: 11.5px; margin: 4px 0 0;">
          同批次图片在模型训练时将作为相关数据聚类。独立场景建议使用新批次。
        </p>
      </div>

      <!-- Material Picker Panel -->
      <div v-if="picker" class="panel button-picker">
        <div class="button-topline">
          <h3 style="margin: 0; font-size: 14px; font-weight: 600;">从已有场景素材库中挑选</h3>
          <NButton text @click="picker = null">收起</NButton>
        </div>
        <p class="muted" style="font-size: 12px; margin: 4px 0 12px;">复制选定图片至按钮标注集，原场景关联将解除。</p>
        <div class="picker-grid">
          <button v-for="item in picker.items" :key="item.index" :disabled="locked || item.excluded" @click="importScene(item.index)">
            <img v-if="item.thumbnail" :src="item.thumbnail" alt="可导入的图片素材"/>
            <span>#{{ item.index }} · {{ item.seconds.toFixed(1) }}s {{ item.excluded ? '已排除' : '' }}</span>
          </button>
        </div>
        <div class="button-toolbar" style="margin-top: 12px; justify-content: center;">
          <NButton size="small" :disabled="locked || pickerPage === 0" @click="openPicker(pickerPage - 1)">上一页</NButton>
          <span style="font-size: 12px; margin: 0 10px;">{{ pickerPage + 1 }} / {{ Math.max(1, Math.ceil(picker.total / 12)) }}</span>
          <NButton size="small" :disabled="locked || (pickerPage + 1) * 12 >= picker.total" @click="openPicker(pickerPage + 1)">下一页</NButton>
        </div>
      </div>

      <!-- Navigation & Partition Bar -->
      <div class="image-navigation">
        <NButton
          size="small"
          :type="showLibrary ? 'primary' : 'default'"
          :disabled="locked"
          @click="showLibrary = !showLibrary"
        >
          <template #icon>
            <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2"><line x1="8" y1="6" x2="21" y2="6"/><line x1="8" y1="12" x2="21" y2="12"/><line x1="8" y1="18" x2="21" y2="18"/><line x1="3" y1="6" x2="3.01" y2="6"/><line x1="3" y1="12" x2="3.01" y2="12"/><line x1="3" y1="18" x2="3.01" y2="18"/></svg>
          </template>
          图片列表 ({{ status?.dataset.samples.length ?? 0 }})
        </NButton>

        <div class="image-title">
          <strong>{{ sample?.name ?? '尚未选择任何图片' }}</strong>
          <small v-if="sample">
            {{ sample.width }} × {{ sample.height }}
            <span v-if="dirty" class="unsaved">· 未保存修改</span>
          </small>
        </div>

        <div v-if="sample" class="image-partition" aria-label="当前图片用途">
          <span style="color: var(--text-muted); font-size: 12px;">数据集归属:</span>
          <div class="partition-pill-group">
            <NButton
              size="tiny"
              :type="sample.split === 'train' ? 'primary' : 'default'"
              :disabled="locked"
              @click="setCurrentPartition('train')"
            >
              训练集
            </NButton>
            <NButton
              size="tiny"
              :type="sample.split === 'val' ? 'primary' : 'default'"
              :disabled="locked"
              @click="setCurrentPartition('val')"
            >
              验证集
            </NButton>
          </div>
        </div>

        <div class="actions" style="margin-left: auto;">
          <NButton
            size="small"
            :disabled="locked || dirty || !sample || status?.dataset.samples[0]?.id === sample.id"
            @click="adjacent(-1)"
          >
            上一张
          </NButton>
          <NButton
            size="small"
            :disabled="locked || dirty || !sample || status?.dataset.samples[(status?.dataset.samples.length ?? 1) - 1]?.id === sample.id"
            @click="adjacent(1)"
          >
            下一张
          </NButton>
        </div>
      </div>

      <!-- Collapsible Library Drawer -->
      <aside v-if="showLibrary || !sample" class="image-library">
        <div class="library-filters">
          <NInput v-model:value="query" placeholder="根据图片名称搜索..." aria-label="搜索图片" clearable size="small"/>
          <NSelect
            v-model:value="filter"
            :options="[
              { label: '全部图片', value: 'all' },
              { label: '待审核', value: 'pending' },
              { label: '已审核', value: 'reviewed' }
            ]"
            aria-label="图片审核状态"
            size="small"
            style="width: 130px;"
          />
        </div>
        <div class="button-sample-list">
          <button
            v-for="item in filtered"
            :key="item.id"
            :disabled="locked"
            :class="{ active: sample?.id === item.id }"
            @click="choose(item.id)"
          >
            <strong>{{ item.name }}</strong>
            <span>
              {{ item.reviewed ? (item.boxes.length ? `${item.boxes.length} 个框` : '无目标按钮') : '待标注' }}
              · {{ item.split === 'val' ? '验证集' : '训练集' }}
            </span>
          </button>
          <p v-if="!filtered.length" class="muted" style="padding: 16px; text-align: center; font-size: 12px;">
            未找到图片。点击上方「模拟器截图」或「导入图片」添加标注素材。
          </p>
        </div>
      </aside>

      <!-- Main Annotation Workspace -->
      <div class="button-workspace">
        <!-- Center Canvas -->
        <section class="button-editor">
          <template v-if="sample">
            <div class="canvas-toolbar">
              <div class="tool-modes">
                <NButton size="small" :type="!drawing ? 'primary' : 'default'" :disabled="busy" @click="drawing = false">
                  选择 / 移动
                </NButton>
                <NButton size="small" :type="drawing ? 'primary' : 'default'" :disabled="!selectedClass || busy" @click="drawing = true">
                  框选按钮
                </NButton>
              </div>

              <NButton size="small" secondary type="info" :disabled="locked" :loading="busy" @click="prelabel">
                模型预标注
              </NButton>

              <span class="toolbar-spacer"/>

              <div class="zoom-controls">
                <NButton size="tiny" :disabled="zoom <= 1" @click="zoom = Math.max(1, zoom - .5)">-</NButton>
                <span class="zoom-label">{{ Math.round(zoom * 100) }}%</span>
                <NButton size="tiny" :disabled="zoom >= 4" @click="zoom += .5">+</NButton>
              </div>
            </div>

            <div class="canvas-viewport">
              <ButtonCanvas
                :image="image"
                :width="sample.width"
                :height="sample.height"
                :boxes="sample.boxes"
                :classes="classes"
                :selected="selected"
                :draw-label="drawing ? selectedClass : undefined"
                :readonly="busy"
                :zoom="zoom"
                @select="selectBox"
                @change="changeBoxes"
              />
            </div>
            <p class="canvas-hint">
              {{ drawing ? '拖动画框以完整框选按钮目标。' : '点击选择检测框，可拖拽移动或在边角缩放调整。' }}
              十字锚点为实际模拟器点击点。
            </p>
          </template>

          <div v-else class="button-empty">
            <svg viewBox="0 0 24 24" width="48" height="48" fill="none" stroke="#94a3b8" stroke-width="1.5"><rect x="3" y="3" width="18" height="18" rx="2" ry="2"/><circle cx="8.5" cy="8.5" r="1.5"/><polyline points="21 15 16 10 5 21"/></svg>
            <h3>选择图片以开始标注</h3>
            <p>从上方截取或导入模拟器截图，然后在右侧创建按钮类别并框选目标。</p>
          </div>
        </section>

        <!-- Right Properties Sidebar -->
        <aside class="button-properties">
          <div class="properties-heading">
            <h3>选择标注类别</h3>
            <NButton text size="small" type="primary" :disabled="!sample || busy" @click="showNewClass = !showNewClass">
              {{ showNewClass ? '取消' : '+ 新增类别' }}
            </NButton>
          </div>

          <NSelect
            v-model:value="selectedClass"
            :options="classOptions"
            placeholder="请选择按钮类别"
            aria-label="按钮类别"
            :disabled="!sample || busy"
            size="small"
          />

          <div v-if="showNewClass" class="new-class-form">
            <label>
              按钮中文名
              <NInput v-model:value="newName" placeholder="例如：挑战" size="small"/>
            </label>
            <label style="margin-top: 6px;">
              唯一英文标识
              <NInput v-model:value="newId" placeholder="例如：challenge" size="small"/>
            </label>
            <NButton type="primary" size="small" style="width: 100%; margin-top: 8px;" :disabled="busy" @click="addClass">
              添加并启用框选
            </NButton>
          </div>

          <p v-if="!classes.length" class="muted" style="font-size: 11.5px; margin-top: 8px;">
            点击「+ 新增类别」创建首个检测目标，或使用「模型预标注」。
          </p>

          <div class="property-section">
            <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 8px;">
              <h3 style="margin: 0; font-size: 13px;">本图标注 · {{ sample?.boxes.length ?? 0 }}</h3>
            </div>

            <div class="box-list">
              <button
                v-for="(box, i) in sample?.boxes"
                :key="i"
                :class="{ active: selected === i }"
                @click="selectBox(i)"
              >
                <span><strong>{{ i + 1 }}.</strong> {{ classes.find(c => c.id === box.label)?.name ?? box.label }}</span>
                <small>{{ Math.round(box.bounds[2] - box.bounds[0]) }} × {{ Math.round(box.bounds[3] - box.bounds[1]) }}</small>
              </button>
            </div>

            <!-- Selected Box Fine-tuning -->
            <template v-if="currentBox && sample">
              <div style="margin-top: 14px; padding-top: 12px; border-top: 1px dashed var(--border-color);">
                <label style="font-size: 12px; color: var(--text-secondary); display: block; margin-bottom: 4px;">
                  选定框的目标类别:
                </label>
                <NSelect
                  :value="currentBox.label"
                  :options="classOptions"
                  size="small"
                  :disabled="busy"
                  @update:value="currentBox.label = $event; selectedClass = $event"
                />

                <div style="margin-top: 10px;">
                  <NButton size="tiny" type="error" tertiary :disabled="busy" @click="changeBoxes(sample.boxes.filter((_, i) => i !== selected))">
                    删除此标注框
                  </NButton>
                </div>

                <details class="advanced-properties" style="margin-top: 12px;">
                  <summary style="font-size: 11.5px; color: var(--primary); cursor: pointer; font-weight: 500;">
                    精确像素边界与点击锚点
                  </summary>
                  <div class="coordinate-pair" style="margin-top: 8px;">
                    <label v-for="(label, i) in ['左 X', '上 Y', '右 X', '下 Y']" :key="label" style="font-size: 11px;">
                      {{ label }}
                      <NInputNumber
                        :value="currentBox.bounds[i]"
                        :min="0"
                        :max="i % 2 ? sample.height : sample.width"
                        :precision="0"
                        size="tiny"
                        :disabled="busy"
                        @update:value="setBound(i, $event)"
                      />
                    </label>
                  </div>

                  <template v-if="currentClass">
                    <p style="font-size: 11px; margin: 10px 0 4px; font-weight: 500; color: var(--text-secondary);">
                      点击点偏移比例 (0.5 为中心):
                    </p>
                    <div class="coordinate-pair">
                      <label style="font-size: 11px;">
                        X 比例
                        <NInputNumber v-model:value="currentClass.anchor[0]" :min="0" :max="1" :step=".05" size="tiny"/>
                      </label>
                      <label style="font-size: 11px;">
                        Y 比例
                        <NInputNumber v-model:value="currentClass.anchor[1]" :min="0" :max="1" :step=".05" size="tiny"/>
                      </label>
                    </div>
                  </template>
                </details>
              </div>
            </template>
            <p v-else class="muted" style="font-size: 11.5px; margin-top: 10px;">
              点击上方框选目标，可在此调整其坐标与点击锚点。
            </p>
          </div>

          <div style="margin-top: auto; padding-top: 14px;">
            <NButton text size="tiny" :disabled="dirty" @click="mode = 'dataset'">
              管理全部类别与用途分配 →
            </NButton>
          </div>
        </aside>
      </div>

      <!-- Sticky Save Bar -->
      <div v-if="sample" class="annotation-savebar">
        <div style="display: flex; align-items: center; gap: 12px;">
          <NCheckbox v-model:checked="negative" :disabled="sample.boxes.length > 0 || busy">
            <span style="font-weight: 500; font-size: 13px;">此图没有目标按钮（负样本）</span>
          </NCheckbox>
          <span class="status-indicator-badge" :class="{ 'is-dirty': dirty }">
            {{ dirty ? '修改尚未保存' : '已就绪' }}
          </span>
        </div>

        <div class="save-actions">
          <NButton :disabled="locked || !dirty" @click="perform(() => load(sample!.id), '已放弃未保存修改')">
            放弃修改
          </NButton>
          <NButton :disabled="locked" @click="save()">
            保存标注
          </NButton>
          <NButton type="primary" :disabled="locked" @click="save(true)">
            保存并下一张
          </NButton>
        </div>
      </div>
    </div>

    <!-- ============================================== -->
    <!-- Mode 2: Dataset & Classes                      -->
    <!-- ============================================== -->
    <div v-show="mode === 'dataset'" class="dataset-page">
      <div class="dataset-summary">
        <div class="summary-card">
          <strong>{{ trainImages }}</strong>
          <span>已审核训练图片</span>
        </div>
        <div class="summary-card">
          <strong>{{ valImages }}</strong>
          <span>已审核验证图片</span>
        </div>
        <div class="summary-card">
          <strong>{{ (status?.dataset.samples.length ?? 0) - reviewed }}</strong>
          <span>待审核图片</span>
        </div>
      </div>

      <NAlert v-if="dirty" type="warning" class="button-feedback">
        当前图片有未保存的标注，请先保存或放弃，再修改数据集。
      </NAlert>
      <NAlert v-if="conflicts.length" type="info" class="button-feedback">
        <strong>已按图片用途划分，可继续训练。</strong>
        {{ conflicts.length }} 个来源批次同时用于训练和验证。同批图片可能比较相似，验证成绩仅供参考。
        <details style="margin-top: 6px;">
          <summary style="cursor: pointer; font-size: 12px;">查看来源批次详情</summary>
          <p v-for="item in conflicts" :key="item" class="batch-code">{{ item }}</p>
        </details>
      </NAlert>

      <!-- Partition Management Section -->
      <section class="dataset-section">
        <div class="section-heading">
          <div>
            <h3>图片用途与归属</h3>
            <p class="muted">调整训练集与验证集分配，支持批量修改。</p>
          </div>
          <NButton text :disabled="locked || dirty" @click="mode = 'annotate'">
            返回继续标注 →
          </NButton>
        </div>

        <div class="selection-toolbar">
          <NCheckbox
            :checked="!!status?.dataset.samples.length && checked.length === status.dataset.samples.length"
            :indeterminate="checked.length > 0 && checked.length !== (status?.dataset.samples.length ?? 0)"
            :disabled="locked || dirty"
            @update:checked="checked = $event ? (status?.dataset.samples.map(s => s.id) ?? []) : []"
          >
            全选
          </NCheckbox>
          <span style="font-size: 12.5px; color: var(--text-secondary);">已选 {{ checked.length }} 张</span>
          <div class="actions" style="margin-left: auto;">
            <NButton size="small" :disabled="locked || dirty || !checked.length" @click="setPartition(checked, 'train')">
              选中图片设为训练集
            </NButton>
            <NButton size="small" :disabled="locked || dirty || !checked.length" @click="setPartition(checked, 'val')">
              选中图片设为验证集
            </NButton>
          </div>
        </div>

        <div class="dataset-table-scroll">
          <table class="button-table partition-table">
            <thead>
              <tr>
                <th style="width: 44px;">选择</th>
                <th>图片 / 批次来源</th>
                <th style="width: 130px;">审核状态</th>
                <th style="width: 140px;">本图用途</th>
                <th style="width: 120px;">操作</th>
              </tr>
            </thead>
            <tbody>
              <template v-for="item in status?.dataset.samples" :key="item.id">
                <tr>
                  <td>
                    <NCheckbox
                      :checked="checked.includes(item.id)"
                      :disabled="locked || dirty"
                      :aria-label="`选择 ${item.name}`"
                      @update:checked="toggleChecked(item.id, $event)"
                    />
                  </td>
                  <td>
                    <div class="dataset-image">
                      <button class="preview-image" :disabled="locked || dirty" :aria-label="`查看 ${item.name} 大图`" @click="inspectSample(item.id)">
                        <ButtonThumbnail :id="item.id" :name="item.name"/>
                        <span>查看大图 / 标注</span>
                      </button>
                      <div>
                        <strong>{{ item.name }}</strong>
                        <small class="batch-code" :title="item.batch">{{ item.batch }}</small>
                      </div>
                    </div>
                  </td>
                  <td>
                    <NTag :type="item.reviewed ? 'success' : 'default'" size="small" round>
                      {{ item.reviewed ? `${item.boxes.length} 框 · 已审核` : '待审核' }}
                    </NTag>
                  </td>
                  <td>
                    <NSelect
                      :value="item.split"
                      :options="splitOptions"
                      :disabled="locked || dirty"
                      size="small"
                      :aria-label="`${item.name} 的用途`"
                      @update:value="setPartition([item.id], $event)"
                    />
                  </td>
                  <td>
                    <div class="actions">
                      <NButton text size="small" :disabled="locked || dirty" @click="editMeta(item)">编辑来源</NButton>
                      <NButton text size="small" type="error" :disabled="locked || dirty" @click="removeSample(item.id)">移出</NButton>
                    </div>
                  </td>
                </tr>

                <tr v-if="editingMeta === item.id">
                  <td colspan="5">
                    <div class="source-editor">
                      <label>
                        此图片的真实来源批次
                        <NInput v-model:value="metaBatch" size="small"/>
                      </label>
                      <NButton size="small" type="primary" :disabled="locked || dirty || !metaBatch.trim()" @click="saveMeta">
                        保存此图片
                      </NButton>
                      <NButton text size="small" @click="editingMeta = ''">取消</NButton>
                      <p class="muted">来源批次仅记录素材采集关联，不强制限制用途划分。</p>
                    </div>
                  </td>
                </tr>
              </template>
            </tbody>
          </table>
        </div>
        <p v-if="!status?.dataset.samples.length" class="empty-inline">
          尚无素材数据，请先在「标注图片」中截取或导入素材。
        </p>
      </section>

      <!-- Classes Management Section -->
      <section class="dataset-section">
        <div class="section-heading">
          <div>
            <h3>按钮检测类别</h3>
            <p class="muted">管理模型检测目标名称、标识与锚点，类别更新将自动同步已有框。</p>
          </div>
          <NButton v-if="unused.length" size="small" :disabled="locked || dirty" @click="pruneClasses">
            清理 {{ unused.length }} 个空类别
          </NButton>
        </div>

        <div class="dataset-table-scroll">
          <table class="button-table">
            <thead>
              <tr>
                <th>类别名称 / 英文标识</th>
                <th>训练集框数</th>
                <th>验证集框数</th>
                <th>数据准备状态</th>
                <th style="width: 120px;">操作</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="item in counts" :key="item.id">
                <td>
                  <strong>{{ item.name }}</strong>
                  <small>{{ item.id }}</small>
                </td>
                <td>{{ item.train }}</td>
                <td>{{ item.val }}</td>
                <td>
                  <span :class="item.train && item.val ? 'state-ok' : 'state-missing'">
                    {{ countState(item) }}
                  </span>
                </td>
                <td class="class-actions">
                  <NButton text size="small" :disabled="locked || dirty" @click="editClass(item)">修改</NButton>
                  <NButton text size="small" type="error" :disabled="locked || dirty" @click="askDeleteClass(item.id)">删除</NButton>
                </td>
              </tr>
            </tbody>
          </table>
        </div>
        <p v-if="!counts.length" class="empty-inline">尚无类别，在「标注图片」时新增。</p>
      </section>
    </div>

    <!-- ============================================== -->
    <!-- Mode 3: Train Button Model                     -->
    <!-- ============================================== -->
    <div v-show="mode === 'train'" class="button-training">
      <!-- Training Readiness Check -->
      <section class="panel">
        <div class="panel-heading">
          <div>
            <h3>数据准备就绪状态</h3>
            <p class="panel-desc">勾选本次训练类别，仅影响训练快照，保留原标注。所选类别都需要训练框和验证框；最低数量满足不代表质量足够。</p>
          </div>
        </div>

        <table class="button-table">
          <thead>
            <tr>
              <th>按钮</th>
              <th>训练框数</th>
              <th>验证框数</th>
              <th>状态</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="item in counts" :key="item.id">
              <td><NCheckbox :checked="!excludedClasses.includes(item.id)" :disabled="running" @update:checked="v => excludedClasses = v ? excludedClasses.filter(id => id !== item.id) : [...excludedClasses, item.id]">{{ item.name }}</NCheckbox><small>{{ item.id }}</small></td>
              <td>{{ item.train }}</td>
              <td>{{ item.val }}</td>
              <td>
                <span :class="item.train && item.val ? 'state-ok' : 'state-missing'">
                  {{ countState(item) }}
                </span>
              </td>
            </tr>
          </tbody>
        </table>

        <p v-if="!counts.length" class="muted" style="margin-top: 14px;">
          先在「标注图片」中添加按钮类别、框选目标并保存。
        </p>

        <div style="margin-top: 16px;">
          <NAlert v-if="dirty" type="warning" class="feedback">当前标注尚未保存，请保存后再执行训练。</NAlert>
          <NAlert v-if="conflicts.length" type="info" class="feedback">允许同批图片分别用于训练和验证；图片可能相似，验证成绩仅供参考。</NAlert>
        </div>

        <div style="margin-top: 14px; display: flex; justify-content: space-between; align-items: center;">
          <NButton text @click="mode = 'dataset'">调整图片用途与按钮类别 →</NButton>
          <span class="muted" style="font-size: 11.5px;">
            {{ status?.dataset.samples.filter(s => s.reviewed && !s.boxes.length).length ?? 0 }} 张已确认为负样本
          </span>
        </div>
      </section>

      <!-- Training Job Execution -->
      <section class="panel">
        <div class="panel-heading">
          <div>
            <h3>训练独立按钮检测模型</h3>
            <p class="panel-desc">使用轻量化高精度 YOLO 目标检测架构</p>
          </div>
        </div>

        <div class="form-grid">
          <label style="font-size: 12.5px; color: var(--text-secondary);">
            计算加速设备
            <NSelect
              v-model:value="device"
              :disabled="running"
              :options="[
                { label: '自动（优先 GPU）', value: 'auto' },
                { label: 'GPU / CUDA', value: 'cuda' },
                { label: 'CPU', value: 'cpu' }
              ]"
              style="margin-top: 4px;"
            />
          </label>
          <label style="font-size: 12.5px; color: var(--text-secondary);">
            训练轮数 (Epochs)
            <NInputNumber
              v-model:value="epochs"
              :min="1"
              :max="300"
              :precision="0"
              :disabled="running"
              style="margin-top: 4px;"
            />
          </label>
        </div>

        <div class="form-grid" style="margin-top:12px">
          <label>输入分辨率<NSelect v-model:value="imgsz" :disabled="running" :options="[640,960,1280].map(value => ({label: `${value} × ${value}`, value}))" /></label>
          <label>早停耐心轮数<NInputNumber v-model:value="patience" :min="1" :max="100" :precision="0" :disabled="running" /></label>
        </div>
        <p class="muted">较高分辨率消耗更多显存，不保证效果更好。连续指定轮数无提升会提前停止。</p>
        <div class="button-toolbar" style="margin-top: 18px;">
          <NButton
            type="primary"
            :disabled="locked || running || !status?.available || !ready || dirty"
            @click="startTraining()"
          >
            开始训练
          </NButton>
          <NButton v-if="running" :disabled="locked" @click="perform(buttonApi.cancel, '训练已取消')">
            取消训练
          </NButton>
        </div>

        <NAlert v-if="status && !status.available" type="warning" style="margin-top: 14px;">
          未检测到 Python 训练环境，请在项目虚拟开发环境中执行训练。
        </NAlert>

        <div v-if="status?.job" class="property-section" aria-live="polite">
          <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 8px;">
            <h3 style="margin: 0; font-size: 14px;">
              {{ ({ running: '训练进行中', completed: '训练完成', failed: '训练失败', cancelled: '已取消', interrupted: '上次训练已中断' } as Record<string, string>)[status.job.status] }}
            </h3>
            <span class="muted" style="font-size: 12px;">
              {{ status.job.progress?.stage }}
              <template v-if="status.job.progress?.epoch">
                ({{ status.job.progress.epoch }} / {{ status.job.progress.epochs }} 轮)
              </template>
            </span>
          </div>

          <p v-if="status.job.progress?.device_name" class="muted" style="font-size: 11.5px; margin-bottom: 6px;">
            计算设备: {{ status.job.progress.device_name }}
          </p>

          <NProgress
            type="line"
            :percentage="Math.round((status.job.progress?.progress ?? 0) * 100)"
            :status="status.job.status === 'failed' ? 'error' : 'default'"
            color="#2563eb"
          />

          <NAlert v-if="status.job.progress?.error" type="error" style="margin-top: 12px;">
            {{ status.job.progress.error }}
          </NAlert>

          <div v-if="status.job.result && status.job.status === 'completed'" class="button-metrics">
            <p style="margin: 0 0 6px;">
              mAP@50 <strong>{{ (status.job.result.map50 * 100).toFixed(1) }}%</strong>
              · mAP@50–95 {{ (status.job.result.map50_95 * 100).toFixed(1) }}%
            </p>
            <p class="muted" style="font-size: 11.5px;">衡量检测框位置与类别准确率。启用后请到「识别测试」中核对。</p>
            <div style="display: flex; gap: 8px; flex-wrap: wrap; margin-top: 10px;">
              <NTag v-for="item in status.job.result.per_class" :key="item.id" size="small" type="info">
                {{ item.name }}: {{ (item.map50 * 100).toFixed(1) }}%
              </NTag>
            </div>
            <div style="margin-top: 16px;">
              <NButton type="primary" :disabled="locked" @click="mode = 'test'">
                选择候选模型并验收
              </NButton>
            </div>
          </div>

          <details style="margin-top: 14px;">
            <summary style="cursor: pointer; font-size: 12px; color: var(--primary);">查看训练终端日志</summary>
            <pre class="studio-log-pre">{{ status.job.log || '等待输出…' }}</pre>
          </details>
        </div>
      </section>
    </div>

    <!-- ============================================== -->
    <!-- Mode 4: Test & Inference                       -->
    <!-- ============================================== -->
    <div v-show="mode === 'test'" class="button-test-wrapper">
      <ButtonModelReview v-if="mode === 'test' && status" :dataset="status.dataset" :checked="checked" :dirty="dirty" :disabled="locked" @annotate="inspectSample" @activated="refresh" @candidate="id => { testJob = id; result = null; }" />
      <div class="studio-toolbar panel">
        <div class="actions">
          <NButton
            type="primary"
            :disabled="locked || store.running || !store.config?.serial"
            @click="capture(false)"
          >
            截图并识别
          </NButton>
          <NButton :disabled="locked" @click="testInput?.click()">
            导入测试图片
          </NButton>
          <NButton :disabled="locked || !sample" @click="testImage = image; testSize = [sample!.width, sample!.height]; result = null">
            使用当前标注图片
          </NButton>
        </div>

        <div class="threshold-input" style="margin-left: auto;">
          <span>置信度阈值:</span>
          <NInputNumber v-model:value="threshold" :min=".05" :max="1" :step=".05" size="small" style="width: 100px;"/>
          <NButton :disabled="locked || !testImage" size="small" @click="perform(recognize)">
            重新识别
          </NButton>
        </div>
      </div>

      <div class="button-test-grid">
        <!-- Canvas Viewport -->
        <section class="panel" style="margin: 0;">
          <div class="canvas-viewport" style="min-height: 420px; display: grid; place-items: center;">
            <ButtonCanvas
              v-if="testImage"
              :image="testImage"
              :width="testSize[0]"
              :height="testSize[1]"
              :boxes="result?.items ?? []"
              :classes="result?.classes ?? []"
              :selected="testSelected"
              readonly
              :zoom="1"
              @select="testSelected = $event"
            />
            <div v-else class="button-empty">
              <svg viewBox="0 0 24 24" width="48" height="48" fill="none" stroke="#94a3b8" stroke-width="1.5"><circle cx="11" cy="11" r="8"/><line x1="21" y1="21" x2="16.65" y2="16.65"/></svg>
              <h3>实机或图片识别验证</h3>
              <p>截取模拟器实时画面或导入图片，模型将检测按钮位置并输出原图点击坐标。</p>
            </div>
          </div>
          <div style="display: flex; justify-content: space-between; margin-top: 10px; font-size: 11.5px; color: var(--text-muted);">
            <span>测试图尺寸: {{ testSize[0] }} × {{ testSize[1] }} px</span>
            <span>左上角为 (0, 0) · 识别测试仅输出坐标</span>
          </div>
        </section>

        <!-- Recognition Results Inspector -->
        <aside class="panel" style="margin: 0;">
          <div class="panel-heading">
            <h3 style="margin: 0; font-size: 15px;">
              检测结果
              <span v-if="result" class="muted" style="font-size: 13px;">({{ result.items.length }})</span>
            </h3>
            <NTag v-if="result" :type="result.compute.provider === 'CUDA' ? 'success' : 'warning'" size="small" round>
              {{ result.compute.provider }}
            </NTag>
          </div>

          <template v-if="result">
            <p class="muted" style="font-size: 11.5px; margin: 4px 0 12px;">
              推理 {{ result.timings.inference_ms.toFixed(1) }} ms · 总耗时 {{ result.timings.total_ms.toFixed(0) }} ms
            </p>
            <NAlert v-if="result.compute.fallback_reason" type="warning" style="margin-bottom: 12px;">
              {{ result.compute.fallback_reason }}
            </NAlert>

            <p v-if="!result.items.length" class="muted" style="padding: 16px 0; text-align: center; font-size: 12px;">
              未检测到达到阈值的按钮。可调低阈值或转为标注素材。
            </p>

            <div style="display: flex; flex-direction: column; gap: 8px; max-height: 400px; overflow: auto;">
              <div
                v-for="(item, i) in result.items"
                :key="i"
                class="detection-card"
                :class="{ active: testSelected === i }"
              >
                <button class="detection-btn" @click="testSelected = i">
                  <div style="display: flex; justify-content: space-between; align-items: center;">
                    <strong>{{ item.name }}</strong>
                    <NTag size="tiny" type="info">{{ (item.confidence * 100).toFixed(1) }}%</NTag>
                  </div>
                  <code>点击锚点: ({{ item.point[0] }}, {{ item.point[1] }})</code>
                </button>
                <div style="display: flex; justify-content: space-between; align-items: center; margin-top: 6px;">
                  <small class="muted">边界: [{{ item.bounds.map(v => Math.round(v)).join(', ') }}]</small>
                  <NButton size="tiny" tertiary :disabled="locked" @click="copyPoint(item.point)">
                    复制坐标
                  </NButton>
                </div>
              </div>
            </div>
          </template>

          <p v-else class="muted" style="padding: 30px 0; text-align: center; font-size: 12.5px;">
            点击上方按钮截取画面并运行识别检测。
          </p>

          <div style="margin-top: 18px;">
            <NButton block :disabled="locked || !testImage || dirty" @click="toAnnotation">
              转为标注素材补充训练
            </NButton>
          </div>
        </aside>
      </div>
    </div>

    <!-- Modals -->
    <NModal
      v-model:show="classModal"
      preset="card"
      title="修改按钮类别"
      class="category-modal"
      :mask-closable="false"
      :style="{ width: '420px', maxWidth: 'calc(100vw - 32px)' }"
    >
      <NAlert v-if="classError" type="error" style="margin-bottom: 12px;">{{ classError }}</NAlert>
      <label style="display: block; margin-bottom: 10px; font-size: 12.5px;">
        显示名称
        <NInput v-model:value="classDraft.name" :disabled="busy" style="margin-top: 4px;"/>
      </label>
      <label style="display: block; margin-bottom: 10px; font-size: 12.5px;">
        唯一标识
        <NInput v-model:value="classDraft.id" :disabled="busy" style="margin-top: 4px;"/>
      </label>
      <p class="muted" style="font-size: 11.5px;">修改标识将自动同步所有标注图片中对应类别的标注框。</p>
      <div class="coordinate-pair" style="margin-top: 12px;">
        <label style="font-size: 12px;">
          点击点 X (比例)
          <NInputNumber v-model:value="classDraft.anchor[0]" :min="0" :max="1" :step=".05" :disabled="busy" style="margin-top: 4px;"/>
        </label>
        <label style="font-size: 12px;">
          点击点 Y (比例)
          <NInputNumber v-model:value="classDraft.anchor[1]" :min="0" :max="1" :step=".05" :disabled="busy" style="margin-top: 4px;"/>
        </label>
      </div>
      <div class="modal-actions" style="margin-top: 20px; display: flex; justify-content: flex-end; gap: 10px;">
        <NButton :disabled="busy" @click="classModal = false">取消</NButton>
        <NButton type="primary" :loading="busy" @click="saveClass">保存类别修改</NButton>
      </div>
    </NModal>

    <NModal
      v-model:show="deleteModal"
      preset="card"
      title="删除按钮类别"
      :mask-closable="false"
      :style="{ width: '420px', maxWidth: 'calc(100vw - 32px)' }"
    >
      <NAlert v-if="classError" type="error" style="margin-bottom: 12px;">{{ classError }}</NAlert>
      <p style="font-size: 13.5px; line-height: 1.6;">
        即将删除 <strong>{{ deleteClassId }}</strong> 类别及其
        <strong style="color: var(--danger)">{{ deleteImpact.boxes }} 个标注框</strong>，
        将影响 {{ deleteImpact.images }} 张图片。
      </p>
      <p class="muted" style="font-size: 12px;">
        原图素材保留，受影响图片将恢复为待审核状态。
      </p>
      <div class="modal-actions" style="margin-top: 20px; display: flex; justify-content: flex-end; gap: 10px;">
        <NButton :disabled="busy" @click="deleteModal = false">取消</NButton>
        <NButton type="error" :loading="busy" @click="deleteClass">确认删除类别及标注</NButton>
      </div>
    </NModal>
  </section>
</template>

<style scoped>
.button-studio {
  min-width: 0;
}

.studio-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  margin-bottom: 18px;
}
.studio-header h2 {
  font-size: 20px;
  font-weight: 700;
  margin: 0 0 4px;
  color: var(--text-main);
}
.studio-header p {
  margin: 0;
  font-size: 13px;
}

/* Step Bar */
.studio-steps-container {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  margin-bottom: 20px;
  flex-wrap: wrap;
}

.button-steps {
  display: flex;
  gap: 4px;
  background: var(--bg-surface);
  padding: 4px;
  border-radius: var(--radius-md);
  border: 1px solid var(--border-color);
  box-shadow: var(--shadow-xs);
}

.button-steps button {
  border: 0;
  border-radius: var(--radius-sm);
  background: transparent;
  padding: 8px 18px;
  cursor: pointer;
  font-size: 13px;
  font-weight: 500;
  color: var(--text-secondary);
  transition: all 0.15s ease;
}

.button-steps button:hover {
  background: var(--bg-hover);
  color: var(--text-main);
}

.button-steps button.active {
  background: var(--primary);
  color: white;
  box-shadow: 0 2px 4px rgba(37, 99, 235, 0.25);
}

.steps-progress-pill {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 6px 14px;
  background: var(--bg-surface);
  border-radius: 999px;
  border: 1px solid var(--border-color);
  font-size: 12px;
}
.steps-progress-pill strong {
  color: var(--primary);
}

.button-feedback {
  margin-bottom: 16px;
}

/* Toolbar */
.studio-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 12px 18px;
  margin-bottom: 12px;
  flex-wrap: wrap;
}

.capture-actions {
  display: flex;
  gap: 10px;
  align-items: center;
  flex-wrap: wrap;
}

.capture-settings-card {
  padding: 14px 18px;
  background: var(--bg-subtle);
  border-radius: var(--radius-md);
  border: 1px solid var(--border-subtle);
  display: flex;
  align-items: flex-end;
  gap: 14px;
  flex-wrap: wrap;
  margin-bottom: 14px;
}

/* Material Picker Grid */
.button-picker {
  margin-bottom: 16px;
  padding: 16px;
}
.picker-grid {
  display: grid;
  grid-template-columns: repeat(4, 1fr);
  gap: 12px;
}
.picker-grid button {
  border: 1px solid var(--border-color);
  border-radius: var(--radius-sm);
  padding: 0;
  background: var(--bg-subtle);
  cursor: pointer;
  overflow: hidden;
  transition: transform 0.1s ease, border-color 0.1s ease;
}
.picker-grid button:hover {
  border-color: var(--primary);
  transform: translateY(-1px);
}
.picker-grid img {
  width: 100%;
  aspect-ratio: 16/9;
  object-fit: cover;
  display: block;
}
.picker-grid span {
  display: block;
  padding: 6px;
  font-size: 11px;
  color: var(--text-secondary);
}

/* Image Navigation Bar */
.image-navigation {
  background: white;
  border: 1px solid var(--border-color);
  border-radius: var(--radius-md) var(--radius-md) 0 0;
  padding: 12px 16px;
  display: flex;
  align-items: center;
  gap: 14px;
  flex-wrap: wrap;
}

.image-title {
  min-width: 0;
}
.image-title strong {
  display: block;
  font-size: 13.5px;
  color: var(--text-main);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.image-title small {
  font-size: 11.5px;
  color: var(--text-muted);
}
.unsaved {
  color: var(--warning);
  font-weight: 600;
}

.image-partition {
  display: flex;
  align-items: center;
  gap: 8px;
}
.partition-pill-group {
  display: flex;
  gap: 4px;
}

/* Library Drawer */
.image-library {
  padding: 14px;
  background: var(--bg-subtle);
  border: 1px solid var(--border-color);
  border-top: 0;
}
.library-filters {
  display: flex;
  gap: 10px;
  margin-bottom: 12px;
}
.button-sample-list {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
  gap: 8px;
  max-height: 220px;
  overflow: auto;
}
.button-sample-list button {
  padding: 10px 12px;
  background: white;
  border: 1px solid var(--border-color);
  border-radius: var(--radius-sm);
  text-align: left;
  cursor: pointer;
  transition: all 0.15s ease;
}
.button-sample-list button:hover {
  border-color: var(--primary);
}
.button-sample-list button.active {
  border-color: var(--primary);
  background: var(--primary-subtle);
  color: var(--primary);
}
.button-sample-list strong {
  display: block;
  font-size: 12px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.button-sample-list span {
  display: block;
  font-size: 11px;
  color: var(--text-muted);
  margin-top: 4px;
}

/* Workspace Layout */
.button-workspace {
  display: grid;
  grid-template-columns: minmax(0, 1fr) 280px;
  background: white;
  border: 1px solid var(--border-color);
  border-top: 0;
  border-radius: 0 0 var(--radius-md) var(--radius-md);
  align-items: stretch;
}

.button-editor {
  padding: 14px 16px;
  min-width: 0;
}

.canvas-toolbar {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-bottom: 12px;
  flex-wrap: wrap;
}

.tool-modes {
  display: flex;
  gap: 4px;
}

.toolbar-spacer {
  flex: 1;
}

.zoom-controls {
  display: flex;
  align-items: center;
  gap: 6px;
}
.zoom-label {
  font-size: 12px;
  font-variant-numeric: tabular-nums;
  color: var(--text-muted);
  min-width: 38px;
  text-align: center;
}

.canvas-viewport {
  background: #0b1120;
  border-radius: var(--radius-md);
  overflow: hidden;
  border: 1px solid #1e293b;
  box-shadow: inset 0 2px 4px rgba(0, 0, 0, 0.4);
}

.canvas-hint {
  font-size: 11.5px;
  color: var(--text-muted);
  margin: 8px 0 0;
}

.button-empty {
  min-height: 380px;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  text-align: center;
  padding: 30px;
  color: var(--text-muted);
}
.button-empty h3 {
  font-size: 16px;
  font-weight: 600;
  color: var(--text-main);
  margin: 12px 0 4px;
}
.button-empty p {
  font-size: 12.5px;
  margin: 0;
}

/* Properties Sidebar */
.button-properties {
  border-left: 1px solid var(--border-color);
  padding: 16px;
  display: flex;
  flex-direction: column;
  background: #fafbfc;
  max-height: 720px;
  overflow: auto;
}

.properties-heading {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 10px;
}
.properties-heading h3 {
  font-size: 13.5px;
  font-weight: 600;
  margin: 0;
  color: var(--text-main);
}

.new-class-form {
  padding: 12px;
  background: white;
  border: 1px solid var(--border-color);
  border-radius: var(--radius-sm);
  margin-top: 8px;
}
.new-class-form label {
  font-size: 11.5px;
  color: var(--text-secondary);
  display: block;
}

.property-section {
  margin-top: 18px;
  padding-top: 14px;
  border-top: 1px solid var(--border-color);
}

.box-list {
  display: flex;
  flex-direction: column;
  gap: 6px;
  max-height: 180px;
  overflow: auto;
}
.box-list button {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px 10px;
  background: white;
  border: 1px solid var(--border-color);
  border-radius: var(--radius-sm);
  cursor: pointer;
  text-align: left;
  font-size: 12px;
  transition: all 0.12s ease;
}
.box-list button:hover {
  border-color: var(--primary);
}
.box-list button.active {
  background: var(--primary-subtle);
  border-color: var(--primary);
  color: var(--primary);
  font-weight: 600;
}
.box-list small {
  color: var(--text-muted);
  font-size: 10.5px;
}

.coordinate-pair {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 8px;
}

/* Savebar */
.annotation-savebar {
  position: sticky;
  bottom: 16px;
  z-index: 30;
  margin-top: 16px;
  background: white;
  border: 1px solid var(--border-color);
  border-radius: var(--radius-md);
  padding: 12px 18px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  box-shadow: var(--shadow-lg);
  flex-wrap: wrap;
  gap: 12px;
}

.status-indicator-badge {
  font-size: 11.5px;
  padding: 2px 8px;
  border-radius: 999px;
  background: var(--bg-hover);
  color: var(--text-muted);
}
.status-indicator-badge.is-dirty {
  background: #fffbeb;
  color: var(--warning);
  font-weight: 600;
}

.save-actions {
  display: flex;
  gap: 8px;
}

/* Dataset Page */
.dataset-summary {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 16px;
  margin-bottom: 20px;
}

.summary-card {
  padding: 16px 20px;
  background: white;
  border: 1px solid var(--border-color);
  border-radius: var(--radius-md);
  box-shadow: var(--shadow-xs);
}
.summary-card strong {
  display: block;
  font-size: 26px;
  font-weight: 700;
  color: var(--primary);
  line-height: 1.2;
}
.summary-card span {
  font-size: 12.5px;
  color: var(--text-muted);
  margin-top: 4px;
  display: block;
}

.dataset-section {
  background: white;
  border: 1px solid var(--border-color);
  border-radius: var(--radius-lg);
  padding: 20px;
  margin-bottom: 20px;
  box-shadow: var(--shadow-xs);
}

.section-heading {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 14px;
}
.section-heading h3 {
  font-size: 15px;
  font-weight: 600;
  margin: 0;
  color: var(--text-main);
}
.section-heading p {
  margin: 3px 0 0;
}

.selection-toolbar {
  display: flex;
  align-items: center;
  gap: 14px;
  padding: 10px 14px;
  background: var(--bg-subtle);
  border-radius: var(--radius-sm);
  margin-bottom: 12px;
}

.dataset-table-scroll {
  overflow-x: auto;
}

.button-table {
  width: 100%;
  border-collapse: collapse;
  font-size: 12.5px;
}
.button-table th {
  text-align: left;
  padding: 10px 12px;
  color: var(--text-muted);
  font-weight: 500;
  border-bottom: 1px solid var(--border-color);
  white-space: nowrap;
}
.button-table td {
  padding: 12px;
  border-bottom: 1px solid var(--border-subtle);
  vertical-align: middle;
}
.button-table small {
  display: block;
  font-size: 11px;
  color: var(--text-muted);
  margin-top: 2px;
}

.dataset-image {
  display: flex;
  align-items: center;
  gap: 12px;
}

.preview-image {
  border: 0;
  background: transparent;
  padding: 0;
  cursor: pointer;
  color: var(--primary);
  font-size: 11px;
  text-align: center;
}

.batch-code {
  font-family: var(--font-mono);
  font-size: 11px;
  color: var(--text-faint);
}

.source-editor {
  padding: 12px;
  background: var(--bg-subtle);
  border-radius: var(--radius-sm);
  display: flex;
  gap: 12px;
  align-items: center;
  flex-wrap: wrap;
}

.state-missing {
  color: var(--warning);
  font-weight: 600;
}
.state-ok {
  color: var(--success);
  font-weight: 600;
}

.empty-inline {
  padding: 24px 0;
  text-align: center;
  color: var(--text-muted);
  font-size: 12.5px;
}

/* Training Grid */
.button-training {
  display: grid;
  grid-template-columns: minmax(0, 1.2fr) minmax(280px, 1fr);
  gap: 20px;
}

.button-toolbar {
  display: flex;
  gap: 10px;
  align-items: center;
}

.button-metrics strong {
  font-size: 22px;
  color: var(--primary);
}

.studio-log-pre {
  font-family: var(--font-mono);
  font-size: 11px;
  background: var(--bg-subtle);
  padding: 12px;
  border-radius: var(--radius-sm);
  border: 1px solid var(--border-subtle);
  max-height: 220px;
  overflow: auto;
  white-space: pre-wrap;
  margin-top: 8px;
}

/* Test Grid */
.button-test-grid {
  display: grid;
  grid-template-columns: minmax(0, 1fr) 300px;
  gap: 20px;
  align-items: start;
}

.threshold-input {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 12.5px;
}

.detection-card {
  padding: 10px 12px;
  background: var(--bg-subtle);
  border-radius: var(--radius-sm);
  border: 1px solid var(--border-subtle);
  transition: all 0.15s ease;
}
.detection-card:hover {
  background: white;
  border-color: var(--primary);
}
.detection-card.active {
  background: white;
  border-color: var(--primary);
  box-shadow: 0 0 0 2px var(--primary-subtle);
}

.detection-btn {
  width: 100%;
  border: 0;
  background: transparent;
  padding: 0;
  text-align: left;
  cursor: pointer;
}
.detection-btn code {
  display: block;
  font-family: var(--font-mono);
  font-size: 12px;
  color: var(--primary);
  margin-top: 4px;
}

@media (max-width: 1000px) {
  .button-workspace, .button-training, .button-test-grid {
    grid-template-columns: 1fr;
  }
  .button-properties {
    border-left: 0;
    border-top: 1px solid var(--border-color);
  }
}
</style>
