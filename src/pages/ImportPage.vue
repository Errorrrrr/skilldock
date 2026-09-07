<script setup lang="ts">
import AppSelect from '@/components/ui/AppSelect.vue'
import { computed, ref, watch } from 'vue'
import { useRouter } from 'vue-router'
import {
  Check,
  FolderSearch,
  ScanSearch,
  ShieldAlert,
  Link2,
  RotateCcw,
  CircleCheck,
  CircleX,
} from 'lucide-vue-next'
import Button from '@/components/ui/Button.vue'
import Badge from '@/components/ui/Badge.vue'
import DirectoryField from '@/components/DirectoryField.vue'
import { api } from '@/services/api'
import { useAppStore } from '@/stores/app'
import type { ScanItem, Target } from '@/services/types'

const app = useAppStore()
const router = useRouter()
const step = ref(1)
const rootPath = ref(app.isNative ? '' : '/Users/demo/SkillDock')
const configuring = ref(false)
const candidates = ref<Target[]>([])
const scanPaths = ref<string[]>([])
const customPath = ref(app.isNative ? '' : '/Users/demo/.agents/skills')
const scanItems = ref<ScanItem[]>([])
const warnings = ref<string[]>([])
const selectedPaths = ref<string[]>([])
const resolutions = ref<Record<string, 'rename' | 'keep' | 'skip'>>({})
const adopt = ref(false)
const busy = ref(false)
const complete = ref(false)
const selectable = (item: ScanItem) => ['ready', 'new', 'same', 'conflict'].includes(item.status)
const selectedItems = computed(() =>
  scanItems.value.filter(
    (item) =>
      selectedPaths.value.includes(item.path) &&
      resolutions.value[item.path] !== 'skip' &&
      selectable(item),
  ),
)
const conflictsOpen = computed(() =>
  selectedItems.value.some((item) => item.status === 'conflict' && !resolutions.value[item.path]),
)
const status = (value: string) =>
  (({
    ready: ['可归集', 'blue'],
    linked: ['已有软链', 'neutral'],
    invalid: ['格式无效', 'red'],
    new: ['新内容', 'blue'],
    same: ['内容相同', 'green'],
    conflict: ['同名冲突', 'red'],
    broken: ['断链', 'amber'],
    external: ['外部管理', 'neutral'],
  })[value] || [value, 'neutral']) as [string, 'blue' | 'green' | 'red' | 'amber' | 'neutral']
