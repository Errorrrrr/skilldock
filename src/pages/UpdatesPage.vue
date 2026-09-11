<script setup lang="ts">
import SourceBindingDialog from '@/components/SourceBindingDialog.vue'
import { sourceUpdateState } from '@/services/sourceUpdates'
import SkillDirectoryActions from '@/components/SkillDirectoryActions.vue'
import { useRoute, useRouter } from 'vue-router'
import AppSelect from '@/components/ui/AppSelect.vue'
import { computed, ref, watch } from 'vue'
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
const bindingSource = ref<Source | null>(null)
const bindingOpen = ref(false)
const sourcePickerOpen = ref(false)
const checkingAll = ref(false)
const detailOpen = ref(false)
const policyOpen = ref(false)
const policyMode = ref<'off' | 'notify' | 'auto'>('notify')
const interval = ref(24)
const busyId = ref('')
const updateGroups = computed(() =>
  (app.snapshot?.sources ?? [])
    .filter((source) => ['available', 'attention'].includes(source.status))
    .map((source) => ({
      sourceId: source.id,
      summary: source.error || '来源内容有新版本',
      skills: (app.snapshot?.skills ?? [])
        .filter((skill) => skill.sourceId === source.id)
        .map((skill) => skill.id),
      version: source.version.slice(0, 12),
    })),
)
const {
  page: pendingPage,
  pageSize: pendingPageSize,
  pagedItems: pagedUpdateGroups,
} = usePagination(updateGroups, { initialPageSize: 10 })

