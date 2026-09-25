<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref } from 'vue'
import { RovingFocusGroup, RovingFocusItem } from 'reka-ui'
import {
  Sparkles,
  AppWindow,
  RefreshCw,
  Settings2,
  Power,
  X,
  CircleAlert,
  CheckCircle2,
  ChevronRight,
  Pause,
  Play,
} from 'lucide-vue-next'
import Button from './ui/Button.vue'
import { trayAction, type TrayStatus } from '@/services/tray'
import { isNative } from '@/services/api'

const isMac = typeof navigator !== 'undefined' && /Mac/i.test(navigator.platform)
const cmdKey = isMac ? '⌘' : 'Ctrl+'

const status = ref<TrayStatus>({
  version: '',
  paused: false,
  active: false,
  skills: 0,
  sources: 0,
  scheduled: 0,
  automatic: 0,
  theme: 'system',
})
const ready = ref(false)
const busy = ref(false)
const message = ref('')
const error = ref(false)
const toggling = ref(false)

const stateLabel = computed(() => {
  if (!ready.value) return '正在读取任务…'
  if (!status.value.scheduled) return '尚未配置任务'
  if (status.value.paused) return '任务已暂停'
  return status.value.active ? '任务执行中' : '任务已开启'
})
const stateDetail = computed(() => {
  if (!ready.value) return '读取完成后可管理任务'
  if (!status.value.scheduled) return '在更新中心配置定时任务'
  if (status.value.paused)
    return status.value.active ? '本轮执行完成后，不再启动新任务' : '恢复后继续按来源计划执行'
  return status.value.active ? '暂停仅影响后续任务，本轮会继续' : '按来源计划检查并处理更新'
})
const statusTone = computed(() => {
  if (!ready.value || !status.value.scheduled) return 'neutral'
  if (status.value.paused) return 'amber'
  return status.value.active ? 'blue' : 'green'
})
async function toggleTasks() {
  if (toggling.value || !ready.value || !status.value.scheduled) return
  toggling.value = true
  message.value = ''
  error.value = false
  try {
    status.value = await trayAction('pause')
  } catch (e) {
    message.value = String(e)
    error.value = true
  } finally {
    toggling.value = false
  }
}

const media = window.matchMedia('(prefers-color-scheme: dark)')
function applyTheme() {
  document.documentElement.dataset.theme =
    status.value.theme === 'dark' || (status.value.theme === 'system' && media.matches)
      ? 'dark'
      : 'light'
}
async function refresh() {
  try {
    status.value = await trayAction('status')
    ready.value = true
    applyTheme()
  } catch (e) {
    message.value = String(e)
    error.value = true
  }
}
async function run(action: 'open' | 'check' | 'configure' | 'quit' | 'hide') {
  if (action === 'check' && (!ready.value || status.value.active)) return
  if (busy.value && action !== 'hide' && action !== 'open' && action !== 'configure') return

  message.value = ''
  error.value = false
  if (action === 'check') busy.value = true
  try {
    status.value = await trayAction(action)
    if (action === 'check')
      message.value = status.value.sources
        ? `已检查 ${status.value.sources} 个来源，详情见更新中心`
        : '尚未添加来源，可打开主窗口添加'
  } catch (e) {
    message.value = String(e)
    error.value = true
  } finally {
    if (action === 'check') busy.value = false
  }
}
function onKey(event: KeyboardEvent) {
  if (event.key === 'Escape') {
    event.preventDefault()
    void run('hide')
  } else if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'o') {
    event.preventDefault()
    void run('open')
  } else if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'r') {
    event.preventDefault()
    void run('check')
  } else if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'q') {
    event.preventDefault()
    void run('quit')
  }
}
let disposed = false
const unlisten: (() => void)[] = []
onMounted(async () => {
  document.documentElement.classList.add('tray-html')
  document.body.classList.add('tray-body')
  document.addEventListener('keydown', onKey)
  media.addEventListener('change', applyTheme)
  if (isNative) {
    const { listen } = await import('@tauri-apps/api/event')
    for (const event of ['skilldock:tray-opened', 'skilldock:changed', 'skilldock:scheduler']) {
      const stop = await listen(event, async () => {
        if (event === 'skilldock:tray-opened') {
          message.value = ''
          error.value = false
        }
        await refresh()
        if (event === 'skilldock:tray-opened' && !disposed) {
          await nextTick()
          document.querySelector<HTMLButtonElement>('[data-tray-primary]')?.focus()
        }
      })
      if (disposed) stop()
      else unlisten.push(stop)
    }
  }
  if (!disposed) {
    await refresh()
    await nextTick()
    if (isNative && !disposed) await trayAction('ready')
  }
})
onUnmounted(() => {
  disposed = true
  unlisten.forEach((stop) => stop())
  document.removeEventListener('keydown', onKey)
  media.removeEventListener('change', applyTheme)
  document.documentElement.classList.remove('tray-html')
  document.body.classList.remove('tray-body')
})
</script>

