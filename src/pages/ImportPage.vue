<script setup lang="ts">
import SkillDirectoryActions from '@/components/SkillDirectoryActions.vue'
import {
  TooltipProvider,
  TooltipRoot,
  TooltipTrigger,
  TooltipPortal,
  TooltipContent,
  TooltipArrow,
} from 'reka-ui'
import AppDialog from '@/components/ui/AppDialog.vue'
import AgentDirectories from '@/components/AgentDirectories.vue'
import { cloneAgentProfiles } from '@/services/agentProfiles'
import AppSelect from '@/components/ui/AppSelect.vue'
import AppPagination from '@/components/ui/AppPagination.vue'
import { computed, nextTick, ref, watch } from 'vue'
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
  Plus,
  Pencil,
  Trash2,
  ChevronLeft,
  ChevronRight,
  Info,
  Copy,
  ShieldCheck,
  AlertTriangle,
  FolderCheck,
} from 'lucide-vue-next'
import Button from '@/components/ui/Button.vue'
import Badge from '@/components/ui/Badge.vue'
import DirectoryField from '@/components/DirectoryField.vue'
import { api, type CollectionPreview } from '@/services/api'
import { useAppStore } from '@/stores/app'
import type { ScanItem, Target } from '@/services/types'
import { usePagination } from '@/composables/usePagination'

const app = useAppStore()
const router = useRouter()
const step = ref(1)
const rootPath = ref(app.isNative ? '' : '/Users/demo/SkillDock')
const configuring = ref(false)
const candidates = ref<Target[]>([])
const directoryPages = ref<Record<string, number>>({})
const candidateGroups = computed(() => {
  const groups = new Map<string, { key: string; name: string; targets: Target[] }>()
  for (const target of candidates.value) {
    const key = `${target.tool}:${target.scope}`
    const group = groups.get(key) || { key, name: app.targetName(target), targets: [] }
    group.targets.push(target)
    groups.set(key, group)
  }
  return [...groups.values()].map((group) => {
    const pages = Math.ceil(group.targets.length / 3)
    const page = Math.min(directoryPages.value[group.key] || 1, pages)
    return {
      ...group,
      page,
      pages,
      visible: group.targets.slice((page - 1) * 3, page * 3),
      selectedCount: group.targets.filter((target) => scanPaths.value.includes(target.path)).length,
    }
  })
})
const scanPaths = ref<string[]>([])
const availableScanPaths = computed(() => [
  ...new Set([...candidates.value.map((item) => item.path), ...customPaths.value]),
])
const customPath = ref('')
const customName = ref('')
const customNames = ref<Record<string, string>>({})
const editingAgent = ref('codex')
const customPaths = ref<string[]>([])
watch(availableScanPaths, (paths, previous = []) => {
  const selected = new Set(scanPaths.value)
  const known = new Set(previous)
  scanPaths.value = paths.filter((path) => selected.has(path) || !known.has(path))
})

const folderOpen = ref(false)
const editingPath = ref<string | null>(null)
const folderError = ref('')
const directoriesOpen = ref(false)
const directoriesSaving = ref(false)
const directoryError = ref('')
const directoryDraft = ref(cloneAgentProfiles())
const explanationOpen = ref(false)
const explanationItem = ref<ScanItem | null>(null)
const copiedExplanationPath = ref(false)

function openExplanation(item: ScanItem) {
  explanationItem.value = item
  copiedExplanationPath.value = false
  explanationOpen.value = true
}

async function copyExplanationPath() {
  if (!explanationItem.value?.path) return
  try {
    await navigator.clipboard.writeText(explanationItem.value.path)
    copiedExplanationPath.value = true
    setTimeout(() => {
      copiedExplanationPath.value = false
    }, 2000)
  } catch {
    // clipboard fallback or unavailable in some contexts
  }
}

function statusTooltipTitle(item: ScanItem): string {
  if (item.status === 'linked') {
    return '外部已有软链 · 保持原样不接管'
  }
  if (item.status === 'broken') {
    return '软链目标丢失 · 链接已失效'
  }
  if (item.status === 'invalid') {
    return '格式无效 · 缺少 SKILL.md'
  }
  if (item.status === 'conflict') {
    return '同名冲突 · 与中央库已有同名'
  }
  return item.error ? item.error.split(/[。\n]/)[0].trim() : '条目说明'
}

