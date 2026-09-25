<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { useRoute } from 'vue-router'
import { Plus, FolderSearch, Link2, Unlink, Settings2, Trash2 } from 'lucide-vue-next'
import Button from '@/components/ui/Button.vue'
import AppDialog from '@/components/ui/AppDialog.vue'
import AppSheet from '@/components/ui/AppSheet.vue'
import AppPagination from '@/components/ui/AppPagination.vue'
import DirectoryField from '@/components/DirectoryField.vue'
import DistributionDialog from '@/components/DistributionDialog.vue'
import SkillDirectoryActions from '@/components/SkillDirectoryActions.vue'
import { useAppStore } from '@/stores/app'
import { usePagination } from '@/composables/usePagination'
import { api, type LocalSourcePreview } from '@/services/api'
import type { Source } from '@/services/types'

const app = useAppStore()
const route = useRoute()
const query = ref('')
const sources = computed(() =>
  (app.snapshot?.sources ?? [])
    .filter((s) => ['local_managed', 'local_reference'].includes(s.kind) && !s.updatesRemoved)
    .map((source) => {
      const ids =
        source.localMemberIds ??
        (app.snapshot?.skills ?? []).filter((s) => s.sourceId === source.id).map((s) => s.id)
      const members = (app.snapshot?.skills ?? []).filter((s) => ids.includes(s.id))
      const targetIds = [
        ...new Set(
          (app.snapshot?.bindings ?? [])
            .filter((b) => b.claims.includes(`source:${source.id}`))
            .map((b) => b.targetId),
        ),
      ]
      return { source, ids, members, targetIds }
    }),
)
const filtered = computed(() =>
  sources.value.filter((row) =>
    `${row.source.name} ${row.source.path}`
      .toLowerCase()
      .includes(query.value.trim().toLowerCase()),
  ),
)
const { page, pageSize, pagedItems, resetPage } = usePagination(filtered, { initialPageSize: 10 })
watch(query, resetPage)
const editorOpen = ref(false)
const editing = ref<Source | null>(null)
const name = ref('')
const path = ref('')
const preview = ref<LocalSourcePreview | null>(null)
const selected = ref<string[]>([])
const busy = ref(false)
const error = ref('')
const legacy = computed(() => editing.value?.kind === 'local_reference')
const migrate = ref(false)
const keepConflicts = ref(false)
const conflictingNames = computed(() => {
  const seen = new Set<string>()
  return (preview.value?.items ?? []).filter((item) => {
    if (!selected.value.includes(item.path)) return false
    const key = item.name.toLowerCase()
    const repeated = seen.has(key)
    seen.add(key)
    return item.sameName || repeated
  })
})
function originalMemberPath(
  source: Source,
  skill: { externalPath?: string; relativePath: string },
) {
  return (
    skill.externalPath ||
    `${source.path.replace(/[\\/]+$/, '')}/${skill.relativePath}`.replace(/\/$/, '')
  ).replaceAll('\\', '/')
}
const missing = computed(() => {
  if (!editing.value || !preview.value) return []
  const row = sources.value.find((row) => row.source.id === editing.value?.id)
  return (
    row?.members.filter(
      (skill) =>
        !preview.value?.items.some(
          (item) => item.path.replaceAll('\\', '/') === originalMemberPath(editing.value!, skill),
        ),
    ) ?? []
  )
})
function edit(source: Source | null = null) {
  editing.value = source
  name.value = source?.name ?? ''
  path.value = source?.path ?? ''
  preview.value = null
  selected.value = []
  error.value = ''
  migrate.value = false
  keepConflicts.value = false
  editorOpen.value = true
}
watch(path, () => {
  preview.value = null
  selected.value = []
  error.value = ''
  keepConflicts.value = false
})
async function scan() {
  busy.value = true
  error.value = ''
  preview.value = null
  try {
    const result = await api.previewLocalSource(path.value.trim(), editing.value?.id)
    preview.value = result
    const current = sources.value.find(
      (row) =>
        row.source.id === editing.value?.id ||
        (!editing.value && row.source.kind === 'local_managed' && row.source.path === result.root),
    )
    if (!editing.value && current) {
      editing.value = current.source
      name.value = current.source.name
    }
    selected.value = result.items
      .filter((item) => {
        const known =
          current &&
          app.snapshot?.skills.find(
            (skill) =>
              (skill.sourceId === current.source.id || current.ids.includes(skill.id)) &&
              originalMemberPath(current.source, skill) === item.path.replaceAll('\\', '/'),
          )
        return !current || !known || current.ids.includes(known.id)
      })
      .map((item) => item.path)
  } catch (reason) {
    error.value = reason instanceof Error ? reason.message : String(reason)
  } finally {
    busy.value = false
  }
}
async function save() {
  if (!preview.value || busy.value) return
  busy.value = true
  error.value = ''
  try {
    const result = await api.saveLocalSource({
      name: name.value.trim(),
      path: preview.value.root,
      selectedPaths: selected.value,
      revision: preview.value.revision,
      contentDigest: preview.value.contentDigest,
      sourceId: editing.value?.id,
      migrate: legacy.value && migrate.value,
      keepConflicts: keepConflicts.value,
    })
    app.snapshot = result.snapshot
    app.notice = '本地内容已同步到统一库，原始文件保留'
    editorOpen.value = false
  } catch (reason) {
    error.value = reason instanceof Error ? reason.message : String(reason)
  } finally {
    busy.value = false
  }
}
onMounted(() => {
  if (route.query.add === 'true') {
    edit()
    if (typeof route.query.path === 'string') path.value = route.query.path
  }
})
const detailId = ref('')
const detailOpen = ref(false)
function openDetail(id: string) {
  detailId.value = id
  detailOpen.value = true
}
const detail = computed(() => sources.value.find((row) => row.source.id === detailId.value))
const distributionOpen = ref(false)
const distributionId = ref('')
const distribution = computed(() =>
  sources.value.find((row) => row.source.id === distributionId.value),
)
function distribute(source: Source) {
  distributionId.value = source.id
  distributionOpen.value = true
}
const confirmation = ref<'revoke' | 'remove' | null>(null)
const confirmId = ref('')
const confirmRevision = ref(0)
const confirmError = ref('')
const confirmBusy = ref(false)
const targetIds = ref<string[]>([])
const confirmed = computed(() => sources.value.find((row) => row.source.id === confirmId.value))
const confirmTargets = computed(() =>
  (app.snapshot?.targets ?? []).filter((t) => confirmed.value?.targetIds.includes(t.id)),
)
function confirm(source: Source, action: 'revoke' | 'remove') {
  confirmId.value = source.id
  confirmRevision.value = app.snapshot?.revision ?? 0
  confirmError.value = ''
  confirmation.value = action
  targetIds.value = [...(sources.value.find((row) => row.source.id === source.id)?.targetIds ?? [])]
}
async function execute() {
  if (confirmBusy.value || !confirmation.value) return
  confirmBusy.value = true
  const action = confirmation.value
  try {
    const ok = await app.mutate(
      () =>
        action === 'remove'
          ? api.removeLocalSource(confirmId.value, confirmRevision.value)
          : api.revokeLocalSource(confirmId.value, targetIds.value, confirmRevision.value),
      action === 'remove' ? '本地来源配置已移除，原文件保留' : '已取消所选目标的来源分发',
    )
    if (ok) {
      confirmation.value = null
      if (action === 'remove') detailOpen.value = false
    } else confirmError.value = app.error
  } finally {
    confirmBusy.value = false
  }
}
</script>

