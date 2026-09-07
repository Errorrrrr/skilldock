<script setup lang="ts">
import { onMounted, onUnmounted, watch } from 'vue'
import { useRouter } from 'vue-router'
import AppShell from '@/components/AppShell.vue'
import { isNative } from '@/services/api'
import { useAppStore } from '@/stores/app'

const app = useAppStore()
const router = useRouter()
watch(
  () => app.snapshot?.initialized,
  (initialized) => {
    if (initialized === false) router.replace('/import')
  },
)
const media = window.matchMedia('(prefers-color-scheme: dark)')
const applyTheme = () => {
  const theme = app.snapshot?.settings.theme
  document.documentElement.dataset.theme =
    theme === 'dark' || (theme === 'system' && media.matches) ? 'dark' : 'light'
}
watch(() => app.snapshot?.settings.theme, applyTheme, { immediate: true })
let stopNavigation: (() => void) | undefined
let disposed = false
onMounted(async () => {
  app.initialize()
  media.addEventListener('change', applyTheme)
  if (isNative) {
    const { listen } = await import('@tauri-apps/api/event')
    const stop = await listen('skilldock:navigate-updates', () =>
      router.push('/updates?tab=sources'),
    )
    if (disposed) stop()
    else stopNavigation = stop
  }
})
onUnmounted(() => {
  disposed = true
  stopNavigation?.()
  app.stopPolling()
  media.removeEventListener('change', applyTheme)
})
</script>

<template>
  <AppShell />
</template>
