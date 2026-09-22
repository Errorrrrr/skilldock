<script setup lang="ts">
import { computed, ref } from 'vue'
import { api } from '@/services/api'
import { useAppStore } from '@/stores/app'
import Button from './ui/Button.vue'
import AppSelect from './ui/AppSelect.vue'
import AppDialog from './ui/AppDialog.vue'
const props = defineProps<{ sourceId: string; skillId: string }>()
const app = useAppStore()
const open = ref(false)
const busy = ref(false)
const revision = ref(0)
const replaceOpen = ref(false)
const replacementId = ref('')
const candidates = computed(
  () =>
    app.snapshot?.skills
      .filter((s) => s.id !== props.skillId && !s.externalPath)
      .map((s) => ({ value: s.id, label: `${s.name} · ${s.description}` })) || [],
)
function confirmReplace() {
  revision.value = app.snapshot?.revision ?? 0
  replacementId.value = ''
  replaceOpen.value = true
}
async function replace() {
  if (busy.value || !replacementId.value) return
  busy.value = true
  const ok = await app.mutate(
    () => api.replaceCurrentContent(props.skillId, replacementId.value, revision.value),
    '当前内容已替换，工具与预设已统一',
  )
  if (ok) replaceOpen.value = false
  busy.value = false
}
const backup = computed(() =>
  app.snapshot?.contentBackups?.find((b) => b.sourceId === props.sourceId),
)
const affectedNames = computed(
  () =>
    app.snapshot?.skills
      .filter((s) => s.sourceId === props.sourceId)
      .map((s) => s.name)
      .join('、') || '',
)
function confirm() {
  revision.value = app.snapshot?.revision ?? 0
  open.value = true
}
async function restore() {
  if (busy.value) return
  busy.value = true
  const ok = await app.mutate(
    () => api.undoContentUpdate(props.sourceId, revision.value),
    '已撤销上次更新，自动更新改为仅提醒',
  )
  if (ok) open.value = false
  busy.value = false
}
</script>
<template>
  <section class="callout">
    <p>所有已管理工具使用当前内容。更新成功后统一生效，无需为每个工具选择版本。</p>
    <Button v-if="backup" @click="confirm">撤销上次更新</Button>
    <p v-else>暂无可恢复的上次更新。</p>
    <Button v-if="candidates.length" @click="confirmReplace">使用另一份已导入内容替换</Button>
  </section>
  <AppDialog
    v-model:open="open"
    title="撤销上次更新"
    description="同一来源包共享依赖，将一起恢复更新前内容并同步所有受管目标。"
  >
    <p>{{ affectedNames }}</p>
    <p>此次恢复成功后将消费恢复备份，并暂停自动应用更新。</p>
    <p v-if="app.error" class="field-error" role="alert">{{ app.error }}</p>
    <template #footer
      ><Button :disabled="busy" @click="open = false">取消</Button
      ><Button variant="primary" :loading="busy" @click="restore">确认恢复</Button></template
    >
  </AppDialog>
  <AppDialog
    v-model:open="replaceOpen"
    title="替换当前内容"
    description="所选内容将替换此 Skill，两个条目的管理工具和预设引用将合并。保留当前名称以及一次恢复备份。"
  >
    <AppSelect
      v-model="replacementId"
      :options="candidates"
      placeholder="选择已导入内容"
      aria-label="替换内容"
    />
    <p v-if="app.error" class="field-error" role="alert">{{ app.error }}</p>
    <template #footer
      ><Button :disabled="busy" @click="replaceOpen = false">取消</Button
      ><Button variant="primary" :disabled="!replacementId" :loading="busy" @click="replace"
        >确认替换并同步全部工具</Button
      ></template
    >
  </AppDialog>
</template>