<template>
  <div class="page">
    <header class="page-heading">
      <div>
        <h1 class="page-title">本地来源</h1>
        <p class="page-subtitle">
          将本地 Skill 和共享资源复制到统一库，再整体分发。原目录修改后手动扫描同步。
        </p>
      </div>
      <Button variant="primary" @click="edit()"><Plus />添加本地来源</Button>
    </header>
    <section class="panel">
      <div class="toolbar">
        <input
          v-model="query"
          class="input"
          aria-label="搜索本地来源"
          placeholder="搜索名称或目录"
        />
      </div>
      <div class="table-wrap">
        <table class="data-table">
          <thead>
            <tr>
              <th>本地来源</th>
              <th style="width: 90px">成员</th>
              <th style="width: 100px">分发目标</th>
              <th style="width: 285px">操作</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="row in pagedItems" :key="row.source.id">
              <td>
                <button class="item-name-button" @click="openDetail(row.source.id)">
                  {{ row.source.name }}
                </button>
                <div class="local-source-path">{{ row.source.path }}</div>
                <div class="subtle">
                  {{
                    row.source.kind === 'local_reference'
                      ? '旧本地引用 · 仍直接使用原目录'
                      : '已统一入库 · 手动同步'
                  }}
                </div>
              </td>
              <td>{{ row.members.length }}</td>
              <td>{{ row.targetIds.length }}</td>
              <td>
                <div class="actions">
                  <Button size="sm" :disabled="!row.members.length" @click="distribute(row.source)"
                    ><Link2 />分发</Button
                  ><Button
                    size="sm"
                    :disabled="!row.targetIds.length"
                    @click="confirm(row.source, 'revoke')"
                    ><Unlink />取消分发</Button
                  ><Button size="sm" @click="edit(row.source)"
                    ><Settings2 />{{
                      row.source.kind === 'local_reference' ? '迁入统一库' : '同步与配置'
                    }}</Button
                  >
                </div>
              </td>
            </tr>
            <tr v-if="!filtered.length">
              <td colspan="4">
                <div class="empty-state">
                  <FolderSearch />
                  <h3>{{ query ? '没有匹配的来源' : '添加一个本地 Skill 文件夹' }}</h3>
                  <p>保留原包结构和共享资源；入库后可整体分发，也可加入预设。</p>
                </div>
              </td>
            </tr>
          </tbody>
        </table>
      </div>
      <AppPagination
        v-if="filtered.length"
        v-model:page="page"
        v-model:page-size="pageSize"
        :total="filtered.length"
      />
    </section>
    <AppDialog
      :open="editorOpen"
      fixed-layout
      :title="legacy ? '将旧本地引用迁入统一库' : editing ? '同步与配置本地来源' : '添加本地来源'"
      description="扫描后确认成员；保存会复制当前内容，并同步已有分发目标。原始文件保留。"
      @update:open="!busy && (editorOpen = $event)"
    >
      <div class="list-stack local-source-editor">
        <label v-if="legacy" class="choice">
          <input v-model="migrate" class="checkbox" type="checkbox" :disabled="busy" />
          <span
            >确认复制到统一库，并将此来源的受管分发链接切换到库内内容。此后原目录修改需手动同步。</span
          >
        </label>
        <label class="field"
          ><span class="field-label">名称</span
          ><input v-model="name" class="input" :disabled="busy" maxlength="120"
        /></label>
        <label class="field"
          ><span class="field-label">原始文件夹</span
          ><DirectoryField v-model="path" :disabled="busy || !!editing"
        /></label>
        <Button :disabled="busy || !path.trim()" @click="scan"
          ><FolderSearch />{{ busy ? '处理中…' : '扫描成员' }}</Button
        >
        <template v-if="preview">
          <label v-if="conflictingNames.length" class="choice">
            <input v-model="keepConflicts" class="checkbox" type="checkbox" :disabled="busy" />
            <span
              >独立保留同名成员：{{
                conflictingNames.map((item) => item.name).join('、')
              }}。同一分发目标只可启用其中一份。</span
            >
          </label>
          <div class="actions">
            <strong>已选择 {{ selected.length }} / {{ preview.items.length }}</strong
            ><Button size="sm" :disabled="busy" @click="selected = preview.items.map((i) => i.path)"
              >全选</Button
            ><Button size="sm" :disabled="busy" @click="selected = []">清空</Button>
          </div>
          <div class="local-member-list">
            <label v-for="item in preview.items" :key="item.path" class="choice"
              ><input
                v-model="selected"
                class="checkbox"
                type="checkbox"
                :value="item.path"
                :disabled="busy"
              />
              <div>
                <strong>{{ item.name }}</strong>
                <div class="local-source-path">{{ item.path }}</div>
                <div v-if="item.sameName" class="subtle">
                  库中存在同名的其他来源；内容不同会独立保留，同一目标分发前需确认冲突。
                </div>
                <div v-if="item.existingLinks.length" class="subtle">
                  发现 {{ item.existingLinks.length }} 处已有链接，分发时可选择复用或接管管理。
                </div>
              </div></label
            >
            <p v-if="!preview.items.length" class="subtle">没有发现可用的 Skill。</p>
          </div>
        </template>
      </div>
      <template #notice v-if="error || missing.length || preview?.warnings.length">
        <p v-if="missing.length" class="callout warning">
          未扫描到：{{
            missing.map((skill) => skill.name).join('、')
          }}。保存将取消这些成员的来源分发引用，其他引用与原文件保留。
        </p>
        <p v-for="warning in preview?.warnings" :key="warning" class="subtle">{{ warning }}</p>
        <p v-if="error" class="field-error" role="alert">{{ error }}</p>
      </template>
      <template #footer
        ><Button :disabled="busy" @click="editorOpen = false">取消</Button
        ><Button
          variant="primary"
          :disabled="
            busy ||
            !name.trim() ||
            !preview ||
            (!editing && !selected.length) ||
            (legacy && !migrate) ||
            (conflictingNames.length > 0 && !keepConflicts)
          "
          @click="save"
          >{{ legacy ? '复制并切换分发' : editing ? '同步到统一库' : '添加到统一库' }}</Button
        ></template
      >
    </AppDialog>
    <AppSheet
      v-model:open="detailOpen"
      :title="detail?.source.name ?? '本地来源'"
      :description="detail?.source.path"
    >
      <p class="callout">
        {{
          detail?.source.kind === 'local_reference'
            ? '这是旧版本的本地引用，实体仍在原目录，修改会直接生效。可明确选择迁入统一库；迁入前保持原有关系。'
            : '分发使用统一库中的内容，不依赖原目录。原目录修改或成员变化后，通过“同步与配置”扫描并保存。'
        }}
      </p>
      <div class="list-stack">
        <div v-for="skill in detail?.members" :key="skill.id" class="list-row">
          <div class="list-row-main">
            <strong>{{ skill.name }}</strong
            ><SkillDirectoryActions :skill="skill" />
          </div>
        </div>
      </div>
      <template #footer
        ><Button :disabled="!detail" @click="detail && edit(detail.source)">{{
          detail?.source.kind === 'local_reference' ? '迁入统一库' : '同步与配置'
        }}</Button
        ><Button
          variant="danger"
          :disabled="!detail"
          @click="detail && confirm(detail.source, 'remove')"
          ><Trash2 />移除配置</Button
        ></template
      >
    </AppSheet>
    <DistributionDialog
      v-model:open="distributionOpen"
      :skill-ids="distribution?.ids ?? []"
      :claim="`source:${distributionId}`"
      :title="`分发本地来源 · ${distribution?.source.name ?? ''}`"
    />
    <AppDialog
      :open="!!confirmation"
      :title="confirmation === 'remove' ? '移除本地来源配置' : '取消来源分发'"
      :description="confirmed?.source.name"
      @update:open="!confirmBusy && !$event && (confirmation = null)"
    >
      <p class="callout">
        仅撤销该来源的分发引用。其他预设和手动引用保留；新建或已接管软链在没有其他引用时删除，仅复用的软链保留。原始文件及
        Skill 库记录不删除。
      </p>
      <div v-if="confirmation === 'revoke'" class="list-stack">
        <label v-for="target in confirmTargets" :key="target.id" class="choice"
          ><input
            v-model="targetIds"
            class="checkbox"
            type="checkbox"
            :value="target.id"
            :disabled="confirmBusy"
          />{{ app.targetName(target) }}</label
        >
      </div>
      <p v-if="confirmError" class="field-error" role="alert">{{ confirmError }}</p>
      <template #footer
        ><Button :disabled="confirmBusy" @click="confirmation = null">取消</Button
        ><Button
          variant="danger"
          :disabled="confirmBusy || (confirmation === 'revoke' && !targetIds.length)"
          @click="execute"
          >{{ confirmBusy ? '处理中…' : '确认执行' }}</Button
        ></template
      >
    </AppDialog>
  </div>
</template>
<style scoped>
.local-source-path {
  color: var(--muted);
  font-size: 12px;
  overflow-wrap: anywhere;
  margin-top: 5px;
}
.local-member-list {
  max-height: 300px;
  overflow-y: auto;
  display: grid;
  gap: 8px;
}
.local-member-list .choice {
  align-items: flex-start;
}
</style>

<style scoped>
.local-source-editor {
  flex: 1;
  min-height: 0;
  overflow: hidden;
}
.local-source-editor > * {
  flex-shrink: 0;
}
.local-source-editor > .local-member-list {
  flex: 1;
  min-height: 0;
  max-height: none;
  overflow: auto;
  overscroll-behavior: contain;
}
</style>
