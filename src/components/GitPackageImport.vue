<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { api, type GitPackagePreview, type GitPackageResult } from '@/services/api'
import { useAppStore } from '@/stores/app'
import Button from './ui/Button.vue'
import Badge from './ui/Badge.vue'
import AppSelect from './ui/AppSelect.vue'
const props = defineProps<{ draft?: boolean }>()
const emit = defineEmits<{
  imported: [result: GitPackageResult, autoAdd: boolean, removedIds: string[]]
}>()
const app = useAppStore()
const url = ref('')
const reference = ref('')
const subdir = ref('')
const preview = ref<GitPackagePreview | null>(null)
const selected = ref<string[]>([])
const removed = ref<string[]>([])
const attach = ref('none')
const presetId = ref('')
const presetName = ref('')
const autoAdd = ref(true)
const busy = ref(false)
const installing = ref(false)
const error = ref('')
let request = 0
watch([url, reference, subdir], () => {
  request++
  preview.value = null
  error.value = ''
})
const presets = computed(() => [
  { value: '', label: '选择预设' },
  ...(app.snapshot?.presets ?? []).map((p) => ({ value: p.id, label: p.name })),
])
async function scan() {
  if (busy.value) return
  const seq = ++request
  busy.value = true
  error.value = ''
  preview.value = null
  removed.value = []
  try {
    const next = await api.previewGitPackage(
      url.value.trim(),
      reference.value.trim() || 'HEAD',
      subdir.value.trim(),
    )
    if (seq !== request) return
    preview.value = next
    selected.value = next.items.map((i) => i.path)
  } catch (e) {
    if (seq === request) error.value = String(e instanceof Error ? e.message : e)
  } finally {
    busy.value = false
  }
}
async function install() {
  if (!preview.value || busy.value) return
  if (!props.draft && attach.value === 'existing' && !presetId.value) {
    error.value = '请选择预设'
    return
  }
  if (!props.draft && attach.value === 'new' && presetName.value.trim().length < 2) {
    error.value = '预设名称至少 2 个字符'
    return
  }
  busy.value = true
  installing.value = true
  error.value = ''
  try {
    const result = await api.importGitPackage({
      token: preview.value.token,
      revision: preview.value.revision,
      selectedPaths: selected.value,
      autoAdd: autoAdd.value,
      presetId: !props.draft && attach.value === 'existing' ? presetId.value : undefined,
      presetName: !props.draft && attach.value === 'new' ? presetName.value.trim() : undefined,
      removedIds: !props.draft && attach.value === 'existing' ? removed.value : [],
    })
    app.snapshot = result.snapshot
    emit('imported', result, autoAdd.value, removed.value)
    preview.value = null
  } catch (e) {
    error.value = String(e instanceof Error ? e.message : e)
  } finally {
    busy.value = false
    installing.value = false
  }
}
</script>
<template>
  <div class="list-stack git-package-import">
    <label class="field"
      ><span class="field-label">Git 仓库或 GitHub 文件夹链接</span>
      <div class="git-source-input">
        <input v-model="url" class="input" :disabled="busy" />
        <Button :disabled="busy || !url.trim()" :loading="busy && !installing" @click="scan">{{
          busy ? '正在处理…' : '预览集合'
        }}</Button>
      </div></label
    >
    <p v-if="!preview" class="subtle">
      支持 GitHub、GitLab 及自建 Git 服务，可导入单个 Skill、部分成员或整个集合。
    </p>
    <div class="form-grid">
      <label class="field"
        ><span class="field-label">分支 / 标签（可选）</span
        ><input
          v-model="reference"
          class="input"
          :disabled="busy"
          placeholder="自动识别，复杂分支可手动填写"
      /></label>
      <label class="field"
        ><span class="field-label">子目录（可选）</span
        ><input v-model="subdir" class="input" :disabled="busy" placeholder="自动识别，例如 skills"
      /></label>
    </div>
    <div v-if="error || preview" class="git-package-notice" role="status">
      <p v-if="error" class="field-error" role="alert">{{ error }}</p>
      <template v-if="preview">
        <p
          :title="`${preview.url} · ${preview.reference} / ${preview.subdir || '.'} · ${preview.commit.slice(0, 12)}`"
        >
          <strong>{{ preview.name }} · {{ preview.items.length }} 个 Skill</strong> ·
          仅所选成员入库，整仓共享资源保留。
        </p>
        <p v-if="preview.removed.length" class="choice-meta">
          来源已移除 {{ preview.removed.length }} 项，默认保留。选择移除后，仅解除所选预设引用。
        </p>
        <p v-for="issue in preview.issues" :key="issue" class="field-error">{{ issue }}</p>
      </template>
    </div>
    <template v-if="preview">
      <div class="git-package-data" aria-label="Git 集合数据">
        <div class="choice-list">
          <label v-for="item in preview.items" :key="item.path" class="choice">
            <input
              v-model="selected"
              class="checkbox"
              type="checkbox"
              :value="item.path"
              :disabled="busy"
            />
            <div class="choice-main">
              <div class="choice-title">
                {{ item.name }}
                <Badge>{{
                  item.change === 'added'
                    ? '新增'
                    : item.change === 'changed'
                      ? '内容变更'
                      : '已入库'
                }}</Badge>
              </div>
              <div class="choice-meta mono">{{ item.path }}</div>
              <div class="choice-meta">{{ item.description }}</div>
            </div>
          </label>
        </div>
        <div v-if="preview.removed.length" class="callout">
          <strong>来源已移除 {{ preview.removed.length }} 项</strong>
          <label v-for="item in preview.removed" :key="item.skillId" class="choice"
            ><input
              v-if="draft || attach === 'existing'"
              v-model="removed"
              class="checkbox"
              type="checkbox"
              :value="item.skillId"
              :disabled="busy"
            />{{ item.name }}</label
          >
        </div>
      </div>
      <div class="git-package-controls">
        <div class="git-package-options">
          <template v-if="!draft">
            <label class="field"
              ><span class="field-label">导入后</span
              ><AppSelect
                v-model="attach"
                :options="[
                  { value: 'none', label: '仅入库' },
                  { value: 'new', label: '新建预设' },
                  { value: 'existing', label: '加入已有预设' },
                ]"
            /></label>
            <AppSelect
              v-if="attach === 'existing'"
              v-model="presetId"
              :options="presets"
              aria-label="选择已有预设"
            />
            <input
              v-if="attach === 'new'"
              v-model="presetName"
              class="input"
              placeholder="预设名称"
            />
          </template>
          <label v-if="draft || attach !== 'none'" class="git-member-policy"
            ><input
              v-model="autoAdd"
              type="checkbox"
              class="checkbox"
            />自动加入后续新增成员；排除项保持不选</label
          >
        </div>
        <Button variant="primary" :disabled="busy" :loading="installing" @click="install">{{
          busy ? '同步中…' : draft ? '同步集合并加入预设' : '同步 Git 集合'
        }}</Button>
      </div>
    </template>
  </div>
