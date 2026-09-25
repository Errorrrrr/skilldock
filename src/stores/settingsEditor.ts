import { defineStore } from 'pinia'
import { watch } from 'vue'
import { api } from '@/services/api'
import { useAppStore } from './app'
import { useAppUpdater } from '@/composables/useAppUpdater'
import { createSettingsAutosave } from '@/services/settingsAutosave'

export const useSettingsEditor = defineStore('settings-editor', () => {
  const app = useAppStore()
  const updater = useAppUpdater()
  const editor = createSettingsAutosave(app.snapshot!.settings, {
    blocked: () => updater.busy.value,
    save: async (patch) => {
      const next = await api.settings(patch)
      if (!app.snapshot || next.revision >= app.snapshot.revision) app.snapshot = next
      return app.snapshot.settings
    },
  })
  watch(
    () => app.snapshot?.settings,
    (next) => {
      if (next) editor.receive(next)
    },
    { flush: 'sync' },
  )
  watch(updater.busy, (busy) => {
    if (!busy && editor.dirty.value) void editor.flush()
  })
  return editor
})
