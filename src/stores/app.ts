import { computed, ref, watch } from 'vue'
import { defineStore } from 'pinia'
import { api, isNative } from '@/services/api'
import type { Snapshot, Source, Target } from '@/services/types'
import { sourceDisplayName, targetDisplayName } from '@/services/agentProfiles'

export const useAppStore = defineStore('app', () => {
  const snapshot = ref<Snapshot | null>(null)
  const loading = ref(false)
  const error = ref('')
  const notice = ref('')
  let polling: number | undefined
  let noticeTimer: number | undefined
  const unlisten: Array<() => void> = []
  let refreshing = false
  watch(notice, (value) => {
    if (noticeTimer) window.clearTimeout(noticeTimer)
    if (value)
      noticeTimer = window.setTimeout(() => {
        notice.value = ''
      }, 4000)
  })

  const ready = computed(() => !!snapshot.value)
  const recoverableTasks = computed(
    () => snapshot.value?.tasks.filter((task) => task.status === 'needsRecovery') ?? [],
  )
  const activeTasks = computed(
    () => snapshot.value?.tasks.filter((task) => task.status === 'running') ?? [],
  )

  async function initialize() {
    await refresh()
    if (isNative) {
      polling = window.setInterval(() => refresh(true), 5000)
      const { listen } = await import('@tauri-apps/api/event')
      unlisten.push(
        await listen<string>('skilldock:notice', (event) => {
          notice.value = event.payload
        }),
      )
      unlisten.push(
        await listen<{ paused: boolean }>('skilldock:scheduler', (event) => {
          notice.value = event.payload.paused ? '已暂停定时更新' : '已恢复定时更新'
        }),
      )
    }
  }
  async function refresh(silent = false) {
    if (refreshing || (silent && loading.value)) return
    refreshing = true
    if (!silent) loading.value = true
    try {
      const next = await api.snapshot()
      if (!snapshot.value || next.revision >= snapshot.value.revision) snapshot.value = next
      if (!silent) error.value = ''
    } catch (reason) {
      error.value = reason instanceof Error ? reason.message : '无法读取 SkillDock 状态'
    } finally {
      refreshing = false
      if (!silent) loading.value = false
    }
  }
  async function mutate(action: () => Promise<Snapshot>, success: string) {
    loading.value = true
    error.value = ''
    try {
      snapshot.value = await action()
      notice.value = success
      return true
    } catch (reason) {
      error.value = reason instanceof Error ? reason.message : '操作失败'
      return false
    } finally {
      loading.value = false
    }
  }
  function clearError() {
    error.value = ''
  }
  function targetName(target: Target | undefined) {
    return targetDisplayName(target, snapshot.value?.settings.agentProfiles)
  }
  function sourceName(source: Source | null | undefined) {
    return sourceDisplayName(source, snapshot.value?.settings.agentProfiles)
  }

  return {
    snapshot,
    loading,
    error,
    notice,
    ready,
    recoverableTasks,
    activeTasks,
    isNative,
    initialize,
    refresh,
    mutate,
    clearError,
    targetName,
    sourceName,
    stopPolling: () => {
      if (polling) clearInterval(polling)
      if (noticeTimer) clearTimeout(noticeTimer)
      unlisten.forEach((stop) => stop())
    },
  }
})
