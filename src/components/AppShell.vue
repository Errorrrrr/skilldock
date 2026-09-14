<script setup lang="ts">
import { sourceUpdateState } from '@/services/sourceUpdates'
import { computed, nextTick, onMounted, onUnmounted } from 'vue'
import { useRouter } from 'vue-router'
import {
  Archive,
  Sparkles,
  FolderSearch,
  Compass,
  Layers3,
  MonitorCog,
  RefreshCcw,
  ListChecks,
  Settings,
  CheckCircle2,
  CircleAlert,
  X,
} from 'lucide-vue-next'
import { useAppStore } from '@/stores/app'
import { librarySkillCount } from '@/services/librarySkills'
const app = useAppStore()
const router = useRouter()
const libraryCount = computed(() => librarySkillCount(app.snapshot))
const updateCount = computed(
  () =>
    app.snapshot?.sources.filter(
      (source) =>
        sourceUpdateState(source).canCheck && ['available', 'attention'].includes(source.status),
    ).length || 0,
)
const groups = computed(() => [
  {
    label: '资料库',
    items: [
      { to: '/library', label: 'Skill 库', icon: Archive, count: libraryCount.value },
      { to: '/discover', label: '发现与安装', icon: Compass },
      { to: '/local-sources', label: '本地来源', icon: FolderSearch },
      { to: '/presets', label: '预设', icon: Layers3, count: app.snapshot?.presets.length },
    ],
  },
  {
    label: '工作空间',
    items: [
      { to: '/import', label: '归集与导入', icon: FolderSearch },
      { to: '/targets', label: '分发目标', icon: MonitorCog },
      { to: '/updates', label: '来源与更新', icon: RefreshCcw, count: updateCount.value },
    ],
  },
])
async function focusSearch() {
  await router.push('/library')
  await nextTick()
  document.querySelector<HTMLInputElement>('[data-library-search]')?.focus()
}
function focusMain() {
  document.getElementById('main-content')?.focus()
}
function onKey(event: KeyboardEvent) {
  if (document.querySelector('.dialog-content, .sheet-content')) return
  if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'k') {
    event.preventDefault()
    focusSearch()
  }
}
onMounted(() => window.addEventListener('keydown', onKey))
onUnmounted(() => window.removeEventListener('keydown', onKey))
</script>
<template>
  <div class="app-shell">
    <button type="button" class="skip-link" @click="focusMain">跳到主要内容</button>
    <aside class="sidebar">
      <RouterLink to="/library" class="brand" aria-label="SkillDock 资料库">
        <Sparkles class="brand-mark" :stroke-width="1.8" aria-hidden="true" />
        <span class="brand-name">SkillDock</span>
      </RouterLink>
      <nav v-for="group in groups" :key="group.label" class="nav" :aria-label="group.label">
        <span class="nav-label">{{ group.label }}</span>
        <RouterLink
          v-for="item in group.items"
          :key="item.to"
          :to="item.to"
          class="nav-item"
          :title="item.label"
          :aria-label="item.label"
        >
          <component :is="item.icon" aria-hidden="true" /><span class="nav-text">{{
            item.label
          }}</span
          ><span v-if="item.count" class="nav-count">{{ item.count }}</span>
        </RouterLink>
      </nav>
      <div class="nav-spacer" />
      <nav class="sidebar-bottom" aria-label="应用">
        <RouterLink to="/tasks" class="nav-item" title="任务记录" aria-label="任务记录"
          ><ListChecks aria-hidden="true" /><span class="nav-text">任务记录</span
          ><span v-if="app.activeTasks.length" class="nav-count">{{
            app.activeTasks.length
          }}</span></RouterLink
        >
        <RouterLink to="/settings" class="nav-item" title="设置" aria-label="设置"
          ><Settings aria-hidden="true" /><span class="nav-text">设置</span></RouterLink
        >
        <span v-if="!app.isNative" class="demo-label" title="使用独立示例数据，不会修改真实文件"
          >浏览器演示 · 独立示例数据</span
        >
      </nav>
    </aside>
    <main id="main-content" class="workspace" tabindex="-1">
      <div class="content">
        <div v-if="app.recoverableTasks.length" class="recovery-bar" role="status">
          <CircleAlert />有 {{ app.recoverableTasks.length }} 个任务需要恢复。<RouterLink
            class="banner-action"
            to="/tasks"
            >查看任务</RouterLink
          >
        </div>
        <div v-if="app.error" class="error-bar" role="alert">
          <CircleAlert /><span>{{ app.error }}</span
          ><button class="banner-action" aria-label="关闭错误" @click="app.clearError">
            <X />
          </button>
        </div>
        <Transition name="notice"
          ><div v-if="app.notice" class="notice-bar" role="status" aria-live="polite">
            <CheckCircle2 /><span>{{ app.notice }}</span
            ><button class="banner-action" aria-label="关闭通知" @click="app.notice = ''">
              <X />
            </button></div
        ></Transition>
        <div
          v-if="app.loading && !app.ready"
          class="initial-loading"
          aria-label="正在读取资料库"
          aria-busy="true"
        >
          <div class="skeleton" style="height: 28px; width: 160px" />
          <div class="skeleton" style="height: 44px; margin-top: 30px" />
          <div
            v-for="row in 4"
            :key="row"
            class="skeleton"
            style="height: 54px; margin-top: 12px"
          />
        </div>
        <RouterView v-else />
      </div>
    </main>
  </div>
</template>
