<script setup lang="ts">
import { computed, ref } from 'vue'
import { api, type SingleContentPreview, type SingleContentSnapshotIssue } from '@/services/api'
import { useAppStore } from '@/stores/app'
import Button from './ui/Button.vue'
import AppDialog from './ui/AppDialog.vue'
const app = useAppStore()
const open = ref(false)
const busy = ref(false)
const error = ref('')
const preview = ref<SingleContentPreview | null>(null)
const selections = ref<Record<string, string>>({})
const onlyPending = ref(true)
const enabled = computed(() => (app.snapshot?.schemaVersion ?? 0) >= 3)
const arranged = computed(() => /[\\/]\.skilldock$/.test(app.snapshot?.storageRoot || ''))
const conflicts = computed(
  () => preview.value?.groups.filter((g) => g.skills.length > 1 && !g.identical) ?? [],
)
const pendingCount = computed(
  () => conflicts.value.filter((group) => !selections.value[group.name]).length,
)
const visibleConflicts = computed(() =>
  onlyPending.value
    ? conflicts.value.filter((group) => !selections.value[group.name])
    : conflicts.value,
)
const gitBatches = computed(() =>
  (app.snapshot?.sources ?? [])
    .filter((source) => source.kind === 'git')
    .flatMap((source) => {
      const members = conflicts.value.flatMap((group) => {
        const candidates = group.skills.filter((skill) => skill.sourceId === source.id)
        // Multiple records within one source still need an explicit member choice.
        return candidates.length === 1 ? [{ name: group.name, skillId: candidates[0]!.id }] : []
      })
      return members.length
        ? [
            {
              sourceId: source.id,
              label: app.sourceName(source),
              url: source.url,
              choices: members.filter((member) => !selections.value[member.name]),
            },
          ]
        : []
    }),
)
function selectGitSource(sourceId: string) {
  if (busy.value || enabled.value) return
  const batch = gitBatches.value.find((item) => item.sourceId === sourceId)
  for (const choice of batch?.choices ?? []) {
    selections.value[choice.name] = choice.skillId
  }
}
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
const selectedSnapshotIssues = computed(() => {
  const selectedIds = new Set(
    preview.value?.groups.flatMap((group) => {
      const choice = selections.value[group.name]
      return group.skills
        .filter(
          (skill) => !choice || choice === 'separate' || group.identical || skill.id === choice,
        )
        .map((skill) => skill.id)
    }) ?? [],
  )
  return (preview.value?.snapshotIssues ?? [])
    .filter((issue) => issue.skillIds.some((id) => selectedIds.has(id)))
    .map((issue) => ({ ...issue, sourceLabel: issue.sourceIds.map(sourceLabel).join('；') }))
})
const incomplete = computed(() => pendingCount.value > 0 || selectedSnapshotIssues.value.length > 0)
async function inspect(resetSelections = true) {
  if (busy.value) return
  busy.value = true
  error.value = ''
  try {
    preview.value = await api.previewSingleContent()
    if (resetSelections) {
      selections.value = {}
      onlyPending.value = true
    } else {
      for (const [name, choice] of Object.entries(selections.value)) {
        const group = preview.value.groups.find((group) => group.name === name)
        if (!group || (choice !== 'separate' && !group.skills.some((skill) => skill.id === choice)))
          delete selections.value[name]
      }
    }
    open.value = true
  } catch (reason) {
    app.error = reason instanceof Error ? reason.message : String(reason)
  } finally {
    busy.value = false
  }
}
async function repairSnapshot(issue: SingleContentSnapshotIssue) {
  if (!preview.value || !issue.contentDigest || busy.value || enabled.value) return
  busy.value = true
  error.value = ''
  try {
    app.snapshot = await api.refreshSnapshot(
      issue.skillId,
      preview.value.revision,
      issue.contentDigest,
    )
    preview.value = await api.previewSingleContent()
    app.notice = '已重新收录当前文件，已有选择保留，请继续确认迁移'
  } catch (reason) {
    error.value = reason instanceof Error ? reason.message : String(reason)
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
    app.snapshot = await api.enableSingleContent(
      preview.value.revision,
      choices,
      preview.value.directoryRepairs,
    )
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
    <Button v-if="!enabled" :loading="busy" @click="inspect()">预览简化资料库</Button>
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
    <Button v-if="!enabled" :loading="busy" @click="inspect(false)">重新检查（保留选择）</Button>
    <section v-if="selectedSnapshotIssues.length" class="callout">
      <strong>所选内容有 {{ selectedSnapshotIssues.length }} 个来源包需要处理</strong>
      <p>重新收录会将当前文件保存为新快照，保留原文件和现有分发链接。确认迁移后才切换工具内容。</p>
      <div
        v-for="issue in selectedSnapshotIssues"
        :key="issue.bundleDigest"
        class="git-source-batch"
      >
        <strong class="repair-path">{{ issue.sourceLabel }}</strong>
        <p>{{ issue.reason }}</p>
        <details>
          <summary>影响 {{ issue.skillNames.length }} 个 Skill</summary>
          <ul>
            <li v-for="name in issue.skillNames" :key="name">{{ name }}</li>
          </ul>
        </details>
        <Button :disabled="busy || enabled || !issue.contentDigest" @click="repairSnapshot(issue)">
          重新收录当前文件
        </Button>
      </div>
    </section>
    <section v-if="gitBatches.length" class="callout">
      <strong>按 Git 来源批量选择</strong>
      <p>使用该来源已收录的内容，保留已手动选择的项目；提交前仍可逐项调整。</p>
      <div v-for="batch in gitBatches" :key="batch.sourceId" class="git-source-batch">
        <span class="repair-path">{{ batch.label }} · {{ batch.url }}</span>
        <Button
          :disabled="busy || enabled || !batch.choices.length"
          @click="selectGitSource(batch.sourceId)"
          >{{
            batch.choices.length ? `为 ${batch.choices.length} 项选择此来源` : '此来源已无待选项'
          }}</Button
        >
      </div>
    </section>
    <section v-if="preview?.directoryRepairs.length" class="callout">
      <strong>检测到 {{ preview.directoryRepairs.length }} 个分发位置已变成实体文件夹</strong>
      <p>
        确认后会先完整备份这些目录，再改为指向所选统一库内容的链接。目录中的不同内容不会自动合入统一库，备份可在“归集备份”中找回。
      </p>
      <ul>
        <li v-for="repair in preview.directoryRepairs" :key="repair.bindingId" class="repair-path">
          {{ repair.path }}
        </li>
      </ul>
    </section>
    <div v-if="conflicts.length" class="selection-progress">
      <p role="status">已选择 {{ conflicts.length - pendingCount }} / {{ conflicts.length }} 项</p>
      <label>
        <input v-model="onlyPending" type="checkbox" />
        仅显示未选择项（取消勾选可复核全部）
      </label>
    </div>
    <div v-for="group in visibleConflicts" :key="group.name" class="list-stack">
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
      <Button v-else variant="primary" :disabled="incomplete" :loading="busy" @click="apply">{{
        preview?.directoryRepairs.length ? '备份目录并统一内容' : '确认统一内容并整理'
      }}</Button>
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
.repair-path {
  overflow-wrap: anywhere;
}
.git-source-batch {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 8px;
  margin-top: 12px;
}
.selection-progress {
  margin-block: 16px;
}
</style>
