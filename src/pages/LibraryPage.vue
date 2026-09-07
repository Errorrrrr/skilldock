<script setup lang="ts">
import AppSelect from '@/components/ui/AppSelect.vue'
import { computed, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { createColumnHelper, FlexRender, getCoreRowModel, useVueTable } from '@tanstack/vue-table'
import {
  Search,
  Plus,
  GitBranch,
  FolderInput,
  Globe2,
  Link2,
  Trash2,
  Layers3,
  RefreshCcw,
  MoreHorizontal,
  SlidersHorizontal,
} from 'lucide-vue-next'
import Button from '@/components/ui/Button.vue'
import Badge from '@/components/ui/Badge.vue'
import EmptyState from '@/components/ui/EmptyState.vue'
import AppDialog from '@/components/ui/AppDialog.vue'
import ConfirmDialog from '@/components/ui/ConfirmDialog.vue'
import DirectoryField from '@/components/DirectoryField.vue'
import SkillDetailSheet from '@/components/SkillDetailSheet.vue'
import DistributionDialog from '@/components/DistributionDialog.vue'
import { useAppStore } from '@/stores/app'
import { api } from '@/services/api'
import type { Skill } from '@/services/types'

const app = useAppStore()
const router = useRouter()
const route = useRoute()
const query = ref(String(route.query.q || ''))
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
const addOpen = ref(false)
const addMode = ref<'menu' | 'git' | 'folder'>('menu')
const gitUrl = ref('')
const gitRef = ref('HEAD')
const gitSubdir = ref('')
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
              source.id === skill.sourceId && ['available', 'attention'].includes(source.status),
          ),
        )
        .map((skill) => skill.id),
    ),
)
const sourceName = (id: string) =>
  app.snapshot?.sources.find((item) => item.id === id)?.name || '独立来源'
