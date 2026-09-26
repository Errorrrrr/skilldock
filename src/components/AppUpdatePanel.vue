<script setup lang="ts">
import { computed, ref, watch, onUnmounted } from 'vue'
import { AppWindow, RefreshCcw, Download, AlertTriangle, ShieldCheck } from 'lucide-vue-next'
import { useAppStore } from '@/stores/app'
import { useAppUpdater } from '@/composables/useAppUpdater'
import Button from './ui/Button.vue'
import Badge from './ui/Badge.vue'
import ConfirmDialog from './ui/ConfirmDialog.vue'

const props = defineProps<{ endpoint: string; publicKey: string; settingsPending?: boolean }>()
const emit = defineEmits<{
  'update:endpoint': [value: string]
  'update:publicKey': [value: string]
}>()
const app = useAppStore()
const {
  result,
  confirming: confirmOpen,
  error,
  phase,
  progress,
  busy,
  currentVersion,
  percent,
  phaseLabel,
  loadInfo,
  check,
  install,
} = useAppUpdater()
onUnmounted(() => {
  confirmOpen.value = false
})
const customOpen = ref(false)
const dirty = computed(
  () =>
    !!props.settingsPending ||
    props.endpoint.trim() !== (app.snapshot?.settings.updateEndpoint || '').trim() ||
    props.publicKey.trim() !== (app.snapshot?.settings.updatePublicKey || '').trim(),
)
const installable = computed(
  () => !!result.value?.available && !!result.value?.token && !dirty.value,
)
const checkLabel = computed(() =>
  phase.value === 'checking' ? '正在检查…' : error.value ? '重新检查' : '检查更新',
)
const downloadedLabel = computed(() => {
  if (!progress.value || phase.value !== 'downloading') return ''
  const mb = (progress.value.downloaded / 1024 / 1024).toFixed(1)
  return progress.value.total
    ? `${mb} / ${(progress.value.total / 1024 / 1024).toFixed(1)} MB`
    : `已下载 ${mb} MB`
})
const confirmation = computed(
  () =>
    `将安装 SkillDock ${result.value?.version || ''}。请保存当前编辑，安装完成后应用将重启；Skill 库与分发配置保留。`,
)
watch(
  () => [
    app.snapshot?.settings.updateEndpoint,
    app.snapshot?.settings.updatePublicKey,
    app.snapshot?.settings.networkProxy?.mode,
    app.snapshot?.settings.networkProxy?.url,
  ],
  (next, previous) => {
    if (!previous || next.some((value, index) => value !== previous[index])) {
      confirmOpen.value = false
      customOpen.value = !!next[0] || !!next[1]
      if (previous) void loadInfo()
    }
  },
  { immediate: true },
)
// Each panel mount represents a new visit, independent of the six-hour background interval.
let entryCheckPending = true
watch(
  [() => !!app.snapshot, dirty, phase, confirmOpen],
  ([ready, settingsDirty, currentPhase, confirming]) => {
    if (!entryCheckPending || !ready || settingsDirty || confirming) return
    if (currentPhase === 'checking') {
      // Reuse a check already in progress instead of issuing a duplicate request.
      entryCheckPending = false
    } else if (currentPhase === 'idle') {
      entryCheckPending = false
      void check()
    }
  },
  { immediate: true },
)
async function confirmInstall() {
  if (dirty.value || busy.value) return
  confirmOpen.value = false
  await install()
}
function restoreOfficial() {
  emit('update:endpoint', '')
  emit('update:publicKey', '')
}
</script>

