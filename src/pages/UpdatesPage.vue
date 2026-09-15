<script setup lang="ts">
import SourceBindingDialog from '@/components/SourceBindingDialog.vue'
import { sourceUpdateState } from '@/services/sourceUpdates'
import SkillDirectoryActions from '@/components/SkillDirectoryActions.vue'
import { useRoute, useRouter } from 'vue-router'
import AppSelect from '@/components/ui/AppSelect.vue'
import { computed, ref, watch, onBeforeUnmount } from 'vue'
import {
  RefreshCcw,
  GitBranch,
  FolderSync,
  Globe2,
  AlertTriangle,
  Play,
  Pause,
  Eye,
  Download,
  LockKeyhole,
  LoaderCircle,
  Search,
  ChevronDown,
} from 'lucide-vue-next'
import Button from '@/components/ui/Button.vue'
import Badge from '@/components/ui/Badge.vue'
import AppSheet from '@/components/ui/AppSheet.vue'
import AppDialog from '@/components/ui/AppDialog.vue'
import AppPagination from '@/components/ui/AppPagination.vue'
import { api } from '@/services/api'
import { useAppStore } from '@/stores/app'
import { formatDate } from '@/lib/utils'
import type { Source } from '@/services/types'
import { usePagination } from '@/composables/usePagination'

const app = useAppStore()
const route = useRoute()
const router = useRouter()
const tab = ref<'pending' | 'sources'>(route.query.tab === 'sources' ? 'sources' : 'pending')
watch(
  () => route.query.tab,
  (value) => {
    if (value === 'sources') tab.value = 'sources'
  },
)
watch(tab, (value) => {
  if (route.query.tab !== value) void router.replace({ query: { ...route.query, tab: value } })
})
const detail = ref<Source | null>(null)
const removeOpen = ref(false)
const removeBusy = ref(false)
const removeError = ref('')
const removeSource = ref<Source | null>(null)
const removeRevision = ref(0)
function confirmRemove() {
  if (!detail.value || !app.snapshot) return
  removeSource.value = detail.value
  removeRevision.value = app.snapshot.revision
  removeError.value = ''
  removeOpen.value = true
}
async function removeUpdateSource() {
  if (!removeSource.value || removeBusy.value) return
  removeBusy.value = true
  try {
    const ok = await app.mutate(
      () => api.removeUpdateSource(removeSource.value!.id, removeRevision.value),
      '已移除更新管理，Skill 与分发已保留',
    )
    if (ok) {
      removeOpen.value = false
      detailOpen.value = false
    } else removeError.value = app.error
  } finally {
    removeBusy.value = false
  }
}
const bindingSource = ref<Source | null>(null)
const bindingOpen = ref(false)
const sourcePickerOpen = ref(false)
const checkingAll = ref(false)
const detailOpen = ref(false)
const policyOpen = ref(false)
const policyMode = ref<'off' | 'notify' | 'auto'>('notify')
const interval = ref(24)
const scheduleMode = ref<'interval' | 'daily'>('interval')
const dailyTime = ref('09:00')
const policyBusy = ref(false)
const policyError = ref('')
const scheduleOptions = [
  { value: 'interval', label: '按间隔' },
  { value: 'daily', label: '每天固定时间' },
]
const busyId = ref('')
const applying = ref(false)
const stopRequested = ref(false)
const resultsOpen = ref(false)
type CheckResult = {
  id: string
  name: string
  status: 'queued' | 'checking' | 'current' | 'available' | 'attention' | 'failed'
  message: string
}
const checkResults = ref<CheckResult[]>([])
const completedChecks = computed(
  () => checkResults.value.filter((r) => !['queued', 'checking'].includes(r.status)).length,
)
const failedChecks = computed(() => checkResults.value.filter((r) => r.status === 'failed'))
const changedChecks = computed(
  () => checkResults.value.filter((r) => ['available', 'attention'].includes(r.status)).length,
)
const currentCheck = computed(() => checkResults.value.find((r) => r.status === 'checking'))
const checkLabels = {
  queued: '未检查',
  checking: '检查中',
  current: '已是最新',
  available: '有更新',
  attention: '需审阅',
  failed: '失败',
}
onBeforeUnmount(() => {
  stopRequested.value = true
})

