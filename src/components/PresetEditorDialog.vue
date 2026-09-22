<script setup lang="ts">
import GitPackageImport from './GitPackageImport.vue'
import SkillDirectoryActions from '@/components/SkillDirectoryActions.vue'
import { computed, ref, watch } from 'vue'
import {
  Search,
  FolderSearch,
  Globe2,
  Plus,
  X,
  ChevronDown,
  History,
  CheckCircle2,
  AlertTriangle,
} from 'lucide-vue-next'
import AppDialog from './ui/AppDialog.vue'
import Button from './ui/Button.vue'
import Badge from './ui/Badge.vue'
import DirectoryField from './DirectoryField.vue'
import { api, type PresetFolderPreview, type GitPackageResult } from '@/services/api'
import { useAppStore } from '@/stores/app'
import type { Preset, PresetPackage } from '@/services/types'
import { resolveToolsFromLinks } from '@/services/agentProfiles'

const props = defineProps<{ open: boolean; preset?: Preset | null }>()
const emit = defineEmits<{ 'update:open': [value: boolean] }>()
const app = useAppStore()
const name = ref('')
const description = ref('')
const skillIds = ref<string[]>([])
const selectedExpanded = ref(false)
const selectedMembers = computed(() => {
  const members = new Map(app.snapshot?.skills.map((skill) => [skill.id, skill]))
  return skillIds.value.map((id) => ({ id, name: members.get(id)?.name || id }))
})
function removeMember(id: string) {
  skillIds.value = skillIds.value.filter((memberId) => memberId !== id)
  for (const subscription of subscriptions.value) {
    subscription.selectedIds = subscription.selectedIds.filter((memberId) => memberId !== id)
  }
}

function clearSelection() {
  skillIds.value = []
  for (const subscription of subscriptions.value) subscription.selectedIds = []
}

const query = ref('')
const mode = ref<'library' | 'folder' | 'git' | 'web'>('library')
const folder = ref(app.isNative ? '' : '/Users/demo/Work/new-pack')
const preview = ref<PresetFolderPreview | null>(null)
const removedSelection = ref<string[]>([])
const removedItems = computed(() => preview.value?.removed ?? [])
const pendingRemoved = ref<string[]>([])
const scanned = computed(() => preview.value?.items ?? [])
const subscriptions = ref<PresetPackage[]>([])
const autoAdd = ref(true)

interface MigrationItem {
  path: string
  entity: string
  status: string
  reason: string
}
const migration = ref<MigrationItem[]>([])
const migrationLoading = ref(false)
const migrationChecked = ref(false)
const migrationCollapsed = ref(false)
const activeMigrationTab = ref<'needsSourceRestore' | 'ready'>('needsSourceRestore')
const migrationRestoreItems = computed(() =>
  migration.value.filter((item) => item.status === 'needsSourceRestore'),
)
const migrationReadyItems = computed(() =>
  migration.value.filter((item) => item.status === 'ready'),
)
const migrationNeedsRestoreCount = computed(() => migrationRestoreItems.value.length)

function getTools(links: string[]) {
  return resolveToolsFromLinks(
    links,
    app.snapshot?.targets ?? [],
    app.snapshot?.settings.agentProfiles,
  )
}

