<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { ArchiveRestore, Check } from 'lucide-vue-next'
import { CheckboxRoot, CheckboxIndicator } from 'reka-ui'
import Button from '@/components/ui/Button.vue'
import Badge from '@/components/ui/Badge.vue'
import AppDialog from '@/components/ui/AppDialog.vue'
import {
  api,
  isNative,
  type BackupEntry,
  type RestorePreview,
  type CleanupReport,
} from '@/services/api'
import { useAppStore } from '@/stores/app'
import { formatDate } from '@/lib/utils'
const app = useAppStore()
const backups = ref<BackupEntry[]>([])
const cleanupReports = ref<CleanupReport[]>([])
const backupGroups = computed(() => {
  const groups = new Map<
    string,
    { key: string; time: string; entries: BackupEntry[]; directoryCount: number }
  >()
  for (const backup of backups.value) {
    const timestamp = Date.parse(backup.createdAt)
    const time = Number.isFinite(timestamp) ? formatDate(backup.createdAt) : '时间未知'
    const key = Number.isFinite(timestamp) ? String(Math.floor(timestamp / 60000)) : backup.id
    const group = groups.get(key) ?? { key, time, entries: [], directoryCount: 0 }
    group.entries.push(backup)
    group.directoryCount += backup.paths.length
    groups.set(key, group)
  }
  return [...groups.values()]
})
const selected = ref<BackupEntry | null>(null)
const cleanupOnly = ref(false)
const dialogOpen = computed(() => !!selected.value || cleanupOnly.value)
const preview = ref<RestorePreview | null>(null)
const previewLoading = ref(false)
const previewError = ref('')
const cleanupSelection = ref<string[]>([])
const selectionCount = computed(() => cleanupSelection.value.length)
const cannotSubmit = computed(
  () => busy.value || previewLoading.value || !preview.value || !!previewError.value,
)
const cleanupLabels: Record<string, string> = {
  ready: '可清理',
  referenced: '被引用 · 保留',
  modified: '已修改 · 保留',
  blocked: '无法校验 · 保留',
  missing: '已不存在',
  removed: '已清理',
  retained: '已保留',
  pending: '待执行',
  failed: '待重试',
}
let previewRequest = 0
async function openRestore(backup?: BackupEntry) {
  if (busy.value) return
  const request = ++previewRequest
  selected.value = backup ?? null
  cleanupOnly.value = !backup
  preview.value = null
  cleanupSelection.value = []
  previewError.value = ''
  previewLoading.value = true
  try {
    const result = backup ? await api.previewRestore(backup.id) : await api.previewObjectCleanup()
    if (request === previewRequest) {
      preview.value = result
      cleanupSelection.value = result.items
        .filter((item) => item.status === 'ready')
        .map((item) => item.digest)
    }
  } catch (error) {
    if (request === previewRequest) previewError.value = String(error)
  } finally {
    if (request === previewRequest) previewLoading.value = false
  }
}
function closeDialog() {
  if (busy.value) return
  previewRequest++
  selected.value = null
  cleanupOnly.value = false
}
function toggleCleanup(digest: string, checked: boolean | 'indeterminate') {
  cleanupSelection.value = cleanupSelection.value.filter((value) => value !== digest)
  if (checked === true) cleanupSelection.value.push(digest)
}
async function retryCleanup(id: string) {
  busy.value = true
  await app.mutate(() => api.retryObjectCleanup(id), '已重新检查清理任务，请查看结果')
  busy.value = false
  await refresh()
}
const retention = ref(0)
const busy = ref(false)
const error = ref('')
const labels: Record<string, string> = {
  available: '可恢复',
  restored: '已恢复',
  expired: '已清理',
  pruning: '清理待重试',
  conflict: '存在冲突',
  unavailable: '不可恢复',
}
async function refresh() {
  try {
    const [nextBackups, nextCleanups] = await Promise.all([
      api.listBackups(),
      api.listObjectCleanups(),
    ])
    backups.value = nextBackups
    cleanupReports.value = nextCleanups
    error.value = ''
  } catch (e) {
    error.value = String(e)
  }
}
watch(
  () => app.snapshot?.revision,
  () => {
    retention.value = app.snapshot?.settings.backupRetention ?? 0
    void refresh()
  },
  { immediate: true },
)
async function save() {
  busy.value = true
  await app.mutate(() => api.settings({ backupRetention: retention.value }), '原目录留存设置已保存')
  busy.value = false
  await refresh()
}
async function restore(includeCleanup = true) {
  if (!preview.value || cannotSubmit.value) return
  const revision = preview.value.revision
  const backupId = selected.value?.id
  const choices = includeCleanup
    ? preview.value.items
        .filter((item) => item.status === 'ready' && cleanupSelection.value.includes(item.digest))
        .map(({ digest, fingerprint }) => ({ digest, fingerprint }))
    : []
  if (!backupId && !choices.length) return
  busy.value = true
  const success = await app.mutate(
    () =>
      backupId
        ? api.restoreBackup(backupId, revision, choices)
        : api.cleanupObjects(revision, choices),
    backupId ? '原目录已恢复，请查看清理结果' : '清理检查已完成，请查看结果',
  )
  busy.value = false
  if (success) closeDialog()
  await refresh()
}
</script>
<template>
  <section class="panel backup-panel">
    <div class="backup-heading">
      <div>
        <h2 class="section-title">恢复与清理</h2>
        <p class="page-subtitle">管理归集后的原目录留存，查看历史恢复记录和统一库残留。</p>
      </div>
      <div class="backup-controls">
        <label for="backup-retention">原目录保留最近</label
        ><input
          id="backup-retention"
          v-model.number="retention"
          type="number"
          min="0"
          max="100"
          step="1"
          class="input"
        /><span>批次</span
        ><Button
          :disabled="busy || !Number.isInteger(retention) || retention < 0 || retention > 100"
          @click="save"
          >保存</Button
        >
      </div>
    </div>
    <div class="backup-data-scroll">
      <p class="muted">
        新库默认为
        0：归集成功并校验后清理临时原目录，不长期保留副本。操作失败或中断时仍可恢复，每个来源的“撤销上次更新”也不受影响。
      </p>
      <p class="muted">
        旧库沿用已有设置。设为 1–100
        可保留最近若干次归集的原目录；修改设置并保存后立即清理超额留存，改为 0
        会清理全部可安全删除的旧副本。异常或已修改的副本继续保留并记录原因。
      </p>
      <Button
        v-if="
          backups.some((backup) => backup.status === 'pruning') ||
          app.snapshot?.tasks.some(
            (task) => task.kind === 'backup_cleanup' && task.status === 'failed',
          )
        "
        :disabled="app.loading"
        @click="app.mutate(() => api.retryBackupCleanup(), '已重新检查原目录留存，请查看任务结果')"
        >重试原目录清理</Button
      >
      <div class="cleanup-heading">
        <Button :disabled="busy || app.loading" @click="openRestore()">清理统一库残留</Button>
        <span class="muted">先查看清单，再确认永久清理；有引用或修改的内容保留。</span>
      </div>
      <div v-if="cleanupReports.length" class="cleanup-reports">
        <details v-for="report in cleanupReports" :key="report.id" class="cleanup-report">
          <summary>
            实体清理 · {{ report.status === 'complete' ? '检查完成' : '有项目待重试' }} ·
            {{ report.items.length }} 项
          </summary>
          <div v-for="item in report.items" :key="item.digest" class="cleanup-item">
            <Badge :tone="item.status === 'failed' ? 'amber' : 'neutral'">{{
              cleanupLabels[item.status]
            }}</Badge>
            <div class="mono backup-path">{{ item.path }}</div>
            <p class="muted">{{ item.reason }}</p>
          </div>
          <Button
            v-if="report.status === 'pending'"
            size="sm"
            :disabled="busy || app.loading"
            @click="retryCleanup(report.id)"
            >重试实体清理</Button
          >
        </details>
      </div>
      <p v-if="!isNative" class="muted">网页演示不读取本机备份，请在桌面应用中查看和恢复。</p>
      <p v-if="error" role="alert">{{ error }}</p>
      <div class="backup-list">
        <p v-if="!backups.length" class="muted">暂无原目录恢复记录</p>
        <details v-for="group in backupGroups" :key="group.key" class="backup-group">
          <summary class="backup-group-heading">
            <ArchiveRestore :size="19" />
            <strong>{{ group.time }}</strong>
            <span class="muted"
              >{{ group.entries.length }} 次归集 · {{ group.directoryCount }} 个原目录</span
            >
            <span class="muted backup-expand">展开 / 收起</span>
          </summary>
          <article v-for="backup in group.entries" :key="backup.id" class="backup-row">
            <div class="backup-main">
              <div v-for="path in backup.paths" :key="path" class="mono backup-path">
                {{ path }}
              </div>
              <p v-for="issue in backup.issues" :key="issue" class="backup-path">{{ issue }}</p>
            </div>
            <Badge>{{ labels[backup.status] || backup.status }}</Badge>
            <Button
              size="sm"
              :disabled="busy || backup.status !== 'available'"
              @click="openRestore(backup)"
              >恢复原目录</Button
            >
          </article>
        </details>
      </div>
    </div>
    <AppDialog
      :open="dialogOpen"
      :title="cleanupOnly ? '清理统一库残留' : '恢复归集前原目录'"
      large
      :description="
        cleanupOnly
          ? '清理统一库中不再使用的实体和历史版本。被引用、已修改或无法校验的项目保留。'
          : '将原位置换回归集时的实体目录，移除无引用的归集记录；可同时清理下方选中的统一库实体。'
      "
      @update:open="
        (value) => {
          if (!value) closeDialog()
        }
      "
    >
      <div v-for="path in selected?.paths" :key="path" class="mono backup-path">{{ path }}</div>
      <p v-if="selected" class="muted">
        执行前会重新检查所有目录；原位置被修改或备份缺失时会停止。
      </p>
      <div class="cleanup-preview">
        <strong>统一库实体清理预览</strong>
        <p class="muted">
          选中的实体将永久删除，无法撤销；执行前会重新校验。软链检查仅覆盖已配置的工具、分发、本地来源目录及原目录周边，无法确认其他位置的引用。
        </p>
        <p v-if="previewLoading" class="muted">正在检查引用和内容…</p>
        <p v-else-if="previewError" role="alert">
          {{ previewError }}
          <Button size="sm" @click="openRestore(selected ?? undefined)">重新预览</Button>
        </p>
        <p v-else-if="!preview?.items.length" class="muted">没有可检查的实体候选。</p>
        <div v-for="item in preview?.items" :key="item.digest" class="cleanup-item">
          <div class="cleanup-item-heading">
            <CheckboxRoot
              v-if="item.status === 'ready'"
              class="cleanup-checkbox"
              :model-value="cleanupSelection.includes(item.digest)"
              :disabled="busy"
              :aria-label="`清理 ${item.path}`"
              @update:model-value="toggleCleanup(item.digest, $event)"
            >
              <CheckboxIndicator><Check :size="14" /></CheckboxIndicator>
            </CheckboxRoot>
            <Badge
              :tone="item.status === 'modified' || item.status === 'blocked' ? 'amber' : 'neutral'"
              >{{ cleanupLabels[item.status] }}</Badge
            >
          </div>
          <div class="mono backup-path">{{ item.path }}</div>
          <p class="muted">{{ item.reason }}</p>
        </div>
      </div>
      <template #footer>
        <Button :disabled="busy" @click="closeDialog">取消</Button>
        <Button v-if="selected && selectionCount" :disabled="cannotSubmit" @click="restore(false)"
          >仅恢复</Button
        >
        <Button
          v-if="preview && !previewLoading"
          :disabled="busy"
          @click="openRestore(selected ?? undefined)"
          >刷新预览</Button
        >
        <Button
          :variant="selectionCount ? 'danger' : 'primary'"
          :disabled="cannotSubmit || (cleanupOnly && !selectionCount)"
          @click="restore()"
        >
          {{
            busy
              ? '处理中…'
              : selectionCount
                ? `${cleanupOnly ? '永久清理' : '恢复并永久清理'}（${selectionCount}）`
                : cleanupOnly
                  ? '选择要清理的实体'
                  : '确认恢复'
          }}
        </Button>
      </template>
    </AppDialog>
  </section>