async function configure() {
  if (!rootPath.value.trim()) return
  configuring.value = true
  const ok = await app.mutate(() => api.configure(rootPath.value), '统一存储目录已配置')
  configuring.value = false
  if (ok) await loadCandidates()
}
async function loadCandidates() {
  busy.value = true
  try {
    candidates.value = await api.discover()
    scanPaths.value = candidates.value.map((item) => item.path)
    if (!scanPaths.value.length && !app.isNative) scanPaths.value = ['/Users/demo/.codex/skills']
  } catch (e) {
    app.error = e instanceof Error ? e.message : '无法探测工具目录'
  } finally {
    busy.value = false
  }
}
function addCustom() {
  if (!customPath.value || scanPaths.value.includes(customPath.value)) return
  scanPaths.value.push(customPath.value)
  customPath.value = ''
}
function toggleReviewAll(event: Event) {
  const checked = (event.target as HTMLInputElement).checked
  selectedPaths.value = checked
    ? scanItems.value.filter((item) => selectable(item)).map((item) => item.path)
    : []
}
async function scan() {
  if (!scanPaths.value.length) return
  busy.value = true
  const items: ScanItem[] = []
  const notices: string[] = []
  try {
    for (const path of scanPaths.value) {
      const result = await api.scan(path)
      items.push(...result.items)
      notices.push(...result.warnings)
    }
    scanItems.value = items
    warnings.value = notices
    selectedPaths.value = items
      .filter((item) => selectable(item) && item.status !== 'broken')
      .map((item) => item.path)
    step.value = 2
  } catch (e) {
    app.error = e instanceof Error ? e.message : '扫描失败'
  } finally {
    busy.value = false
  }
}
function nextReview() {
  if (conflictsOpen.value) return
  step.value = 3
}
async function execute() {
  busy.value = true
  const grouped = new Map<string, string[]>()
  for (const item of selectedItems.value) {
    const root =
      [...scanPaths.value]
        .sort((a, b) => b.length - a.length)
        .find(
          (path) =>
            item.path === path ||
            item.path.startsWith(path.replace(/[\\/]+$/, '') + (path.includes('\\') ? '\\' : '/')),
        ) || scanPaths.value[0]
    grouped.set(root, [...(grouped.get(root) || []), item.path])
  }
  let ok = true
  for (const [path, paths] of grouped) {
    ok = await app.mutate(
      () => api.importFolder(path, paths, adopt.value),
      `已归集 ${selectedItems.value.length} 个 Skill`,
    )
    if (!ok) break
  }
  busy.value = false
  if (ok) {
    complete.value = true
    step.value = 4
  }
}
let candidatesLoaded = false
watch(
  () => app.snapshot,
  (snapshot) => {
    if (!snapshot) return
    if (!rootPath.value && snapshot.storageRoot) rootPath.value = snapshot.storageRoot
    if (snapshot.initialized && !candidatesLoaded) {
      candidatesLoaded = true
      loadCandidates()
    }
  },
  { immediate: true },
)
function restartScan() {
  scanItems.value = []
  step.value = 1
  complete.value = false
  loadCandidates()
}

const resolutionOptions = [
  { value: '', label: '请选择', disabled: true },
  { value: 'keep', label: '保留不同版本' },
  { value: 'skip', label: '跳过' },
]
</script>

