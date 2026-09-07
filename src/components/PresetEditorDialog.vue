<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { Search, FolderSearch, Globe2, Plus, X } from 'lucide-vue-next'
import AppDialog from './ui/AppDialog.vue'
import Button from './ui/Button.vue'
import Badge from './ui/Badge.vue'
import DirectoryField from './DirectoryField.vue'
import { api } from '@/services/api'
import { useAppStore } from '@/stores/app'
import type { Preset, ScanItem } from '@/services/types'

const props = defineProps<{ open: boolean; preset?: Preset | null }>()
const emit = defineEmits<{ 'update:open': [value: boolean] }>()
const app = useAppStore()
const name = ref('')
const description = ref('')
const skillIds = ref<string[]>([])
const query = ref('')
const mode = ref<'library' | 'folder' | 'web'>('library')
const folder = ref(app.isNative ? '' : '/Users/demo/Work/new-pack')
const scanned = ref<ScanItem[]>([])
const scanSelected = ref<string[]>([])
const error = ref('')
const busy = ref(false)
const filtered = computed(
  () =>
    app.snapshot?.skills.filter(
      (skill) =>
        !query.value ||
        `${skill.name} ${skill.description}`.toLowerCase().includes(query.value.toLowerCase()),
    ) ?? [],
)
watch(
  () => props.open,
  (open) => {
    if (!open) return
    name.value = props.preset?.name || ''
    description.value = props.preset?.description || ''
    skillIds.value = [...(props.preset?.skillIds || [])]
    query.value = ''
    mode.value = 'library'
    scanned.value = []
    error.value = ''
  },
)
async function scanFolder() {
  busy.value = true
  error.value = ''
  try {
    const result = await api.scan(folder.value)
    scanned.value = result.items.filter((item) => ['ready', 'new', 'same'].includes(item.status))
    scanSelected.value = scanned.value.map((item) => item.path)
  } catch (e) {
    error.value = e instanceof Error ? e.message : '扫描失败'
  } finally {
    busy.value = false
  }
}
async function importSelected() {
  if (!scanSelected.value.length) return
  const before = new Set(app.snapshot?.skills.map((item) => item.id))
  const ok = await app.mutate(
    () => api.importFolder(folder.value, scanSelected.value, false),
    '文件夹成员已导入，可继续编辑预设',
  )
  if (ok) {
    const added =
      app.snapshot?.skills.filter((item) => !before.has(item.id)).map((item) => item.id) ?? []
    skillIds.value = [...new Set([...skillIds.value, ...added])]
    mode.value = 'library'
  }
}
async function save() {
  if (name.value.trim().length < 2) {
    error.value = '预设名称至少 2 个字符'
    return
  }
  if (!skillIds.value.length) {
    error.value = '请至少添加一个 Skill'
    return
  }
  busy.value = true
  const ok = await app.mutate(
    () =>
      api.savePreset({
        id: props.preset?.id,
        name: name.value.trim(),
        description: description.value.trim(),
        skillIds: skillIds.value,
      }),
    props.preset ? '预设已保存；已应用目标不会自动切换成员' : '预设已创建',
  )
  busy.value = false
  if (ok) emit('update:open', false)
}
</script>

<template>
  <AppDialog
    :open="open"
    :title="preset ? '编辑预设' : '新建预设'"
    description="成员会自动去重；保存定义不会自动分发。"
    large
    @update:open="emit('update:open', $event)"
    ><div class="form-grid" style="margin-bottom: 16px">
      <label class="field"
        ><span class="field-label">预设名称</span
        ><input v-model="name" class="input" placeholder="例如：前端交付" /></label
      ><label class="field"
        ><span class="field-label">描述</span
        ><input v-model="description" class="input" placeholder="说明适用场景"
      /></label>
    </div>
    <div class="segmented" style="margin-bottom: 13px">
      <button class="segment" :class="{ active: mode === 'library' }" @click="mode = 'library'">
        <Plus style="width: 13px; display: inline" /> 从库选择</button
      ><button class="segment" :class="{ active: mode === 'folder' }" @click="mode = 'folder'">
        <FolderSearch style="width: 13px; display: inline" /> 从文件夹</button
      ><button class="segment" :class="{ active: mode === 'web' }" @click="mode = 'web'">
        <Globe2 style="width: 13px; display: inline" /> 从网站
      </button>
    </div>
    <section v-if="mode === 'library'">
      <div class="search-field" style="margin-bottom: 10px">
        <Search /><input v-model="query" placeholder="筛选 Skill" />
      </div>
      <div class="choice-list" style="max-height: 280px; overflow: auto">
        <label v-for="skill in filtered" :key="skill.id" class="choice"
          ><input v-model="skillIds" class="checkbox" type="checkbox" :value="skill.id" />
          <div class="choice-main">
            <div class="choice-title">{{ skill.name }}</div>
            <div class="choice-meta">{{ skill.description }}</div>
          </div>
          <Badge>v{{ skill.version }}</Badge></label
        >
      </div>
    </section>
    <section v-else-if="mode === 'folder'">
      <label class="field"
        ><span class="field-label">包含 Skill 的文件夹</span
        ><DirectoryField v-model="folder" /></label
      ><Button style="margin-top: 10px" :disabled="busy" @click="scanFolder"
        ><FolderSearch />扫描文件夹</Button
      >
      <div v-if="scanned.length" class="choice-list" style="margin-top: 12px">
        <label v-for="item in scanned" :key="item.path" class="choice"
          ><input v-model="scanSelected" class="checkbox" type="checkbox" :value="item.path" />
          <div class="choice-main">
            <div class="choice-title">{{ item.name }}</div>
            <div class="choice-meta mono">{{ item.path }}</div>
          </div></label
        ><Button variant="primary" @click="importSelected">导入所选并加入预设</Button>
      </div>
    </section>
    <section v-else class="empty" style="min-height: 200px">
      <div>
        <div class="empty-icon"><Globe2 /></div>
        <h3>先安装，再加入预设</h3>
        <p>网站结果需要先完成内容确认并安装到中央库。请先保存当前预设，再前往安装。</p>
        <RouterLink class="btn btn-primary" to="/discover" @click="emit('update:open', false)"
          >前往发现与安装</RouterLink
        >
      </div>
    </section>
    <div v-if="skillIds.length" class="callout" style="margin-top: 13px">
      已选择 {{ skillIds.length }} 个成员。<span
        v-for="id in skillIds.slice(0, 4)"
        :key="id"
        class="badge badge-neutral"
        style="margin-left: 5px"
        >{{ app.snapshot?.skills.find((skill) => skill.id === id)?.name }}</span
      ><span v-if="skillIds.length > 4" class="subtle"> 等</span>
    </div>
    <p v-if="error" class="field-error">{{ error }}</p>
    <template #footer
      ><Button @click="emit('update:open', false)">取消</Button
      ><Button variant="primary" :disabled="busy" @click="save">{{
        busy ? '保存中…' : '保存预设'
      }}</Button></template
    ></AppDialog
  >
</template>