function statusTooltipHint(_item: ScanItem): string {
  return '点击查看详情与处理说明'
}
function editDirectories(target: Target) {
  const profiles = app.snapshot?.settings.agentProfiles || []
  const profile = profiles.find(
    (item) => item.id === target.tool || item.name === target.tool || item.name === target.name,
  )
  if (!profile) {
    router.push('/targets')
    return
  }
  editingAgent.value = profile.id
  directoryError.value = ''
  directoryDraft.value = cloneAgentProfiles(app.snapshot?.settings.agentProfiles)
  directoriesOpen.value = true
}
async function saveDirectories() {
  directoryError.value = ''
  for (const profile of directoryDraft.value) {
    for (const paths of [profile.userPaths, profile.projectPaths]) {
      const normalized = paths.map((path) => path.trim().replace(/[\\/]+$/, ''))
      if (normalized.some((path) => !path) || new Set(normalized).size !== normalized.length) {
        directoryError.value = `${profile.name} 的目录不能为空或重复`
        return
      }
    }
  }
  directoriesSaving.value = true
  const ok = await app.mutate(
    () => api.settings({ agentProfiles: cloneAgentProfiles(directoryDraft.value) }),
    '默认目录已保存',
  )
  directoriesSaving.value = false
  if (!ok) directoryError.value = app.error || '保存失败，请检查目录配置'
  if (ok) {
    directoriesOpen.value = false
    await loadCandidates()
  }
}
function openFolder(path: string | null = null) {
  editingPath.value = path
  customPath.value = path || ''
  customName.value = path ? customNames.value[path] || '' : ''
  folderError.value = ''
  folderOpen.value = true
}
function removeFolder(path: string) {
  delete customNames.value[path]
  customPaths.value = customPaths.value.filter((item) => item !== path)
}
const scanItems = ref<ScanItem[]>([])
const warnings = ref<string[]>([])
const selectedPaths = ref<string[]>([])
const resolutions = ref<Record<string, 'rename' | 'keep' | 'skip'>>({})
const adopt = ref(true)
const collectionMode = ref<'package' | 'individual'>('package')
const collectionPreview = ref<CollectionPreview | null>(null)
const busy = ref(false)
const complete = ref(false)
const selectable = (item: ScanItem) => ['ready', 'new', 'same', 'conflict'].includes(item.status)
const activeScanTab = ref<'eligible' | 'ineligible'>('eligible')
const eligibleScanItems = computed(() => scanItems.value.filter((item) => selectable(item)))
const ineligibleScanItems = computed(() => scanItems.value.filter((item) => !selectable(item)))
const conflictsOnly = ref(false)
const focusedConflictPath = ref('')
const reviewTable = ref<HTMLTableElement | null>(null)
const conflictItems = computed(() =>
  eligibleScanItems.value.filter((item) => item.status === 'conflict'),
)
const currentScanTabItems = computed(() =>
  activeScanTab.value === 'eligible'
    ? conflictsOnly.value
      ? conflictItems.value
      : eligibleScanItems.value
    : ineligibleScanItems.value,
)
function showReviewItems(onlyConflicts: boolean) {
  conflictsOnly.value = onlyConflicts
  switchScanTab('eligible')
}
function switchScanTab(tab: 'eligible' | 'ineligible') {
  activeScanTab.value = tab
  resetScanPage()
}
const selectedItems = computed(() =>
  scanItems.value.filter(
    (item) =>
      selectedPaths.value.includes(item.path) &&
      resolutions.value[item.path] !== 'skip' &&
      selectable(item),
  ),
)
// Use a separator boundary so sibling names such as foo and foo-bar never conflict.
const nestedPairs = computed(() => {
  const pairs: { parent: ScanItem; child: ScanItem }[] = []
  const items = [...selectedItems.value].sort((a, b) => a.path.length - b.path.length)
  for (let i = 0; i < items.length; i++) {
    const parent = items[i]!
    const prefix = parent.path.replaceAll('\\', '/').replace(/\/+$/, '') + '/'
    for (const child of items.slice(i + 1)) {
      if (child.path.replaceAll('\\', '/').startsWith(prefix)) pairs.push({ parent, child })
    }
  }
  return pairs
})
const nestedPaths = computed(
  () => new Set(nestedPairs.value.flatMap((pair) => [pair.parent.path, pair.child.path])),
)
const {
  page: scanPage,
  pageSize: scanPageSize,
  pagedItems: pagedScanItems,
  resetPage: resetScanPage,
} = usePagination(currentScanTabItems, { initialPageSize: 15 })

const {
  page: planPage,
  pageSize: planPageSize,
  pagedItems: pagedSelectedItems,
  resetPage: resetPlanPage,
} = usePagination(selectedItems, { initialPageSize: 10 })
const unresolvedConflicts = computed(() =>
  selectedItems.value.filter((item) => item.status === 'conflict' && !resolutions.value[item.path]),
)
const conflictsOpen = computed(() => unresolvedConflicts.value.length > 0)
async function locateNextConflict() {
  if (!conflictsOpen.value) return
  const previousIndex = conflictItems.value.findIndex(
    (item) => item.path === focusedConflictPath.value,
  )
  const unresolvedPaths = new Set(unresolvedConflicts.value.map((item) => item.path))
  const next =
    conflictItems.value.find(
      (item, index) => index > previousIndex && unresolvedPaths.has(item.path),
    ) ?? unresolvedConflicts.value[0]!
  showReviewItems(true)
  focusedConflictPath.value = next.path
  scanPage.value = Math.floor(conflictItems.value.indexOf(next) / scanPageSize.value) + 1
  await nextTick()
  const row = Array.from(
    reviewTable.value?.querySelectorAll<HTMLTableRowElement>('tbody tr') ?? [],
  ).find((row) => row.dataset.reviewPath === next.path)
  row?.scrollIntoView({ block: 'center', behavior: 'smooth' })
  row?.querySelector<HTMLElement>('[role="combobox"]')?.focus({ preventScroll: true })
}
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
  await app.mutate(() => api.configure(rootPath.value), '统一存储目录已配置')
  configuring.value = false
}
async function loadCandidates() {
  if (busy.value) return
  busy.value = true
  try {
    candidates.value = await api.discover()
  } catch (e) {
    app.error = e instanceof Error ? e.message : '无法探测工具目录'
  } finally {
    busy.value = false
  }
}
function addCustom() {
  if (busy.value) return
  const path = customPath.value.trim()
  if (!customName.value.trim()) {
    folderError.value = '请输入文件夹名称'
    return
  }
  if (!path) {
    folderError.value = '请选择或输入文件夹路径'
    return
  }
  const key = path.replace(/[\\/]+$/, '')
  const existing = [...candidates.value.map((item) => item.path), ...customPaths.value].find(
    (item) => item !== editingPath.value && item.replace(/[\\/]+$/, '') === key,
  )
  if (existing) {
    folderError.value = '该文件夹已在扫描范围中，无需重复添加'
    return
  }
  const previous = editingPath.value
  if (previous) {
    delete customNames.value[previous]
    customPaths.value = customPaths.value.map((item) => (item === previous ? path : item))
  } else customPaths.value.push(path)
  customNames.value[path] = customName.value.trim()
  folderOpen.value = false
}
function toggleReviewAll(event: Event) {
  const checked = (event.target as HTMLInputElement).checked
  const eligiblePaths = eligibleScanItems.value.map((item) => item.path)
  if (checked) {
    selectedPaths.value = [...new Set([...selectedPaths.value, ...eligiblePaths])]
  } else {
    selectedPaths.value = selectedPaths.value.filter((path) => !eligiblePaths.includes(path))
  }
}
async function scan() {
  if (busy.value || !scanPaths.value.length) return
  busy.value = true
  collectionPreview.value = null
  const paths = [...scanPaths.value]
  const mode = collectionMode.value
  const shouldAdopt = adopt.value
  try {
    const result = await api.previewCollection(paths, mode, shouldAdopt)
    collectionPreview.value = result
    resolutions.value = {}
    conflictsOnly.value = false
    focusedConflictPath.value = ''
    const items = result.items
    scanItems.value = items
    warnings.value = result.warnings
    selectedPaths.value = items
      .filter((item) => selectable(item) && item.status !== 'broken')
      .map((item) => item.path)
    activeScanTab.value = items.some((item) => selectable(item)) ? 'eligible' : 'ineligible'
    resetScanPage()
    step.value = 2
  } catch (e) {
    app.error = e instanceof Error ? e.message : '扫描失败'
  } finally {
    busy.value = false
  }
}
function nextReview() {
  if (conflictsOpen.value) return
  resetPlanPage()
  step.value = 3
}
async function execute() {
  if (busy.value || !collectionPreview.value) return
  busy.value = true
  const plan = collectionPreview.value
  const ok = await app.mutate(
    () =>
      api.collectSkills({
        paths: plan.paths,
        mode: plan.mode,
        adopt: adopt.value,
        expectedRevision: plan.revision,
        fingerprint: plan.fingerprint,
        selectedPaths: selectedItems.value.map((item) => item.path),
        resolutions: { ...resolutions.value },
      }),
    `已归集 ${selectedItems.value.length} 个 Skill`,
  )
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
  if (busy.value) return
  scanItems.value = []
  collectionPreview.value = null
  resolutions.value = {}
  selectedPaths.value = []
  conflictsOnly.value = false
  focusedConflictPath.value = ''
  resetScanPage()
  resetPlanPage()
  step.value = 1
  complete.value = false
  loadCandidates()
}

