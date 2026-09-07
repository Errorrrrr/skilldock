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
} from 'lucide-vue-next'
import Button from './ui/Button.vue'
import { trayAction, type TrayStatus } from '@/services/tray'
import { isNative } from '@/services/api'

const isMac = typeof navigator !== 'undefined' && /Mac/i.test(navigator.platform)
const cmdKey = isMac ? '⌘' : 'Ctrl+'

const status = ref<TrayStatus>({
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
const isOpen = ref(false)

const stateLabel = computed(() =>
  !ready.value
    ? '正在读取状态…'
    : status.value.active || busy.value
      ? '正在检查更新…'
      : !status.value.scheduled
        ? '定时更新未配置'
        : status.value.paused
          ? '定时更新已暂停'
          : `${status.value.scheduled} 个来源已配置定时检查`,
)

const statusTone = computed(() => {
  if (!ready.value) return 'neutral'
  if (status.value.active || busy.value) return 'blue'
  if (!status.value.scheduled) return 'neutral'
  if (status.value.paused) return 'amber'
  return 'green'
})

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
  if (busy.value && action !== 'hide' && action !== 'open' && action !== 'configure') return

  if (action === 'hide' || action === 'open' || action === 'configure' || action === 'quit') {
    isOpen.value = false
    await new Promise((resolve) => setTimeout(resolve, 140))
  }

  message.value = ''
  error.value = false
  if (action === 'check') busy.value = true
  try {
    status.value = await trayAction(action)
    if (action === 'check')
      message.value = status.value.sources
        ? '检查完成；不会自动安装或更新入库'
        : '尚未添加来源，可打开主窗口添加'
  } catch (e) {
    message.value = String(e)
    error.value = true
  } finally {
    busy.value = false
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
    for (const event of [
      'skilldock:tray-opened',
      'skilldock:tray-close',
      'skilldock:changed',
      'skilldock:scheduler',
    ]) {
      const stop = await listen(event, async () => {
        if (event === 'skilldock:tray-close') {
          isOpen.value = false
          return
        }
        await refresh()
        if (event === 'skilldock:tray-opened') {
          message.value = ''
          isOpen.value = false
          requestAnimationFrame(() => {
            requestAnimationFrame(() => {
              isOpen.value = true
            })
          })
          await nextTick()
          document.querySelector<HTMLButtonElement>('[data-tray-primary]')?.focus()
        }
      })
      if (disposed) stop()
      else unlisten.push(stop)
    }
  } else {
    isOpen.value = true
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
    <section
      class="tray-panel"
      :class="{ 'is-open': isOpen }"
      aria-label="SkillDock 快捷菜单"
    >
    <!-- Header -->
    <header class="tray-header">
      <div class="tray-logo" aria-hidden="true">
        <Sparkles :size="15" :stroke-width="2" />
      </div>
      <div class="tray-title-wrap">
        <strong>SkillDock</strong>
        <span class="tray-ver">v0.1.0</span>
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

    <!-- Dashboard Widget -->
    <div class="tray-dashboard">
      <div class="tray-metrics">
        <div class="tray-metric-item">
          <span class="tray-metric-val">{{ ready ? status.skills : '—' }}</span>
          <span class="tray-metric-lbl">已收录 Skill</span>
        </div>
        <div class="tray-metric-sep" />
        <div class="tray-metric-item">
          <span class="tray-metric-val">{{ ready ? status.sources : '—' }}</span>
          <span class="tray-metric-lbl">已连接来源</span>
        </div>
      </div>
      <div class="tray-status-bar">
        <div class="tray-status-left">
          <span class="tray-dot" :class="`tone-${statusTone}`" />
          <span>{{ stateLabel }}</span>
        </div>
        <span class="tray-status-tag">{{
          status.automatic ? `${status.automatic} 个自动入库` : '手动入库'
        }}</span>
      </div>
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
          class="tray-action"
          variant="ghost"
          role="menuitem"
          @click="run('open')"
        >
          <div class="tray-action-icon">
            <AppWindow :size="15" />
          </div>
          <span class="tray-action-label">打开主工作台</span>
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
          <span class="tray-badge-pill">{{ status.scheduled ? '管理' : '未配置' }}</span>
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

    <!-- Footer Toast / Hint -->
    <footer class="tray-footer">
      <Transition name="tray-fade" mode="out-in">
        <div
          v-if="error"
          key="error"
          class="tray-alert tray-alert-error"
          role="alert"
          :title="message"
        >
          <CircleAlert :size="13" class="shrink-0" />
          <span class="truncate">{{ message }}</span>
        </div>
        <div
          v-else-if="message"
          key="info"
          class="tray-alert tray-alert-info"
          role="status"
          :title="message"
        >
          <CheckCircle2 :size="13" class="shrink-0" />
          <span class="truncate">{{ message }}</span>
        </div>
        <div v-else key="hint" class="tray-hint">
          <span>按 <kbd>Esc</kbd> 收起 · 快捷键 {{ cmdKey }}O / {{ cmdKey }}R</span>
        </div>
      </Transition>
    </footer>
    </section>
  </div>
</template>