const expandedPackages = ref<string[]>([])
function togglePackage(id: string) {
  expandedPackages.value = expandedPackages.value.includes(id)
    ? expandedPackages.value.filter((value) => value !== id)
    : [...expandedPackages.value, id]
}
const selectedPackages = computed(() =>
  subscriptions.value.map((subscription) => ({
    subscription,
    package: app.snapshot?.packages?.find((p) => p.id === subscription.packageId),
    members: (app.snapshot?.skills ?? []).filter((skill) =>
      app.snapshot?.packages
        ?.find((p) => p.id === subscription.packageId)
        ?.memberIds.includes(skill.id),
    ),
  })),
)
function removePackage(packageId: string) {
  const ids = subscriptions.value.find((p) => p.packageId === packageId)?.selectedIds ?? []
  subscriptions.value = subscriptions.value.filter((p) => p.packageId !== packageId)
  const otherSelected = new Set(subscriptions.value.flatMap((p) => p.selectedIds))
  skillIds.value = skillIds.value.filter((id) => !ids.includes(id) || otherSelected.has(id))
}
function selectPackageMember(packageId: string, id: string, selected: boolean) {
  const subscription = subscriptions.value.find((p) => p.packageId === packageId)
  if (!subscription) return
  subscription.selectedIds = selected
    ? [...new Set([...subscription.selectedIds, id])]
    : subscription.selectedIds.filter((memberId) => memberId !== id)
  if (selected) skillIds.value = [...new Set([...skillIds.value, id])]
  else if (!subscriptions.value.some((p) => p.selectedIds.includes(id)))
    skillIds.value = skillIds.value.filter((memberId) => memberId !== id)
}
async function migrationPreview() {
  if (!folder.value.trim()) {
    error.value = '请先输入包含 Skill 的文件夹路径'
    return
  }
  migrationLoading.value = true
  error.value = ''
  try {
    const res = await api.packageMigrationPreview(folder.value.trim())
    migration.value = res.items || []
    migrationChecked.value = true
    migrationCollapsed.value = false
    activeMigrationTab.value =
      migrationRestoreItems.value.length > 0 ? 'needsSourceRestore' : 'ready'
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  } finally {
    migrationLoading.value = false
  }
}
const scanWarning = ref('')
let scanRequest = 0
let editSession = 0
watch(folder, () => {
  scanRequest++
  preview.value = null
  scanSelected.value = []
  error.value = ''
  migration.value = []
  migrationChecked.value = false
  migrationLoading.value = false
  activeMigrationTab.value = 'needsSourceRestore'
})
const scanSelected = ref<string[]>([])
const error = ref('')
const busy = ref(false)
const importing = ref(false)
const filtered = computed(
  () =>
    app.snapshot?.skills.filter(
      (skill) =>
        !selectedPackages.value.some((entry) => entry.package?.memberIds.includes(skill.id)) &&
        (!query.value ||
          `${skill.name} ${skill.description}`.toLowerCase().includes(query.value.toLowerCase())),
    ) ?? [],
)
watch(
  () => [props.open, props.preset?.id] as const,
  ([open]) => {
    editSession++
    scanRequest++
    if (!open) return
    name.value = props.preset?.name || ''
    description.value = props.preset?.description || ''
    skillIds.value = [...(props.preset?.skillIds || [])]
    pendingRemoved.value = []
    removedSelection.value = []
    query.value = ''
    mode.value = 'library'
    selectedExpanded.value = false
    preview.value = null
    scanSelected.value = []
    subscriptions.value = structuredClone(
      (app.snapshot?.presetPackages ?? [])
        .filter((p) => p.presetId === props.preset?.id)
        .map((p) => ({ ...p, selectedIds: [...p.selectedIds], excludedIds: [...p.excludedIds] })),
    )
    autoAdd.value = true
    migration.value = []
    migrationChecked.value = false
    migrationLoading.value = false
    migrationCollapsed.value = false
    activeMigrationTab.value = 'needsSourceRestore'
    scanRequest++
    error.value = ''
  },
)
async function scanFolder() {
  const request = ++scanRequest
  const path = folder.value
  busy.value = true
  error.value = ''
  preview.value = null
  scanSelected.value = []
  removedSelection.value = []
  try {
    const result = await api.previewPresetFolder(path)
    if (request !== scanRequest || !props.open) return
    preview.value = result
    const subscription = subscriptions.value.find((p) => p.packageId === result.packageId)
    if (subscription) autoAdd.value = subscription.autoAdd
    scanSelected.value = result.items
      .filter((item) => {
        if (item.error) return false
        if (!subscription) return true
        const id = item.snapshotId || item.existingId
        return id ? subscription.selectedIds.includes(id) : subscription.autoAdd
      })
      .map((item) => item.path)
    scanWarning.value = result.warnings.join('；')
    if (!result.items.length && !result.packageId) error.value = '未发现可添加的 Skill'
  } catch (e) {
    if (request === scanRequest) error.value = e instanceof Error ? e.message : '扫描失败'
  } finally {
    busy.value = false
  }
}
async function importSelected() {
  if (busy.value || !preview.value || (!scanSelected.value.length && !preview.value.packageId))
    return
  const session = editSession
  const removals = [...removedSelection.value]
  busy.value = true
  importing.value = true
  error.value = ''
  try {
    const result = await api.importPackage(
      preview.value.root,
      scanSelected.value,
      preview.value.revision,
    )
    if (!app.snapshot || result.snapshot.revision >= app.snapshot.revision)
      app.snapshot = result.snapshot
    if (session !== editSession || !props.open) return
    const pkg = result.snapshot.packages.find((p) => p.id === result.packageId)!
    const confirmed = removals.filter((id) => pkg.missingMemberIds.includes(id))
    pendingRemoved.value = [...new Set([...pendingRemoved.value, ...confirmed])]
    const existingSubscription = subscriptions.value.find((p) => p.packageId === result.packageId)
    const previous = existingSubscription?.selectedIds ?? []
    const retainedMissing = previous.filter(
      (id) =>
        pkg.missingMemberIds.includes(id) && !confirmed.includes(id) && skillIds.value.includes(id),
    )
    const nextSelected = [...new Set([...result.skillIds, ...retainedMissing])]
    const otherSelected = new Set(
      subscriptions.value
        .filter((p) => p.packageId !== result.packageId)
        .flatMap((p) => p.selectedIds),
    )
    skillIds.value = [
      ...new Set([
        ...skillIds.value.filter(
          (id) => !previous.includes(id) || nextSelected.includes(id) || otherSelected.has(id),
        ),
        ...nextSelected,
      ]),
    ].filter((id) => !confirmed.includes(id) || otherSelected.has(id))
    if (existingSubscription) {
      existingSubscription.autoAdd = autoAdd.value
      existingSubscription.selectedIds = nextSelected
      existingSubscription.excludedIds = pkg.memberIds.filter((id) => !nextSelected.includes(id))
    }
    if (!subscriptions.value.some((p) => p.packageId === result.packageId)) {
      const pkg = result.snapshot.packages.find((p) => p.id === result.packageId)!
      subscriptions.value.push({
        presetId: props.preset?.id ?? '',
        packageId: result.packageId,
        autoAdd: autoAdd.value,
        selectedIds: result.skillIds,
        excludedIds: pkg.memberIds.filter((id) => !result.skillIds.includes(id)),
      })
    }
    preview.value = null
    mode.value = 'folder'
    app.notice =
      `包已同步，已选 ${result.skillIds.length} 个成员` +
      (confirmed.length ? `；${confirmed.length} 个移除项将在保存预设后生效` : '')
  } catch (e) {
    if (session === editSession) error.value = e instanceof Error ? e.message : '添加失败'
  } finally {
    busy.value = false
    importing.value = false
  }
}
function addGitPackage(result: GitPackageResult, auto: boolean, removedIds: string[]) {
  const pkg = result.snapshot.packages.find((p) => p.id === result.packageId)!
  const old = subscriptions.value.find((p) => p.packageId === pkg.id)
  const previous = old?.selectedIds ?? []
  const chosen = [
    ...new Set([
      ...result.skillIds,
      ...previous.filter((id) => pkg.missingMemberIds.includes(id) && !removedIds.includes(id)),
    ]),
  ]
  const others = new Set(
    subscriptions.value.filter((p) => p.packageId !== pkg.id).flatMap((p) => p.selectedIds),
  )
  skillIds.value = [
    ...new Set([
      ...skillIds.value.filter(
        (id) => !previous.includes(id) || chosen.includes(id) || others.has(id),
      ),
      ...chosen,
    ]),
  ]
  subscriptions.value = subscriptions.value.filter((p) => p.packageId !== pkg.id)
  subscriptions.value.push({
    presetId: props.preset?.id || '',
    packageId: pkg.id,
    autoAdd: auto,
    selectedIds: chosen,
    excludedIds: pkg.memberIds.filter((id) => !chosen.includes(id)),
  })
  pendingRemoved.value = [
    ...new Set([...pendingRemoved.value, ...removedIds.filter((id) => previous.includes(id))]),
  ]
  app.notice = 'Git 集合已同步，保存后生效'
}
async function save() {
  if (name.value.trim().length < 2) {
    error.value = '预设名称至少 2 个字符'
    return
  }
  if (!skillIds.value.length && !props.preset) {
    error.value = '请至少添加一个 Skill'
    return
  }
  busy.value = true
  const ok = await app.mutate(
    () =>
      api.savePreset({
        id: props.preset?.id,
        name: name.value.trim(),
        description: description.value.trim(),
        skillIds: skillIds.value,
        syncApplied: true,
        packages: subscriptions.value.map((subscription) => {
          const pkg = app.snapshot?.packages.find((p) => p.id === subscription.packageId)
          const selectedIds = subscription.selectedIds.filter(
            (id) => skillIds.value.includes(id) && pkg?.memberIds.includes(id),
          )
          return {
            ...subscription,
            selectedIds,
            excludedIds: (pkg?.memberIds ?? []).filter((id) => !selectedIds.includes(id)),
          }
        }),
      }),
    props.preset ? '预设已保存，跟随目标已尝试同步；冲突可在预设详情重试' : '预设已创建',
  )
  busy.value = false
  if (ok) emit('update:open', false)
}
</script>