const resolutionOptions = [
  { value: '', label: '请选择', disabled: true },
  { value: 'keep', label: '独立保留（名称加后缀）' },
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
      <Button v-if="app.snapshot?.initialized" :disabled="busy" @click="restartScan"
        ><ScanSearch />重新开始</Button
      >
    </header>
    <div v-if="app.snapshot?.initialized && app.snapshot.schemaVersion < 3" class="callout warning">
      当前库仍使用旧版存储规则。请先在 Skill 库中预览并启用单份当前内容，再归集现有目录。
      <Button size="sm" @click="router.push('/library')">前往 Skill 库</Button>
    </div>
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
            :loading="configuring"
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
      <div
        v-if="(step === 2 || step === 3) && (nestedPairs.length || warnings.length)"
        class="wizard-notice"
        role="status"
      >
        <section
          v-if="(step === 2 || step === 3) && nestedPairs.length"
          class="nested-warning"
          role="status"
        >
          <div class="nested-heading">
            <Info :size="16" /><strong>{{ '嵌套目录将自动处理' }}</strong
            ><span>{{ nestedPairs.length }} 组</span>
          </div>
          <p>
            {{
              adopt
                ? '父、子 Skill 均归集入库；仅替换最外层选中目录，子目录通过父目录软链访问。'
                : '父、子 Skill 均归集入库，原目录保持不变。'
            }}
          </p>
          <div class="nested-pairs">
            <div
              v-for="pair in nestedPairs"
              :key="`${pair.parent.path}::${pair.child.path}`"
              class="nested-pair"
            >
              <div class="nested-path-row">
                <span class="nested-label">父目录</span>
                <div>
                  <strong>{{ pair.parent.name }}</strong
                  ><code>{{ pair.parent.path }}</code>
                  <SkillDirectoryActions :path="pair.parent.path" />
                </div>
              </div>
              <div class="nested-path-row">
                <span class="nested-label">子目录</span>
                <div>
                  <strong>{{ pair.child.name }}</strong
                  ><code>{{ pair.child.path }}</code>
                  <SkillDirectoryActions :path="pair.child.path" />
                </div>
              </div>
            </div>
          </div>
        </section>
        <div
          v-for="warning in warnings"
          :key="warning"
          class="callout warning"
          style="margin-bottom: 10px"
        >
          {{ warning }}
        </div>
      </div>
      <div class="wizard-body">
        <section v-if="step === 1">
          <div class="page-heading" style="margin-bottom: 14px">
            <div>
              <h3 class="panel-title">选择扫描范围</h3>
              <p class="page-subtitle">
                勾选本次需要扫描的目录；未选中的不参与扫描与归集，相同实际目录自动去重。
              </p>
            </div>
            <span class="subtle"
              >已选 {{ scanPaths.length }} / {{ availableScanPaths.length }} 个目录</span
            >
          </div>
          <div class="scan-directory-viewport" tabindex="0" role="region" aria-label="扫描目录列表">
            <div class="directory-groups">
              <section v-for="group in candidateGroups" :key="group.key" class="directory-group">
                <header class="directory-group-heading">
                  <div class="directory-group-title">
                    <FolderSearch :size="16" /><strong>{{ group.name }}</strong
                    ><span class="subtle"
                      >{{ group.selectedCount }} / {{ group.targets.length }} 已选</span
                    >
                  </div>
                  <div class="directory-pagination">
                    <template v-if="group.pages > 1">
                      <Button
                        variant="ghost"
                        size="icon"
                        :disabled="group.page === 1"
                        :aria-label="`${group.name} 上一页目录`"
                        @click="directoryPages[group.key] = group.page - 1"
                        ><ChevronLeft :size="14"
                      /></Button>
                      <span>{{ group.page }} / {{ group.pages }}</span>
                      <Button
                        variant="ghost"
                        size="icon"
                        :disabled="group.page === group.pages"
                        :aria-label="`${group.name} 下一页目录`"
                        @click="directoryPages[group.key] = group.page + 1"
                        ><ChevronRight :size="14"
                      /></Button>
                    </template>
                    <Button
                      variant="ghost"
                      size="icon"
                      :disabled="busy"
                      :aria-label="`编辑 ${group.name} 配置`"
                      @click="editDirectories(group.targets[0]!)"
                      ><Pencil :size="14"
                    /></Button>
                  </div>
                </header>
                <div v-for="target in group.visible" :key="target.id" class="directory-row">
                  <input
                    v-model="scanPaths"
                    :disabled="busy"
                    class="checkbox"
                    type="checkbox"
                    :value="target.path"
                    :aria-label="`扫描 ${app.targetName(target)} ${target.path}`"
                  />
                  <span class="directory-path mono" :title="target.path">{{ target.path }}</span>
                  <Badge>{{ target.scope === 'user' ? '用户级' : '项目级' }}</Badge>
                </div>
              </section>
            </div>
            <div class="scan-folder-heading">
              <div>
                <h4>其他文件夹</h4>
                <p class="subtle">为本次归集补充目录，可命名、编辑或移除。</p>
              </div>
              <Button size="sm" :disabled="busy" @click="openFolder()"
                ><Plus :size="14" />添加文件夹</Button
              >
            </div>
            <div v-if="customPaths.length" class="choice-list">
              <div v-for="path in customPaths" :key="path" class="choice">
                <input
                  v-model="scanPaths"
                  :disabled="busy"
                  class="checkbox"
                  type="checkbox"
                  :value="path"
                  :aria-label="`扫描 ${customNames[path] || path}`"
                />
                <div class="item-icon"><FolderSearch /></div>
                <div class="choice-main">
                  <div class="choice-title">{{ customNames[path] || '自定义文件夹' }}</div>
                  <div class="choice-meta mono">{{ path }}</div>
                </div>
                <Button
                  variant="ghost"
                  size="icon"
                  :aria-label="`编辑 ${path}`"
                  :disabled="busy"
                  @click="openFolder(path)"
                  ><Pencil :size="16"
                /></Button>
                <Button
                  variant="ghost"
                  size="icon"
                  :aria-label="`移除 ${path}`"
                  :disabled="busy"
                  @click="removeFolder(path)"
                  ><Trash2 :size="16"
                /></Button>
              </div>
            </div>
            <p v-else class="scan-folder-empty">
              暂无其他文件夹。添加后默认勾选，可取消以排除本次归集。
            </p>
          </div>
          <div class="callout" style="margin-top: 16px">
            <label class="field">
              <span class="field-label">内容组织方式</span>
              <AppSelect
                v-model="collectionMode"
                :disabled="busy"
                :options="[
                  { value: 'package', label: '保留整包结构（含共享资源）' },
                  { value: 'individual', label: '各 Skill 相互独立（按成员去重）' },
                ]"
              />
            </label>
            <p>
              工作流包保留原始目录结构；只有确认各 Skill
              不依赖包根文件或兄弟目录时，才选择独立模式。
            </p>
            <label class="choice">
              <input v-model="adopt" class="checkbox" type="checkbox" :disabled="busy" />
              <span>归集后将原目录替换为指向统一库的软链</span>
            </label>
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
            <label
              v-if="activeScanTab === 'eligible' && eligibleScanItems.length"
              class="choice"
              style="padding: 7px 10px"
            >
              <input
                class="checkbox"
                type="checkbox"
                :checked="
                  eligibleScanItems.length > 0 &&
                  eligibleScanItems.every((i) => selectedPaths.includes(i.path))
                "
                @change="toggleReviewAll"
              /><span class="choice-title"
                >全选全部可归集项（含筛选外） ({{
                  selectedPaths.filter((p) => eligibleScanItems.some((i) => i.path === p)).length
                }}
                / {{ eligibleScanItems.length }})</span
              ></label
            >
          </div>

          <div class="import-scan-cards">
            <div
              class="import-stat-card card-blue"
              :class="{ active: activeScanTab === 'eligible' }"
              role="button"
              tabindex="0"
              @click="switchScanTab('eligible')"
              @keydown.enter.space="switchScanTab('eligible')"
            >
              <div class="stat-card-top">
                <div class="stat-card-label">
                  <FolderCheck class="stat-card-icon text-blue" />
                  <span>可以归集</span>
                </div>
                <span class="stat-card-count text-blue">{{ eligibleScanItems.length }}</span>
              </div>
              <div class="stat-card-desc">发现的完整独立实体，勾选后将统一归集入库</div>
            </div>

            <div
              class="import-stat-card card-neutral"
              :class="{ active: activeScanTab === 'ineligible' }"
              role="button"
              tabindex="0"
              @click="switchScanTab('ineligible')"
              @keydown.enter.space="switchScanTab('ineligible')"
            >
              <div class="stat-card-top">
                <div class="stat-card-label">
                  <ShieldAlert class="stat-card-icon text-muted" />
                  <span>不可归集</span>
                </div>
                <span class="stat-card-count text-muted">{{ ineligibleScanItems.length }}</span>
              </div>
              <div class="stat-card-desc">已有软链、断链或外部管理，受到保护不改动</div>
            </div>
          </div>

          <div v-if="conflictItems.length" class="callout" style="margin-bottom: 14px">
            <p role="status">
              共 {{ conflictItems.length }} 个同名冲突，已选项中还有
              {{ unresolvedConflicts.length }} 个待处理（包含其他页）。
            </p>
            <div class="toolbar" style="margin-top: 10px">
              <button
                type="button"
                class="btn btn-secondary"
                :aria-pressed="activeScanTab === 'eligible' && !conflictsOnly"
                @click="showReviewItems(false)"
              >
                全部可归集
              </button>
              <button
                type="button"
                class="btn btn-secondary"
                :aria-pressed="activeScanTab === 'eligible' && conflictsOnly"
                @click="showReviewItems(true)"
              >
                仅看冲突 ({{ conflictItems.length }})
              </button>
              <button
                type="button"
                class="btn btn-primary"
                :disabled="!conflictsOpen"
                @click="locateNextConflict"
              >
                定位下一个待处理
              </button>
            </div>
          </div>

          <div
            v-if="!currentScanTabItems.length"
            class="callout"
            style="margin-top: 10px; padding: 16px; text-align: center"
          >
            {{
              activeScanTab === 'eligible'
                ? '未发现可以归集的独立实体。'
                : '本次扫描未发现不可归集的条目，所有内容均可正常归集。'
            }}
          </div>
          <div v-else class="table-wrap">
            <table ref="reviewTable" class="data-table">
              <thead>
                <tr v-if="activeScanTab === 'eligible'">
                  <th style="width: 42px"></th>
                  <th>Skill / 当前位置</th>
                  <th style="width: 130px">内容关系</th>
                  <th style="width: 190px">处理方式</th>
                </tr>
                <tr v-else>
                  <th>Skill / 当前位置</th>
                  <th style="width: 150px">原因状态</th>
                  <th style="width: 220px">保护说明</th>
                </tr>
              </thead>
              <tbody>
                <tr v-for="item in pagedScanItems" :key="item.path" :data-review-path="item.path">
                  <td v-if="activeScanTab === 'eligible'">
                    <input
                      v-model="selectedPaths"
                      class="checkbox"
                      type="checkbox"
                      :value="item.path"
                    />
                  </td>
                  <td>
                    <div class="item-name">
                      {{ item.name }}
                      <Badge v-if="nestedPaths.has(item.path)" tone="neutral">嵌套目录</Badge>
                    </div>
                    <div class="item-desc mono">{{ item.path }}</div>
                    <SkillDirectoryActions :path="item.path" />
                    <div
                      v-if="item.status === 'linked' && item.description"
                      class="item-desc subtle"
                      style="margin-top: 2px"
                    >
                      {{ item.description }}
                    </div>
                  </td>
                  <td>
                    <TooltipProvider v-if="item.error" :delay-duration="200">
                      <TooltipRoot>
                        <TooltipTrigger as-child>
                          <button
                            type="button"
                            class="scan-status-trigger"
                            :aria-label="`${status(item.status)[0]}：查看说明`"
                            @click="openExplanation(item)"
                          >
                            <Badge :tone="status(item.status)[1]">{{
                              status(item.status)[0]
                            }}</Badge>
                            <Info class="scan-status-info-icon" />
                          </button>
                        </TooltipTrigger>
                        <TooltipPortal>
                          <TooltipContent
                            class="scan-status-tooltip"
                            side="top"
                            align="center"
                            :side-offset="6"
                            :collision-padding="8"
                            position-strategy="fixed"
                          >
                            <div class="scan-status-tooltip-body">
                              <span class="scan-status-tooltip-title">{{
                                statusTooltipTitle(item)
                              }}</span>
                              <span class="scan-status-tooltip-hint">{{
                                statusTooltipHint(item)
                              }}</span>
                            </div>
                            <TooltipArrow class="scan-status-arrow" />
                          </TooltipContent>
                        </TooltipPortal>
                      </TooltipRoot>
                    </TooltipProvider>
                    <Badge v-else :tone="status(item.status)[1]">{{
                      status(item.status)[0]
                    }}</Badge>
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
                            ? '跳过（软链断开）'
                            : item.status === 'linked'
                              ? '保留外部软链'
                              : '安装为新 Skill'
                    }}</span>
                  </td>
                </tr>
              </tbody>
            </table>
          </div>
          <AppPagination
            v-if="currentScanTabItems.length > 10"
            v-model:page="scanPage"
            v-model:page-size="scanPageSize"
            :total="currentScanTabItems.length"
            :page-sizes="[10, 15, 25, 50]"
            style="margin-top: 12px"
          />
          <p v-if="conflictsOpen" class="field-error">
            还有 {{ unresolvedConflicts.length }} 个已选冲突待处理，可点击上方“定位下一个待处理”。
          </p>
        </section>
        <section v-else-if="step === 3">
          <h3 class="panel-title">确认归集计划</h3>
          <p class="page-subtitle">
            共 {{ selectedItems.length }} 项将进入统一库，{{
              scanItems.length - selectedItems.length
            }}
            项跳过或由外部管理。
          </p>
          <div style="margin-top: 16px">
            <div class="list-stack">
              <div v-for="item in pagedSelectedItems" :key="item.path" class="list-row">
                <div class="item-icon"><Link2 /></div>
                <div class="list-row-main">
                  <div class="list-row-title">{{ item.name }}</div>
                  <SkillDirectoryActions :path="item.path" />
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
              <AppPagination
                v-if="selectedItems.length > 10"
                v-model:page="planPage"
                v-model:page-size="planPageSize"
                :total="selectedItems.length"
                :page-sizes="[5, 10, 20]"
                compact
                :show-size-changer="false"
                style="margin-top: 12px"
              />
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
      <div v-if="step === 3" class="wizard-options">
        <div>
          <label class="choice"
            ><input :checked="adopt" class="checkbox" type="checkbox" disabled />
            <div class="choice-main">
              <div class="choice-title">归集后将原目录替换为软链</div>
              <div class="choice-meta">
                {{
                  adopt ? '已选择接管原目录。验证后替换为软链。' : '仅复制入库，原目录保持不变。'
                }}返回选择扫描范围可修改。
              </div>
            </div></label
          >
          <div class="callout warning" style="margin-top: 10px">
            <ShieldAlert style="width: 15px; display: inline; vertical-align: -3px" />
            {{
              (app.snapshot?.settings.backupRetention ?? 3) === 0
                ? '原件仅在操作期间临时保留；成功提交后自动清理。失败或中断仍可恢复。'
                : `按现有设置保留最近 ${app.snapshot?.settings.backupRetention ?? 3} 批原目录备份。`
            }}
            来源内容或资料库发生变化时，需要重新扫描确认。
          </div>
        </div>
      </div>
      <footer class="wizard-footer">
        <span class="subtle">步骤 {{ step }} / 4</span>
        <div class="actions">
          <Button v-if="step > 1 && step < 4" :disabled="busy" @click="step--">返回</Button
          ><Button
            v-if="step === 1"
            variant="primary"
            :disabled="busy || !scanPaths.length || (app.snapshot?.schemaVersion ?? 0) < 3"
            :loading="busy"
            @click="scan"
            >{{ busy ? '扫描中…' : '开始扫描' }}</Button
          ><Button v-if="step === 2" variant="primary" :disabled="conflictsOpen" @click="nextReview"
            >确认选择</Button
          ><Button
            v-if="step === 3"
            variant="primary"
            :disabled="busy || !selectedItems.length"
            :loading="busy"
            @click="execute"
            >{{ busy ? '正在安全步骤中…' : '执行归集' }}</Button
          >
        </div>
      </footer>
    </section>
    <AppDialog
      v-model:open="folderOpen"
      :title="editingPath ? '编辑文件夹' : '添加文件夹'"
      description="选择包含 Skill 的文件夹，扫描时会自动查找其中的 SKILL.md。"
    >
      <div class="field">
        <label class="field-label" for="scan-folder-name">名称</label>
        <input
          id="scan-folder-name"
          v-model="customName"
          class="input"
          maxlength="100"
          placeholder="例如：团队共享 Skills"
        />
        <span class="field-label">文件夹路径</span>
        <DirectoryField v-model="customPath" />
        <p v-if="folderError" role="alert" class="callout warning">{{ folderError }}</p>
      </div>
      <template #footer>
        <Button @click="folderOpen = false">取消</Button>
        <Button variant="primary" @click="addCustom">{{
          editingPath ? '保存修改' : '加入扫描范围'
        }}</Button>
      </template>
    </AppDialog>
    <AppDialog
      v-model:open="directoriesOpen"
      large
      title="编辑默认目录"
      description="保存后会更新默认配置，并重新探测扫描范围。"
    >
      <AgentDirectories
        v-if="directoriesOpen"
        :key="editingAgent"
        v-model="directoryDraft"
        :initial-agent="editingAgent"
      />
      <p v-if="directoryError" role="alert" class="callout warning">{{ directoryError }}</p>
      <template #footer>
        <Button :disabled="directoriesSaving" @click="directoriesOpen = false">取消</Button>
        <Button
          variant="primary"
          :disabled="directoriesSaving"
          :loading="directoriesSaving"
          @click="saveDirectories"
          >{{ directoriesSaving ? '保存中…' : '保存并重新探测' }}</Button
        >
      </template>
    </AppDialog>
    <AppDialog
      v-model:open="explanationOpen"
      :title="explanationItem?.status === 'linked' ? '已有软链说明' : '条目状态说明'"
      :description="
        explanationItem?.status === 'linked'
          ? '该条目在工具目录中已被建立为软链接，SkillDock 会安全保留原样。'
          : explanationItem?.description || explanationItem?.name || ''
      "
    >
      <div v-if="explanationItem" class="explanation-dialog-body">
        <div class="explanation-summary-card">
          <div class="explanation-title-row">
            <span class="explanation-name">{{ explanationItem.name }}</span>
            <Badge :tone="status(explanationItem.status)[1]">{{
              status(explanationItem.status)[0]
            }}</Badge>
          </div>
          <SkillDirectoryActions :path="explanationItem.path" />
          <div class="explanation-detail-list">
            <div class="explanation-detail-item">
              <span class="explanation-detail-label">当前位置</span>
              <div class="explanation-path-box mono">
                <span>{{ explanationItem.path }}</span>
                <button
                  type="button"
                  class="explanation-copy-btn"
                  :title="copiedExplanationPath ? '已复制' : '复制路径'"
                  @click="copyExplanationPath"
                >
                  <Check
                    v-if="copiedExplanationPath"
                    style="width: 13px; height: 13px; color: var(--green)"
                  />
                  <Copy v-else style="width: 13px; height: 13px" />
                </button>
              </div>
            </div>
            <div
              v-if="explanationItem.description?.startsWith('指向：')"
              class="explanation-detail-item"
            >
              <span class="explanation-detail-label">链接指向</span>
              <div class="explanation-target-box mono">
                <Link2 class="explanation-target-icon" />
                <span>{{ explanationItem.description.replace(/^指向[：:]\s*/, '') }}</span>
              </div>
            </div>
          </div>
        </div>

        <template v-if="explanationItem.status === 'linked'">
          <div class="explanation-section">
            <div class="explanation-section-title">
              <ShieldCheck class="explanation-section-icon" />
              <span>软链安全与保护原则</span>
            </div>
            <div class="explanation-card">
              <ul class="explanation-bullet-list">
                <li>
                  <strong>不接管外部软链：</strong
                  >该路径是系统或第三方工具建立的符号链接（Pointer），而非存放代码的实体源码目录。接管软链本身可能引发循环引用或破坏已有工具管理。
                </li>
                <li>
                  <strong>完好保留原样：</strong
                  >归集与后续操作绝不会修改、覆盖、解引用或删除此软链接，确保原 Agent
                  工具的日常调用不受任何影响。
                </li>
              </ul>
            </div>
          </div>

          <div class="explanation-section">
            <div class="explanation-section-title">
              <FolderSearch class="explanation-section-icon" />
              <span>后续处理建议</span>
            </div>
            <div class="explanation-action-grid">
              <div class="explanation-action-item">
                <div class="explanation-action-header">
                  <strong>保持现状（推荐）</strong>
                  <Badge tone="neutral">无需操作</Badge>
                </div>
                <p class="explanation-action-text">
                  若原工具中此 Skill 运行正常，直接保留即可。SkillDock 会自动跳过此项。
                </p>
              </div>
              <div class="explanation-action-item">
                <div class="explanation-action-header">
                  <strong>纳入中央库管理</strong>
                  <Badge tone="blue">统一分发</Badge>
                </div>
                <p class="explanation-action-text">
                  若需使用 SkillDock
                  统一管理与跨工具分发，请在第一步添加其<strong>原始实体源码目录</strong>进行归集，入库后再统筹分发。
                </p>
              </div>
            </div>
          </div>
        </template>

        <template v-else-if="explanationItem.status === 'broken'">
          <div class="callout warning">
            <AlertTriangle
              style="
                width: 15px;
                height: 15px;
                flex-shrink: 0;
                vertical-align: -2px;
                margin-right: 6px;
              "
            />
            <span>
              <strong>链接目标已失效：</strong
              >此软链指向的目标位置不存在或已被移除。建议检查原始目录是否存在，或在对应工具目录中清理该失效链接。
            </span>
          </div>
        </template>

        <template v-else-if="explanationItem.error">
          <div class="callout">
            {{ explanationItem.error }}
          </div>
        </template>
      </div>
      <template #footer>
        <div class="explanation-footer-row">
          <Button v-if="explanationItem" size="sm" @click="copyExplanationPath">
            <Check v-if="copiedExplanationPath" class="btn-icon-svg" style="color: var(--green)" />
            <Copy v-else class="btn-icon-svg" />
            {{ copiedExplanationPath ? '已复制路径' : '复制软链路径' }}
          </Button>
          <Button variant="primary" @click="explanationOpen = false">我知道了</Button>
        </div>
      </template>
    </AppDialog>
  </div>