const pendingQuery = ref('')
const pendingFilter = ref('all')
const expandedSources = ref<string[]>([])
const applyErrors = ref<Record<string, string>>({})
const pendingFilterOptions = [
  { value: 'all', label: '全部待处理' },
  { value: 'available', label: '可更新' },
  { value: 'attention', label: '需审阅' },
]
const updateGroups = computed(() =>
  (app.snapshot?.sources ?? [])
    .filter(
      (source) =>
        sourceUpdateState(source).canCheck && ['available', 'attention'].includes(source.status),
    )
    .map((source) => {
      const skills = (app.snapshot?.skills ?? []).filter((skill) => skill.sourceId === source.id)
      return {
        source,
        sourceId: source.id,
        name: app.sourceName(source),
        needsReview: source.status === 'attention' || !!source.error,
        summary: source.error || '检测到上游内容变化，可更新到统一库。',
        skills,
        previewNames: skills
          .slice(0, 3)
          .map((s) => s.name)
          .join('、'),
        version: source.version.slice(0, 12) || '未记录',
      }
    })
    .sort((a, b) => Number(b.needsReview) - Number(a.needsReview)),
)
const reviewCount = computed(() => updateGroups.value.filter((g) => g.needsReview).length)
const filteredUpdateGroups = computed(() => {
  const query = pendingQuery.value.trim().toLowerCase()
  return updateGroups.value.filter(
    (g) =>
      (pendingFilter.value === 'all' ||
        (pendingFilter.value === 'attention' ? g.needsReview : !g.needsReview)) &&
      (!query ||
        [g.name, g.source.url, g.source.path, ...g.skills.map((s) => s.name)].some((v) =>
          v.toLowerCase().includes(query),
        )),
  )
})
const {
  page: pendingPage,
  pageSize: pendingPageSize,
  pagedItems: pagedUpdateGroups,
  resetPage: resetPendingPage,
} = usePagination(filteredUpdateGroups, { initialPageSize: 10 })
watch([pendingQuery, pendingFilter], resetPendingPage)
function toggleMembers(id: string) {
  expandedSources.value = expandedSources.value.includes(id)
    ? expandedSources.value.filter((v) => v !== id)
    : [...expandedSources.value, id]
}
function clearPendingFilters() {
  pendingQuery.value = ''
  pendingFilter.value = 'all'
}

const allSources = computed(() =>
  (app.snapshot?.sources ?? []).map((source) => ({
    ...source,
    updateState: sourceUpdateState(source),
  })),
)
const configuredSources = computed(() =>
  allSources.value.filter((source) => source.updateState.canCheck),
)
const checkableSources = computed(() =>
  allSources.value.filter((source) => source.updateState.canCheck),
)
const pendingSources = computed(() =>
  allSources.value.filter((source) => source.updateState.needsSetup),
)
const manualSourceCount = computed(
  () =>
    allSources.value.filter((source) => source.updateState.manual && !source.updateState.removed)
      .length,
)
const headerAction = computed(() =>
  checkableSources.value.length
    ? '检查更新'
    : pendingSources.value.length
      ? '配置更新来源'
      : '暂无可检查来源',
)
const detailState = computed(() => (detail.value ? sourceUpdateState(detail.value) : null))
watch(
  [() => route.query.sourceId, () => app.snapshot?.revision],
  () => {
    const source = app.snapshot?.sources.find((source) => source.id === route.query.sourceId)
    if (source && sourceUpdateState(source).needsSetup) configureSource(source)
    if (detail.value)
      detail.value = app.snapshot?.sources.find((source) => source.id === detail.value?.id) || null
  },
  { immediate: true },
)
function configureSource(source: Source) {
  sourcePickerOpen.value = false
  bindingSource.value = source
  bindingOpen.value = true
}
function chooseSource() {
  if (pendingSources.value.length === 1) configureSource(pendingSources.value[0]!)
  else if (pendingSources.value.length) sourcePickerOpen.value = true
}
function bindingCompleted() {
  router.replace({ query: { ...route.query, sourceId: undefined } })
}
const {
  page: sourcesPage,
  pageSize: sourcesPageSize,
  pagedItems: pagedSources,
} = usePagination(configuredSources, { initialPageSize: 10 })

const detailSkills = computed(() =>
  (app.snapshot?.skills ?? []).filter((s) => s.sourceId === detail.value?.id),
)
const {
  page: detailSkillsPage,
  pageSize: detailSkillsSize,
  pagedItems: pagedDetailSkills,
  resetPage: resetDetailSkillsPage,
} = usePagination(detailSkills, { initialPageSize: 10 })

const sourceIcon = (kind: string) =>
  kind === 'git'
    ? GitBranch
    : ['folder', 'local', 'local_reference'].includes(kind)
      ? FolderSync
      : Globe2
const statusInfo = (status: string): [string, 'green' | 'amber' | 'red'] =>
  status === 'local_reference'
    ? ['本地引用', 'green']
    : ['current', 'healthy'].includes(status)
      ? ['正常', 'green']
      : status === 'available'
        ? ['有更新', 'amber']
        : status === 'detached'
          ? ['待配置来源', 'amber']
          : status === 'attention'
            ? ['需审阅', 'amber']
            : ['检查失败', 'red']
