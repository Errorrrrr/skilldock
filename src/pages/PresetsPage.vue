<script setup lang="ts">
import { distributionActionLabel } from '@/services/distributionLabels'
import SkillDirectoryActions from '@/components/SkillDirectoryActions.vue'
import { computed, ref } from 'vue'
import {
  Layers3,
  Plus,
  Link2,
  Unlink,
  Pencil,
  Download,
  Upload,
  Trash2,
  PackageOpen,
} from 'lucide-vue-next'
import Button from '@/components/ui/Button.vue'
import Badge from '@/components/ui/Badge.vue'
import EmptyState from '@/components/ui/EmptyState.vue'
import AppSheet from '@/components/ui/AppSheet.vue'
import AppDialog from '@/components/ui/AppDialog.vue'
import ConfirmDialog from '@/components/ui/ConfirmDialog.vue'
import PresetEditorDialog from '@/components/PresetEditorDialog.vue'
import AppPagination from '@/components/ui/AppPagination.vue'
import { api } from '@/services/api'
import { useAppStore } from '@/stores/app'
import type { DistributionPlan, Preset } from '@/services/types'
import { usePagination } from '@/composables/usePagination'

const app = useAppStore()
const editorOpen = ref(false)
const editing = ref<Preset | null>(null)
const detail = ref<Preset | null>(null)
const detailOpen = ref(false)
const applyOpen = ref(false)
const revokeOpen = ref(false)
const deleteOpen = ref(false)
const targetIds = ref<string[]>([])
const plan = ref<DistributionPlan | null>(null)
const busy = ref(false)
const takeover = ref(false)
const applications = computed(() =>
  (app.snapshot?.presetApplications ?? []).filter((a) => a.presetId === currentPreset.value?.id),
)
async function toggleFollow(targetId: string, follow: boolean) {
  if (!currentPreset.value) return
  await app.mutate(
    () => api.setPresetFollow(currentPreset.value!.id, targetId, follow),
    follow ? '已启用跟随更新' : '已固定当前安装版本',
  )
}
async function retrySync() {
  await app.mutate(() => api.retryPresetSync(), '已重试预设同步，请查看目标状态')
}

const currentPreset = computed(
  () => app.snapshot?.presets.find((p) => p.id === detail.value?.id) || detail.value,
)
const appliedTargetIds = computed(() => {
  const claim = `preset:${currentPreset.value?.id}`
  return [
    ...new Set(
      app.snapshot?.bindings.filter((b) => b.claims.includes(claim)).map((b) => b.targetId) ?? [],
    ),
  ]
})
const appliedCount = (preset: Preset) =>
  new Set(
    app.snapshot?.bindings
      .filter((b) => b.claims.includes(`preset:${preset.id}`))
      .map((b) => b.targetId),
  ).size
const allPresets = computed(() => app.snapshot?.presets ?? [])
const {
  page: presetsPage,
  pageSize: presetsPageSize,
  pagedItems: pagedPresets,
} = usePagination(allPresets, { initialPageSize: 10 })

const presetSkillIds = computed(() => currentPreset.value?.skillIds ?? [])
const presetMemberVersions = computed(() =>
  Object.fromEntries(
    presetSkillIds.value.map((id) => {
      const member =
        currentPreset.value?.locks[id] || app.snapshot?.skills.find((skill) => skill.id === id)
      return [
        id,
        member?.externalPath
          ? '跟随本地内容'
          : (app.snapshot?.schemaVersion ?? 0) >= 3
            ? '当前内容'
            : `v${member?.version || '未知'}`,
      ]
    }),
  ),
)
const {
  page: presetSkillsPage,
  pageSize: presetSkillsSize,
  pagedItems: pagedPresetSkillIds,
  resetPage: resetPresetSkillsPage,
} = usePagination(presetSkillIds, { initialPageSize: 10 })