</template>
<style scoped>
.nested-warning {
  margin-bottom: 18px;
  padding: 14px;
  border: 1px solid var(--line);
  border-radius: 8px;
  background: var(--surface);
}
.nested-heading {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 13px;
  color: var(--muted);
}
.nested-warning p {
  margin: 8px 0 12px;
  font-size: 12px;
  color: var(--muted);
}
.nested-pairs {
  max-height: 220px;
  overflow-y: auto;
}
.nested-pair + .nested-pair {
  border-top: 1px solid var(--line);
  margin-top: 8px;
  padding-top: 8px;
}
.nested-path-row {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 5px 0;
  font-size: 12px;
}
.nested-path-row > div {
  flex: 1;
  min-width: 0;
}
.nested-path-row code {
  display: block;
  white-space: normal;
  overflow-wrap: anywhere;
  color: var(--muted);
  font-size: 11px;
  margin-top: 3px;
}
.nested-label {
  flex-shrink: 0;
  color: var(--muted);
  font-size: 11px;
}

.scan-status-trigger {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  border: 0;
  padding: 2px 5px;
  margin: -2px -5px;
  background: transparent;
  border-radius: 6px;
  cursor: pointer;
  transition: background-color 140ms ease;
}
.scan-status-trigger:hover {
  background-color: var(--surface);
}
.scan-status-trigger:hover .scan-status-info-icon {
  color: var(--blue);
}
.scan-status-info-icon {
  width: 13px;
  height: 13px;
  color: var(--muted);
  flex-shrink: 0;
  transition: color 140ms ease;
}