</template>
<style scoped>
.backup-panel {
  padding: 20px;
  margin-bottom: 20px;
}
.backup-heading,
.backup-controls,
.backup-row {
  display: flex;
  align-items: center;
  gap: 12px;
}
.backup-heading {
  justify-content: space-between;
  flex-wrap: wrap;
}
.backup-controls .input {
  width: 72px;
}
.backup-list {
  max-height: 320px;
  overflow: auto;
  margin-top: 16px;
}
.backup-row {
  padding: 14px 0;
  border-top: 1px solid var(--line);
  align-items: flex-start;
}
.backup-main {
  flex: 1;
  min-width: 0;
}
.backup-path {
  overflow-wrap: anywhere;
  font-size: 12px;
  margin: 8px 0;
}
summary {
  cursor: pointer;
  font-size: 12px;
  margin-top: 6px;
}
.muted {
  color: var(--muted);
  font-size: 12px;
}
</style>

<style scoped>
.backup-group {
  border-top: 1px solid var(--line);
}
.backup-group-heading {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 14px 0;
  margin: 0;
  list-style: none;
}
.backup-group-heading::-webkit-details-marker {
  display: none;
}
.backup-expand {
  margin-left: auto;
}
.backup-group .backup-row {
  margin-left: 28px;
}
</style>

<style scoped>
.cleanup-preview {
  margin-top: 20px;
  padding-top: 16px;
  border-top: 1px solid var(--line);
}
.cleanup-item {
  padding: 12px 0;
  border-top: 1px solid var(--line);
}
</style>

<style scoped>
.cleanup-heading,
.cleanup-item-heading {
  display: flex;
  align-items: center;
  gap: 10px;
  margin: 12px 0;
}
.cleanup-reports {
  max-height: 220px;
  overflow: auto;
}
.cleanup-report {
  padding: 8px 0;
  border-top: 1px solid var(--line);
}
.cleanup-checkbox {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 18px;
  height: 18px;
  border: 1px solid var(--control-line);
  border-radius: 4px;
  background: var(--panel);
}
.cleanup-checkbox[data-state='checked'] {
  color: #fff;
  background: var(--blue);
  border-color: var(--blue);
}
.cleanup-checkbox:focus-visible {
  outline: 2px solid var(--blue);
  outline-offset: 2px;
}
</style>

<style scoped>
.cleanup-item-heading {
  margin: 0 0 6px;
}
</style>