function create() {
  editing.value = null
  editorOpen.value = true
}
function edit(preset: Preset) {
  editing.value = preset
  editorOpen.value = true
}
function show(preset: Preset) {
  detail.value = preset
  resetPresetSkillsPage()
  detailOpen.value = true
}
async function previewApply() {
  if (!currentPreset.value || !targetIds.value.length) return
  busy.value = true
  try {
    plan.value = await api.plan(
      currentPreset.value.skillIds,
      targetIds.value,
      `preset:${currentPreset.value.id}`,
    )
  } catch (e) {
    app.error = e instanceof Error ? e.message : '无法生成计划'
  } finally {
    busy.value = false
  }
}
async function apply() {
  if (!currentPreset.value || !plan.value) return
  if (plan.value.items.some((i) => i.action === 'takeover') && !takeover.value) {
    app.error = '请确认接管已有外部链接'
    return
  }
  busy.value = true
  const ok = await app.mutate(
    () =>
      api.applyPreset(
        currentPreset.value!.id,
        targetIds.value,
        plan.value!.revision,
        takeover.value,
      ),
    `预设「${currentPreset.value.name}」已应用到 ${targetIds.value.length} 个目标`,
  )
  busy.value = false
  if (ok) {
    applyOpen.value = false
    plan.value = null
  }
}
async function revoke() {
  if (!currentPreset.value) return
  busy.value = true
  const ok = await app.mutate(
    () => api.revokePreset(currentPreset.value!.id, targetIds.value),
    `已取消预设在 ${targetIds.value.length} 个目标上的分发`,
  )
  busy.value = false
  if (ok) revokeOpen.value = false
}
async function remove() {
  if (!currentPreset.value) return
  const ok = await app.mutate(() => api.deletePreset(currentPreset.value!.id), '预设定义已删除')
  if (ok) {
    deleteOpen.value = false
    detailOpen.value = false
  }
}
const importingPreset = ref(false)

async function importPreset() {
  if (importingPreset.value) return
  importingPreset.value = true
  try {
    const path = await api.pickFile()
    if (!path) return
    await app.mutate(() => api.importPreset(path), '预设已导入')
  } finally {
    importingPreset.value = false
  }
}
async function exportPreset() {
  if (!currentPreset.value) return
  const path = await api.saveFile(`${currentPreset.value.name}.skilldock.zip`)
  if (!path) return
  try {
    await api.exportPreset(currentPreset.value.id, path)
    app.notice = `预设已导出到 ${path}`
  } catch (e) {
    app.error = e instanceof Error ? e.message : '导出失败'
  }
}
function openApply() {
  takeover.value = false
  targetIds.value = []
  plan.value = null
  applyOpen.value = true
}
function openRevoke() {
  targetIds.value = [...appliedTargetIds.value]
  revokeOpen.value = true
}
function revokeFromRow(preset: Preset) {
  detail.value = preset
  openRevoke()
}
function applyFromCard(preset: Preset) {
  detail.value = preset
  openApply()
}
</script>

