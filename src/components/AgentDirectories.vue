<script setup lang="ts">
import { computed, ref, watch, nextTick } from 'vue'
import { Plus, Trash2, RotateCcw, ChevronLeft, ChevronRight, FolderOpen } from 'lucide-vue-next'
import Button from './ui/Button.vue'
import DirectoryField from './DirectoryField.vue'
import type { AgentProfile } from '@/services/types'
import { cloneAgentProfiles } from '@/services/agentProfiles'
const profiles = defineModel<AgentProfile[]>({ required: true })
const props = defineProps<{ initialAgent?: string }>()
const selected = ref(props.initialAgent || 'codex')
const scope = ref<'userPaths' | 'projectPaths'>('userPaths')
const page = ref(1)
const editor = ref<HTMLElement>()
const profile = computed(() => profiles.value.find((p) => p.id === selected.value))
const paths = computed(() => profile.value?.[scope.value] || [])
const pages = computed(() => Math.max(1, Math.ceil(paths.value.length / 3)))
const visiblePaths = computed(() =>
  paths.value
    .slice((page.value - 1) * 3, page.value * 3)
    .map((_, offset) => (page.value - 1) * 3 + offset),
)
const userScope = computed(() => scope.value === 'userPaths')
watch([selected, scope], () => {
  page.value = 1
})
watch(pages, (count) => {
  page.value = Math.min(page.value, count)
})
function restore() {
  const defaults = cloneAgentProfiles().find((p) => p.id === selected.value)
  if (defaults) profiles.value = profiles.value.map((p) => (p.id === selected.value ? defaults : p))
  page.value = 1
}
async function addPath() {
  paths.value.push('')
  page.value = pages.value
  await nextTick()
  const inputs = editor.value?.querySelectorAll<HTMLInputElement>('input')
  inputs?.[inputs.length - 1]?.focus()
}
</script>
<template>
  <div class="directory-config">
    <nav class="tool-list" aria-label="选择配置工具">
      <span class="tool-list-label">工具配置</span>
      <button
        v-for="item in profiles"
        :key="item.id"
        type="button"
        class="tool-option"
        :class="{ active: selected === item.id }"
        :aria-pressed="selected === item.id"
        @click="selected = item.id"
      >
        <span>{{ item.name }}</span
        ><span class="tool-count">{{ item.userPaths.length + item.projectPaths.length }}</span>
      </button>
    </nav>
    <section v-if="profile" ref="editor" class="directory-editor">
      <header class="editor-heading">
        <div>
          <h3>{{ profile.name }}</h3>
          <p>管理归集扫描与分发使用的目录</p>
        </div>
        <Button
          variant="ghost"
          size="icon"
          title="恢复此工具默认目录"
          aria-label="恢复此工具默认目录"
          @click="restore"
          ><RotateCcw :size="15"
        /></Button>
      </header>
      <div class="scope-switch" role="group" aria-label="目录作用域">
        <button
          type="button"
          :class="{ active: userScope }"
          :aria-pressed="userScope"
          @click="scope = 'userPaths'"
        >
          用户级 <span>{{ profile.userPaths.length }}</span>
        </button>
        <button
          type="button"
          :class="{ active: !userScope }"
          :aria-pressed="!userScope"
          @click="scope = 'projectPaths'"
        >
          项目级 <span>{{ profile.projectPaths.length }}</span>
        </button>
      </div>
      <p class="scope-hint">
        {{
          userScope
            ? '填写绝对路径或 ~/ 路径，扫描时自动去重。'
            : '填写项目内相对路径，分发时再选择项目文件夹。'
        }}
      </p>
      <div class="directory-paths">
        <div
          v-for="index in visiblePaths"
          :key="`${selected}-${scope}-${index}`"
          class="path-entry"
        >
          <div class="path-caption">
            <span>目录 {{ index + 1 }}</span
            ><Button
              variant="ghost"
              size="icon"
              :aria-label="`删除${userScope ? '用户' : '项目'}目录 ${index + 1}`"
              @click="paths.splice(index, 1)"
              ><Trash2 :size="14"
            /></Button>
          </div>
          <DirectoryField
            v-if="userScope"
            v-model="profile.userPaths[index]!"
            placeholder="例如：~/.agents/skills"
          />
          <input
            v-else
            v-model="profile.projectPaths[index]"
            class="input mono"
            :aria-label="`项目目录 ${index + 1}`"
            placeholder="例如：.agents/skills"
          />
        </div>
        <div v-if="!paths.length" class="paths-empty">
          <FolderOpen :size="24" /><span>尚未配置{{ userScope ? '用户级' : '项目级' }}目录</span
          ><span>添加目录后即可用于{{ userScope ? '扫描与分发' : '项目分发' }}</span>
        </div>
      </div>
      <footer class="paths-actions">
        <Button size="sm" :disabled="paths.length >= 32" @click="addPath"
          ><Plus :size="14" />添加目录</Button
        >
        <div v-if="pages > 1" class="path-pagination">
          <Button
            variant="ghost"
            size="icon"
            :disabled="page === 1"
            aria-label="上一页目录"
            @click="page--"
            ><ChevronLeft :size="14" /></Button
          ><span>{{ page }} / {{ pages }}</span
          ><Button
            variant="ghost"
            size="icon"
            :disabled="page === pages"
            aria-label="下一页目录"
            @click="page++"
            ><ChevronRight :size="14"
          /></Button>
        </div>
      </footer>
      <p class="editor-note">
        {{
          profile.id === 'workbuddy'
            ? 'WorkBuddy 统一管理 .workbuddy 与 .codebuddy 两套路径。'
            : '按工具实际配置填写路径；保存后生效。'
        }}
      </p>
    </section>
  </div>
