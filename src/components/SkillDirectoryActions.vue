<script setup lang="ts">
import { computed, ref } from 'vue'
import { FolderOpen, FolderSymlink } from 'lucide-vue-next'
import Button from '@/components/ui/Button.vue'
import AppDialog from '@/components/ui/AppDialog.vue'
import { useAppStore } from '@/stores/app'
import { api } from '@/services/api'
import { skillDirectories, type SkillDirectoryContext } from '@/services/skillDirectories'

const props = defineProps<SkillDirectoryContext>()
const app = useAppStore()
const busy = ref(false)
const chooseOpen = ref(false)
const locations = computed(() => skillDirectories(app.snapshot, props))
const currentTitle = computed(() =>
  locations.value.directories.length > 1
    ? '选择要打开的分发软链目录'
    : locations.value.directory
      ? `${locations.value.directory}（定位软链所在位置）`
      : locations.value.directoryReason,
)
const originalTitle = computed(() =>
  locations.value.original
    ? `${locations.value.original}（打开实体目录）`
    : locations.value.originalReason,
)
async function open(original: boolean, selectedPath?: string) {
  if (!original && !selectedPath && locations.value.directories.length > 1) {
    chooseOpen.value = true
    return
  }
  const path = selectedPath || (original ? locations.value.original : locations.value.directory)
  if (!path || busy.value) return
  busy.value = true
  try {
    await api.openDirectory(path, !original)
    chooseOpen.value = false
  } catch (error) {
    app.error = error instanceof Error ? error.message : String(error)
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <div class="skill-directory-actions" @click.stop @keydown.stop>
    <span :title="currentTitle">
      <Button
        size="sm"
        variant="ghost"
        :disabled="busy || !locations.directory"
        @click="open(false)"
      >
        <FolderSymlink />打开目录
      </Button>
    </span>
    <span :title="originalTitle">
      <Button size="sm" variant="ghost" :disabled="busy || !locations.original" @click="open(true)">
        <FolderOpen />打开原目录
      </Button>
    </span>
    <AppDialog
      v-model:open="chooseOpen"
      title="选择软链目录"
      description="此 Skill 分发到了多个位置，选择要定位的软链。"
    >
      <div class="list-stack">
        <Button
          v-for="entry in locations.directories"
          :key="entry.path"
          class="directory-option"
          :disabled="busy"
          @click="open(false, entry.path)"
        >
          <FolderSymlink /><span
            ><strong>{{ entry.label }}</strong
            ><span class="mono">{{ entry.path }}</span></span
          >
        </Button>
      </div>
    </AppDialog>
  </div>
</template>

<style scoped>
.skill-directory-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 2px 6px;
  margin-top: 6px;
}
.skill-directory-actions > span :deep(.btn) {
  height: 26px;
  padding: 0 6px;
  font-size: 12px;
  white-space: nowrap;
}
.skill-directory-actions :deep(svg) {
  width: 13px;
  height: 13px;
}
.directory-option {
  width: 100%;
  height: auto;
  justify-content: flex-start;
  text-align: left;
  padding: 10px;
}
.directory-option > span {
  min-width: 0;
}
.directory-option strong,
.directory-option .mono {
  display: block;
  overflow-wrap: anywhere;
  white-space: normal;
}
.directory-option .mono {
  margin-top: 4px;
  color: var(--muted);
  font-size: 12px;
}
</style>