</template>

<style scoped>
.git-package-import {
  min-height: 0;
  overflow: hidden;
  gap: 8px;
}
.git-package-import > * {
  flex-shrink: 0;
}
.git-package-import > .field,
.git-package-import > .form-grid {
  margin-bottom: 0;
}
.git-package-data {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  overscroll-behavior: contain;
  scrollbar-gutter: stable;
  display: flex;
  flex-direction: column;
  gap: 12px;
}
.git-package-data > * {
  flex-shrink: 0;
}
.git-package-import > .choice-meta {
  margin: 0;
}
</style>

<style scoped>
.git-package-options {
  border-top: 1px solid var(--line);
  padding-top: 10px;
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.git-package-options .field {
  margin: 0;
}
</style>

<style scoped>
.git-source-input {
  display: flex;
  align-items: center;
  gap: 8px;
}
.git-source-input .input {
  flex: 1;
  min-width: 0;
}
.git-member-policy {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 13px;
}
</style>

<style scoped>
.git-package-controls {
  display: flex;
  align-items: flex-end;
  gap: 12px;
  border-top: 1px solid var(--line);
  padding-top: 10px;
}
.git-package-controls > .git-package-options {
  flex: 1;
  min-width: 0;
  border: 0;
  padding: 0;
}
.git-package-controls > .btn {
  flex-shrink: 0;
}
</style>

<style scoped>
.git-package-notice {
  max-height: 80px;
  overflow: auto;
  overscroll-behavior: contain;
  padding: 6px 8px;
  background: var(--surface);
  border-radius: 6px;
  font-size: 12px;
}
.git-package-notice p {
  margin: 0;
}
</style>