</template>
<style scoped>
.directory-config {
  display: grid;
  grid-template-columns: 168px minmax(0, 1fr);
  border: 1px solid var(--line);
  border-radius: 10px;
  overflow: hidden;
  height: 440px;
}
.tool-list {
  display: flex;
  flex-direction: column;
  gap: 3px;
  background: var(--surface);
  padding: 12px 8px;
  overflow-y: auto;
  border-right: 1px solid var(--line);
}
.tool-list-label {
  padding: 2px 10px 10px;
  font-size: 11px;
  color: var(--muted);
}
.tool-option {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 6px;
  padding: 9px 10px;
  border: 0;
  border-radius: 6px;
  text-align: left;
  background: transparent;
  color: var(--muted);
  font-size: 12px;
  flex-shrink: 0;
}
.tool-option:hover {
  background: var(--panel);
}
.tool-option.active {
  background: var(--blue-soft);
  color: var(--blue);
  font-weight: 600;
}
.tool-count {
  font-size: 10px;
  font-variant-numeric: tabular-nums;
  opacity: 0.8;
}
.directory-editor {
  padding: 18px;
  overflow-y: auto;
  min-width: 0;
}
.editor-heading {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 10px;
  margin-bottom: 16px;
}
.editor-heading h3 {
  margin: 0 0 5px;
  font-size: 16px;
}
.editor-heading p,
.scope-hint,
.editor-note {
  margin: 0;
  color: var(--muted);
  font-size: 12px;
  line-height: 1.6;
}
.scope-switch {
  display: flex;
  gap: 4px;
  padding: 3px;
  border-radius: 7px;
  background: var(--surface);
}
.scope-switch button {
  flex: 1;
  padding: 7px;
  border: 0;
  border-radius: 5px;
  background: transparent;
  color: var(--muted);
  font-size: 12px;
}
.scope-switch button.active {
  background: var(--panel);
  color: var(--blue);
  box-shadow: 0 1px 3px #162c5010;
}
.scope-switch span {
  margin-left: 6px;
  font-size: 11px;
}
.scope-hint {
  margin: 10px 0;
}
.directory-paths {
  display: grid;
  gap: 8px;
}
.path-entry {
  min-width: 0;
}
.path-caption {
  display: flex;
  align-items: center;
  justify-content: space-between;
  color: var(--muted);
  font-size: 11px;
  height: 26px;
}
.path-caption .btn {
  width: 26px;
  height: 26px;
}
.paths-actions {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-top: 12px;
}
.path-pagination {
  display: flex;
  align-items: center;
  gap: 4px;
  color: var(--muted);
  font-size: 11px;
}
.editor-note {
  margin-top: 12px;
  font-size: 11px;
}
.paths-empty {
  display: grid;
  justify-items: center;
  gap: 10px;
  padding: 24px 8px;
  color: var(--muted);
  font-size: 12px;
  border: 1px dashed var(--control-line);
  border-radius: 8px;
}
@media (max-width: 640px) {
  .directory-config {
    grid-template-columns: 128px minmax(0, 1fr);
  }
  .directory-editor {
    padding: 12px;
  }
}
</style>