<template>
  <div class="page">
    <header class="page-heading">
      <div>
        <h1 class="page-title">归集已有 Skill</h1>
        <p class="page-subtitle">扫描散落目录，审阅内容关系，再决定是否把原位置替换为软链。</p>
      </div>
      <Button v-if="app.snapshot?.initialized" @click="restartScan"><ScanSearch />重新开始</Button>
    </header>
    <section v-if="app.snapshot && !app.snapshot.initialized" class="onboarding">
      <div class="onboarding-main">
        <FolderSearch aria-hidden="true" />
        <h2>给散落的 Skill 一个共同的位置</h2>
        <p class="page-subtitle">
          先选择统一存储目录，再扫描已有的 Skill。以后也可以在设置中迁移。
        </p>
        <label class="field"
          ><span class="field-label">统一存储目录</span><DirectoryField v-model="rootPath" /><span
            class="field-hint"
            >默认位置已准备好，也可以选择其他可读写目录。</span
          ></label
        >
        <div class="actions">
          <Button
            variant="primary"
            :disabled="configuring || !rootPath.trim()"
            @click="configure"
            >{{ configuring ? '检测中…' : '确认目录并继续' }}</Button
          >
        </div>
      </div>
      <ol class="onboarding-steps">
        <li>
          <span class="step-number">1</span>
          <div>
            <strong>集中保管</strong>
            <p>已有与新安装的 Skill，统一进入资料库。</p>
          </div>
        </li>
        <li>
          <span class="step-number">2</span>
          <div>
            <strong>审阅后归集</strong>
            <p>查看扫描结果与冲突，再决定是否接管原目录。</p>
          </div>
        </li>
        <li>
          <span class="step-number">3</span>
          <div>
            <strong>随处使用</strong>
            <p>通过软链分发到工具，也能整套应用预设。</p>
          </div>
        </li>
      </ol>
    </section>
    <section v-else class="panel">
      <div class="steps">
        <div
          v-for="(label, index) in ['选择扫描范围', '审阅扫描结果', '确认归集计划', '执行与结果']"
          :key="label"
          class="step"
          :class="{ active: step === index + 1, done: step > index + 1 }"
        >
          <span class="step-number"
            ><Check v-if="step > index + 1" /> <template v-else>{{ index + 1 }}</template></span
          >{{ label }}
        </div>
      </div>
      <div class="wizard-body">
        <section v-if="step === 1">
          <div class="page-heading" style="margin-bottom: 14px">
            <div>
              <h3 class="panel-title">选择扫描范围</h3>
              <p class="page-subtitle">已探测到的目录只作为候选，不会在扫描前修改任何内容。</p>
            </div>
            <Badge :tone="busy ? 'blue' : 'green'">{{ busy ? '正在探测' : '探测完成' }}</Badge>
          </div>
          <div class="choice-list">
            <label v-for="target in candidates" :key="target.id" class="choice"
              ><input v-model="scanPaths" class="checkbox" type="checkbox" :value="target.path" />
              <div class="item-icon"><FolderSearch /></div>
              <div class="choice-main">
                <div class="choice-title">
                  {{ target.name }}
                  <Badge>{{ target.scope === 'user' ? '用户级' : '项目级' }}</Badge>
                </div>
                <div class="choice-meta mono">{{ target.path }}</div>
              </div>
              <Badge tone="green">可扫描</Badge></label
            >
            <div
              v-for="path in scanPaths.filter(
                (path) => !candidates.some((item) => item.path === path),
              )"
              :key="path"
              class="choice"
            >
              <input v-model="scanPaths" class="checkbox" type="checkbox" :value="path" />
              <div class="choice-main">
                <div class="choice-title">自定义文件夹</div>
                <div class="choice-meta mono">{{ path }}</div>
              </div>
            </div>
          </div>
          <div class="field" style="margin-top: 14px">
            <span class="field-label">添加其他文件夹</span>
            <div class="path-field">
              <DirectoryField v-model="customPath" /><Button @click="addCustom">添加</Button>
            </div>
          </div>
        </section>
        <section v-else-if="step === 2">
          <div class="page-heading" style="margin-bottom: 14px">
            <div>
              <h3 class="panel-title">审阅 {{ scanItems.length }} 个结果</h3>
              <p class="page-subtitle">
                同名不同内容不会自动合并。返回并重新扫描时，当前审阅结果会失效。
              </p>
            </div>
            <label class="choice" style="padding: 7px 10px"
              ><input
                class="checkbox"
                type="checkbox"
                :checked="selectedPaths.length === scanItems.filter((i) => selectable(i)).length"
                @change="toggleReviewAll"
              /><span class="choice-title">全选可管理项</span></label
            >
          </div>
          <div
            v-for="warning in warnings"
            :key="warning"
            class="callout warning"
            style="margin-bottom: 10px"
          >
            {{ warning }}
          </div>
          <div class="table-wrap">
            <table class="data-table">
              <thead>
                <tr>
                  <th style="width: 42px"></th>
                  <th>Skill / 当前位置</th>
                  <th style="width: 130px">内容关系</th>
                  <th style="width: 190px">处理方式</th>
                </tr>
              </thead>
              <tbody>
                <tr v-for="item in scanItems" :key="item.path">
                  <td>
                    <input
                      v-model="selectedPaths"
                      class="checkbox"
                      type="checkbox"
                      :value="item.path"
                      :disabled="!selectable(item)"
                    />
                  </td>
                  <td>
                    <div class="item-name">{{ item.name }}</div>
                    <div class="item-desc mono">{{ item.path }}</div>
                  </td>
                  <td>
                    <Badge :tone="status(item.status)[1]">{{ status(item.status)[0] }}</Badge>
                    <div v-if="item.error" class="item-desc">{{ item.error }}</div>
                  </td>
                  <td>
                    <AppSelect
                      v-if="item.status === 'conflict' && selectedPaths.includes(item.path)"
                      v-model="resolutions[item.path]"
                      aria-label="冲突处理"
                      :options="resolutionOptions"
                    /><span v-else class="subtle">{{
                      item.status === 'same'
                        ? '复用中央库内容'
                        : item.status === 'external'
                          ? '由工具管理'
                          : item.status === 'broken'
                            ? '跳过并在任务中诊断'
                            : '安装为新 Skill'
                    }}</span>
                  </td>
                </tr>
              </tbody>
            </table>
          </div>
          <p v-if="conflictsOpen" class="field-error">请选择每个冲突项的处理方式后继续。</p>
        </section>
        <section v-else-if="step === 3">
          <h3 class="panel-title">确认归集计划</h3>
          <p class="page-subtitle">
            共 {{ selectedItems.length }} 项将进入统一库，{{
              scanItems.length - selectedItems.length
            }}
            项跳过或由外部管理。
          </p>
          <div class="form-grid" style="margin-top: 16px">
            <div class="list-stack">
              <div v-for="item in selectedItems" :key="item.path" class="list-row">
                <div class="item-icon"><Link2 /></div>
                <div class="list-row-main">
                  <div class="list-row-title">{{ item.name }}</div>
                  <div class="list-row-meta">
                    {{
                      item.status === 'same'
                        ? '复用已有内容'
                        : item.status === 'conflict'
                          ? '按已选冲突策略保留'
                          : '复制并验证内容'
                    }}
                  </div>
                </div>
                <Badge tone="blue">入库</Badge>
              </div>
            </div>
            <div>
              <label class="choice"
                ><input v-model="adopt" class="checkbox" type="checkbox" />
                <div class="choice-main">
                  <div class="choice-title">归集后将原目录替换为软链</div>
                  <div class="choice-meta">
                    验证成功后将原实体目录替换为指向中央库的软链；默认关闭。
                  </div>
                </div></label
              >
              <div class="callout warning" style="margin-top: 10px">
                <ShieldAlert style="width: 15px; display: inline; vertical-align: -3px" />
                原实体目录会保留为相邻的隐藏备份；若归集未完成，可从任务记录恢复。
              </div>
            </div>
          </div>
        </section>
        <section v-else>
          <div class="empty" style="min-height: 330px">
            <div>
              <div class="empty-icon"><CircleCheck v-if="complete" /><CircleX v-else /></div>
              <h3>{{ complete ? '归集完成' : '归集未完成' }}</h3>
              <p>
                {{
                  complete
                    ? `${selectedItems.length} 个 Skill 已处理。可前往 Skill 库查看并分发。`
                    : '请从任务记录恢复失败或中断的条目。'
                }}
              </p>
              <div class="actions" style="justify-content: center">
                <Button variant="primary" @click="router.push('/library')">查看 Skill 库</Button
                ><Button @click="router.push('/tasks')">查看任务记录</Button>
              </div>
              <div class="callout" style="margin-top: 16px; text-align: left">
                结果可能同时包含成功、跳过、失败和待恢复项，请以任务记录的逐项状态为准。
              </div>
            </div>
          </div>
        </section>
      </div>
      <footer class="wizard-footer">
        <span class="subtle">步骤 {{ step }} / 4</span>
        <div class="actions">
          <Button v-if="step > 1 && step < 4" :disabled="busy" @click="step--">返回</Button
          ><Button
            v-if="step === 1"
            variant="primary"
            :disabled="busy || !scanPaths.length"
            @click="scan"
            >{{ busy ? '扫描中…' : '开始扫描' }}</Button
          ><Button v-if="step === 2" variant="primary" :disabled="conflictsOpen" @click="nextReview"
            >确认选择</Button
          ><Button
            v-if="step === 3"
            variant="primary"
            :disabled="busy || !selectedItems.length"
            @click="execute"
            >{{ busy ? '正在安全步骤中…' : '执行归集' }}</Button
          >
        </div>
      </footer>
    </section>
  </div>
</template>
