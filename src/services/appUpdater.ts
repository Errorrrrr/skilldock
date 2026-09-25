import { computed, ref } from 'vue'
import type { AppUpdateResult, AppUpdateProgress } from './api'
type UpdateApi = Pick<
  typeof import('./api').api,
  'appUpdateInfo' | 'checkAppUpdate' | 'installAppUpdate' | 'onAppUpdateProgress'
>
export function createAppUpdater(api: UpdateApi, buildVersion: string) {
  // Keep the in-flight operation and progress when navigating away from Settings.
  const result = ref<AppUpdateResult | null>(null)
  const error = ref('')
  const confirming = ref(false)
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
  let checkedAt = 0
  let lastAutomaticCheck = 0
  let monitor: ReturnType<typeof setInterval> | undefined
  async function loadInfo() {
    if (busy.value) return
    if (result.value?.available && Date.now() - checkedAt < 25 * 60_000) return
    if (result.value?.available) {
      await check()
      return
    }
    const sequence = ++infoSequence
    result.value = null
    confirming.value = false
    error.value = ''
    try {
      const next = await api.appUpdateInfo()
      if (sequence === infoSequence && !busy.value) result.value = next
    } catch (reason) {
      if (sequence === infoSequence && !busy.value)
        error.value = String(reason instanceof Error ? reason.message : reason)
    }
  }
  async function runCheck(quiet: boolean) {
    if (busy.value) return
    const sequence = ++infoSequence
    const previous = result.value
    phase.value = 'checking'
    error.value = ''
    if (!quiet) result.value = null
    progress.value = null
    try {
      const next = await api.checkAppUpdate()
      if (sequence === infoSequence) {
        result.value = next
        checkedAt = Date.now()
      }
    } catch (reason) {
      if (sequence === infoSequence) {
        error.value = reason instanceof Error ? reason.message : String(reason)
        // A failed check invalidates the native install token. Keep only the notice.
        result.value = previous?.available ? { ...previous, token: undefined } : null
      }
    } finally {
      phase.value = 'idle'
    }
  }
  async function check() {
    await runCheck(false)
  }
  async function automaticCheck() {
    if (busy.value || confirming.value || Date.now() - lastAutomaticCheck < 6 * 60 * 60_000) return
    lastAutomaticCheck = Date.now()
    await runCheck(true)
  }
  function invalidate() {
    ++infoSequence
    result.value = null
    confirming.value = false
    error.value = ''
    checkedAt = 0
    lastAutomaticCheck = 0
  }
  function startMonitor() {
    if (monitor) return
    void automaticCheck()
    monitor = setInterval(() => {
      void automaticCheck()
    }, 60_000)
  }
  function stopMonitor() {
    if (monitor) clearInterval(monitor)
    monitor = undefined
  }
  async function install() {
    if (busy.value || !result.value?.available || !result.value.token) return
    if (Date.now() - checkedAt >= 25 * 60_000) {
      await check()
      error.value ||= '更新信息已刷新，请再次确认安装'
      return
    }
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
  return {
    result,
    confirming,
    error,
    phase,
    progress,
    busy,
    currentVersion,
    percent,
    phaseLabel,
    loadInfo,
    invalidate,
    automaticCheck,
    startMonitor,
    stopMonitor,
    check,
    install,
  }
}