.explanation-dialog-body {
  display: flex;
  flex-direction: column;
  gap: 16px;
}
.explanation-summary-card {
  background: var(--surface);
  border: 1px solid var(--line);
  border-radius: 8px;
  padding: 14px 16px;
}
.explanation-title-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  margin-bottom: 12px;
}
.explanation-name {
  font-size: 15px;
  font-weight: 600;
  color: var(--ink);
}
.explanation-detail-list {
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.explanation-detail-item {
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.explanation-detail-label {
  font-size: 11px;
  font-weight: 500;
  color: var(--muted);
  text-transform: uppercase;
  letter-spacing: 0.04em;
}
.explanation-path-box,
.explanation-target-box {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  background: var(--panel);
  border: 1px solid var(--line);
  padding: 6px 10px;
  border-radius: 6px;
  font-size: 12px;
  color: var(--ink);
  overflow-wrap: anywhere;
  word-break: break-all;
}
.explanation-target-box {
  color: var(--blue);
  background: color-mix(in srgb, var(--blue-soft) 45%, transparent);
  border-color: color-mix(in srgb, var(--blue) 25%, transparent);
}
.explanation-target-icon {
  width: 14px;
  height: 14px;
  flex-shrink: 0;
}
.explanation-copy-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 24px;
  padding: 0;
  border: 1px solid transparent;
  background: transparent;
  border-radius: 4px;
  cursor: pointer;
  color: var(--muted);
  flex-shrink: 0;
  transition: all 120ms ease;
}
.explanation-copy-btn:hover {
  background: var(--surface);
  border-color: var(--line);
  color: var(--ink);
}
.explanation-section {
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.explanation-section-title {
  display: flex;
  align-items: center;
  gap: 7px;
  font-size: 13px;
  font-weight: 600;
  color: var(--ink);
}
.explanation-section-icon {
  width: 15px;
  height: 15px;
  color: var(--blue);
  flex-shrink: 0;
}
.explanation-card {
  background: var(--surface);
  border: 1px solid var(--line);
  border-radius: 8px;
  padding: 12px 14px;
}
.explanation-bullet-list {
  margin: 0;
  padding-left: 18px;
  font-size: 13px;
  line-height: 1.65;
  color: var(--muted);
}
.explanation-bullet-list strong {
  color: var(--ink);
}
.explanation-bullet-list li + li {
  margin-top: 6px;
}
.explanation-action-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 10px;
}
.explanation-action-item {
  padding: 12px;
  border: 1px solid var(--line);
  border-radius: 8px;
  background: var(--surface);
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.explanation-action-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  font-size: 13px;
  color: var(--ink);
}
.explanation-action-text {
  margin: 0;
  font-size: 12px;
  line-height: 1.55;
  color: var(--muted);
}
.explanation-footer-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  width: 100%;
}
.btn-icon-svg {
  width: 13px;
  height: 13px;
  margin-right: 4px;
}

