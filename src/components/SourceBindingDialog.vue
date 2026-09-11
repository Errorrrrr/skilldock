<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import AppDialog from './ui/AppDialog.vue'
import AppSelect from './ui/AppSelect.vue'
import Button from './ui/Button.vue'
import DirectoryField from './DirectoryField.vue'
import { api, type SourceBindingInput } from '@/services/api'
import { useAppStore } from '@/stores/app'
import type { Source } from '@/services/types'
const props = defineProps<{ open: boolean; source: Source | null }>()
const emit = defineEmits<{ 'update:open': [boolean]; completed: [] }>()
const app = useAppStore()
const kind = ref<'git' | 'local'>('git')
const url = ref('')
const path = ref('')
const reference = ref('HEAD')
const subdir = ref('')
const busy = ref(false)
const error = ref('')
const preview = ref<Awaited<ReturnType<typeof api.previewBindSource>> | null>(null)
const count = computed(
  () => app.snapshot?.skills.filter((s) => s.sourceId === props.source?.id).length || 0,
)
const kinds = [
  { value: 'git', label: 'Git 仓库' },
  { value: 'local', label: '独立本地源码目录' },
]
const input = computed<SourceBindingInput>(() => ({
  sourceId: props.source?.id || '',
  kind: kind.value,
  url: url.value.trim(),
  path: path.value.trim(),
  reference: reference.value.trim() || 'HEAD',
  subdir: subdir.value.trim(),
}))
const valid = computed(
  () => !!props.source && (kind.value === 'git' ? !!url.value.trim() : !!path.value.trim()),
)
watch(
  () => props.open,
  (open) => {
    if (open) {
      kind.value = 'git'
      url.value = ''
      path.value = ''
      reference.value = 'HEAD'
      subdir.value = ''
      preview.value = null
      error.value = ''
    }
  },
)
watch(input, () => {
  preview.value = null
  error.value = ''
})
function close(open: boolean) {
  if (!busy.value) emit('update:open', open)
}
async function submit() {
  if (!valid.value || busy.value) return
  busy.value = true
  error.value = ''
  const requested = input.value
  try {
    if (!preview.value) {
      const result = await api.previewBindSource(requested)
      if (JSON.stringify(input.value) === JSON.stringify(requested)) preview.value = result
      return
    }
    const confirmed = preview.value
    const ok = await app.mutate(
      () => api.bindSource(requested, confirmed.revision, confirmed.digest),
      '更新来源已配置；现有安装和分发保持不变',
    )
    if (ok) {
      emit('completed')
      emit('update:open', false)
    } else {
      error.value = app.error
      preview.value = null
    }
  } catch (reason) {
    error.value = reason instanceof Error ? reason.message : '来源核对失败'
  } finally {
    busy.value = false
  }
}
</script>
<template>
  <AppDialog
    :open="open"
    title="配置更新来源"
    :description="`此配置用于该来源下的 ${count} 个 Skill。先核对成员，再保存来源。`"
    @update:open="close"
  >
    <div v-if="!preview" class="list-stack">
      <p class="callout">归集目录已成为分发位置。请选择原始 Git 仓库，或独立维护源码的文件夹。</p>
      <label class="field"
        ><span class="field-label">来源类型</span
        ><AppSelect v-model="kind" :options="kinds" :disabled="busy" aria-label="来源类型"
      /></label>
      <template v-if="kind === 'git'">
        <label class="field"
          ><span class="field-label">Git 仓库地址</span
          ><input
            v-model="url"
            class="input"
            :disabled="busy"
            placeholder="https://github.com/组织/仓库.git"
        /></label>
        <label class="field"
          ><span class="field-label">分支或标签</span
          ><input v-model="reference" class="input" :disabled="busy" placeholder="HEAD"
        /></label>
        <label class="field"
          ><span class="field-label">扫描子目录（可选）</span
          ><input v-model="subdir" class="input" :disabled="busy" placeholder="例如 skills"
        /></label>
      </template>
      <label v-else class="field"
        ><span class="field-label">独立源码目录</span><DirectoryField v-model="path"
      /></label>
      <p class="subtle">
        新来源需包含现有成员，名称及相对目录保持一致。不同 Skill
        来自不同仓库时，不应将整个来源包绑定到其中一个仓库。
      </p>
    </div>
    <div v-else class="list-stack">
      <p>已匹配 {{ preview.memberNames.length }} 个 Skill，可以保存。</p>
      <p class="subtle">{{ preview.memberNames.join('、') }}</p>
      <div class="callout">
        只保存更新来源，不替换当前内容或分发链接。发现新内容后可在更新中心查看；定时更新默认关闭。
      </div>
    </div>
    <p v-if="error" class="field-error" role="alert">{{ error }}</p>
    <template #footer>
      <Button :disabled="busy" @click="close(false)">取消</Button>
      <Button v-if="preview" :disabled="busy" @click="preview = null">返回修改</Button>
      <Button variant="primary" :disabled="busy || !valid" @click="submit">{{
        busy ? '处理中…' : preview ? '确认保存来源' : '核对来源成员'
      }}</Button>
    </template>
  </AppDialog>
</template>
