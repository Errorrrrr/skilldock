import { computed, ref } from 'vue'
import { api, type AppUpdateResult, type AppUpdateProgress } from '@/services/api'
import { version as buildVersion } from '../../package.json'

// Keep the in-flight operation and progress when navigating away from Settings.
const result = ref<AppUpdateResult | null>(null)
const error = ref('')
const phase = ref<'idle' | 'checking' | AppUpdateProgress['phase']>('idle')
const progress = ref<AppUpdateProgress | null>(null)
const busy = computed(() => phase.value !== 'idle')
const currentVersion = computed(() => result.value?.currentVersion || buildVersion)
const percent = computed(() => {
  const value = progress.value
  return value?.total
    ? Math.min(100, Math.round((value.downloaded / value.total) * 100))
    : undefined
})
const phaseLabel = computed(
  () =>
    ({
      idle: '',
      checking: '正在检查更新…',
      downloading: '正在下载更新…',
      verifying: '正在验证更新包签名…',
      installing: '正在安装，请勿退出…',
      restarting: '安装完成，正在重启…',
    })[phase.value],
)
let infoSequence = 0
async function loadInfo() {
  if (busy.value) return
  const sequence = ++infoSequence
  result.value = null
  error.value = ''
  try {
    const next = await api.appUpdateInfo()
    if (sequence === infoSequence && !busy.value) result.value = next
  } catch (reason) {
    if (sequence === infoSequence && !busy.value)
      error.value = String(reason instanceof Error ? reason.message : reason)
  }
}
async function check() {
  if (busy.value) return
  ++infoSequence
  phase.value = 'checking'
  error.value = ''
  result.value = null
  progress.value = null
  try {
    result.value = await api.checkAppUpdate()
  } catch (reason) {
    error.value = reason instanceof Error ? reason.message : String(reason)
  } finally {
    phase.value = 'idle'
  }
}
async function install() {
  if (busy.value || !result.value?.available || !result.value.token) return
  const token = result.value.token
  phase.value = 'downloading'
  progress.value = null
  error.value = ''
  let unlisten: (() => void) | undefined
  try {
    // Subscribe before invoking so the first progress event cannot be lost.
    unlisten = await api.onAppUpdateProgress((next) => {
      if (next.token !== token) return
      progress.value = next
      phase.value = next.phase
    })
    await api.installAppUpdate(token)
  } catch (reason) {
    error.value = reason instanceof Error ? reason.message : String(reason)
  } finally {
    unlisten?.()
    phase.value = 'idle'
  }
}
export function useAppUpdater() {
  return {
    result,
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
  }
}