.directory-groups {
  display: grid;
  gap: 10px;
}
.directory-group {
  border: 1px solid var(--line);
  border-radius: 8px;
  overflow: hidden;
}
.directory-group-heading {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  min-height: 38px;
  padding: 5px 12px;
  background: var(--surface);
}
.directory-group-title {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 13px;
}
.directory-group-title .subtle {
  font-size: 11px;
}
.directory-row {
  display: flex;
  align-items: center;
  gap: 10px;
  min-height: 40px;
  padding: 4px 10px 4px 12px;
  border-top: 1px solid var(--line);
}
.directory-path {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 12px;
  color: var(--muted);
}
.directory-pagination {
  display: flex;
  align-items: center;
  gap: 3px;
  font-size: 11px;
  color: var(--muted);
}
</style>
<style scoped>
.scan-directory-viewport {
  height: clamp(240px, 44vh, 400px);
  overflow-y: auto;
  scrollbar-gutter: stable;
  overscroll-behavior: contain;
  padding-right: 6px;
}
.scan-directory-viewport .choice-list {
  gap: 6px;
}
.scan-directory-viewport .choice {
  padding: 9px 12px;
  gap: 10px;
}
.scan-directory-viewport .item-icon {
  width: 30px;
  height: 30px;
}
.scan-directory-viewport .choice-meta {
  font-size: 12px;
  margin-top: 2px;
  overflow-wrap: anywhere;
}
.scan-directory-viewport .choice-main {
  min-width: 0;
}
.scan-directory-viewport .choice-title {
  font-size: 13px;
}