<template>
  <AppDialog
    :open="open"
    fixed-layout
    :title="preset ? '编辑预设' : '新建预设'"
    description="保存成员组合，单一内容模式下所有受管目标使用当前内容。"
    large
    @update:open="emit('update:open', $event)"
    ><div class="form-grid" style="margin-bottom: 8px">
      <label class="field"
        ><span class="field-label">预设名称</span
        ><input v-model="name" class="input" placeholder="例如：前端交付" /></label
      ><label class="field"
        ><span class="field-label">描述</span
        ><input v-model="description" class="input" placeholder="说明适用场景"
      /></label>
    </div>
    <div class="segmented" style="margin-bottom: 8px">
      <button class="segment" :class="{ active: mode === 'library' }" @click="mode = 'library'">
        <Plus style="width: 13px; display: inline" /> 从库选择</button
      ><button class="segment" :class="{ active: mode === 'folder' }" @click="mode = 'folder'">
        <FolderSearch style="width: 13px; display: inline" /> 从文件夹</button
      ><button class="segment" :class="{ active: mode === 'git' }" @click="mode = 'git'">
        从 Git 集合</button
      ><button class="segment" :class="{ active: mode === 'web' }" @click="mode = 'web'">
        <Globe2 style="width: 13px; display: inline" /> 从网站
      </button>
    </div>
    <section v-if="mode === 'library'">
      <div class="search-field" style="margin-bottom: 10px">
        <Search /><input v-model="query" placeholder="筛选 Skill" />
      </div>
      <div class="choice-list" style="max-height: 280px; overflow: auto">
        <div v-for="skill in filtered" :key="skill.id" class="choice">
          <div class="choice-main">
            <label class="skill-choice-label"
              ><input v-model="skillIds" class="checkbox" type="checkbox" :value="skill.id" />
              <div class="choice-main">
                <div class="choice-title">{{ skill.name }}</div>
                <div class="choice-meta">{{ skill.description }}</div>
              </div></label
            ><SkillDirectoryActions :skill="skill" />
          </div>
          <Badge>{{ skill.externalPath ? '跟随本地内容' : `v${skill.version}` }}</Badge>
        </div>
      </div>
    </section>
    <section v-else-if="mode === 'folder'" class="folder-scan-panel">
      <p class="choice-meta" style="margin: 0 0 8px">本地包仅手动同步到统一库，原目录保留。</p>
      <label class="field"
        ><span class="field-label">包含 Skill 的文件夹</span>
        <div class="folder-scan-input">
          <DirectoryField v-model="folder" :disabled="busy" />
          <Button :disabled="busy" @click="scanFolder"><FolderSearch />扫描文件夹</Button>
        </div></label
      >

      <div class="folder-scan-results">
        <div v-if="scanned.length" class="choice-list" style="margin-top: 12px">
          <div v-for="item in scanned" :key="item.path" class="choice">
            <div class="choice-main">
              <label class="skill-choice-label"
                ><input
                  v-model="scanSelected"
                  class="checkbox"
                  type="checkbox"
                  :value="item.path"
                  :disabled="busy || !!item.error"
                />
                <div class="choice-main">
                  <div class="choice-title">
                    {{ item.name }}
                    <Badge v-if="item.change && item.change !== 'unchanged'">{{
                      item.change === 'added' ? '新增' : '内容变更'
                    }}</Badge>
                  </div>
                  <div class="choice-meta mono">{{ item.path }}</div>
                  <div v-if="item.error" class="field-error">{{ item.error }}</div>
                  <div v-if="item.existingId" class="choice-meta">
                    同一实体已登记，将检查来源关系
                  </div>
                  <div v-if="item.snapshotId" class="choice-meta">
                    同一来源已入库；本次同步内容变化并复用已有成员
                  </div>
                  <div v-if="item.sameName" class="field-error">
                    已有同名的其他来源，不自动合并；分发时需处理同名冲突
                  </div>
                  <div v-if="item.existingLinks?.length" class="existing-tools-row">
                    <span class="existing-tools-label">使用位置：</span>
                    <div class="existing-tools-tags">
                      <span
                        v-for="tool in getTools(item.existingLinks)"
                        :key="tool.name"
                        class="tool-tag"
                        :title="`在 ${tool.name} 中使用：\n${tool.paths.join('\n')}`"
                      >
                        {{ tool.name }}
                      </span>
                    </div>
                  </div>
                </div></label
              ><SkillDirectoryActions :path="item.path" />
            </div>
          </div>
        </div>
        <div v-if="removedItems.length" class="callout" style="margin-top: 12px">
          <strong>来源已移除 {{ removedItems.length }} 项</strong>
          <label v-for="item in removedItems" :key="item.skillId" class="choice">
            <input
              v-model="removedSelection"
              type="checkbox"
              class="checkbox"
              :value="item.skillId"
              :disabled="busy"
            />
            <span
              >从本预设移除 {{ item.name
              }}<span class="choice-meta mono"> · {{ item.path }}</span></span
            >
          </label>
        </div>
      </div>
    </section>
    <GitPackageImport v-else-if="mode === 'git'" draft @imported="addGitPackage" />
    <section v-else class="empty" style="min-height: 200px">
      <div>
        <div class="empty-icon"><Globe2 /></div>
        <h3>先安装，再加入预设</h3>
        <p>网站结果需要先完成内容确认并安装到中央库。请先保存当前预设，再前往安装。</p>
        <RouterLink class="btn btn-primary" to="/discover" @click="emit('update:open', false)"
          >前往发现与安装</RouterLink
        >
      </div>
    </section>
    <div class="git-selection-summary">
      <section v-if="selectedPackages.length" class="list-stack" style="margin-top: 16px">
        <div v-for="entry in selectedPackages" :key="entry.subscription.packageId" class="choice">
          <div class="choice-main">
            <Button
              size="sm"
              variant="ghost"
              :aria-expanded="expandedPackages.includes(entry.subscription.packageId)"
              @click="togglePackage(entry.subscription.packageId)"
              ><ChevronDown />{{ entry.package?.name }} ·
              {{ entry.subscription.selectedIds.length }} /
              {{ entry.members.length }} 个成员</Button
            >
            <div class="choice-meta mono">{{ entry.package?.path }}</div>
            <div
              v-if="expandedPackages.includes(entry.subscription.packageId)"
              class="package-member-list"
            >
              <label v-for="member in entry.members" :key="member.id" class="choice"
                ><input
                  :checked="entry.subscription.selectedIds.includes(member.id)"
                  class="checkbox"
                  type="checkbox"
                  @change="
                    selectPackageMember(
                      entry.subscription.packageId,
                      member.id,
                      ($event.target as HTMLInputElement).checked,
                    )
                  "
                />{{ member.name
                }}<Badge v-if="entry.package?.missingMemberIds?.includes(member.id)" tone="amber"
                  >来源已移除 · 保留旧内容</Badge
                ></label
              >
            </div>
            <p v-for="issue in entry.package?.issues" :key="issue" class="field-error">
              {{ issue }}
            </p>
            <label
              ><input v-model="entry.subscription.autoAdd" type="checkbox" class="checkbox" />
              同步后加入新增成员</label
            >
          </div>
          <Button size="sm" variant="ghost" @click="removePackage(entry.subscription.packageId)"
            >移除包</Button
          >
        </div>
      </section>
      <div v-if="mode === 'folder'" class="migration-section">
        <div class="migration-header-row">
          <Button
            size="sm"
            variant="ghost"
            :disabled="busy || migrationLoading || !folder.trim()"
            @click="migrationPreview"
          >
            <History style="width: 14px; height: 14px; margin-right: 4px" />
            {{ migrationLoading ? '正在检查旧归集迁移…' : '检查旧归集迁移' }}
          </Button>
          <span class="choice-meta" style="font-size: 12px"
            >检查目录内是否含有待恢复的旧本地软链或归集入口</span
          >
        </div>

        <div v-if="migrationChecked" class="migration-panel">
          <div class="migration-panel-header" @click="migrationCollapsed = !migrationCollapsed">
            <div class="migration-panel-title">
              <ChevronDown class="collapse-icon" :class="{ collapsed: migrationCollapsed }" />
              <span>旧归集迁移预检</span>
              <span class="migration-count">共 {{ migration.length }} 项</span>
            </div>
            <div class="migration-panel-badges">
              <Badge v-if="migrationNeedsRestoreCount > 0" tone="amber">
                {{ migrationNeedsRestoreCount }} 项需恢复来源
              </Badge>
              <Badge v-else-if="migration.length > 0" tone="green"> 全部就绪 </Badge>
            </div>
          </div>

          <div v-show="!migrationCollapsed" class="migration-panel-body">
            <div v-if="!migration.length" class="migration-empty subtle">
              <CheckCircle2
                style="
                  width: 15px;
                  height: 15px;
                  color: var(--green, #10b981);
                  display: inline-block;
                  vertical-align: -2px;
                  margin-right: 4px;
                "
              />
              未检测到旧归集软链条目，当前目录下 Skill 实体均可直接作为预设包导入。
            </div>
            <template v-else>
              <div class="migration-cards-grid">
                <div
                  class="migration-stat-card card-amber"
                  :class="{ active: activeMigrationTab === 'needsSourceRestore' }"
                  role="button"
                  tabindex="0"
                  @click="activeMigrationTab = 'needsSourceRestore'"
                  @keydown.enter.space="activeMigrationTab = 'needsSourceRestore'"
                >
                  <div class="stat-card-top">
                    <div class="stat-card-label">
                      <AlertTriangle class="stat-card-icon text-amber" />
                      <span>需恢复来源</span>
                    </div>
                    <span class="stat-card-count text-amber">{{
                      migrationRestoreItems.length
                    }}</span>
                  </div>
                  <div class="stat-card-desc">含历史软链或归集入口，需恢复独立来源</div>
                </div>

                <div
                  class="migration-stat-card card-green"
                  :class="{ active: activeMigrationTab === 'ready' }"
                  role="button"
                  tabindex="0"
                  @click="activeMigrationTab = 'ready'"
                  @keydown.enter.space="activeMigrationTab = 'ready'"
                >
                  <div class="stat-card-top">
                    <div class="stat-card-label">
                      <CheckCircle2 class="stat-card-icon text-green" />
                      <span>可直接登记</span>
                    </div>
                    <span class="stat-card-count text-green">{{ migrationReadyItems.length }}</span>
                  </div>
                  <div class="stat-card-desc">纯净完整本地实体，可直接作为预设包导入</div>
                </div>
              </div>

              <div class="migration-tab-content">
                <div class="migration-tab-header">
                  <span class="migration-tab-title">
                    {{
                      activeMigrationTab === 'needsSourceRestore'
                        ? '需恢复来源条目'
                        : '可直接登记条目'
                    }}
                    （{{
                      (activeMigrationTab === 'needsSourceRestore'
                        ? migrationRestoreItems
                        : migrationReadyItems
                      ).length
                    }}）
                  </span>
                </div>

                <div
                  v-if="
                    !(
                      activeMigrationTab === 'needsSourceRestore'
                        ? migrationRestoreItems
                        : migrationReadyItems
                    ).length
                  "
                  class="migration-empty subtle"
                >
                  <template v-if="activeMigrationTab === 'needsSourceRestore'">
                    <CheckCircle2
                      style="
                        width: 15px;
                        height: 15px;
                        color: var(--green, #10b981);
                        display: inline-block;
                        vertical-align: -2px;
                        margin-right: 4px;
                      "
                    />
                    太棒了！未发现需要恢复来源的条目，所有实体均可正常登记。
                  </template>
                  <template v-else> 暂无可直接登记的条目。 </template>
                </div>

                <div v-else class="migration-items-list">
                  <div
                    v-for="item in activeMigrationTab === 'needsSourceRestore'
                      ? migrationRestoreItems
                      : migrationReadyItems"
                    :key="item.path"
                    class="migration-item-card"
                    :class="{ 'needs-restore': item.status === 'needsSourceRestore' }"
                  >
                    <div class="migration-item-header">
                      <span class="migration-item-path mono" :title="item.path">{{
                        item.path
                      }}</span>
                      <Badge :tone="item.status === 'needsSourceRestore' ? 'amber' : 'green'">
                        {{ item.status === 'needsSourceRestore' ? '需恢复来源' : '可直接登记' }}
                      </Badge>
                    </div>
                    <div
                      v-if="item.entity && item.entity !== item.path"
                      class="migration-item-entity mono"
                    >
                      指向实体：{{ item.entity }}
                    </div>
                    <div class="migration-item-reason choice-meta">
                      {{ item.reason }}
                    </div>
                  </div>
                </div>
              </div>
            </template>
          </div>
        </div>
      </div>
      <section v-if="skillIds.length" class="selected-members" aria-label="已选择成员">
        <div class="selected-members-header">
          <button
            type="button"
            class="selected-members-toggle"
            :aria-expanded="selectedExpanded"
            aria-controls="preset-selected-members"
            @click="selectedExpanded = !selectedExpanded"
          >
            <span>已选择成员</span><span class="selected-members-count">{{ skillIds.length }}</span>
            <ChevronDown :class="{ expanded: selectedExpanded }" />
          </button>
          <Button size="sm" variant="ghost" :disabled="busy" @click="clearSelection"
            >清空选择</Button
          >
        </div>
        <div v-show="selectedExpanded" id="preset-selected-members" class="selected-members-list">
          <div v-for="member in selectedMembers" :key="member.id" class="selected-member-row">
            <span class="selected-member-name" :title="member.name">{{ member.name }}</span>
            <SkillDirectoryActions :skill-id="member.id" />
            <Button
              size="icon"
              variant="ghost"
              :disabled="busy"
              :aria-label="`移除 ${member.name}`"
              title="从本次预设选择中移除"
              @click="removeMember(member.id)"
              ><X
            /></Button>
          </div>
        </div>
      </section>
    </div>
    <template
      #notice
      v-if="
        error ||
        pendingRemoved.length ||
        (mode === 'folder' &&
          (scanWarning || removedItems.length || scanned.length || preview?.packageId))
      "
    >
      <p v-if="mode === 'folder' && (scanned.length || preview?.packageId)" class="choice-meta">
        同步会立即入库；保存预设后关联生效。关闭窗口不撤销入库。
      </p>
      <p v-if="error" class="field-error" role="alert">{{ error }}</p>
      <p v-if="mode === 'folder' && scanWarning" class="field-error">{{ scanWarning }}</p>
      <p v-if="mode === 'folder' && removedItems.length" class="choice-meta">
        来源已移除 {{ removedItems.length }} 项，默认保留；在移除项中选择后，保存预设才生效。
      </p>
      <p v-if="pendingRemoved.length" class="callout">
        已确认 {{ pendingRemoved.length }} 个来源移除项，点击“保存预设”后生效。
      </p>
    </template>
    <template #options v-if="mode === 'folder'">
      <div class="folder-scan-actions">
        <label class="folder-scan-policy"
          ><input v-model="autoAdd" type="checkbox" class="checkbox" />
          <div>
            同步后加入包的新增成员
            <div class="choice-meta">排除项保持不选；关闭后仅保留本次选择。</div>
          </div>
        </label>
        <Button
          v-if="scanned.length || preview?.packageId"
          variant="primary"
          :disabled="busy || (!scanSelected.length && !preview?.packageId)"
          :loading="importing"
          @click="importSelected"
          >{{ importing ? '正在同步…' : '同步包并加入预设' }}</Button
        >
      </div>
    </template>
    <template #footer
      ><Button :disabled="busy" @click="emit('update:open', false)">取消</Button
      ><Button variant="primary" :disabled="busy" :loading="busy && !importing" @click="save">{{
        busy && !importing ? '保存中…' : '保存预设'
      }}</Button></template
    ></AppDialog
  >
</template>

<style scoped>
.package-member-list {
  max-height: 220px;
  overflow: auto;
  margin: 10px 0;
}
.selected-members {
  margin-top: 16px;
  border-top: 1px solid var(--line);
}
.selected-members-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 10px 0;
}
.selected-members-toggle {
  display: flex;
  align-items: center;
  gap: 8px;
  min-height: 32px;
  border: 0;
  background: transparent;
  color: inherit;
  font: inherit;
  font-size: 13px;
  font-weight: 600;
  cursor: pointer;
}
.selected-members-toggle:focus-visible {
  outline: 2px solid var(--blue);
  outline-offset: 3px;
  border-radius: 4px;
}
.selected-members-count {
  min-width: 22px;
  padding: 1px 6px;
  border-radius: 6px;
  background: var(--panel);
  color: var(--blue);
  text-align: center;
  font-size: 12px;
}
.selected-members-toggle svg {
  width: 14px;
  height: 14px;
  color: var(--muted);
  transition: transform 0.15s;
}
.selected-members-toggle svg.expanded {
  transform: rotate(180deg);
}
.selected-members-list {
  max-height: 220px;
  overflow-y: auto;
  overscroll-behavior: contain;
  padding-bottom: 4px;
}
.selected-member-row {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 7px 0;
  border-top: 1px solid var(--line);
}
.selected-member-name {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 13px;
}
.selected-member-row :deep(.skill-directory-actions) {
  margin-top: 0;
  flex-shrink: 0;
}
.selected-member-row > .btn {
  flex-shrink: 0;
  width: 28px;
  height: 28px;
}
@media (max-width: 640px) {
  .selected-member-row {
    flex-wrap: wrap;
  }
  .selected-member-name {
    flex-basis: calc(100% - 45px);
  }
  .selected-member-row :deep(.skill-directory-actions) {
    order: 3;
    width: 100%;
  }
}

/* 扫描结果中的使用位置 Tag */
.existing-tools-row {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-top: 4px;
  flex-wrap: wrap;
}
.existing-tools-label {
  font-size: 12px;
  color: var(--muted);
  flex-shrink: 0;
}
.existing-tools-tags {
  display: flex;
  align-items: center;
  gap: 5px;
  flex-wrap: wrap;
}
.tool-tag {
  display: inline-flex;
  align-items: center;
  font-size: 11px;
  line-height: 1.2;
  padding: 2px 8px;
  border-radius: 9999px;
  background: color-mix(in srgb, var(--blue) 10%, transparent);
  color: var(--blue);
  border: 1px solid color-mix(in srgb, var(--blue) 25%, transparent);
  font-weight: 500;
  cursor: default;
  transition: all 0.15s ease;
}
.tool-tag:hover {
  background: color-mix(in srgb, var(--blue) 18%, transparent);
  border-color: color-mix(in srgb, var(--blue) 40%, transparent);
}

/* 旧归集迁移区域 */
.migration-section {
  margin-top: 14px;
}
.migration-header-row {
  display: flex;
  align-items: center;
  gap: 10px;
  flex-wrap: wrap;
}
.migration-panel {
  margin-top: 8px;
  border: 1px solid var(--line);
  border-radius: 8px;
  background: var(--surface);
  overflow: hidden;
}
.migration-panel-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px 12px;
  cursor: pointer;
  user-select: none;
  background: color-mix(in srgb, var(--surface) 80%, var(--panel));
  border-bottom: 1px solid var(--line);
}
.migration-panel-title {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  font-weight: 600;
}
.collapse-icon {
  width: 14px;
  height: 14px;
  color: var(--muted);
  transition: transform 0.15s ease;
}
.collapse-icon.collapsed {
  transform: rotate(-90deg);
}
.migration-count {
  font-size: 11px;
  color: var(--muted);
  font-weight: normal;
  margin-left: 2px;
}
.migration-panel-badges {
  display: flex;
  align-items: center;
  gap: 6px;
}
.migration-panel-body {
  padding: 10px 12px;
  max-height: 320px;
  overflow-y: auto;
}
.migration-cards-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 10px;
  margin-bottom: 12px;
}
.migration-stat-card {
  padding: 10px 12px;
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
.migration-stat-card:hover {
  border-color: var(--control-line);
  box-shadow: 0 1px 4px rgb(0 0 0 / 4%);
}
.migration-stat-card.active {
  border-width: 1.5px;
  box-shadow: 0 2px 8px rgb(0 0 0 / 6%);
}
.migration-stat-card.card-amber.active {
  border-color: #f59e0b;
  background: color-mix(in srgb, #f59e0b 6%, var(--panel));
}
.migration-stat-card.card-green.active {
  border-color: #10b981;
  background: color-mix(in srgb, #10b981 6%, var(--panel));
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
  width: 15px;
  height: 15px;
}
.text-amber {
  color: #d97706;
}
.text-green {
  color: #059669;
}
.stat-card-count {
  font-size: 15px;
  font-weight: 700;
  font-family: ui-monospace, monospace;
}
.stat-card-desc {
  font-size: 11px;
  color: var(--muted);
  line-height: 1.3;
}
.migration-tab-content {
  border-top: 1px dashed var(--line);
  padding-top: 10px;
}
.migration-tab-header {
  margin-bottom: 8px;
}
.migration-tab-title {
  font-size: 12px;
  font-weight: 600;
  color: var(--muted);
}
@media (max-width: 540px) {
  .migration-cards-grid {
    grid-template-columns: 1fr;
  }
}
.migration-empty {
  font-size: 12px;
  padding: 4px 0;
}
.migration-items-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.migration-item-card {
  padding: 8px 10px;
  border-radius: 6px;
  background: var(--panel);
  border: 1px solid var(--line);
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.migration-item-card.needs-restore {
  border-left: 3px solid #f59e0b;
}
.migration-item-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
}
.migration-item-path {
  font-size: 12px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--ink);
  flex: 1;
}
.migration-item-entity {
  font-size: 11px;
  color: var(--muted);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.migration-item-reason {
  font-size: 12px;
  line-height: 1.4;
}
</style>

<style scoped>
.git-selection-summary {
  max-height: 150px;
  overflow-y: auto;
  overscroll-behavior: contain;
}
</style>

<style scoped>
section.folder-scan-panel {
  display: flex;
  flex-direction: column;
  overflow: hidden !important;
  min-height: 0;
}
.folder-scan-panel > * {
  flex-shrink: 0;
}
.folder-scan-panel > .folder-scan-results {
  flex: 1;
  min-height: 0;
  overflow: auto;
  overscroll-behavior: contain;
}
</style>

<style scoped>
.folder-scan-input,
.folder-scan-actions,
.folder-scan-policy {
  display: flex;
  align-items: center;
  gap: 12px;
}
.folder-scan-input > :first-child {
  flex: 1;
  min-width: 0;
}
.folder-scan-actions {
  justify-content: space-between;
}
.folder-scan-policy {
  font-size: 13px;
}
@media (max-height: 760px) {
  .git-selection-summary {
    max-height: 68px;
  }
}
</style>