const bindingSummary = (skillId: string) => {
  const rows = app.snapshot?.bindings.filter((item) => item.skillId === skillId) ?? []
  const names = rows.map(
    (item) =>
      app.snapshot?.targets.find((target) => target.id === item.targetId)?.name || '未知目标',
  )
  return names.length > 1 ? `${names[0]} +${names.length - 1}` : names[0] || '未分发'
}
const filteredSkills = computed(() =>
  (app.snapshot?.skills ?? []).filter((skill) => {
    const text = `${skill.name} ${skill.description} ${sourceName(skill.sourceId)}`.toLowerCase()
    return (
      (scope.value === 'all' ||
        app.snapshot?.bindings.some((binding) => binding.skillId === skill.id)) &&
      (!query.value || text.includes(query.value.toLowerCase())) &&
      (!sourceFilter.value || skill.sourceId === sourceFilter.value) &&
      (!targetFilter.value ||
        app.snapshot?.bindings.some(
          (binding) => binding.skillId === skill.id && binding.targetId === targetFilter.value,
        )) &&
      (!presetFilter.value ||
        app.snapshot?.presets
          .find((preset) => preset.id === presetFilter.value)
          ?.skillIds.includes(skill.id)) &&
      (!updateFilter.value ||
        (updateFilter.value === 'available'
          ? updateSkillIds.value.has(skill.id)
          : !updateSkillIds.value.has(skill.id)))
    )
  }),
)
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
const column = createColumnHelper<Skill>()
const columns = [
  column.display({ id: 'select', header: () => '', cell: () => '' }),
  column.accessor('name', { header: '名称', cell: (info) => info.getValue() }),
  column.accessor('sourceId', { header: '来源', cell: (info) => sourceName(info.getValue()) }),
  column.display({
    id: 'bindings',
    header: '分发至',
    cell: (info) => bindingSummary(info.row.original.id),
  }),
  column.display({
    id: 'updates',
    header: '更新',
    cell: (info) => (updateSkillIds.value.has(info.row.original.id) ? '可更新' : '最新'),
  }),
  column.display({ id: 'actions', header: '', cell: () => '' }),
]
const table = useVueTable({
  get data() {
    return filteredSkills.value
  },
  columns,
  getCoreRowModel: getCoreRowModel(),
})
function openDetail(skill: Skill) {
  detail.value = skill
  detailOpen.value = true
}
function openDistribute(ids: string[]) {
  distributeIds.value = ids
  distributeOpen.value = true
}
async function importGit() {
  busy.value = true
  const ok = await app.mutate(
    () => api.importGit(gitUrl.value, gitRef.value || 'HEAD', gitSubdir.value || undefined),
    'Git 仓库已导入中央库',
  )
  busy.value = false
  if (ok) addOpen.value = false
}
async function importFolder() {
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
  const ids = [...selected.value]
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
        skillIds: [...new Set([...preset.skillIds, ...selected.value])],
      }),
    `已加入预设「${preset.name}」`,
  )
  if (ok) addPresetOpen.value = false
}
async function checkUpdates() {
  const sourceIds = [
    ...new Set(
      (app.snapshot?.skills.filter((item) => selected.value.includes(item.id)) ?? []).map(
        (item) => item.sourceId,
      ),
    ),
  ]
  for (const sourceId of sourceIds)
    await app.mutate(() => api.checkSource(sourceId, false), '已完成所选 Skill 的来源检查')
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
  ...(app.snapshot?.sources ?? []).map((item) => ({ value: item.id, label: item.name })),
])
const targetFilterOptions = computed(() => [
  { value: '', label: '全部目标' },
  ...(app.snapshot?.targets ?? []).map((item) => ({ value: item.id, label: item.name })),
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
          Skill 库<span class="title-count">{{ app.snapshot?.skills.length || 0 }}</span>
        </h1>
        <p class="page-subtitle">集中保管，按需分发。</p>
      </div>
      <div class="actions">
        <Button variant="primary" @click="openAdd"><Plus />添加 Skill</Button>
      </div>
    </header>
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
                      ? 'width:36%'
                      : header.id === 'actions'
                        ? 'width:40px'
                        : header.id === 'updates'
                          ? 'width:90px'
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
                    <div class="item-desc">{{ row.original.description }}</div>
                  </div>
                </div>
              </td>
              <td>
                <span class="source-name">{{ sourceName(row.original.sourceId) }}</span>
              </td>
              <td class="binding-name">{{ bindingSummary(row.original.id) }}</td>
              <td>
                <Badge v-if="updateSkillIds.has(row.original.id)" tone="amber">待更新</Badge
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
      <div v-if="filteredSkills.length" class="library-footer">
        <span>{{ filteredSkills.length }} 个 Skill</span
        ><RouterLink to="/targets" class="link-button">管理分发目标</RouterLink>
      </div>
    </section>
    <div v-if="selected.length" class="bulk-bar library-bulk">
      <strong>已选 {{ selected.length }} 项</strong
      ><Button size="sm" variant="primary" @click="openDistribute(selected)"><Link2 />分发</Button
      ><Button size="sm" @click="addPresetOpen = true"><Layers3 />加入预设</Button
      ><Button size="sm" @click="checkUpdates"><RefreshCcw />检查更新</Button
      ><Button size="sm" variant="danger" @click="removeOpen = true"><Trash2 />从库中卸载</Button
      ><span class="bulk-spacer" /><button class="link-button" @click="selected = []">
        取消选择
      </button>
    </div>
    <SkillDetailSheet
      v-model:open="detailOpen"
      :skill="detail"
      @distribute="openDistribute([$event])"
    /><DistributionDialog v-model:open="distributeOpen" :skill-ids="distributeIds" />
    <AppDialog
      v-model:open="addOpen"
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
            <div class="choice-meta">扫描并安装内容，不替换原目录</div>
          </div></button
        ><button class="choice" @click="openCatalog">
          <div class="item-icon"><Globe2 /></div>
          <div class="choice-main" style="text-align: left">
            <div class="choice-title">从网站查找</div>
            <div class="choice-meta">跨已配置站点搜索和安装</div>
          </div>
        </button>
      </div>
      <div v-else-if="addMode === 'git'" class="list-stack">
        <label class="field"
          ><span class="field-label">仓库地址</span
          ><input v-model="gitUrl" class="input" placeholder="https://..."
        /></label>
        <div class="form-grid">
          <label class="field"
            ><span class="field-label">引用</span
            ><input v-model="gitRef" class="input" placeholder="main / tag / commit" /></label
          ><label class="field"
            ><span class="field-label">子目录（可选）</span
            ><input v-model="gitSubdir" class="input" placeholder="skills/review"
          /></label>
        </div>
        <div class="callout">仓库包含多个 Skill 时，核心会按目录识别；共享资源保持在来源包内。</div>
      </div>
      <div v-else>
        <label class="field"
          ><span class="field-label">本地目录</span><DirectoryField v-model="folderPath"
        /></label>
        <div class="callout" style="margin-top: 12px">
          快速导入只处理无冲突项。如需替换原目录为软链，请使用完整归集向导。
        </div>
      </div>
      <template #footer
        ><Button v-if="addMode !== 'menu'" @click="addMode = 'menu'">返回</Button
        ><Button v-else @click="addOpen = false">取消</Button
        ><Button v-if="addMode === 'git'" variant="primary" :disabled="busy" @click="importGit"
          >导入仓库</Button
        ><Button
          v-if="addMode === 'folder'"
          variant="primary"
          :disabled="busy"
          @click="importFolder"
          >扫描并导入</Button
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
