<script setup lang="ts">
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
import { api } from '@/services/api'
import { useAppStore } from '@/stores/app'
import { formatDate } from '@/lib/utils'
import type { Source } from '@/services/types'

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
const sourceIcon = (kind: string) =>
  kind === 'git' ? GitBranch : ['folder', 'local'].includes(kind) ? FolderSync : Globe2
const statusInfo = (status: string): [string, 'green' | 'amber' | 'red'] =>
  ['current', 'healthy'].includes(status)
    ? ['正常', 'green']
    : status === 'available'
      ? ['有更新', 'amber']
      : status === 'detached'
        ? ['已归集', 'amber']
        : status === 'attention'
          ? ['需审阅', 'amber']
          : ['检查失败', 'red']
async function check(source: Source, apply = false) {
  busyId.value = source.id
  await app.mutate(
    () => api.checkSource(source.id, apply),
    apply ? `${source.name} 已更新入库` : `${source.name} 检查完成`,
  )
  busyId.value = ''
}
async function checkAll() {
  for (const source of app.snapshot?.sources ?? []) {
    if (source.status !== 'detached') await check(source, false)
  }
}
function openSource(source: Source) {
  detail.value = source
  detailOpen.value = true
}
function editPolicy(source: Source) {
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
      <Button variant="primary" @click="checkAll"><RefreshCcw />检查更新</Button>
    </header>
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
        暂未发现待处理更新。可点击“检查更新”获取来源的最新状态。
      </p>
      <div class="list-stack" style="padding: 12px">
        <div v-for="group in updateGroups" :key="group.sourceId" class="list-row">
          <div class="item-icon"><RefreshCcw /></div>
          <div class="list-row-main">
            <div class="list-row-title">
              {{ app.snapshot?.sources.find((s) => s.id === group.sourceId)?.name }}
            </div>
            <div class="list-row-meta">
              {{ group.summary }} ·
              {{
                group.skills
                  .map((id) => app.snapshot?.skills.find((s) => s.id === id)?.name)
                  .join('、')
              }}
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
            <tr
              v-for="source in app.snapshot?.sources"
              :key="source.id"
              class="clickable"
              @click="openSource(source)"
            >
              <td>
                <div class="name-cell">
                  <div class="item-icon"><component :is="sourceIcon(source.kind)" /></div>
                  <div>
                    <div class="item-name">{{ source.name }}</div>
                    <div class="item-desc">{{ source.url || source.path }}</div>
                  </div>
                </div>
              </td>
              <td>
                {{
                  source.kind === 'git'
                    ? 'Git 仓库'
                    : ['folder', 'local'].includes(source.kind)
                      ? '本地文件夹'
                      : '站点目录'
                }}
              </td>
              <td>{{ app.snapshot?.skills.filter((s) => s.sourceId === source.id).length }}</td>
              <td>
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
                  <Button size="sm" :disabled="busyId === source.id" @click="check(source)"
                    >检查</Button
                  ><Button size="sm" @click="editPolicy(source)">设置</Button>
                </div>
              </td>
            </tr>
          </tbody>
        </table>
      </div>
    </section>
    <AppSheet
      v-model:open="detailOpen"
      :title="detail?.name || '来源详情'"
      :description="detail?.url || detail?.path"
      ><div class="actions" style="margin-bottom: 16px">
        <Button
          variant="primary"
          :disabled="detail?.status === 'detached'"
          @click="detail && check(detail, false)"
          ><RefreshCcw />检查更新</Button
        ><Button @click="detail && editPolicy(detail)">更新设置</Button>
      </div>
      <div v-if="detail?.error" class="callout warning">
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
          <dd class="mono">{{ detail?.reference || '由站点提供' }}</dd>
        </div>
        <div class="detail-row">
          <dt>扫描范围</dt>
          <dd class="mono">{{ detail?.scanSubdir || detail?.path || detail?.url }}</dd>
        </div>
        <div class="detail-row">
          <dt>更新策略</dt>
          <dd>{{ detail?.policy.mode }} · 每 {{ detail?.policy.intervalHours }} 小时</dd>
        </div>
        <div class="detail-row">
          <dt>下次检查</dt>
          <dd>{{ formatDate(detail?.nextCheck || '') }}</dd>
        </div>
      </dl>
      <h3 class="panel-title" style="margin: 20px 0 10px">成员 Skill</h3>
      <div class="list-stack">
        <div
          v-for="skill in app.snapshot?.skills.filter((s) => s.sourceId === detail?.id)"
          :key="skill.id"
          class="list-row"
        >
          <div class="list-row-main">
            <div class="list-row-title">{{ skill.name }}</div>
            <div class="list-row-meta">
              v{{ skill.version }} · 上游新增成员默认只进入待发现列表，不自动分发
            </div>
          </div>
        </div>
      </div></AppSheet
    >
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
