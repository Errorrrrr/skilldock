<script setup lang="ts">
import { computed, ref } from 'vue'
import { api, type SingleContentPreview } from '@/services/api'
import { useAppStore } from '@/stores/app'
import Button from './ui/Button.vue'
import AppDialog from './ui/AppDialog.vue'
const app = useAppStore()
const open = ref(false)
const busy = ref(false)
const error = ref('')
const preview = ref<SingleContentPreview | null>(null)
const selections = ref<Record<string, string>>({})
const enabled = computed(() => (app.snapshot?.schemaVersion ?? 0) >= 3)
const arranged = computed(() => /[\\/]\.skilldock$/.test(app.snapshot?.storageRoot || ''))
const conflicts = computed(
  () => preview.value?.groups.filter((g) => g.skills.length > 1 && !g.identical) ?? [],
)
const affectedTargets = computed(() =>
  [
    ...new Set(
      preview.value?.bindings.map(
        (b) => app.snapshot?.targets.find((t) => t.id === b.targetId)?.name || b.path,
      ) || [],
    ),
  ].join('、'),
)
function sourceLabel(sourceId: string) {
  const source = app.snapshot?.sources.find((s) => s.id === sourceId)
  return source ? `${app.sourceName(source)} · ${source.url || source.path}` : '来源已移除'
}
const incomplete = computed(() => conflicts.value.some((g) => !selections.value[g.name]))
async function inspect() {
  if (busy.value) return
  busy.value = true
  error.value = ''
  try {
    preview.value = await api.previewSingleContent()
    selections.value = {}
    open.value = true
  } catch (reason) {
    app.error = reason instanceof Error ? reason.message : String(reason)
  } finally {
    busy.value = false
  }
}
async function apply() {
  if (!preview.value || busy.value || incomplete.value) return
  busy.value = true
  error.value = ''
  try {
    const choices = conflicts.value.map((group) =>
      selections.value[group.name] === 'separate'
        ? { name: group.name, keepSeparate: true }
        : { name: group.name, skillId: selections.value[group.name] },
    )
    app.snapshot = await api.enableSingleContent(preview.value.revision, choices)
    app.snapshot = await api.arrangeLibrary()
    app.notice = '已切换为单一当前内容，目录已整理'
    open.value = false
  } catch (reason) {
    error.value = reason instanceof Error ? reason.message : String(reason)
    await app.refresh(true)
  } finally {
    busy.value = false
  }
}
async function arrange() {
  if (busy.value) return
  busy.value = true
  const ok = await app.mutate(() => api.arrangeLibrary(), '目录已整理，可按 Skill 名称浏览')
  if (ok) open.value = false
  busy.value = false
}
</script>
<template>
  <section class="callout single-content-settings">
    <strong>{{ enabled ? '单一当前内容' : '简化资料库' }}</strong>
    <p v-if="enabled">所有工具共用当前内容，每个来源包最多保留一份更新恢复备份。</p>
    <p v-else>旧库仍保留多版本规则。预览并确认后，可统一工具内容、减少历史副本。</p>
    <Button v-if="!enabled" :loading="busy" @click="inspect">预览简化资料库</Button>
    <Button v-else-if="!arranged" :loading="busy" @click="arrange">整理为名称目录</Button>
  </section>
  <AppDialog
    v-model:open="open"
    title="切换为单一当前内容"
    description="请确认每个同名 Skill 使用哪份内容。所有已管理工具将统一使用所选内容。"
  >
    <p>
      影响 {{ preview?.bindings.length || 0 }} 条分发和
      {{ preview?.presets.length || 0 }}
      个预设。目标不再固定旧内容，预设只保存成员；相同完整内容会复用。
    </p>
    <p v-if="affectedTargets">受影响工具：{{ affectedTargets }}</p>
    <div v-for="group in conflicts" :key="group.name" class="list-stack">
      <h3>{{ group.name }}</h3>
      <label v-for="skill in group.skills" :key="skill.id" class="choice">
        <input
          v-model="selections[group.name]"
          type="radio"
          :name="group.name"
          :value="skill.id"
          :disabled="busy || enabled"
        />
        <span
          >{{ skill.description || skill.name }}<br />{{ skill.externalPath || skill.version
          }}<br />{{ sourceLabel(skill.sourceId) }}</span
        >
      </label>
      <label class="choice"
        ><input
          v-model="selections[group.name]"
          type="radio"
          :name="group.name"
          value="separate"
          :disabled="busy || enabled"
        />分别保留为独立 Skill（目录加名称后缀）</label
      >
    </div>
    <p>
      迁移后只保留当前内容和仍有实际引用的对象。无引用的旧历史会回收，不能再从任务记录选择旧版本。原目录恢复备份和外部链接继续受保护。
    </p>
    <p v-if="error" class="field-error" role="alert">{{ error }}</p>
    <template #footer>
      <Button :disabled="busy" @click="open = false">取消</Button>
      <Button v-if="enabled" variant="primary" :loading="busy" @click="arrange"
        >重试目录整理</Button
      >
      <Button v-else variant="primary" :disabled="incomplete" :loading="busy" @click="apply"
        >确认统一内容并整理</Button
      >
    </template>
  </AppDialog>
</template>
<style scoped>
.single-content-settings {
  margin-bottom: 16px;
}
.single-content-settings p {
  margin: 8px 0;
}
</style>