<template>
  <div class="page">
    <header class="page-heading">
      <div>
        <h1 class="page-title">预设</h1>
        <p class="page-subtitle">组合常用 Skill，一次分发到多个工具。</p>
      </div>
      <div class="actions">
        <Button :disabled="importingPreset" :loading="importingPreset" @click="importPreset"
          ><Upload v-if="!importingPreset" />{{
            importingPreset ? '正在导入…' : '导入预设'
          }}</Button
        ><Button variant="primary" @click="create"><Plus />新建预设</Button>
      </div>
    </header>
    <EmptyState
      v-if="!app.snapshot?.presets.length"
      title="把常用 Skill 组成一套预设"
      description="从文件夹导入，或自由选择成员，再整体分发到你的工具。"
      ><Button variant="primary" @click="create"><Plus />新建预设</Button></EmptyState
    >
    <section v-else class="preset-list" aria-label="预设列表">
      <div class="preset-data-scroll">
        <article v-for="preset in pagedPresets" :key="preset.id" class="preset-row">
          <div class="item-icon"><Layers3 /></div>
          <div class="preset-main">
            <h3>
              <button class="item-name-button" @click="show(preset)">{{ preset.name }}</button>
            </h3>
            <p v-if="preset.description">{{ preset.description }}</p>
            <div class="preset-meta">
              <span>{{ preset.skillIds.length }} 个 Skill</span
              ><span>{{
                appliedCount(preset) ? `已分发到 ${appliedCount(preset)} 个目标` : '尚未分发'
              }}</span>
            </div>
          </div>
          <div class="preset-actions">
            <Button
              v-if="appliedCount(preset)"
              size="sm"
              variant="ghost"
              @click="revokeFromRow(preset)"
              ><Unlink />取消分发</Button
            ><Button size="sm" variant="ghost" @click="show(preset)">详情</Button
            ><Button size="sm" @click="applyFromCard(preset)"><Link2 />整体分发</Button>
          </div>
        </article>
      </div>
      <AppPagination
        v-if="allPresets.length"
        v-model:page="presetsPage"
        v-model:page-size="presetsPageSize"
        :total="allPresets.length"
        style="
          grid-column: 1 / -1;
          margin-top: 8px;
          border-radius: 8px;
          border: 1px solid var(--line);
        "
      />
    </section>
    <PresetEditorDialog v-model:open="editorOpen" :preset="editing" />
    <AppSheet
      v-model:open="detailOpen"
      :title="currentPreset?.name || '预设详情'"
      :description="currentPreset?.description"
      ><template #toolbar
        ><div class="actions">
          <Button variant="primary" @click="openApply"><Link2 />整体分发</Button
          ><Button :disabled="!appliedTargetIds.length" @click="openRevoke"
            ><Unlink />取消分发</Button
          ><Button @click="currentPreset && edit(currentPreset)"><Pencil />编辑</Button>
        </div></template
      >
      <dl class="detail-list">
        <div class="detail-row">
          <dt>保存修订</dt>
          <dd>r{{ currentPreset?.revision }}</dd>
        </div>
        <div class="detail-row">
          <dt>成员数量</dt>
          <dd>{{ currentPreset?.skillIds.length }} 个</dd>
        </div>
        <div class="detail-row">
          <dt>应用目标</dt>
          <dd>{{ appliedTargetIds.length }} 个；预设成员更新后同步所有受管目标。</dd>
        </div>
      </dl>
      <div
        v-if="
          currentPreset?.skillIds.some(
            (id) =>
              currentPreset?.locks[id]?.externalPath ||
              app.snapshot?.skills.find((skill) => skill.id === id)?.externalPath,
          )
        "
        class="callout"
        style="margin-top: 12px"
      >
        包含本地引用：跟随原目录内容，保存修订不会锁定这些成员的文件；不支持导出为便携快照包。
      </div>
      <section v-if="applications.length" class="list-stack" style="margin-top: 16px">
        <h3 class="panel-title">应用与同步</h3>
        <div v-for="application in applications" :key="application.targetId" class="choice">
          <div class="choice-main">
            <strong>{{
              app.snapshot?.targets.find((t) => t.id === application.targetId)?.name
            }}</strong>
            <div class="choice-meta">
              已应用 r{{ application.appliedRevision }} ·
              {{
                (app.snapshot?.schemaVersion ?? 0) >= 3
                  ? '使用当前内容'
                  : application.follow
                    ? '跟随包更新'
                    : '固定版本'
              }}
            </div>
            <p v-if="application.error" class="field-error">{{ application.error }}</p>
          </div>
          <Button
            v-if="(app.snapshot?.schemaVersion ?? 0) < 3"
            size="sm"
            @click="toggleFollow(application.targetId, !application.follow)"
            >{{ application.follow ? '固定版本' : '跟随更新' }}</Button
          >
        </div>
        <Button size="sm" @click="retrySync">同步 / 重试目标</Button>
      </section>
      <h3 class="panel-title" style="margin: 20px 0 10px">成员</h3>
      <div class="list-stack">
        <div v-for="skillId in pagedPresetSkillIds" :key="skillId" class="list-row">
          <div class="item-icon"><PackageOpen /></div>
          <div class="list-row-main">
            <div class="list-row-title">
              {{
                currentPreset?.locks?.[skillId]?.name ||
                app.snapshot?.skills.find((s) => s.id === skillId)?.name ||
                skillId
              }}
            </div>
            <div class="list-row-meta">
              {{ presetMemberVersions[skillId] }}
            </div>
            <SkillDirectoryActions :skill-id="skillId" :preset-id="currentPreset?.id" />
          </div>
        </div>
      </div>
      <AppPagination
        v-if="presetSkillIds.length > 10"
        v-model:page="presetSkillsPage"
        v-model:page-size="presetSkillsSize"
        :total="presetSkillIds.length"
        compact
        :show-size-changer="false"
      />
      <template #footer
        ><div class="actions">
          <Button @click="exportPreset"><Download />导出</Button
          ><Button variant="danger" @click="deleteOpen = true"><Trash2 />删除预设</Button>
        </div></template
      ></AppSheet
    >
    <AppDialog
      v-model:open="applyOpen"
      title="整体分发预设"
      :description="`选择目标并预览 ${currentPreset?.skillIds.length || 0} 个成员的链接计划。`"
      large
      ><div v-if="!plan" class="choice-list">
        <label v-for="target in app.snapshot?.targets" :key="target.id" class="choice"
          ><input v-model="targetIds" class="checkbox" type="checkbox" :value="target.id" />
          <div class="choice-main">
            <div class="choice-title">{{ app.targetName(target) }}</div>
            <div class="choice-meta mono">{{ target.path }}</div>
          </div></label
        >
      </div>
      <div v-else class="list-stack">
        <div v-for="item in plan.items" :key="`${item.skillId}-${item.targetId}`" class="list-row">
          <div class="list-row-main">
            <div class="list-row-title">
              {{ app.snapshot?.skills.find((s) => s.id === item.skillId)?.name }} →
              {{ app.targetName(app.snapshot?.targets.find((t) => t.id === item.targetId)) }}
            </div>
            <div class="list-row-meta mono">{{ item.path }}</div>
            <SkillDirectoryActions :skill-id="item.skillId" :preset-id="currentPreset?.id" />
          </div>
          <Badge :tone="item.error ? 'red' : item.action === 'keep' ? 'neutral' : 'blue'">{{
            item.error || distributionActionLabel(item.action)
          }}</Badge>
        </div>
        <div class="callout">
          执行会为每条关系添加此预设的分发记录，并保留手动或其他预设的记录。
        </div>
      </div>
      <label v-if="plan?.items.some((i) => i.action === 'takeover')" class="choice"
        ><input v-model="takeover" class="checkbox" type="checkbox" /><span
          >接管已有外部链接，切换到统一库版本；取消全部引用时恢复原链接。</span
        ></label
      >
      <template #footer
        ><Button v-if="plan" @click="plan = null">返回</Button
        ><Button v-else @click="applyOpen = false">取消</Button
        ><Button
          variant="primary"
          :disabled="busy || !targetIds.length || !!plan?.items.some((i) => i.error)"
          @click="plan ? apply() : previewApply()"
          >{{ busy ? '处理中…' : plan ? '确认分发' : '预览计划' }}</Button
        ></template
      ></AppDialog
    >
    <ConfirmDialog
      v-model:open="revokeOpen"
      title="取消预设分发"
      :description="`将取消此预设在所选 ${targetIds.length} 个目标上的分发。被手动添加或其他预设使用的 Skill 将保留。`"
      confirm-text="确认取消"
      :busy="busy"
      @confirm="revoke"
      ><div class="choice-list">
        <label v-for="targetId in appliedTargetIds" :key="targetId" class="choice"
          ><input v-model="targetIds" class="checkbox" type="checkbox" :value="targetId" />
          <div class="choice-main">
            <div class="choice-title">
              {{ app.targetName(app.snapshot?.targets.find((t) => t.id === targetId)) }}
            </div>
          </div></label
        >
      </div></ConfirmDialog
    >
    <ConfirmDialog
      v-model:open="deleteOpen"
      title="删除预设定义"
      description="只删除预设定义，不删除中央库内容。仍应用在目标上的预设必须先取消分发。"
      confirm-text="删除预设"
      danger
      @confirm="remove"
    />
  </div>
</template>