async function check(source: Source, apply = false) {
  const state = sourceUpdateState(source)
  if (busyId.value || checkingAll.value) return
  if (state.needsSetup) {
    configureSource(source)
    return
  }
  if (!state.canCheck) return
  if (!apply) {
    await runChecks([source])
    return
  }
  busyId.value = source.id
  applying.value = true
  try {
    delete applyErrors.value[source.id]
    const ok = await app.mutate(
      () => api.checkSource(source.id, true),
      `${app.sourceName(source)} 已更新入库`,
    )
    if (!ok) applyErrors.value[source.id] = app.error || '更新失败，请重试'
    else if (app.snapshot?.sources.find((s) => s.id === source.id)?.status === 'attention')
      app.notice = `${app.sourceName(source)} 已处理，仍有事项需要审阅`
  } finally {
    busyId.value = ''
    applying.value = false
  }
}
async function runChecks(sources: Source[]) {
  if (checkingAll.value || busyId.value || !sources.length) return
  stopRequested.value = false
  checkingAll.value = true
  checkResults.value = sources.map((source) => ({
    id: source.id,
    name: app.sourceName(source),
    status: 'queued',
    message: '',
  }))
  try {
    for (const row of checkResults.value) {
      if (stopRequested.value) break
      busyId.value = row.id
      row.status = 'checking'
      try {
        const snapshot = await api.checkSource(row.id, false)
        app.snapshot = snapshot
        const source = snapshot.sources.find((s) => s.id === row.id)
        if (!source || !['available', 'attention', 'current', 'healthy'].includes(source.status))
          throw new Error(source?.error || '未获得有效检查结果')
        row.status =
          source.status === 'available'
            ? 'available'
            : source.status === 'attention'
              ? 'attention'
              : 'current'
        row.message =
          source.error ||
          (row.status === 'available'
            ? '可在待处理更新中查看并更新入库'
            : row.status === 'attention'
              ? '请查看来源详情后处理'
              : '无需更新')
      } catch (error) {
        row.status = 'failed'
        row.message = error instanceof Error ? error.message : String(error)
      }
    }
  } finally {
    busyId.value = ''
    checkingAll.value = false
  }
}
async function checkAll() {
  if (checkingAll.value || busyId.value) return
  if (!checkableSources.value.length) {
    tab.value = 'sources'
    chooseSource()
    return
  }
  await runChecks([...checkableSources.value])
}
async function retryFailed() {
  const ids = new Set(failedChecks.value.map((r) => r.id))
  await runChecks(checkableSources.value.filter((s) => ids.has(s.id)))
}
function openSource(source: Source) {
  detail.value = source
  resetDetailSkillsPage()
  detailOpen.value = true
}
function editPolicy(source: Source) {
  if (sourceUpdateState(source).needsSetup) {
    configureSource(source)
    return
  }
  detail.value = source
  policyMode.value = source.policy.mode
  interval.value = source.policy.intervalHours
  scheduleMode.value = source.policy.dailyTime ? 'daily' : 'interval'
  dailyTime.value = source.policy.dailyTime || '09:00'
  policyError.value = ''
  policyOpen.value = true
}
async function savePolicy() {
  if (!detail.value || policyBusy.value) return
  if (scheduleMode.value === 'daily' && !/^([01]\d|2[0-3]):[0-5]\d$/.test(dailyTime.value)) {
    policyError.value = '请选择每天执行的时间'
    return
  }
  const sourceId = detail.value.id
  policyBusy.value = true
  policyError.value = ''
  try {
    const ok = await app.mutate(
      () =>
        api.setPolicy(
          sourceId,
          policyMode.value,
          interval.value,
          scheduleMode.value === 'daily' ? dailyTime.value : undefined,
        ),
      '来源更新计划已保存',
    )
    if (ok) {
      detail.value = app.snapshot?.sources.find((source) => source.id === sourceId) || null
      policyOpen.value = false
    } else policyError.value = app.error
  } finally {
    policyBusy.value = false
  }
}

const policyModeOptions = [
  { value: 'off', label: '关闭' },
  { value: 'notify', label: '仅检查提醒' },
  { value: 'auto', label: '自动更新入库' },
]
const intervalOptions = [
  { value: 12, label: '每 12 小时' },
  { value: 24, label: '每天' },
  { value: 168, label: '每周' },
]
</script>

