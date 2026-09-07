<script setup lang="ts">
import AppSelect from '@/components/ui/AppSelect.vue'
import { computed, ref, watch } from 'vue'
import { useField, useForm } from 'vee-validate'
import { z } from 'zod'
import AppDialog from './ui/AppDialog.vue'
import Button from './ui/Button.vue'
import DirectoryField from './DirectoryField.vue'
import { api } from '@/services/api'
import { useAppStore } from '@/stores/app'

const props = defineProps<{ open: boolean }>()
const emit = defineEmits<{ 'update:open': [value: boolean] }>()
const app = useAppStore()
const schema = z.object({
  name: z.string().trim().min(2, '名称至少 2 个字符'),
  tool: z.string().min(1, '请选择工具'),
  scope: z.enum(['user', 'project']),
  path: z.string().trim().min(1, '请输入目标目录'),
})
const { handleSubmit, resetForm, setErrors } = useForm<{
  name: string
  tool: string
  scope: 'user' | 'project'
  path: string
}>({
  initialValues: { name: 'Codex · 用户级', tool: 'Codex', scope: 'user', path: '~/.codex/skills' },
})
const { value: name, errorMessage: nameError } = useField<string>('name')
const { value: tool } = useField<string>('tool')
const { value: scope } = useField<'user' | 'project'>('scope')
const { value: path, errorMessage: pathError } = useField<string>('path')
const busy = ref(false)
const projectPath = ref('')
// Codex uses its compatible user directory by product choice; shared skills are a separate target.
const profiles = [
  {
    name: '通用 Agent Skills',
    user: '~/.agents/skills',
    project: '.agents/skills',
    docs: 'https://agentskills.io',
  },
  {
    name: 'Codex',
    user: '~/.codex/skills',
    project: '.agents/skills',
    docs: 'https://developers.openai.com/codex/skills/',
  },
  {
    name: 'Claude Code',
    user: '~/.claude/skills',
    project: '.claude/skills',
    docs: 'https://code.claude.com/docs/en/skills',
  },
  {
    name: 'Cursor',
    user: '~/.cursor/skills',
    project: '.cursor/skills',
    docs: 'https://cursor.com/docs/skills',
  },
  {
    name: 'OpenCode',
    user: '~/.config/opencode/skills',
    project: '.opencode/skills',
    docs: 'https://opencode.ai/docs/skills/',
  },
  {
    name: 'Gemini CLI',
    user: '~/.gemini/skills',
    project: '.gemini/skills',
    docs: 'https://geminicli.com/docs/cli/skills/',
  },
  {
    name: 'OpenClaw',
    user: '~/.openclaw/skills',
    project: 'skills',
    docs: 'https://docs.openclaw.ai/tools/skills',
  },
]
const profile = computed(() => profiles.find((item) => item.name === tool.value))
const defaultName = computed(() => {
  if (!profile.value) return ''
  const projectName = projectPath.value
    .trim()
    .replace(/[\\/]+$/, '')
    .split(/[\\/]/)
    .pop()
  return `${tool.value} · ${scope.value === 'user' ? '用户级' : projectName || '项目级'}`
})
const defaultPath = computed(() => {
  if (!profile.value) return ''
  if (scope.value === 'user') return profile.value.user
  const base = projectPath.value.trim().replace(/[\\/]+$/, '')
  if (!base && projectPath.value.trim() !== '/') return ''
  const separator = base.includes('\\') ? '\\' : '/'
  return `${base}${separator}${profile.value.project.replaceAll('/', separator)}`
})
const projectLabel = computed(() =>
  tool.value === 'OpenClaw' ? 'OpenClaw 工作空间目录' : '项目目录',
)
const directoryHint = computed(() => {
  if (!profile.value) return '输入该工具实际读取 Skill 的目录。'
  if (tool.value === '通用 Agent Skills')
    return '.agents/skills 是共享目录，可被支持此约定的多个工具读取。'
  if (tool.value === 'Codex')
    return scope.value === 'user'
      ? '默认使用 ~/.codex/skills；需要共享目录时，请选择「通用 Agent Skills」。'
      : 'Codex 项目级使用 .agents/skills，兼容工具也可能读取此目录。'
  if (tool.value === 'OpenClaw')
    return '使用自定义状态目录时请修改路径；项目级应选择 OpenClaw 配置中的工作空间。'
  return '已按工具默认规则生成；如果修改过工具配置，可在下方调整。'
})
watch(defaultName, (value, previous) => {
  if (!name.value || name.value === previous) name.value = value
})
watch(defaultPath, (value, previous) => {
  if (!path.value || path.value === previous) path.value = value
})
function restoreDefaults() {
  name.value = defaultName.value
  path.value = defaultPath.value
}
watch(
  () => props.open,
  (open) => {
    if (!open) return
    projectPath.value = ''
    resetForm()
  },
)
const submit = handleSubmit(async (values) => {
  const parsed = schema.safeParse(values)
  if (!parsed.success) {
    setErrors(
      Object.fromEntries(
        parsed.error.issues.map((issue) => [String(issue.path[0]), issue.message]),
      ),
    )
    return
  }
  busy.value = true
  const ok = await app.mutate(() => api.addTarget(parsed.data), '分发目标已添加')
  busy.value = false
  if (ok) emit('update:open', false)
})

const toolOptions = [
  ...profiles.map((item) => ({ value: item.name, label: item.name })),
  { value: '自定义', label: '自定义' },
]
const scopeOptions = [
  { value: 'user', label: '用户级' },
  { value: 'project', label: '项目级' },
]
</script>

<template>
  <AppDialog
    :open="open"
    title="添加分发目标"
    description="选择常用工具即可使用默认配置。添加后登记目录，分发时创建软链。"
    @update:open="emit('update:open', $event)"
    ><form id="target-form" class="form-grid" @submit="submit">
      <label class="field">
        <span class="field-label">Agent / Harness</span>
        <AppSelect v-model="tool" aria-label="工具" :options="toolOptions" />
      </label>
      <label class="field">
        <span class="field-label">作用域</span>
        <AppSelect v-model="scope" aria-label="作用域" :options="scopeOptions" />
      </label>
      <div v-if="scope === 'project' && profile" class="field" style="grid-column: 1/-1">
        <span class="field-label">{{ projectLabel }}</span>
        <DirectoryField
          v-model="projectPath"
          :placeholder="`选择${projectLabel}，自动生成 Skill 目录`"
        />
      </div>
      <label class="field" style="grid-column: 1/-1">
        <span class="field-label">目标名称</span>
        <input v-model="name" class="input" placeholder="例如：营销项目" />
        <span v-if="nameError" class="field-error">{{ nameError }}</span> </label
      ><label class="field" style="grid-column: 1/-1"
        ><span class="field-label">Skill 目录</span><DirectoryField v-model="path" /><span
          v-if="pathError"
          class="field-error"
          >{{ pathError }}</span
        ></label
      >
      <div class="field" style="grid-column: 1/-1">
        <span class="subtle">{{ directoryHint }}</span>
        <span class="subtle">用户级供多个项目使用；项目级仅用于所选项目。~ 表示当前用户目录。</span>
        <Button v-if="profile" variant="ghost" size="sm" @click="restoreDefaults"
          >恢复默认名称与目录</Button
        >
      </div>
    </form>
    <template #footer
      ><Button @click="emit('update:open', false)">取消</Button
      ><Button variant="primary" type="submit" form="target-form" :disabled="busy">{{
        busy ? '添加中…' : '添加目标'
      }}</Button></template
    ></AppDialog
  >
</template>