.scan-folder-heading {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  margin: 18px 0 10px;
}
.scan-folder-heading h4 {
  margin: 0 0 6px;
}
.scan-folder-heading p {
  margin: 0;
}
.scan-folder-empty {
  padding: 18px;
  border: 1px dashed var(--control-line);
  border-radius: 8px;
  color: var(--muted);
  font-size: 13px;
}

/* 审阅扫描结果双卡片切换 */
.import-scan-cards {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 12px;
  margin-bottom: 14px;
}
.import-stat-card {
  padding: 12px 14px;
  border-radius: 8px;
  background: var(--panel);
  border: 1px solid var(--line);
  cursor: pointer;
  transition: all 0.15s ease;
  user-select: none;
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.import-stat-card:hover {
  border-color: var(--control-line);
  box-shadow: 0 1px 4px rgb(0 0 0 / 4%);
}
.import-stat-card.active {
  border-width: 1.5px;
  box-shadow: 0 2px 8px rgb(0 0 0 / 6%);
}
.import-stat-card.card-blue.active {
  border-color: var(--blue);
  background: color-mix(in srgb, var(--blue) 5%, var(--panel));
}
.import-stat-card.card-neutral.active {
  border-color: var(--muted);
  background: color-mix(in srgb, var(--muted) 8%, var(--panel));
}
.stat-card-top {
  display: flex;
  align-items: center;
  justify-content: space-between;
}
.stat-card-label {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 13px;
  font-weight: 600;
  color: var(--ink);
}
.stat-card-icon {
  width: 16px;
  height: 16px;
}
.text-blue {
  color: var(--blue);
}
.text-muted {
  color: var(--muted);
}
.stat-card-count {
  font-size: 16px;
  font-weight: 700;
  font-family: ui-monospace, monospace;
}
.stat-card-desc {
  font-size: 12px;
  color: var(--muted);
  line-height: 1.4;
}
@media (max-width: 540px) {
  .import-scan-cards {
    grid-template-columns: 1fr;
  }
}
</style>

<style scoped>
.wizard-options {
  flex-shrink: 0;
  padding: 12px 24px;
  border-top: 1px solid var(--line);
}
</style>

<style scoped>
.wizard-notice {
  flex-shrink: 0;
  max-height: 96px;
  overflow: auto;
  overscroll-behavior: contain;
  padding: 8px 24px;
  border-bottom: 1px solid var(--line);
}
.wizard-notice .nested-warning {
  margin: 0;
}
</style>