<template>
  <div class="page">
    <header class="page-heading">
      <div>
        <h1 class="page-title">更新中心</h1>
        <p class="page-subtitle">内容更新先进入中央库；只有跟随目标会同步，固定版本保持不变。</p>
      </div>
      <Button
        variant="primary"
        :disabled="checkingAll || !!busyId || (!checkableSources.length && !pendingSources.length)"
        @click="checkAll"
        ><LoaderCircle v-if="checkingAll" class="check-spinner" /><RefreshCcw v-else />{{
          checkingAll ? `检查中 ${completedChecks}/${checkResults.length}` : headerAction
        }}</Button
      >
    </header>
    <section v-if="checkResults.length" class="check-progress" aria-label="检查进度">
      <div class="check-progress-main" role="status" aria-live="polite">
        <LoaderCircle v-if="checkingAll" class="check-spinner" :size="18" />
        <div>
          <strong>{{
            checkingAll
              ? `正在检查：${currentCheck?.name || '准备中'}`
              : stopRequested
                ? '已停止后续检查'
                : '检查完成'
          }}</strong>
          <p>
            {{ completedChecks }} / {{ checkResults.length }} 个来源 ·
            {{ changedChecks }} 个有更新或需审阅 · {{ failedChecks.length }} 个失败
          </p>
          <p v-if="checkingAll" class="choice-meta">
            {{
              stopRequested
                ? '等待当前来源完成，不再启动下一项。'
                : '仅检查上游变化，不应用更新。网络较慢时单个来源可能需要几分钟。'
            }}
          </p>
        </div>
      </div>
      <div class="actions">
        <Button
          v-if="checkingAll"
          size="sm"
          :disabled="stopRequested"
          @click="stopRequested = true"
          >{{ stopRequested ? '等待当前项完成…' : '停止后续检查' }}</Button
        >
        <Button v-else-if="failedChecks.length" size="sm" :disabled="!!busyId" @click="retryFailed"
          >重试失败项</Button
        >
        <Button size="sm" @click="resultsOpen = true">查看结果</Button>
        <Button v-if="!checkingAll && changedChecks" size="sm" @click="tab = 'pending'"
          >查看待处理</Button
        >
        <Button v-if="!checkingAll" size="sm" variant="ghost" @click="checkResults = []"
          >收起</Button
        >
      </div>
      <progress
        v-if="checkingAll"
        :value="completedChecks"
        :max="checkResults.length"
        aria-label="已完成来源"
      />
    </section>
    <div v-if="pendingSources.length" class="callout" style="margin-bottom: 16px">
      {{ pendingSources.length }} 个来源尚未配置更新地址，现有 Skill 可正常使用和分发。
      <Button size="sm" variant="ghost" @click="chooseSource">配置更新来源</Button>
    </div>
    <div v-if="manualSourceCount" class="callout" style="margin-bottom: 12px">
      {{ manualSourceCount }} 个本地来源由文件夹手动同步，不参与定时更新。
      <Button size="sm" variant="ghost" @click="router.push('/presets')">前往预设同步本地包</Button>
    </div>
    <div class="segmented">
      <button class="segment" :class="{ active: tab === 'pending' }" @click="tab = 'pending'">
        待处理更新</button
      ><button class="segment" :class="{ active: tab === 'sources' }" @click="tab = 'sources'">
        来源与计划
      </button>
    </div>
    <section v-if="tab === 'pending'" class="panel pending-panel">
      <div class="pending-toolbar">
        <div class="pending-totals">
          <strong>待处理来源 {{ updateGroups.length }}</strong
          ><span>可更新 {{ updateGroups.length - reviewCount }} · 需审阅 {{ reviewCount }}</span>
        </div>
        <div class="pending-controls">
          <div class="search-field">
            <Search /><input
              v-model="pendingQuery"
              aria-label="搜索待处理来源或 Skill"
              placeholder="搜索来源或 Skill"
            />
          </div>
          <AppSelect
            v-model="pendingFilter"
            :options="pendingFilterOptions"
            aria-label="待处理状态"
          />
        </div>
      </div>
      <div class="list-stack pending-data">
        <div v-if="!filteredUpdateGroups.length" class="pending-empty">
          <RefreshCcw :size="28" />
          <h3>{{ updateGroups.length ? '没有匹配的待处理项' : '暂无待处理更新' }}</h3>
          <p>
            {{
              updateGroups.length
                ? '更换关键词或筛选条件，再查看其他来源。'
                : '检查来源后，新版本和需要审阅的事项会出现在这里。'
            }}
          </p>
          <Button v-if="updateGroups.length" @click="clearPendingFilters">清除筛选</Button
          ><Button
            v-else
            :disabled="
              checkingAll || !!busyId || (!checkableSources.length && !pendingSources.length)
            "
            @click="checkAll"
            >{{ checkingAll ? '正在检查…' : headerAction }}</Button
          >
        </div>
        <article
          v-for="group in pagedUpdateGroups"
          :key="group.sourceId"
          class="pending-card"
          :class="{ 'pending-review': group.needsReview }"
        >
          <div class="pending-card-header">
            <div class="item-icon"><component :is="sourceIcon(group.source.kind)" /></div>
            <div class="pending-source">
              <button class="item-name-button" @click="openSource(group.source)">
                {{ group.name }}
              </button>
              <div class="pending-location mono" :title="group.source.url || group.source.path">
                {{ group.source.url || group.source.path }}
              </div>
            </div>
            <Badge :tone="group.needsReview ? 'amber' : 'blue'">{{
              group.needsReview ? '需审阅' : '可更新'
            }}</Badge>
          </div>
          <p class="pending-summary">{{ group.summary }}</p>
          <p v-if="applyErrors[group.sourceId]" class="field-error" role="alert">
            {{ applyErrors[group.sourceId] }}
          </p>
          <div class="pending-meta">
            <span
              >当前库内版本 <code>{{ group.version }}</code></span
            ><span>库内 {{ group.skills.length }} 个成员</span
            ><span>检查于 {{ formatDate(group.source.lastChecked) }}</span>
          </div>
          <div class="pending-card-footer">
            <button
              class="pending-members-toggle"
              :aria-expanded="expandedSources.includes(group.sourceId)"
              :aria-controls="`members-${group.sourceId}`"
              @click="toggleMembers(group.sourceId)"
            >
              <ChevronDown
                :size="14"
                :class="{ expanded: expandedSources.includes(group.sourceId) }"
              /><span
                >{{ group.previewNames || '暂无库内成员'
                }}{{ group.skills.length > 3 ? ` 等 ${group.skills.length} 个` : '' }}</span
              >
            </button>
            <div class="actions">
              <Button size="sm" :disabled="!!busyId || checkingAll" @click="check(group.source)"
                ><RefreshCcw />重新检查</Button
              ><Button
                v-if="group.needsReview"
                size="sm"
                variant="primary"
                @click="openSource(group.source)"
                ><Eye />查看处理原因</Button
              ><Button
                v-else
                size="sm"
                variant="primary"
                :disabled="!!busyId || checkingAll"
                @click="check(group.source, true)"
                ><LoaderCircle
                  v-if="busyId === group.sourceId && applying"
                  class="check-spinner"
                /><Download v-else />{{
                  busyId === group.sourceId && applying ? '更新中…' : '更新入库'
                }}</Button
              >
            </div>
          </div>
          <div
            v-if="expandedSources.includes(group.sourceId)"
            :id="`members-${group.sourceId}`"
            class="pending-members"
          >
            <p class="choice-meta">这里列出来源在库中的成员，不代表每个成员都有改动。</p>
            <div v-for="skill in group.skills" :key="skill.id" class="pending-member">
              <span>{{ skill.name }}</span
              ><SkillDirectoryActions :skill="skill" />
            </div>
          </div>
        </article>
      </div>
      <AppPagination
        v-if="filteredUpdateGroups.length"
        v-model:page="pendingPage"
        v-model:page-size="pendingPageSize"
        :total="filteredUpdateGroups.length"
      />
      <div class="pending-note">
        <LockKeyhole :size="14" /><span
          >更新后仅同步跟随关系；固定版本保留。新增、移除成员按包与预设规则处理。</span
        >
      </div>
    </section>
    <section v-else class="panel">
      <div class="table-wrap">
        <table class="data-table">
          <thead>
            <tr>
              <th>来源</th>
              <th style="width: 100px">类型</th>
              <th style="width: 90px">成员</th>
              <th style="width: 135px">策略</th>
              <th style="width: 150px">上次检查</th>
              <th style="width: 120px">状态</th>
              <th style="width: 180px"></th>
            </tr>
          </thead>
          <tbody>
            <tr v-if="!configuredSources.length">
              <td colspan="7" class="subtle" style="padding: 24px; text-align: center">
                通过 Git 或网站导入后，远程来源会显示在这里。本地文件夹在预设中手动扫描并确认同步。
              </td>
            </tr>
            <tr
              v-for="source in pagedSources"
              :key="source.id"
              class="clickable"
              @click="openSource(source)"
            >
              <td>
                <div class="name-cell">
                  <div class="item-icon"><component :is="sourceIcon(source.kind)" /></div>
                  <div>
                    <div class="item-name">{{ app.sourceName(source) }}</div>
                    <div class="item-desc">{{ source.url || source.path }}</div>
                  </div>
                </div>
              </td>
              <td>
                {{
                  source.kind === 'local_reference'
                    ? '本地引用'
                    : source.kind === 'git'
                      ? 'Git 仓库'
                      : ['folder', 'local'].includes(source.kind)
                        ? '本地文件夹'
                        : '站点目录'
                }}
              </td>
              <td>{{ app.snapshot?.skills.filter((s) => s.sourceId === source.id).length }}</td>
              <td v-if="source.kind === 'local_reference'">跟随本地内容</td>
              <td v-else-if="source.updateState.needsSetup">配置来源后可设置</td>
              <td v-else>
                {{
                  source.policy.mode === 'off'
                    ? '已关闭'
                    : source.policy.mode === 'auto'
                      ? '自动更新'
                      : '仅提醒'
                }}
                ·
                {{
                  source.policy.dailyTime
                    ? `每天 ${source.policy.dailyTime}`
                    : `每 ${source.policy.intervalHours} 小时`
                }}
              </td>
              <td>
                {{ formatDate(source.lastChecked) }}
                <div v-if="source.policy.mode !== 'off' && source.nextCheck" class="item-desc">
                  下次：{{ formatDate(source.nextCheck) }}
                </div>
              </td>
              <td>
                <Badge v-if="busyId === source.id">{{ applying ? '更新中' : '检查中' }}</Badge>
                <Badge v-else :tone="statusInfo(source.status)[1]">{{
                  statusInfo(source.status)[0]
                }}</Badge>
              </td>
              <td @click.stop>
                <div class="actions">
                  <Button
                    size="sm"
                    :disabled="
                      !!busyId ||
                      checkingAll ||
                      (!source.updateState.canCheck &&
                        !source.updateState.needsSetup &&
                        !source.updateState.collected)
                    "
                    @click="check(source)"
                    ><LoaderCircle v-if="busyId === source.id" class="check-spinner" />{{
                      busyId === source.id
                        ? applying
                          ? '更新中…'
                          : '检查中…'
                        : source.updateState.action
                    }}</Button
                  ><Button
                    size="sm"
                    :disabled="!source.updateState.canCheck || checkingAll || !!busyId"
                    @click="editPolicy(source)"
                    >设置时间</Button
                  >
                </div>
              </td>
            </tr>
          </tbody>
        </table>
      </div>
      <AppPagination
        v-if="configuredSources.length"
        v-model:page="sourcesPage"
        v-model:page-size="sourcesPageSize"
        :total="configuredSources.length"
      />
    </section>
    <AppDialog
      v-model:open="resultsOpen"
      title="本次检查结果"
      description="检查不会应用更新；失败项可单独重试。"
      large
    >
      <div class="list-stack">
        <div v-for="result in checkResults" :key="result.id" class="check-result-row">
          <div>
            <strong>{{ result.name }}</strong>
            <p>
              {{
                result.message ||
                (result.status === 'checking' ? '正在获取并比较来源内容…' : '尚未执行')
              }}
            </p>
          </div>
          <Badge
            :tone="
              result.status === 'failed'
                ? 'red'
                : ['available', 'attention'].includes(result.status)
                  ? 'amber'
                  : 'neutral'
            "
            >{{ checkLabels[result.status] }}</Badge
          >
        </div>
      </div>
      <template #footer
        ><Button :disabled="checkingAll || !!busyId || !failedChecks.length" @click="retryFailed"
          >重试失败项</Button
        ><Button @click="resultsOpen = false">关闭</Button></template
      >
    </AppDialog>
    <AppSheet
      v-model:open="detailOpen"
      :title="app.sourceName(detail) || '来源详情'"
      :description="detail?.url || detail?.path"
      ><template #toolbar
        ><div class="actions">
          <Button
            v-if="detail && ['available', 'attention'].includes(detail.status)"
            variant="primary"
            :disabled="!!busyId || checkingAll"
            @click="check(detail, true)"
            ><Download />{{
              busyId === detail.id && applying
                ? '更新中…'
                : detail.status === 'attention' || detail.error
                  ? '尝试更新入库'
                  : '更新入库'
            }}</Button
          >
          <Button
            variant="primary"
            :disabled="
              !!busyId || checkingAll || (!detailState?.canCheck && !detailState?.needsSetup)
            "
            @click="detail && check(detail, false)"
            ><LoaderCircle v-if="busyId === detail?.id" class="check-spinner" /><RefreshCcw
              v-else
            />{{ busyId === detail?.id ? '检查中…' : detailState?.action }}</Button
          ><Button
            :disabled="!!busyId || checkingAll || !detailState?.canCheck"
            @click="detail && editPolicy(detail)"
            >更新设置</Button
          >
        </div></template
      >
      <p v-if="detail && applyErrors[detail.id]" class="field-error" role="alert">
        {{ applyErrors[detail.id] }}
      </p>
      <div v-if="detail?.kind === 'local_reference'" class="callout">
        直接使用本地包和现有运行环境，内容随原目录变化；不进行快照更新，也不自动添加新成员。
      </div>
      <div v-if="detailState?.needsSetup" class="callout">
        此来源尚未配置更新地址。配置原始 Git 仓库或独立源码目录后即可检查更新，现有 Skill
        可正常使用和分发。
      </div>
      <div v-else-if="detail?.error" class="callout warning">
        <AlertTriangle style="width: 15px; display: inline; vertical-align: -3px" />
        {{ detail.error }}
      </div>
      <dl class="detail-list">
        <div class="detail-row">
          <dt>类型</dt>
          <dd>{{ detail?.kind }}</dd>
        </div>
        <div class="detail-row">
          <dt>跟踪引用</dt>
          <dd class="mono">
            {{
              detail?.kind === 'local_reference'
                ? '跟随本地内容'
                : detail?.reference || '由站点提供'
            }}
          </dd>
        </div>
        <div class="detail-row">
          <dt>扫描范围</dt>
          <dd class="mono">{{ detail?.scanSubdir || detail?.path || detail?.url }}</dd>
        </div>
        <div class="detail-row">
          <dt>更新策略</dt>
          <dd v-if="detail?.kind === 'local_reference'">跟随本地内容，无定时任务</dd>
          <dd v-else>
            {{ policyModeOptions.find((option) => option.value === detail?.policy.mode)?.label }} ·
            {{
              detail?.policy.dailyTime
                ? `每天 ${detail.policy.dailyTime}`
                : `每 ${detail?.policy.intervalHours} 小时`
            }}
          </dd>
        </div>
        <div class="detail-row">
          <dt>下次检查</dt>
          <dd>{{ formatDate(detail?.nextCheck || '') }}</dd>
        </div>
      </dl>
      <h3 class="panel-title" style="margin: 20px 0 10px">成员 Skill</h3>
      <div class="list-stack">
        <div v-for="skill in pagedDetailSkills" :key="skill.id" class="list-row">
          <div class="list-row-main">
            <div class="list-row-title">{{ skill.name }}</div>
            <SkillDirectoryActions :skill="skill" />
            <div class="list-row-meta">v{{ skill.version }} · 新增成员按包订阅与预设设置处理</div>
          </div>
        </div>
      </div>
      <AppPagination
        v-if="detailSkills.length > 10"
        v-model:page="detailSkillsPage"
        v-model:page-size="detailSkillsSize"
        :total="detailSkills.length"
        compact
        :show-size-changer="false"
      />
      <template #footer>
        <Button :disabled="!!busyId || checkingAll || removeBusy" @click="confirmRemove"
          >移除更新管理</Button
        >
      </template>
    </AppSheet>
    <AppDialog
      :open="removeOpen"
      title="移除更新管理"
      :description="removeSource ? app.sourceName(removeSource) : ''"
      @update:open="!removeBusy && (removeOpen = $event)"
    >
      <p>
        停止此来源的更新任务，并从更新中心移除。已入库 Skill、预设、分发软链和原始源码目录均保留。
      </p>
      <p v-if="removeError" class="field-error" role="alert">{{ removeError }}</p>
      <template #footer>
        <Button :disabled="removeBusy" @click="removeOpen = false">取消</Button>
        <Button :disabled="removeBusy" @click="removeUpdateSource">{{
          removeBusy ? '移除中…' : '确认移除'
        }}</Button>
      </template>
    </AppDialog>
    <AppDialog
      v-model:open="sourcePickerOpen"
      title="配置更新来源"
      description="选择需要配置的来源，核对并保存更新地址后加入来源与计划。"
    >
      <div class="list-stack" style="max-height: 360px; overflow-y: auto">
        <div v-for="source in pendingSources" :key="source.id" class="list-row">
          <div class="list-row-main">
            <div class="list-row-title">{{ app.sourceName(source) }}</div>
            <div class="list-row-meta">{{ source.path }}</div>
          </div>
          <Button size="sm" @click="configureSource(source)">配置</Button>
        </div>
      </div>
    </AppDialog>
    <SourceBindingDialog
      v-model:open="bindingOpen"
      :source="bindingSource"
      @completed="bindingCompleted"
      @update:open="!$event && bindingCompleted()"
    />
    <AppDialog
      v-model:open="policyOpen"
      title="来源更新计划"
      :description="`${detail ? app.sourceName(detail) : ''} · 按本机时区执行，关闭策略后停止定时检查。`"
      ><div class="form-grid">
        <label class="field"
          ><span class="field-label">策略</span
          ><AppSelect
            v-model="policyMode"
            aria-label="策略"
            :options="policyModeOptions"
            :disabled="policyBusy" /></label
        ><label class="field">
          <span class="field-label">执行方式</span>
          <AppSelect
            v-model="scheduleMode"
            aria-label="执行方式"
            :options="scheduleOptions"
            :disabled="policyBusy || policyMode === 'off'"
          /> </label
        ><label v-if="scheduleMode === 'daily'" class="field">
          <span class="field-label">每天执行时间（本机时区）</span>
          <input
            v-model="dailyTime"
            type="time"
            step="60"
            class="input"
            aria-label="每天执行时间"
            :disabled="policyBusy || policyMode === 'off'"
          /> </label
        ><label v-else class="field"
          ><span class="field-label">检查间隔（小时）</span
          ><AppSelect
            v-model="interval"
            aria-label="检查间隔（小时）"
            :options="intervalOptions"
            :disabled="policyBusy || policyMode === 'off'"
        /></label>
      </div>
      <p v-if="policyError" role="alert" class="policy-error">{{ policyError }}</p>
      <p class="subtle" style="margin-top: 12px">
        完全退出应用后停止检查；重新打开后补检查已到期的计划。
      </p>
      <div class="callout" style="margin-top: 12px">
        自动更新只收集到中央库；目标是否同步由各分发关系的“跟随更新”决定。
      </div>
      <template #footer
        ><Button :disabled="policyBusy" @click="policyOpen = false">取消</Button
        ><Button variant="primary" :disabled="policyBusy" @click="savePolicy">{{
          policyBusy ? '保存中…' : '保存'
        }}</Button></template
      ></AppDialog
    >
  </div>