<template>
  <section class="app-update-panel">
    <div class="app-update-heading">
      <div class="app-update-icon"><AppWindow /></div>
      <div class="app-update-title">
        <h3>
          SkillDock <span>{{ currentVersion }}</span>
        </h3>
        <p>应用更新</p>
      </div>
      <Button :disabled="busy || dirty" @click="check"
        ><RefreshCcw :class="{ spinning: phase === 'checking' }" />{{ checkLabel }}</Button
      >
    </div>
    <div class="app-update-source">
      <ShieldCheck /><span>{{ result?.custom ? '自定义发布源' : 'GitHub 官方发布源' }}</span
      ><Badge>签名验证</Badge>
    </div>
    <p class="app-update-description">
      检查应用新版本，确认后安装并重启。使用「网络代理」设置连接发布源。
    </p>
    <div v-if="dirty" class="callout warning">
      设置尚未自动保存完成。请补全有误的字段或等待保存后，再检查和安装更新。
    </div>
    <div v-if="error" class="app-update-error" role="alert">
      <AlertTriangle /><span>{{ error }}</span>
    </div>
    <div v-if="busy" class="app-update-progress" role="status" aria-live="polite">
      <div>
        <strong>{{ phaseLabel }}</strong
        ><span>{{ downloadedLabel }}</span>
      </div>
      <progress
        v-if="phase === 'downloading'"
        :value="percent"
        max="100"
        aria-label="应用更新下载进度"
      />
    </div>
    <div v-else-if="result" class="app-update-result" :class="{ 'has-release': result.available }">
      <div class="app-update-release-heading">
        <div>
          <strong>{{ result.available ? `新版本 ${result.version}` : result.message }}</strong>
          <p v-if="result.available">已获取更新信息，等待确认安装。</p>
        </div>
        <Button v-if="installable" variant="primary" :disabled="busy" @click="confirmOpen = true"
          ><Download />{{ error ? '重试安装' : '安装并重启' }}</Button
        >
      </div>
      <div v-if="result.available" class="app-update-notes">
        <h4>更新说明</h4>
        <pre>{{ result.notes || '此版本未提供更新说明。' }}</pre>
      </div>
    </div>
    <details
      class="app-update-advanced"
      :open="customOpen"
      @toggle="customOpen = ($event.target as HTMLDetailsElement).open"
    >
      <summary>高级设置 · 自定义更新源</summary>
      <p>通常无需修改。使用其他发布渠道时，需同时配置更新地址和验证公钥。</p>
      <div class="app-update-fields">
        <label class="field"
          ><span class="field-label">HTTPS 更新地址</span
          ><input
            class="input"
            :value="endpoint"
            :disabled="busy"
            @input="emit('update:endpoint', ($event.target as HTMLInputElement).value)"
        /></label>
        <label class="field"
          ><span class="field-label">签名公钥</span
          ><textarea
            class="input"
            rows="3"
            :value="publicKey"
            :disabled="busy"
            @input="emit('update:publicKey', ($event.target as HTMLTextAreaElement).value)"
          />
        </label>
      </div>
      <Button size="sm" :disabled="busy || (!endpoint && !publicKey)" @click="restoreOfficial"
        >恢复官方更新源</Button
      >
    </details>
    <ConfirmDialog
      v-model:open="confirmOpen"
      :busy="busy || dirty"
      title="安装更新并重启"
      :description="confirmation"
      confirm-text="安装并重启"
      @confirm="confirmInstall"
      >安装前会检查正在执行的任务和待恢复事务。</ConfirmDialog
    >
  </section>
</template>

<style scoped>
.app-update-panel {
  min-width: 0;
}
.app-update-heading {
  display: flex;
  align-items: center;
  gap: 12px;
  padding-bottom: 20px;
  border-bottom: 1px solid var(--line);
}
.app-update-icon {
  display: grid;
  place-items: center;
  width: 44px;
  height: 44px;
  border-radius: 10px;
  color: var(--blue);
  background: var(--blue-soft);
}
.app-update-icon svg {
  width: 23px;
  height: 23px;
}
.app-update-title {
  flex: 1;
  min-width: 0;
}
.app-update-title h3 {
  margin: 0;
  font-size: 18px;
}
.app-update-title h3 span {
  font-size: 13px;
  font-weight: 400;
  color: var(--muted);
  margin-left: 5px;
}
.app-update-title p {
  margin: 4px 0 0;
  color: var(--muted);
  font-size: 12px;
}
.app-update-source {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-top: 20px;
  font-size: 13px;
}
.app-update-source svg {
  width: 15px;
  height: 15px;
  color: var(--blue);
}
.app-update-description,
.app-update-advanced > p {
  color: var(--muted);
  font-size: 12px;
  line-height: 1.8;
}
.app-update-error {
  display: flex;
  align-items: flex-start;
  gap: 8px;
  padding: 12px;
  margin: 14px 0;
  border-radius: 8px;
  color: #b43a2b;
  background: #fff3f0;
  font-size: 12px;
  overflow-wrap: anywhere;
}
.app-update-error svg {
  width: 16px;
  height: 16px;
  margin-top: 1px;
}
.app-update-result,
.app-update-progress {
  padding: 16px;
  margin: 16px 0;
  border: 1px solid var(--line);
  border-radius: 9px;
  background: var(--surface);
}
.has-release {
  background: var(--panel);
}
.app-update-release-heading,
.app-update-progress > div {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 16px;
  font-size: 13px;
}
.app-update-release-heading strong {
  overflow-wrap: anywhere;
}
.app-update-release-heading p {
  font-size: 12px;
  color: var(--muted);
  margin: 6px 0 0;
}
.app-update-release-heading .btn {
  flex-shrink: 0;
}
.app-update-notes {
  border-top: 1px solid var(--line);
  margin-top: 16px;
  padding-top: 12px;
}
.app-update-notes h4 {
  font-size: 12px;
  margin: 0 0 8px;
  color: var(--muted);
}
.app-update-notes pre {
  max-height: 220px;
  overflow: auto;
  overscroll-behavior: contain;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
  margin: 0;
  font: inherit;
  font-size: 13px;
  line-height: 1.8;
}
.app-update-progress span {
  color: var(--muted);
  font-size: 12px;
}
.app-update-progress progress {
  width: 100%;
  height: 6px;
  margin-top: 14px;
  accent-color: var(--blue);
  display: block;
}
.app-update-advanced {
  margin-top: 24px;
  padding-top: 16px;
  border-top: 1px solid var(--line);
}
.app-update-advanced summary {
  color: var(--muted);
  cursor: pointer;
  font-size: 12px;
}
.app-update-fields {
  display: grid;
  gap: 12px;
  margin: 12px 0;
}
.app-update-fields textarea {
  height: auto;
  min-height: 70px;
  resize: vertical;
}
.spinning {
  animation: spin 1s linear infinite;
}
@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}
@media (prefers-reduced-motion: reduce) {
  .spinning {
    animation: none;
  }
}
</style>