const allSources = computed(() =>
  (app.snapshot?.sources ?? []).map((source) => ({
    ...source,
    updateState: sourceUpdateState(source),
  })),
)
const configuredSources = computed(() =>
  allSources.value.filter((source) => !source.updateState.needsSetup),
)
const checkableSources = computed(() =>
  allSources.value.filter((source) => source.updateState.canCheck),
)
const pendingSources = computed(() =>
  allSources.value.filter((source) => source.updateState.needsSetup),
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
  if (state.needsSetup) {
    configureSource(source)
    return
  }
  if (!state.canCheck || busyId.value) return
  busyId.value = source.id
  const ok = await app.mutate(
    () => api.checkSource(source.id, apply),
    apply ? `${app.sourceName(source)} 已更新入库` : `${app.sourceName(source)} 检查完成`,
  )
  busyId.value = ''
  return ok
}
async function checkAll() {
  if (checkingAll.value || busyId.value) return
  if (!checkableSources.value.length) {
    tab.value = 'sources'
    chooseSource()
    return
  }
  checkingAll.value = true
  const skipped = allSources.value.length - checkableSources.value.length
  let completed = 0
  const failed: string[] = []
  for (const source of [...checkableSources.value]) {
    if (await check(source, false)) completed++
    else failed.push(`${app.sourceName(source)}：${app.error}`)
  }
  checkingAll.value = false
  app.notice = `已检查 ${completed} 个来源${skipped ? `，跳过 ${skipped} 个未配置或无需检查的来源` : ''}`
  if (failed.length) app.error = failed.join('；')
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
  policyOpen.value = true
}
async function savePolicy() {
  if (!detail.value) return
  const ok = await app.mutate(
    () => api.setPolicy(detail.value!.id, policyMode.value, interval.value),
    '来源更新策略已保存',
  )
  if (ok) policyOpen.value = false
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
        ><RefreshCcw />{{ checkingAll ? '检查中…' : headerAction }}</Button
      >
    </header>
    <div v-if="pendingSources.length" class="callout" style="margin-bottom: 16px">
      {{ pendingSources.length }} 个来源尚未配置更新地址，现有 Skill 可正常使用和分发。
      <Button size="sm" variant="ghost" @click="chooseSource">配置更新来源</Button>
    </div>
    <div class="segmented">
      <button class="segment" :class="{ active: tab === 'pending' }" @click="tab = 'pending'">
        待处理更新</button
      ><button class="segment" :class="{ active: tab === 'sources' }" @click="tab = 'sources'">
        来源与计划
      </button>
    </div>
    <section v-if="tab === 'pending'" class="panel">
      <div class="panel-header">
        <h3 class="panel-title">发现的变化</h3>
        <Badge v-if="updateGroups.length" tone="amber">{{ updateGroups.length }} 组待处理</Badge>
      </div>
      <p v-if="!updateGroups.length" class="page-subtitle" style="padding: 20px">
        {{
          checkableSources.length
            ? '暂未发现待处理更新，可检查已配置来源。'
            : pendingSources.length
              ? '配置原始来源后，即可检查上游更新。'
              : '当前没有需要检查更新的来源，本地引用直接跟随原目录。'
        }}
      </p>
      <div class="list-stack" style="padding: 12px">
        <div v-for="group in pagedUpdateGroups" :key="group.sourceId" class="list-row">
          <div class="item-icon"><RefreshCcw /></div>
          <div class="list-row-main">
            <div class="list-row-title">
              {{ app.sourceName(app.snapshot?.sources.find((s) => s.id === group.sourceId)) }}
            </div>
            <div class="list-row-meta">{{ group.summary }}</div>
            <div v-for="skillId in group.skills" :key="skillId" class="update-skill-member">
              <span>{{
                app.snapshot?.skills.find((skill) => skill.id === skillId)?.name || skillId
              }}</span>
              <SkillDirectoryActions :skill-id="skillId" />
            </div>
          </div>
          <div class="mono">{{ group.version }}</div>
          <Badge tone="amber">待确认</Badge
          ><Button
            size="sm"
            @click="
              app.snapshot?.sources.find((s) => s.id === group.sourceId) &&
              openSource(app.snapshot.sources.find((s) => s.id === group.sourceId)!)
            "
            ><Eye />查看</Button
          ><Button
            size="sm"
            variant="primary"
            :disabled="busyId === group.sourceId"
            @click="
              app.snapshot?.sources.find((s) => s.id === group.sourceId) &&
              check(
                app.snapshot.sources.find((s) => s.id === group.sourceId)!,
                true,
              )
            "
            ><Download />更新入库</Button
          >
        </div>
      </div>
      <AppPagination
        v-if="updateGroups.length"
        v-model:page="pendingPage"
        v-model:page-size="pendingPageSize"
        :total="updateGroups.length"
      />
      <div class="callout" style="margin: 0 12px 12px">
        <LockKeyhole style="width: 15px; display: inline; vertical-align: -3px" />
        固定版本和预设锁定项不会跟随更新；更新后可在 Skill 详情中分别调整。
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
                尚未配置更新来源，配置并保存后会显示在这里。
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
                · {{ source.policy.intervalHours }}h
              </td>
              <td>{{ formatDate(source.lastChecked) }}</td>
              <td>
                <Badge :tone="statusInfo(source.status)[1]">{{
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
                      (!source.updateState.canCheck && !source.updateState.needsSetup)
                    "
                    @click="check(source)"
                    >{{ source.updateState.action }}</Button
                  ><Button
                    size="sm"
                    :disabled="!source.updateState.canCheck || checkingAll || !!busyId"
                    @click="editPolicy(source)"
                    >设置</Button
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
    <AppSheet
      v-model:open="detailOpen"
      :title="app.sourceName(detail) || '来源详情'"
      :description="detail?.url || detail?.path"
      ><div class="actions" style="margin-bottom: 16px">
        <Button
          variant="primary"
          :disabled="
            !!busyId || checkingAll || (!detailState?.canCheck && !detailState?.needsSetup)
          "
          @click="detail && check(detail, false)"
          ><RefreshCcw />{{ detailState?.action }}</Button
        ><Button :disabled="!detailState?.canCheck" @click="detail && editPolicy(detail)"
          >更新设置</Button
        >
      </div>
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
          <dd v-else>{{ detail?.policy.mode }} · 每 {{ detail?.policy.intervalHours }} 小时</dd>
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
            <div class="list-row-meta">
              v{{ skill.version }} · 上游新增成员默认只进入待发现列表，不自动分发
            </div>
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
    /></AppSheet>
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
      title="来源更新策略"
      description="来源整体暂停时，其成员的继承策略也会暂停。"
      ><div class="form-grid">
        <label class="field"
          ><span class="field-label">策略</span
          ><AppSelect v-model="policyMode" aria-label="策略" :options="policyModeOptions" /></label
        ><label class="field"
          ><span class="field-label">检查间隔（小时）</span
          ><AppSelect v-model="interval" aria-label="检查间隔（小时）" :options="intervalOptions"
        /></label>
      </div>
      <div class="callout" style="margin-top: 12px">
        自动更新只收集到中央库；目标是否同步由各分发关系的“跟随更新”决定。
      </div>
      <template #footer
        ><Button @click="policyOpen = false">取消</Button
        ><Button variant="primary" @click="savePolicy">保存</Button></template
      ></AppDialog
    >
  </div>
</template>

<style scoped>
.update-skill-member {
  margin-top: 8px;
  font-size: 13px;
}
</style>