<template>
  <div class="tray-container" @click.self="run('hide')">
    <section class="tray-panel" aria-label="SkillDock 快捷菜单">
      <!-- Header -->
      <header class="tray-header">
        <div class="tray-logo" aria-hidden="true">
          <Sparkles :size="15" :stroke-width="2" />
        </div>
        <div class="tray-title-wrap">
          <strong>SkillDock</strong>
        </div>
        <Button
          variant="ghost"
          size="icon"
          class="tray-close-btn"
          aria-label="收起菜单 (Esc)"
          title="收起菜单 (Esc)"
          @click="run('hide')"
        >
          <X :size="14" />
        </Button>
      </header>

      <dl class="tray-summary" aria-label="当前配置">
        <div>
          <dt>已收录 Skill</dt>
          <dd>{{ ready ? status.skills.toLocaleString() : '—' }}</dd>
        </div>
        <div>
          <dt>更新任务</dt>
          <dd>{{ ready ? status.scheduled.toLocaleString() : '—' }}<span>个</span></dd>
        </div>
      </dl>
      <div class="tray-status">
        <div class="tray-status-heading">
          <div class="tray-status-left" role="status">
            <span class="tray-dot" :class="`tone-${statusTone}`" aria-hidden="true" /><span>{{
              stateLabel
            }}</span>
          </div>
          <Button
            class="tray-task-toggle"
            variant="secondary"
            size="sm"
            :disabled="!ready || !status.scheduled || toggling"
            :aria-label="status.paused ? '开启更新任务' : '暂停更新任务'"
            :aria-busy="toggling"
            @click="toggleTasks"
          >
            <Play v-if="status.paused" :size="13" /><Pause v-else :size="13" />{{
              toggling ? '处理中…' : status.paused ? '开启' : '暂停'
            }}
          </Button>
        </div>
        <span class="tray-status-detail">{{ stateDetail }}</span>
      </div>

      <!-- Actions List -->
      <RovingFocusGroup
        orientation="vertical"
        loop
        role="menu"
        aria-label="快捷操作"
        class="tray-actions"
      >
        <RovingFocusItem as-child>
          <Button
            data-tray-primary
            class="tray-action tray-action-primary"
            variant="ghost"
            role="menuitem"
            @click="run('open')"
          >
            <div class="tray-action-icon">
              <AppWindow :size="15" />
            </div>
            <span class="tray-action-label">打开工作台</span>
            <kbd class="tray-shortcut">{{ cmdKey }}O</kbd>
          </Button>
        </RovingFocusItem>

        <RovingFocusItem as-child :focusable="!busy && !status.active && ready">
          <Button
            class="tray-action"
            variant="ghost"
            role="menuitem"
            :disabled="busy || status.active || !ready"
            @click="run('check')"
          >
            <div class="tray-action-icon">
              <RefreshCw :size="15" :class="{ 'tray-spinning': busy || status.active }" />
            </div>
            <span class="tray-action-label">{{
              busy || status.active ? '正在检查更新…' : '检查 Skill 更新'
            }}</span>
            <kbd class="tray-shortcut">{{ cmdKey }}R</kbd>
          </Button>
        </RovingFocusItem>

        <RovingFocusItem as-child>
          <Button class="tray-action" variant="ghost" role="menuitem" @click="run('configure')">
            <div class="tray-action-icon"><Settings2 :size="15" /></div>
            <span class="tray-action-label">配置定时更新</span>
            <ChevronRight :size="14" class="tray-config-chevron" aria-hidden="true" />
          </Button>
        </RovingFocusItem>

        <div class="tray-divider" role="separator" />

        <RovingFocusItem as-child :focusable="!busy && !status.active">
          <Button
            class="tray-action tray-quit"
            variant="ghost"
            role="menuitem"
            :disabled="busy || status.active"
            @click="run('quit')"
          >
            <div class="tray-action-icon">
              <Power :size="15" />
            </div>
            <span class="tray-action-label">完全退出</span>
            <kbd class="tray-shortcut">{{ cmdKey }}Q</kbd>
          </Button>
        </RovingFocusItem>
      </RovingFocusGroup>

      <footer class="tray-footer">
        <div
          v-if="error || message"
          class="tray-alert"
          :class="error ? 'tray-alert-error' : 'tray-alert-info'"
          :role="error ? 'alert' : 'status'"
          tabindex="0"
          aria-label="操作结果"
        >
          <CircleAlert v-if="error" :size="14" aria-hidden="true" />
          <CheckCircle2 v-else :size="14" aria-hidden="true" />
          <div>
            <span>{{ message }}</span
            ><button v-if="error" class="tray-recovery" @click="run('configure')">
              打开更新中心处理
            </button>
          </div>
        </div>
        <div v-else class="tray-hint">
          <span
            >SkillDock
            <span v-if="status.version" class="tray-version">v{{ status.version }}</span></span
          ><span><kbd>Esc</kbd> 收起</span>
        </div>
      </footer>
    </section>
  </div>
