<script setup lang="ts">
import SingleContentSettings from '@/components/SingleContentSettings.vue'
import GitPackageImport from '@/components/GitPackageImport.vue'
import SkillDirectoryActions from '@/components/SkillDirectoryActions.vue'
import AppSelect from '@/components/ui/AppSelect.vue'
import { computed, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import {
  createColumnHelper,
  FlexRender,
  getCoreRowModel,
  getPaginationRowModel,
  useVueTable,
  type PaginationState,
} from '@tanstack/vue-table'
import {
  Search,
  Plus,
  GitBranch,
  FolderInput,
  Globe2,
  Link2,
  Unlink,
  Check,
  LockKeyhole,
  Trash2,
  Layers3,
  RefreshCcw,
  MoreHorizontal,
  SlidersHorizontal,
  LayoutGrid,
  List,
} from 'lucide-vue-next'
import Button from '@/components/ui/Button.vue'
import Badge from '@/components/ui/Badge.vue'
import EmptyState from '@/components/ui/EmptyState.vue'
import AppDialog from '@/components/ui/AppDialog.vue'
import ConfirmDialog from '@/components/ui/ConfirmDialog.vue'
import DirectoryField from '@/components/DirectoryField.vue'
import SkillDetailSheet from '@/components/SkillDetailSheet.vue'
import AppPagination from '@/components/ui/AppPagination.vue'
import DistributionDialog from '@/components/DistributionDialog.vue'
import { useAppStore } from '@/stores/app'
import { api } from '@/services/api'
import type { Skill } from '@/services/types'
import { sourceUpdateState } from '@/services/sourceUpdates'
import { librarySkills, type LibrarySkill } from '@/services/librarySkills'
import { useLibraryDistribution } from '@/composables/useLibraryDistribution'

const app = useAppStore()
const needsSimplification = computed(
  () =>
    !!app.snapshot?.initialized &&
    (app.snapshot.schemaVersion < 3 || !/[\\/]\.skilldock$/.test(app.snapshot.storageRoot)),
)
const router = useRouter()
const route = useRoute()
const query = ref(String(route.query.q || ''))
const VIEW_MODE_KEY = 'skilldock_library_view_mode'
const getStoredViewMode = (): 'grid' | 'table' => {
  try {
    const saved = localStorage.getItem(VIEW_MODE_KEY)
    if (saved === 'grid' || saved === 'table') return saved
  } catch {}
  return 'grid'
}
const viewMode = ref<'grid' | 'table'>(getStoredViewMode())
watch(viewMode, (mode) => {
  try {
    localStorage.setItem(VIEW_MODE_KEY, mode)
  } catch {}
})
const scope = ref('all')
const filtersOpen = ref(false)
const sourceFilter = ref('')
const targetFilter = ref('')
const presetFilter = ref('')
const updateFilter = ref('')
const selected = ref<string[]>([])
const detail = ref<Skill | null>(null)
const detailOpen = ref(false)
const distributeOpen = ref(false)
const distributeIds = ref<string[]>([])
const distributeTargetIds = ref<string[]>([])
const addOpen = ref(false)
const addMode = ref<'menu' | 'git' | 'folder'>('menu')
const folderPath = ref(app.isNative ? '' : '/Users/demo/Downloads/skills')
const busy = ref(false)
const removeOpen = ref(false)
const addPresetOpen = ref(false)
const presetId = ref('')
const updateSkillIds = computed(
  () =>
    new Set(
      app.snapshot?.skills
        .filter((skill) =>
          app.snapshot?.sources.some(
            (source) =>
              sourceUpdateState(source).canCheck &&
              source.id === skill.sourceId &&
              ['available', 'attention'].includes(source.status),
          ),
        )
        .map((skill) => skill.id),
    ),
)
const sourceName = (id: string) =>
  app.sourceName(app.snapshot?.sources.find((item) => item.id === id)) || '独立来源'
const rows = computed(() => librarySkills(app.snapshot))
const distribution = useLibraryDistribution(rows)
const {
  operationError,
  repairSkillId,
  repairOpen,
  repairPreview,
  distributionBusy,
  batchOpen,
  batchMode,
  batchTargetIds,
  batchError,
  batchSummary,
} = distribution
const selectedRows = computed(() => rows.value.filter((row) => selected.value.includes(row.id)))
const selectedMemberIds = computed(() => selectedRows.value.flatMap((row) => row.memberIds))
const filteredSkills = computed(() =>
  rows.value.filter((skill) => {
    const text = skill.members
      .map((member) => `${member.name} ${member.description} ${sourceName(member.sourceId)}`)
      .join(' ')
      .toLowerCase()
    return (
      (scope.value === 'all' || skill.tools.some((tool) => tool.active)) &&
      (!query.value || text.includes(query.value.toLowerCase())) &&
      (!sourceFilter.value ||
        skill.members.some((member) => member.sourceId === sourceFilter.value)) &&
      (!targetFilter.value ||
        skill.tools.some((tool) => tool.id === targetFilter.value && tool.active)) &&
      (!presetFilter.value ||
        app.snapshot?.presets
          .find((preset) => preset.id === presetFilter.value)
          ?.skillIds.some((id) => skill.memberIds.includes(id))) &&
      (!updateFilter.value ||
        (updateFilter.value === 'available'
          ? skill.memberIds.some((id) => updateSkillIds.value.has(id))
          : !skill.memberIds.some((id) => updateSkillIds.value.has(id))))
    )
  }),
)
function openBatch(mode: 'distribute' | 'revoke') {
  if (mode === 'distribute') openDistribute(selected.value)
  else distribution.openBatch(selected.value, mode)
}
const filterCount = computed(
  () =>
    [sourceFilter.value, targetFilter.value, presetFilter.value, updateFilter.value].filter(Boolean)
      .length,
)
function clearFilters() {
  query.value = ''
  sourceFilter.value = ''
  targetFilter.value = ''
  presetFilter.value = ''
  updateFilter.value = ''
  scope.value = 'all'
}
watch(query, (value) => router.replace({ query: { ...route.query, q: value || undefined } }))
watch(
  () => route.query.q,
  (value) => {
    query.value = String(value || '')
  },
)
watch(
  () => app.snapshot?.skills,
  (skills) => {
    selected.value = selected.value.filter((id) => skills?.some((skill) => skill.id === id))
  },
)
const allChecked = computed(
  () =>
    filteredSkills.value.length > 0 &&
    filteredSkills.value.every((skill) => selected.value.includes(skill.id)),
)
function toggleAll() {
  selected.value = allChecked.value
    ? selected.value.filter((id) => !filteredSkills.value.some((skill) => skill.id === id))
    : [...new Set([...selected.value, ...filteredSkills.value.map((skill) => skill.id)])]
}
const column = createColumnHelper<LibrarySkill>()
const columns = [
  column.display({ id: 'select', header: () => '', cell: () => '' }),
  column.accessor('name', { header: '名称', cell: (info) => info.getValue() }),
  column.display({
    id: 'bindings',
    header: '工具分发 · 点击切换',
    cell: () => '',
  }),
  column.display({
    id: 'updates',
    header: '更新',
    cell: (info) => (updateSkillIds.value.has(info.row.original.id) ? '可更新' : '最新'),
  }),
  column.display({ id: 'actions', header: '', cell: () => '' }),
]
const pagination = ref<PaginationState>({
  pageIndex: 0,
  pageSize: 20,
})
const table = useVueTable({
  data: filteredSkills,
  columns,
  state: {
    get pagination() {
      return pagination.value
    },
  },
  onPaginationChange: (updater) => {
    const next = typeof updater === 'function' ? updater(pagination.value) : updater
    pagination.value = {
      pageIndex: Math.max(0, next.pageIndex),
      pageSize: Math.max(1, Math.floor(next.pageSize || 20)),
    }
  },
  getCoreRowModel: getCoreRowModel(),
  getPaginationRowModel: getPaginationRowModel(),
  autoResetPageIndex: false,
})
watch([query, scope, sourceFilter, targetFilter, presetFilter, updateFilter], () => {
  pagination.value = { ...pagination.value, pageIndex: 0 }
})
watch(
  () => filteredSkills.value.length,
  (total) => {
    const lastPage = Math.max(0, Math.ceil(total / pagination.value.pageSize) - 1)
    if (pagination.value.pageIndex > lastPage)
      pagination.value = { ...pagination.value, pageIndex: lastPage }
  },
)
function changePage(page: number) {
  const lastPage = Math.max(
    0,
    Math.ceil(filteredSkills.value.length / pagination.value.pageSize) - 1,
  )
  pagination.value = { ...pagination.value, pageIndex: Math.min(lastPage, Math.max(0, page - 1)) }
}
function changePageSize(size: number) {
  pagination.value = { pageIndex: 0, pageSize: Math.max(1, Math.floor(Number(size) || 20)) }
}
function openDetail(skill: Skill) {
  detail.value = skill
  detailOpen.value = true
}
function openDistribute(ids: string[], targetIds: string[] = []) {
  distributeTargetIds.value = targetIds
  distributeIds.value = ids
  distributeOpen.value = true
}
function toggleDistribution(row: LibrarySkill, targetId: string) {
  const tool = row.tools.find((tool) => tool.id === targetId)
  if (!tool || tool.protected) return
  if (!tool.active && row.needsSourceChoice) openDistribute([row.id], [targetId])
  else void distribution.toggle(row.id, targetId)
}
function gitImported() {
  addOpen.value = false
  app.notice = 'Git 集合已同步'
}
async function importFolder() {
  if (busy.value) return
  busy.value = true
  try {
    const result = await api.scan(folderPath.value)
    const paths = result.items
      .filter((item) => ['ready', 'new', 'same'].includes(item.status))
      .map((item) => item.path)
    if (!paths.length) throw new Error('此目录没有可安全导入的 Skill，请前往归集向导处理冲突')
    const ok = await app.mutate(
      () => api.importFolder(folderPath.value, paths, false),
      `已导入 ${paths.length} 个 Skill，原目录未改动`,
    )
    if (ok) addOpen.value = false
  } catch (e) {
    app.error = e instanceof Error ? e.message : '导入失败'
  } finally {
    busy.value = false
  }
}
async function removeSelected() {
  const ids = [...selectedMemberIds.value]
  for (const id of ids) {
    const ok = await app.mutate(() => api.removeSkill(id), `已从库中卸载 ${ids.length} 个 Skill`)
    if (!ok) break
    selected.value = selected.value.filter((item) => item !== id)
  }
  removeOpen.value = false
}
async function addToPreset() {
  const preset = app.snapshot?.presets.find((item) => item.id === presetId.value)
  if (!preset) return
  const ok = await app.mutate(
    () =>
      api.savePreset({
        id: preset.id,
        name: preset.name,
        description: preset.description,
        skillIds: [
          ...new Set([
            ...preset.skillIds,
            ...selectedRows.value
              .filter((row) => !row.memberIds.some((id) => preset.skillIds.includes(id)))
              .map((row) => row.id),
          ]),
        ],
      }),
    `已加入预设「${preset.name}」`,
  )
  if (ok) addPresetOpen.value = false
}
const rowUpdateSources = computed(() =>
  Object.fromEntries(
    rows.value.map((row) => {
      const sources = (app.snapshot?.sources ?? []).filter((source) =>
        row.members.some((member) => member.sourceId === source.id),
      )
      return [
        row.id,
        {
          manualOnly:
            sources.length > 0 && sources.every((source) => sourceUpdateState(source).manual),
          pendingId: sources.find((source) => sourceUpdateState(source).needsSetup)?.id,
          localOnly:
            sources.length > 0 &&
            sources.every((source) => sourceUpdateState(source).localReference),
        },
      ]
    }),
  ),
)
const selectedSources = computed(() =>
  (app.snapshot?.sources ?? []).filter((source) =>
    selectedRows.value.some((row) => row.members.some((member) => member.sourceId === source.id)),
  ),
)
const selectedCheckable = computed(() =>
  selectedSources.value.filter((source) => sourceUpdateState(source).canCheck),
)
const selectedPending = computed(() =>
  selectedSources.value.filter((source) => sourceUpdateState(source).needsSetup),
)
const checkingUpdates = ref(false)
const updateActionLabel = computed(() =>
  checkingUpdates.value
    ? '检查中…'
    : selectedCheckable.value.length
      ? '检查更新'
      : selectedPending.value.length
        ? '配置更新来源'
        : selectedSources.value.every((source) => sourceUpdateState(source).localReference)
          ? '跟随本地内容'
          : '暂无可检查来源',
)
function configureUpdateSource(sourceId?: string) {
  router.push({ path: '/updates', query: { tab: 'sources', sourceId } })
}
async function checkUpdates() {
  if (checkingUpdates.value || app.loading) return
  if (!selectedCheckable.value.length) {
    if (selectedPending.value.length) configureUpdateSource(selectedPending.value[0]!.id)
    return
  }
  checkingUpdates.value = true
  const skipped = selectedSources.value.length - selectedCheckable.value.length
  const failures: string[] = []
  let completed = 0
  for (const source of [...selectedCheckable.value]) {
    if (await app.mutate(() => api.checkSource(source.id, false), '来源检查完成')) completed++
    else failures.push(`${app.sourceName(source)}：${app.error}`)
  }
  checkingUpdates.value = false
  app.notice = `已检查 ${completed} 个来源${skipped ? `，跳过 ${skipped} 个未配置或无需检查的来源` : ''}`
  if (failures.length) app.error = failures.join('；')
}
function openAdd() {
  addOpen.value = true
  addMode.value = 'menu'
}
function openCatalog() {
  addOpen.value = false
  router.push('/discover')
}

const sourceFilterOptions = computed(() => [
  { value: '', label: '全部来源' },
  ...(app.snapshot?.sources ?? []).map((item) => ({ value: item.id, label: app.sourceName(item) })),
])
const targetFilterOptions = computed(() => [
  { value: '', label: '全部目标' },
  ...(app.snapshot?.targets ?? []).map((item) => ({ value: item.id, label: app.targetName(item) })),
])
const presetFilterOptions = computed(() => [
  { value: '', label: '全部预设' },
  ...(app.snapshot?.presets ?? []).map((item) => ({ value: item.id, label: item.name })),
])
const updateFilterOptions = [
  { value: '', label: '全部状态' },
  { value: 'available', label: '待处理更新' },
  { value: 'latest', label: '无待处理更新' },
]
const presetIdOptions = computed(() => [
  { value: '', label: '请选择', disabled: true },
  ...(app.snapshot?.presets ?? []).map((item) => ({
    value: item.id,
    label: `${item.name} · ${item.skillIds.length} 项`,
  })),
])
</script>

<template>
  <div class="page library-page" :class="{ 'library-has-selection': selected.length }">
    <header class="page-heading">
      <div>
        <h1 class="page-title">
          Skill 库<span class="title-count">{{ rows.length }}</span>
        </h1>
        <p class="page-subtitle">一个 Skill 一行，点击工具即可分发或取消。</p>
      </div>
      <div class="actions">
        <Button variant="primary" @click="openAdd"><Plus />添加 Skill</Button>
      </div>
    </header>
    <div v-if="operationError" class="callout warning" role="alert" style="margin-bottom: 16px">
      <p>{{ operationError }}</p>
      <Button
        v-if="repairSkillId"
        size="sm"
        :disabled="distributionBusy"
        @click="distribution.previewRepair"
        >重新收录当前内容</Button
      >
    </div>
    <AppDialog
      v-model:open="repairOpen"
      title="重新收录当前内容"
      :description="`此来源包包含 ${repairPreview?.skillCount || 0} 个 Skill，将以统一库当前文件生成新快照。`"
    >
      <p>
        现有链接、预设锁定版本和旧快照保留。完成后可再次分发；Skill
        文件的实际修改也会作为新版本收录。
      </p>
      <p v-if="operationError" class="field-error">{{ operationError }}</p>
      <template #footer>
        <Button :disabled="distributionBusy" @click="repairOpen = false">取消</Button>
        <Button variant="primary" :disabled="distributionBusy" @click="distribution.repair"
          >确认重新收录</Button
        >
      </template>
    </AppDialog>
    <SingleContentSettings v-if="needsSimplification" />
    <p v-if="app.snapshot && app.snapshot.schemaVersion >= 3" class="subtle">
      每个 Skill 保留当前内容。同名且内容不同的导入会独立保留，可在详情的更新页替换当前内容。
    </p>
    <section class="panel library-panel">
      <div class="toolbar library-toolbar">
        <div class="search-field">
          <Search /><input
            v-model="query"
            data-library-search
            aria-label="搜索名称、描述或来源"
            placeholder="搜索名称、描述或来源…"
          />
        </div>
        <div class="segmented" role="group" aria-label="分发筛选">
          <button
            class="segment"
            :class="{ active: scope === 'all' }"
            :aria-pressed="scope === 'all'"
            @click="scope = 'all'"
          >
            全部</button
          ><button
            class="segment"
            :class="{ active: scope === 'distributed' }"
            :aria-pressed="scope === 'distributed'"
            @click="scope = 'distributed'"
          >
            已分发
          </button>
        </div>
        <Button
          class="library-filter-toggle"
          :aria-expanded="filtersOpen"
          aria-controls="library-filters"
          @click="filtersOpen = !filtersOpen"
          ><SlidersHorizontal />筛选<span v-if="filterCount" class="filter-count">{{
            filterCount
          }}</span></Button
        >
        <div class="segmented library-view-mode" role="group" aria-label="视图模式">
          <button
            class="segment"
            :class="{ active: viewMode === 'grid' }"
            :aria-pressed="viewMode === 'grid'"
            title="卡片视图"
            aria-label="卡片视图"
            @click="viewMode = 'grid'"
          >
            <LayoutGrid style="width: 14px; height: 14px" />
          </button>
          <button
            class="segment"
            :class="{ active: viewMode === 'table' }"
            :aria-pressed="viewMode === 'table'"
            title="列表视图"
            aria-label="列表视图"
            @click="viewMode = 'table'"
          >
            <List style="width: 14px; height: 14px" />
          </button>
        </div>
      </div>
      <div v-if="filtersOpen" id="library-filters" class="library-filters">
        <label class="field"
          ><span class="field-label">来源</span
          ><AppSelect v-model="sourceFilter" aria-label="来源" :options="sourceFilterOptions"
        /></label>
        <label class="field"
          ><span class="field-label">目标</span
          ><AppSelect v-model="targetFilter" aria-label="目标" :options="targetFilterOptions"
        /></label>
        <label class="field"
          ><span class="field-label">预设</span
          ><AppSelect v-model="presetFilter" aria-label="预设" :options="presetFilterOptions"
        /></label>
        <label class="field"
          ><span class="field-label">更新</span
          ><AppSelect v-model="updateFilter" aria-label="更新" :options="updateFilterOptions"
        /></label>
        <Button variant="ghost" @click="clearFilters">清除筛选</Button>
      </div>
      <EmptyState
        v-if="!filteredSkills.length"
        :title="app.snapshot?.skills.length ? '没有匹配的 Skill' : '让你的 Skill 在这里归档'"
        :description="
          app.snapshot?.skills.length
            ? '试试其他关键词，或清除当前筛选条件。'
            : '扫描已有目录，将散落在不同工具中的 Skill 统一管理。'
        "
      >
        <Button v-if="app.snapshot?.skills.length" @click="clearFilters">清除筛选</Button
        ><Button v-else variant="primary" @click="router.push('/import')"
          ><FolderInput />归集已有 Skill</Button
        >
      </EmptyState>

      <!-- 卡片网格视图 -->
      <div v-else-if="viewMode === 'grid'" class="library-cards-container">
        <div class="library-grid-header">
          <label class="library-select-all">
            <input
              class="checkbox"
              type="checkbox"
              :checked="allChecked"
              aria-label="全选当前结果"
              @change="toggleAll"
            />
            <span>全选当前结果 ({{ filteredSkills.length }})</span>
          </label>
          <span v-if="selected.length" class="library-selected-count">
            已选 {{ selected.length }} 项
          </span>
        </div>

        <div class="library-grid">
          <article
            v-for="row in table.getRowModel().rows"
            :key="row.id"
            class="library-card"
            :class="{ 'card-selected': selected.includes(row.original.id) }"
            @click="openDetail(row.original)"
          >
            <header class="library-card-header" @click.stop>
              <div class="library-card-title-group">
                <input
                  v-model="selected"
                  class="checkbox"
                  type="checkbox"
                  :value="row.original.id"
                  :aria-label="`选择 ${row.original.name}`"
                />
                <h3 class="library-card-name">
                  <button class="item-name-button" @click.stop="openDetail(row.original)">
                    {{ row.original.name }}
                  </button>
                </h3>
              </div>
              <div class="library-card-actions">
                <Button
                  size="icon"
                  variant="ghost"
                  title="查看详情"
                  aria-label="查看详情"
                  @click.stop="openDetail(row.original)"
                >
                  <MoreHorizontal />
                </Button>
              </div>
            </header>

            <div class="library-card-badges">
              <Badge v-if="row.original.externalPath" tone="blue">本地引用 · 跟随内容</Badge>
              <span
                v-if="row.original.members.length > 1"
                class="library-copy-note"
                :title="row.original.memberSummary"
              >
                {{ row.original.sourceSummary }}
              </span>
            </div>

            <p class="library-card-desc" :title="row.original.description">
              {{ row.original.description || '暂无描述' }}
            </p>

            <div class="library-card-dir" @click.stop>
              <SkillDirectoryActions :skill="row.original" :members="row.original.members" />
              <span
                v-if="row.original.members.length > 1"
                class="library-copy-note"
                style="display: block; margin-top: 4px"
                :title="row.original.memberSummary"
                >{{ row.original.sourceSummary }}</span
              >
              <Button
                v-if="row.original.needsSourceChoice"
                size="sm"
                variant="ghost"
                style="margin-top: 4px"
                @click="openDistribute([row.original.id])"
                >选择实体分发（{{ row.original.entityCount }} 个候选）</Button
              >
            </div>

            <footer class="library-card-footer" @click.stop>
              <div class="library-card-tools-section">
                <span class="library-card-section-label">分发目标：</span>
                <div v-if="row.original.tools.length" class="library-tools">
                  <button
                    v-for="tool in row.original.tools"
                    :key="tool.id"
                    class="tool-toggle"
                    :class="{ active: tool.active, protected: tool.protected }"
                    :aria-pressed="tool.active"
                    :aria-label="`${row.original.name} · ${tool.name} · ${tool.actionLabel}`"
                    :title="tool.hint"
                    :disabled="distributionBusy || app.loading || tool.protected"
                    @click.stop="toggleDistribution(row.original, tool.id)"
                  >
                    <LockKeyhole v-if="tool.protected" />
                    <Check v-else-if="tool.active" />
                    <Plus v-else />
                    <span>{{ tool.name }}</span>
                    <span v-if="tool.external" class="tool-note">外部</span>
                    <span v-else-if="tool.protected" class="tool-note">受管</span>
                  </button>
                </div>
                <Button v-else size="sm" variant="ghost" @click="router.push('/targets')">
                  配置工具
                </Button>
              </div>

              <div class="library-card-update-status">
                <Button
                  v-if="rowUpdateSources[row.original.id]?.pendingId"
                  size="sm"
                  variant="ghost"
                  @click="configureUpdateSource(rowUpdateSources[row.original.id]?.pendingId)"
                >
                  配置更新来源
                </Button>
                <Button
                  v-else-if="rowUpdateSources[row.original.id]?.manualOnly"
                  size="sm"
                  variant="ghost"
                  @click="router.push('/presets')"
                  >手动同步本地包</Button
                >
                <span
                  v-else-if="rowUpdateSources[row.original.id]?.localOnly"
                  class="subtle"
                  style="font-size: 11px"
                >
                  跟随本地内容
                </span>
                <Badge
                  v-else-if="row.original.memberIds.some((id) => updateSkillIds.has(id))"
                  tone="amber"
                >
                  待更新
                </Badge>
              </div>
            </footer>
          </article>
        </div>
      </div>

      <!-- 表格列表视图 -->
      <div v-else class="table-wrap">
        <table class="data-table library-table">
          <thead>
            <tr v-for="group in table.getHeaderGroups()" :key="group.id">
              <th
                v-for="header in group.headers"
                :key="header.id"
                :style="
                  header.id === 'select'
                    ? 'width:42px'
                    : header.id === 'name'
                      ? 'width:38%'
                      : header.id === 'actions'
                        ? 'width:40px'
                        : header.id === 'updates'
                          ? 'width:150px'
                          : ''
                "
              >
                <input
                  v-if="header.id === 'select'"
                  class="checkbox"
                  type="checkbox"
                  :checked="allChecked"
                  aria-label="选择当前结果"
                  @change="toggleAll"
                /><FlexRender
                  v-else
                  :render="header.column.columnDef.header"
                  :props="header.getContext()"
                />
              </th>
            </tr>
          </thead>
          <tbody>
            <tr
              v-for="row in table.getRowModel().rows"
              :key="row.id"
              class="clickable"
              :class="{ 'row-selected': selected.includes(row.original.id) }"
              :aria-selected="selected.includes(row.original.id)"
              @click="openDetail(row.original)"
            >
              <td @click.stop>
                <input
                  v-model="selected"
                  class="checkbox"
                  type="checkbox"
                  :value="row.original.id"
                  :aria-label="`选择 ${row.original.name}`"
                />
              </td>
              <td>
                <div class="name-cell">
                  <div style="min-width: 0">
                    <div class="item-name">
                      <button class="item-name-button" @click.stop="openDetail(row.original)">
                        {{ row.original.name }}
                      </button>
                    </div>
                    <Badge v-if="row.original.externalPath" tone="blue">本地引用 · 跟随内容</Badge>
                    <div class="item-desc">{{ row.original.description }}</div>
                    <SkillDirectoryActions :skill="row.original" :members="row.original.members" />
                    <span
                      v-if="row.original.members.length > 1"
                      class="library-copy-note"
                      :title="row.original.memberSummary"
                      >{{ row.original.sourceSummary }}</span
                    >
                    <Button
                      v-if="row.original.needsSourceChoice"
                      size="sm"
                      variant="ghost"
                      @click.stop="openDistribute([row.original.id])"
                      >选择实体分发</Button
                    >
                  </div>
                </div>
              </td>
              <td class="library-tools-cell" @click.stop>
                <div v-if="row.original.tools.length" class="library-tools">
                  <button
                    v-for="tool in row.original.tools"
                    :key="tool.id"
                    class="tool-toggle"
                    :class="{ active: tool.active, protected: tool.protected }"
                    :aria-pressed="tool.active"
                    :aria-label="`${row.original.name} · ${tool.name} · ${tool.actionLabel}`"
                    :title="tool.hint"
                    :disabled="distributionBusy || app.loading || tool.protected"
                    @click="toggleDistribution(row.original, tool.id)"
                  >
                    <LockKeyhole v-if="tool.protected" />
                    <Check v-else-if="tool.active" />
                    <Plus v-else />
                    <span>{{ tool.name }}</span>
                    <span v-if="tool.external" class="tool-note">外部安装</span
                    ><span v-else-if="tool.protected" class="tool-note">受管</span>
                  </button>
                </div>
                <Button v-else size="sm" variant="ghost" @click="router.push('/targets')"
                  >配置工具</Button
                >
              </td>
              <td @click.stop>
                <Button
                  v-if="rowUpdateSources[row.original.id]?.pendingId"
                  size="sm"
                  variant="ghost"
                  @click="configureUpdateSource(rowUpdateSources[row.original.id]?.pendingId)"
                  >配置更新来源</Button
                >
                <Button
                  v-else-if="rowUpdateSources[row.original.id]?.manualOnly"
                  size="sm"
                  variant="ghost"
                  @click="router.push('/presets')"
                  >手动同步本地包</Button
                >
                <span v-else-if="rowUpdateSources[row.original.id]?.localOnly" class="subtle"
                  >跟随本地内容</span
                >
                <Badge
                  v-else-if="row.original.memberIds.some((id) => updateSkillIds.has(id))"
                  tone="amber"
                  >待更新</Badge
                ><span v-else class="update-empty" aria-label="无待处理更新">—</span>
              </td>
              <td @click.stop>
                <Button
                  size="icon"
                  variant="ghost"
                  title="查看详情"
                  aria-label="查看详情"
                  @click="openDetail(row.original)"
                  ><MoreHorizontal
                /></Button>
              </td>
            </tr>
          </tbody>
        </table>
      </div>
      <AppPagination
        v-if="filteredSkills.length"
        :page="pagination.pageIndex + 1"
        :page-size="pagination.pageSize"
        :total="filteredSkills.length"
        :page-size-options="[10, 20, 50, 100]"
        @update:page="changePage"
        @update:page-size="changePageSize"
      />
    </section>
    <div v-if="selected.length" class="bulk-bar library-bulk">
      <strong>已选 {{ selected.length }} 项</strong
      ><Button
        size="sm"
        variant="primary"
        @click="openBatch('distribute')"
        :disabled="distributionBusy || app.loading"
        ><Link2 />批量分发</Button
      ><Button size="sm" @click="openBatch('revoke')" :disabled="distributionBusy || app.loading"
        ><Unlink />批量取消</Button
      ><Button size="sm" @click="addPresetOpen = true"><Layers3 />加入预设</Button
      ><Button
        size="sm"
        :disabled="
          checkingUpdates || app.loading || (!selectedCheckable.length && !selectedPending.length)
        "
        @click="checkUpdates"
        ><RefreshCcw />{{ updateActionLabel }}</Button
      ><Button
        v-if="selectedCheckable.length && selectedPending.length"
        size="sm"
        @click="configureUpdateSource(selectedPending[0]?.id)"
        >配置更新来源（{{ selectedPending.length }}）</Button
      ><Button size="sm" variant="danger" @click="removeOpen = true"><Trash2 />从库中卸载</Button
      ><span class="bulk-spacer" /><button class="link-button" @click="selected = []">
        取消选择
      </button>
    </div>
    <AppDialog
      :open="batchOpen"
      :title="batchMode === 'distribute' ? '批量分发' : '批量取消分发'"
      :description="batchSummary"
      @update:open="distribution.closeBatch($event)"
    >
      <div class="choice-list">
        <label v-for="target in app.snapshot?.targets" :key="target.id" class="choice">
          <input
            v-model="batchTargetIds"
            type="checkbox"
            class="checkbox"
            :value="target.id"
            :disabled="distributionBusy"
          />
          <div class="choice-main">
            <div class="choice-title">{{ app.targetName(target) }}</div>
            <div class="choice-meta mono">{{ target.path }}</div>
          </div>
        </label>
      </div>
      <p v-if="!app.snapshot?.targets.length" class="subtle">
        尚未配置工具，请先到工具目录添加目标。
      </p>
      <p v-if="batchError" class="field-error" role="alert">{{ batchError }}</p>
      <template #footer>
        <Button :disabled="distributionBusy" @click="distribution.closeBatch(false)">取消</Button>
        <Button
          variant="primary"
          :disabled="distributionBusy || !batchTargetIds.length"
          @click="distribution.submitBatch"
        >
          {{
            distributionBusy ? '处理中…' : batchMode === 'distribute' ? '确认分发' : '确认取消分发'
          }}
        </Button>
      </template>
    </AppDialog>
    <SkillDetailSheet
      v-model:open="detailOpen"
      :skill="detail"
      @distribute="openDistribute([$event])"
    /><DistributionDialog
      v-model:open="distributeOpen"
      :skill-ids="distributeIds"
      :initial-target-ids="distributeTargetIds"
    />
    <AppDialog
      v-model:open="addOpen"
      :fixed-layout="addMode === 'git'"
      title="添加 Skill"
      description="先安装到统一库，之后可按需分发。"
      ><div v-if="addMode === 'menu'" class="choice-list">
        <button class="choice" @click="addMode = 'git'">
          <div class="item-icon"><GitBranch /></div>
          <div class="choice-main" style="text-align: left">
            <div class="choice-title">从 Git 仓库导入</div>
            <div class="choice-meta">指定仓库、引用和可选子目录</div>
          </div></button
        ><button class="choice" @click="addMode = 'folder'">
          <div class="item-icon"><FolderInput /></div>
          <div class="choice-main" style="text-align: left">
            <div class="choice-title">从本地文件夹导入</div>
            <div class="choice-meta">复制内容到统一库；原文件夹保留</div>
          </div></button
        ><button class="choice" @click="openCatalog">
          <div class="item-icon"><Globe2 /></div>
          <div class="choice-main" style="text-align: left">
            <div class="choice-title">从网站查找</div>
            <div class="choice-meta">跨已配置站点搜索和安装</div>
          </div>
        </button>
      </div>
      <GitPackageImport v-else-if="addMode === 'git'" @imported="gitImported" />
      <div v-else>
        <label class="field"
          ><span class="field-label">本地目录</span
          ><DirectoryField v-model="folderPath" :disabled="busy"
        /></label>
        <div class="callout" style="margin-top: 12px">
          快速导入只处理无冲突项。如需替换原目录为软链，请使用完整归集向导。
        </div>
      </div>
      <template #footer
        ><Button v-if="addMode !== 'menu'" :disabled="busy" @click="addMode = 'menu'">返回</Button
        ><Button v-else @click="addOpen = false">取消</Button
        ><Button
          v-if="addMode === 'folder'"
          variant="primary"
          :disabled="busy"
          :loading="busy"
          @click="importFolder"
          >{{ busy ? '正在扫描并导入…' : '扫描并导入' }}</Button
        ></template
      ></AppDialog
    >
    <AppDialog
      v-model:open="addPresetOpen"
      title="加入预设"
      :description="`将 ${selected.length} 个 Skill 加入所选预设，重复成员会自动去重。`"
      ><label class="field"
        ><span class="field-label">预设</span
        ><AppSelect v-model="presetId" aria-label="预设" :options="presetIdOptions" /></label
      ><template #footer
        ><Button @click="addPresetOpen = false">取消</Button
        ><Button variant="primary" :disabled="!presetId" @click="addToPreset"
          >确认加入</Button
        ></template
      ></AppDialog
    >
    <ConfirmDialog
      v-model:open="removeOpen"
      title="从中央库卸载"
      :description="`将尝试卸载所选 ${selected.length} 个 Skill。仍被目标或预设引用的项目不会被级联删除。`"
      confirm-text="确认卸载"
      danger
      :busy="app.loading"
      @confirm="removeSelected"
    />
  </div>
</template>

<style scoped>
.library-tools {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}
.library-tools-cell {
  white-space: normal;
}
.tool-toggle {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  min-height: 32px;
  padding: 5px 9px;
  border: 1px solid var(--line);
  border-radius: 6px;
  background: var(--surface);
  color: var(--muted);
  font-size: 12px;
  cursor: pointer;
}
.tool-toggle svg {
  width: 13px;
  height: 13px;
  flex-shrink: 0;
}
.tool-toggle.active {
  background: var(--blue-soft);
  color: var(--blue);
  border-color: var(--blue);
}
.tool-toggle:hover:not(:disabled) {
  border-color: var(--blue);
  color: var(--blue);
}
.tool-toggle:disabled {
  cursor: default;
  opacity: 0.65;
}
.tool-toggle:focus-visible {
  outline: 2px solid var(--blue);
  outline-offset: 2px;
}
.tool-note,
.library-copy-note {
  font-size: 11px;
  color: var(--muted);
}
.library-copy-note {
  display: block;
  margin-top: 4px;
}

/* 视图切换器 */
.library-view-mode {
  flex-shrink: 0;
}

/* 卡片网格布局 */
.library-cards-container {
  display: flex;
  flex-direction: column;
  gap: 12px;
}
.library-grid-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 4px 6px;
  font-size: 13px;
  color: var(--muted);
}
.library-select-all {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  cursor: pointer;
  user-select: none;
}
.library-selected-count {
  font-size: 12px;
  color: var(--blue);
  font-weight: 500;
}
.library-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(320px, 1fr));
  gap: 16px;
}
.library-card {
  display: flex;
  flex-direction: column;
  background: var(--panel);
  border: 1px solid var(--line);
  border-radius: 10px;
  padding: 16px;
  box-shadow: 0 1px 3px rgb(25 45 80 / 3%);
  transition:
    border-color 0.2s var(--ease-out),
    box-shadow 0.2s var(--ease-out),
    transform 0.2s var(--ease-out);
  cursor: pointer;
}
.library-card:hover {
  border-color: var(--control-line);
  box-shadow: 0 4px 16px rgb(25 45 80 / 6%);
  transform: translateY(-1px);
}
.library-card.card-selected {
  border-color: var(--blue);
  background: var(--blue-soft);
  box-shadow: 0 2px 8px rgb(36 99 212 / 12%);
}
.library-card-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  margin-bottom: 8px;
}
.library-card-title-group {
  display: flex;
  align-items: center;
  gap: 10px;
  min-width: 0;
  flex: 1;
}
.library-card-name {
  margin: 0;
  font-size: 15px;
  font-weight: 600;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.library-card-actions {
  flex-shrink: 0;
}
.library-card-badges {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 6px;
  margin-bottom: 8px;
}
.library-card-desc {
  font-size: 13px;
  color: var(--muted);
  line-height: 1.5;
  margin: 0 0 10px;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
  min-height: 38px;
}
.library-card-dir {
  margin-bottom: 12px;
}
.library-card-footer {
  margin-top: auto;
  padding-top: 12px;
  border-top: 1px solid var(--line);
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.library-card-tools-section {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.library-card-section-label {
  font-size: 11px;
  color: var(--muted);
}
.library-card-update-status {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  min-height: 24px;
}
</style>