</template>

<style scoped>
.policy-error {
  color: var(--danger, #c53030);
  margin-top: 12px;
}
.update-skill-member {
  margin-top: 8px;
  font-size: 13px;
}
</style>

<style scoped>
.check-progress {
  padding: 14px 16px;
  border: 1px solid var(--line);
  border-radius: 8px;
  background: var(--surface);
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 12px;
}
.check-progress-main {
  display: flex;
  align-items: flex-start;
  gap: 10px;
  flex: 1;
  min-width: 200px;
}
.check-progress-main strong {
  display: block;
  overflow-wrap: anywhere;
  font-size: 13px;
}
.check-progress-main p {
  margin: 4px 0 0;
  font-size: 12px;
  color: var(--muted);
}
.check-progress progress {
  flex-basis: 100%;
  width: 100%;
  height: 4px;
  accent-color: var(--blue);
}
.check-result-row {
  display: flex;
  gap: 16px;
  justify-content: space-between;
  padding: 12px 0;
  border-bottom: 1px solid var(--line);
}
.check-result-row > div {
  min-width: 0;
}
.check-result-row p {
  white-space: pre-wrap;
  overflow-wrap: anywhere;
  color: var(--muted);
  font-size: 12px;
  margin: 6px 0 0;
}
.check-spinner {
  flex-shrink: 0;
  animation: check-spin 1s linear infinite;
}
@keyframes check-spin {
  to {
    transform: rotate(360deg);
  }
}
@media (prefers-reduced-motion: reduce) {
  .check-spinner {
    animation: none;
  }
}
</style>
<style scoped>
.pending-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  padding: 16px;
  border-bottom: 1px solid var(--line);
}
.pending-totals {
  display: flex;
  flex-direction: column;
  gap: 5px;
  font-size: 13px;
  flex-shrink: 0;
}
.pending-totals span {
  font-size: 12px;
  color: var(--muted);
}
.pending-controls {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}
.pending-controls :deep(.select) {
  width: 140px;
}
.pending-data {
  padding: 16px;
  gap: 12px;
}
.pending-card {
  padding: 16px;
  border: 1px solid var(--line);
  border-radius: 10px;
  background: var(--panel);
  flex-shrink: 0;
}
.pending-review {
  border-left: 3px solid var(--amber, #d18c2d);
}
.pending-card-header {
  display: flex;
  align-items: center;
  gap: 12px;
}
.pending-source {
  flex: 1;
  min-width: 0;
}
.pending-location {
  font-size: 11px;
  color: var(--muted);
  overflow: hidden;
  white-space: nowrap;
  text-overflow: ellipsis;
  margin-top: 4px;
}
.pending-summary {
  margin: 14px 0 10px;
  font-size: 13px;
  line-height: 1.6;
  overflow-wrap: anywhere;
  white-space: pre-wrap;
  max-height: 120px;
  overflow: auto;
}
.pending-meta {
  display: flex;
  flex-wrap: wrap;
  gap: 8px 18px;
  color: var(--muted);
  font-size: 11px;
}
.pending-card-footer {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding-top: 14px;
  margin-top: 14px;
  border-top: 1px solid var(--line);
}
.pending-members-toggle {
  border: 0;
  background: none;
  color: var(--muted);
  display: flex;
  gap: 6px;
  align-items: center;
  font-size: 12px;
  min-width: 0;
  cursor: pointer;
  padding: 4px 0;
}
.pending-members-toggle span {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.pending-members-toggle svg {
  flex-shrink: 0;
  transition: transform 0.15s;
}
.pending-members-toggle svg.expanded {
  transform: rotate(180deg);
}
.pending-card-footer .actions {
  flex-shrink: 0;
}
.pending-members {
  margin-top: 12px;
  max-height: 220px;
  overflow: auto;
  overscroll-behavior: contain;
}
.pending-member {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 8px 0;
  font-size: 12px;
  border-bottom: 1px solid var(--line);
}
.pending-member > span {
  overflow-wrap: anywhere;
  min-width: 0;
}
.pending-note {
  display: flex;
  gap: 8px;
  align-items: center;
  padding: 12px 16px;
  border-top: 1px solid var(--line);
  font-size: 11px;
  color: var(--muted);
}
.pending-note svg {
  flex-shrink: 0;
}
.pending-empty {
  text-align: center;
  margin: auto;
  padding: 24px;
  color: var(--muted);
}
.pending-empty h3 {
  color: var(--ink);
  font-size: 15px;
}
.pending-empty p {
  font-size: 13px;
}
@media (max-width: 1050px) {
  .pending-toolbar {
    align-items: stretch;
    flex-direction: column;
    gap: 10px;
  }
  .pending-controls .search-field {
    flex: 1;
    max-width: none;
  }
}
</style>