</template>

<style scoped>
.tray-panel {
  padding: 12px;
  border-radius: 12px;
  overflow: auto;
  scrollbar-width: thin;
  opacity: 1;
  transform: none;
  transition: none;
  will-change: auto;
  box-shadow:
    0 2px 6px rgb(32 41 54 / 5%),
    0 4px 18px rgb(32 41 54 / 4%);
}
.tray-header {
  gap: 8px;
  padding: 0;
  flex-shrink: 0;
}
.tray-logo {
  width: 28px;
  height: 28px;
  background: transparent;
  border: 0;
  box-shadow: none;
}
.tray-title-wrap {
  min-width: 0;
}
.tray-header strong {
  font-size: 15px;
  font-weight: 600;
}
.tray-header .tray-close-btn {
  width: 28px;
  height: 28px;
  border-radius: 6px;
  transition: none;
}
.tray-summary {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 16px;
  margin: 0;
  padding: 8px 4px;
}
.tray-summary > div {
  min-width: 0;
}
.tray-summary dt {
  color: var(--muted);
  font-size: 12px;
  line-height: 18px;
}
.tray-summary dd {
  margin: 2px 0 0;
  font-size: 24px;
  font-weight: 600;
  line-height: 28px;
  font-variant-numeric: tabular-nums;
  overflow-wrap: anywhere;
}
.tray-summary dd span {
  margin-left: 6px;
  color: var(--muted);
  font-size: 12px;
  font-weight: 400;
}
.tray-status {
  padding: 8px 4px 12px;
  border-top: 1px solid var(--line);
}
.tray-status-heading {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
}
.tray-task-toggle.btn {
  min-width: 66px;
  min-height: 28px;
  padding: 3px 8px;
  gap: 4px;
  font-size: 12px;
  white-space: nowrap;
}
.tray-status-left {
  gap: 8px;
  font-size: 14px;
  line-height: 20px;
  font-weight: 500;
}
.tray-status-detail {
  display: block;
  padding-left: 14px;
  margin-top: 2px;
  font-size: 12px;
  line-height: 18px;
  color: var(--muted);
  overflow-wrap: anywhere;
}
.tray-dot {
  width: 6px;
  height: 6px;
  box-shadow: none;
  animation: none;
}
.tray-dot.tone-green {
  background: #23845b;
  box-shadow: none;
  animation: none;
}
.tray-dot.tone-blue,
.tray-dot.tone-amber {
  box-shadow: none;
}
.tray-actions {
  gap: 4px;
  flex-shrink: 0;
}
.tray-action.btn {
  min-height: 36px;
  height: auto;
  padding: 7px 8px;
  gap: 8px;
  font-size: 14px;
  line-height: 20px;
  border-radius: 6px;
  font-weight: 400;
  transform: none;
  transition:
    background-color 100ms ease-out,
    color 100ms ease-out;
}
.tray-action-icon {
  width: 20px;
  height: 20px;
}
.tray-action-label {
  min-width: 0;
  overflow-wrap: anywhere;
}
.tray-action-primary.btn {
  background: var(--blue-soft);
  color: var(--blue);
  font-weight: 500;
}
.tray-action-primary .tray-action-icon {
  color: var(--blue);
}
.tray-shortcut {
  font-family: SFMono-Regular, Menlo, monospace;
  font-size: 11px;
  background: transparent;
  border: 0;
  padding: 0;
  white-space: nowrap;
}
.tray-config-chevron {
  color: var(--muted);
  margin-left: auto;
}
.tray-divider {
  margin: 4px 8px;
}
.tray-action.tray-quit {
  color: var(--muted);
}
.tray-panel .btn:active {
  transform: none;
  background: var(--surface);
}
.tray-panel .btn:focus-visible,
.tray-recovery:focus-visible,
.tray-alert:focus-visible {
  outline: 2px solid var(--blue);
  outline-offset: -2px;
}
.tray-panel .btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
.tray-footer {
  padding: 10px 4px 0;
  border: 0;
  min-height: 28px;
  flex-shrink: 0;
}
.tray-hint {
  justify-content: space-between;
  font-size: 11px;
  line-height: 18px;
  gap: 8px;
}
.tray-version {
  font-variant-numeric: tabular-nums;
  margin-left: 3px;
}
.tray-hint kbd {
  font-family: inherit;
  background: none;
  border: 0;
  padding: 0;
}
.tray-alert {
  gap: 6px;
  align-items: flex-start;
  padding: 6px 8px;
  font-size: 12px;
  line-height: 18px;
  max-height: 72px;
  overflow: auto;
  overflow-wrap: anywhere;
  scrollbar-width: thin;
}
.tray-alert > svg {
  flex-shrink: 0;
  margin-top: 2px;
}
.tray-alert > div {
  min-width: 0;
}
.tray-alert-error {
  color: #a62c36;
  background: #fff1f2;
}
.tray-recovery {
  display: block;
  background: none;
  border: 0;
  padding: 4px 0 0;
  font: inherit;
  color: inherit;
  text-decoration: underline;
  text-underline-offset: 3px;
  cursor: pointer;
}
@media (hover: hover) and (pointer: fine) {
  .tray-panel .btn:not(:disabled):hover {
    background: var(--surface);
    color: var(--ink);
    transform: none;
  }
  .tray-panel .tray-action-primary:not(:disabled):hover {
    background: color-mix(in srgb, var(--blue) 16%, var(--panel));
    color: var(--blue);
  }
  .tray-action.tray-quit:not(:disabled):hover {
    background: #fff1f2;
    color: #a62c36;
  }
  .tray-action.btn:hover .tray-shortcut {
    background: none;
    color: inherit;
  }
}
@media (hover: none) {
  .tray-panel .btn:hover {
    background: transparent;
    color: var(--ink);
    transform: none;
  }
  .tray-panel .tray-action-primary:hover {
    background: var(--blue-soft);
    color: var(--blue);
  }
}
@media (prefers-reduced-motion: reduce) {
  .tray-panel .btn {
    transition: none;
  }
}
:global([data-theme='dark']) .tray-dot.tone-green {
  background: #61c49a;
  box-shadow: none;
}
:global([data-theme='dark']) .tray-alert-error {
  background: #3f2026;
  color: #ffb4bc;
}
@media (hover: hover) and (pointer: fine) {
  :global([data-theme='dark']) .tray-action.tray-quit:not(:disabled):hover {
    background: #3f2026;
    color: #ffb4bc;
  }
}
</style>
