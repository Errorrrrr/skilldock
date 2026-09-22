<script setup lang="ts">
import ContentUpdateRecovery from './ContentUpdateRecovery.vue'
import SkillDirectoryActions from '@/components/SkillDirectoryActions.vue'
import AppSelect from '@/components/ui/AppSelect.vue'
import AppPagination from './ui/AppPagination.vue'
import { computed, ref, watch } from 'vue'
import { TabsRoot, TabsList, TabsTrigger, TabsContent } from 'reka-ui'
import MarkdownIt from 'markdown-it'
import DOMPurify from 'dompurify'
import { Copy, Link2, RotateCcw, RefreshCw, Trash2 } from 'lucide-vue-next'
import AppSheet from './ui/AppSheet.vue'
import AppDialog from './ui/AppDialog.vue'
import Button from './ui/Button.vue'
import Badge from './ui/Badge.vue'
import { useAppStore } from '@/stores/app'
import { api } from '@/services/api'
import { copyText, formatDate } from '@/lib/utils'
import type { Skill } from '@/services/types'
import { usePagination } from '@/composables/usePagination'
import {
  libraryDirectory,
  skillDistributionChoices,
  skillEntityKey,
  skillEntityLabel,
} from '@/services/librarySkills'

const props = defineProps<{ open: boolean; skill: Skill | null }>()
const emit = defineEmits<{ 'update:open': [value: boolean]; distribute: [skillId: string] }>()
const app = useAppStore()
const singleContent = computed(() => (app.snapshot?.schemaVersion ?? 0) >= 3)
const markdown = ref('')
const loadingContent = ref(false)
const copied = ref(false)
const historyOpen = ref(false)
const versions = ref<Skill[]>([])
const rollbackBinding = ref('')
const rollbackDigest = ref('')
const activeSkillId = ref('')
watch(
  () => [props.open, props.skill?.id] as const,
  ([open, skillId]) => {
    if (open && skillId) {
      activeSkillId.value = skillId
    }
  },
  { immediate: true },
)

const currentSkill = computed(
  () => app.snapshot?.skills.find((s) => s.id === activeSkillId.value) || props.skill,
)

const allMembers = computed(() => {
  if (!app.snapshot || !currentSkill.value) return []
  return app.snapshot.skills.filter(
    (s) => s.name.toLocaleLowerCase() === currentSkill.value!.name.toLocaleLowerCase(),
  )
})

const entityChoices = computed(() => {
  if (!app.snapshot || !allMembers.value.length) return []
  return skillDistributionChoices(allMembers.value, app.snapshot)
})

const entitySelectOptions = computed(() =>
  entityChoices.value.map((skill) => {
    const src = app.snapshot?.sources.find((s) => s.id === skill.sourceId)
    const srcName = app.sourceName(src) || '独立来源'
    const label = skillEntityLabel(skill, app.snapshot!)
    return {
      value: skill.id,
      label: `${srcName} · ${label}`,
    }
  }),
)

const unreferencedEntities = computed(() => {
  if (!app.snapshot || !currentSkill.value) return []
  return allMembers.value.filter((member) => {
    if (member.id === currentSkill.value!.id) return false
    const isBound = app.snapshot!.bindings.some((b) => b.skillId === member.id)
    if (isBound) return false
    const inPreset = app.snapshot!.presets.some((p) => p.skillIds.includes(member.id))
    if (inPreset) return false
    return true
  })
})

const cleaningUnreferenced = ref(false)

async function cleanUnreferenced() {
  if (!unreferencedEntities.value.length || cleaningUnreferenced.value) return
  cleaningUnreferenced.value = true
  try {
    const targets = [...unreferencedEntities.value]
    for (const target of targets) {
      await api.removeSkill(target.id)
    }
    app.notice = `已清理 ${targets.length} 个未引用的多余副本`
    await app.refresh(true)
  } catch (err) {
    app.error = err instanceof Error ? err.message : '清理未引用副本失败'
  } finally {
    cleaningUnreferenced.value = false
  }
}

const contentPath = computed(() =>
  app.snapshot && currentSkill.value
    ? (singleContent.value
        ? `${libraryDirectory(app.snapshot)}/${app.snapshot.libraryEntries[currentSkill.value.id] || currentSkill.value.name}`
        : '') ||
      app.snapshot.skillEntityPaths?.[currentSkill.value.id] ||
      currentSkill.value.externalPath ||
      `${app.snapshot.storageRoot}/objects/${currentSkill.value.bundleDigest}/tree/${currentSkill.value.relativePath}`
    : '',
)
const entityLabel = computed(() =>
  app.snapshot && currentSkill.value
    ? skillEntityLabel(currentSkill.value, app.snapshot)
    : '实体位置未确认',
)
const currentEntityRecords = computed(() => {
  if (!app.snapshot || !currentSkill.value) return []
  const snapshot = app.snapshot
  const key = skillEntityKey(currentSkill.value, snapshot)
  return snapshot.skills
    .filter(
      (member) =>
        member.name.toLocaleLowerCase() === currentSkill.value!.name.toLocaleLowerCase() &&
        skillEntityKey(member, snapshot) === key,
    )
    .map((member) => ({
      id: member.id,
      sourceName:
        app.sourceName(snapshot.sources.find((source) => source.id === member.sourceId)) ||
        '独立来源',
      version: member.version || '未记录',
      isCurrent: member.id === currentSkill.value?.id,
    }))
})

const origins = computed(() => {
  const snapshot = app.snapshot
  const skill = currentSkill.value
  if (!snapshot || !skill) return []
  return (snapshot.skillOrigins[skill.id] || [skill.sourceId]).map((id) => ({
    id,
    source: snapshot.sources.find((s) => s.id === id),
    current: id === skill.sourceId,
  }))
})

const otherEntityRecords = computed(() => {
  if (!app.snapshot || !currentSkill.value) return []
  const snapshot = app.snapshot
  const key = skillEntityKey(currentSkill.value, snapshot)
  return snapshot.skills
    .filter(
      (member) =>
        member.name.toLocaleLowerCase() === currentSkill.value!.name.toLocaleLowerCase() &&
        skillEntityKey(member, snapshot) !== key,
    )
    .map((member) => ({
      id: member.id,
      sourceName:
        app.sourceName(snapshot.sources.find((source) => source.id === member.sourceId)) ||
        '独立来源',
      version: member.version || '未记录',
      entityLabel: skillEntityLabel(member, snapshot),
    }))
})

const bindings = computed(
  () => app.snapshot?.bindings.filter((item) => item.skillId === currentSkill.value?.id) ?? [],
)

const otherEntityBindings = computed(() => {
  if (!app.snapshot || !currentSkill.value) return []
  const currentKey = skillEntityKey(currentSkill.value, app.snapshot)
  const otherSkills = allMembers.value.filter(
    (s) => skillEntityKey(s, app.snapshot!) !== currentKey,
  )
  const otherSkillIds = new Set(otherSkills.map((s) => s.id))
  return app.snapshot.bindings
    .filter((b) => otherSkillIds.has(b.skillId))
    .map((b) => {
      const boundSkill = otherSkills.find((s) => s.id === b.skillId)!
      const target = app.snapshot?.targets.find((t) => t.id === b.targetId)
      return {
        ...b,
        targetName: app.targetName(target),
        entityLabel: skillEntityLabel(boundSkill, app.snapshot!),
        sourceName:
          app.sourceName(app.snapshot?.sources.find((s) => s.id === boundSkill.sourceId)) ||
          '独立来源',
      }
    })
})

const {
  page: distPage,
  pageSize: distPageSize,
  pagedItems: pagedDistBindings,
  resetPage: resetDistPage,
} = usePagination(bindings, { initialPageSize: 10 })
const {
  page: updatesPage,
  pageSize: updatesPageSize,
  pagedItems: pagedUpdatesBindings,
  resetPage: resetUpdatesPage,
} = usePagination(bindings, { initialPageSize: 10 })
const source = computed(() =>
  app.snapshot?.sources.find((item) => item.id === currentSkill.value?.sourceId),
)
const rendered = computed(() =>
  DOMPurify.sanitize(new MarkdownIt({ html: false, linkify: true }).render(markdown.value)),
)

watch(
  () => [props.open, currentSkill.value?.id] as const,
  async ([open, skillId]) => {
    if (!open || !skillId) return
    resetDistPage()
    resetUpdatesPage()
    loadingContent.value = true
    try {
      markdown.value = await api.readSkill(skillId)
    } catch (error) {
      markdown.value = `> 无法读取内容：${error instanceof Error ? error.message : '未知错误'}`
    } finally {
      loadingContent.value = false
    }
  },
)
async function copyPath() {
  if (!currentSkill.value || !app.snapshot) return
  await copyText(contentPath.value)
  copied.value = true
  setTimeout(() => (copied.value = false), 1600)
}
async function toggleFollow(bindingId: string, follow: boolean) {
  await app.mutate(
    () => api.setFollow(bindingId, follow),
    follow ? '已设为跟随来源更新' : '已固定当前分发版本',
  )
}
async function rollback(bindingId: string) {
  if (!currentSkill.value) return
  try {
    versions.value = await api.skillHistory(currentSkill.value.id)
    rollbackBinding.value = bindingId
    rollbackDigest.value = versions.value[0]?.bundleDigest || ''
    historyOpen.value = true
  } catch (e) {
    app.error = String(e)
  }
}
async function confirmRollback() {
  if (!rollbackDigest.value) return
  const ok = await app.mutate(
    () => api.rollback(rollbackBinding.value, rollbackDigest.value),
    '分发版本已回滚，跟随更新已暂停',
  )
  if (ok) historyOpen.value = false
}

const rollbackSkill = computed(() =>
  versions.value.find((version) => version.bundleDigest === rollbackDigest.value),
)
const rollbackDigestOptions = computed(() =>
  versions.value.map((version) => ({
    value: version.bundleDigest,
    label: `快照 ${version.bundleDigest.slice(0, 12)} · 来源记录版本：${version.version}`,
  })),
)

const detailRepairOpen = ref(false)
const detailRepairing = ref(false)
const detailRepairError = ref('')
const detailRepairPreview = ref<{
  revision: number
  contentDigest: string
  skillCount: number
  path: string
} | null>(null)

async function openDetailRepair() {
  if (!currentSkill.value || currentSkill.value.externalPath || detailRepairing.value) return
  detailRepairError.value = ''
  detailRepairing.value = true
  try {
    detailRepairPreview.value = await api.previewSnapshotRefresh(currentSkill.value.id)
    detailRepairOpen.value = true
  } catch (err) {
    app.error = err instanceof Error ? err.message : '无法读取当前快照状态'
  } finally {
    detailRepairing.value = false
  }
}

async function confirmDetailRepair() {
  if (!currentSkill.value || !detailRepairPreview.value || detailRepairing.value) return
  detailRepairing.value = true
  detailRepairError.value = ''
  const preview = detailRepairPreview.value
  const skillName = currentSkill.value.name
  try {
    const ok = await app.mutate(
      () => api.refreshSnapshot(currentSkill.value!.id, preview.revision, preview.contentDigest),
      `已重新收录「${skillName}」当前快照`,
    )
    if (ok) {
      detailRepairOpen.value = false
      detailRepairPreview.value = null
      await app.refresh(true)
    } else {
      detailRepairError.value = app.error || '重新收录失败'
    }
  } catch (err) {
    detailRepairError.value = err instanceof Error ? err.message : '重新收录失败'
  } finally {
    detailRepairing.value = false
  }
}
</script>

<template>
  <AppSheet
    :open="open"
    :title="currentSkill?.name || 'Skill 详情'"
    :description="currentSkill?.description"
    @update:open="emit('update:open', $event)"
  >
    <template #toolbar><SkillDirectoryActions :skill="currentSkill" /></template>
    <TabsRoot default-value="overview" class="skill-detail-tabs">
      <TabsList class="tabs-list"
        ><TabsTrigger class="tab-trigger" value="overview">概览</TabsTrigger
        ><TabsTrigger class="tab-trigger" value="content">内容</TabsTrigger
        ><TabsTrigger class="tab-trigger" value="distribution">分发</TabsTrigger
        ><TabsTrigger class="tab-trigger" value="updates">更新</TabsTrigger></TabsList
      >
      <TabsContent class="skill-detail-content" value="overview">
        <div v-if="entityChoices.length > 1" class="callout warning" style="margin-bottom: 14px">
          <div style="font-weight: 600; margin-bottom: 4px">
            存在 {{ entityChoices.length }} 个不同的实体版本
          </div>
          <p class="subtle" style="font-size: 13px; margin-bottom: 8px">
            由于来自不同的历史归集目录或来源，系统保存了多个版本。您可以在此切换查看各版本的详情：
          </p>
          <div style="display: flex; gap: 8px; align-items: center; flex-wrap: wrap">
            <AppSelect
              v-model="activeSkillId"
              :options="entitySelectOptions"
              aria-label="切换查看实体版本"
              style="flex: 1; min-width: 180px"
            />
            <Button
              v-if="unreferencedEntities.length > 0"
              size="sm"
              variant="secondary"
              :disabled="cleaningUnreferenced"
              title="清理未被任何工具或预设引用的多余历史副本"
              @click="cleanUnreferenced"
            >
              <Trash2 style="width: 12px; height: 12px; margin-right: 4px" />
              {{
                cleaningUnreferenced
                  ? '正在清理…'
                  : `清理未引用的多余副本 (${unreferencedEntities.length})`
              }}
            </Button>
          </div>
        </div>

        <div class="actions" style="margin-bottom: 14px">
          <Button variant="primary" @click="currentSkill && emit('distribute', currentSkill.id)"
            ><Link2 />{{ singleContent ? '分发' : '分发此版本' }}</Button
          ><Badge tone="blue">{{
            currentSkill?.externalPath ? '跟随本地内容' : singleContent ? '当前内容' : '统一库快照'
          }}</Badge>
        </div>
        <dl class="detail-list">
          <div class="detail-row">
            <dt>{{ singleContent ? '当前内容' : '当前实体' }}</dt>
            <dd class="full-location">{{ entityLabel }}</dd>
          </div>
          <div class="detail-row">
            <dt>当前记录来源</dt>
            <dd class="full-location">
              {{ app.sourceName(source) || '独立来源' }}
              <div v-if="source?.url" class="mono">{{ source.url }}</div>
              <div v-if="source?.path && source.path !== source.url" class="mono">
                {{ source.path }}
              </div>
            </dd>
          </div>
          <div class="detail-row">
            <dt>{{ currentSkill?.externalPath ? '本地实体路径' : '中央库路径' }}</dt>
            <dd>
              <span class="mono">{{ contentPath }}</span
              ><button class="link-button" style="margin-left: 8px" @click="copyPath">
                <Copy style="width: 12px; display: inline" /> {{ copied ? '已复制' : '复制' }}
              </button>
            </dd>
          </div>
          <div class="detail-row">
            <dt>整包快照摘要</dt>
            <dd>
              <span class="mono">{{
                currentSkill?.externalPath ? '本地引用，不锁定内容摘要' : currentSkill?.bundleDigest
              }}</span>
              <button
                v-if="!currentSkill?.externalPath"
                class="link-button"
                style="margin-left: 8px"
                :disabled="detailRepairing"
                title="当本地文件发生变动或快照校验失败时，重新收录当前内容"
                @click="openDetailRepair"
              >
                <RefreshCw
                  :class="{ spin: detailRepairing }"
                  style="width: 12px; display: inline; vertical-align: -1px; margin-right: 2px"
                />
                {{ detailRepairing ? '检查中…' : '重新收录快照' }}
              </button>
            </dd>
          </div>
          <div class="detail-row">
            <dt>入库时间</dt>
            <dd>{{ formatDate(currentSkill?.installedAt || '') }}</dd>
          </div>
          <div class="detail-row">
            <dt>适用环境</dt>
            <dd>请查阅 SKILL.md 中声明的系统与依赖要求</dd>
          </div>
        </dl>
        <section v-if="singleContent" style="margin-top: 16px" aria-label="收录来源">
          <h3 class="field-label">收录来源</h3>
          <p class="subtle">来源记录不额外保存内容。更新以当前来源为准。</p>
          <div v-for="origin in origins" :key="origin.id" class="detail-row">
            <span class="full-location"
              >{{ app.sourceName(origin.source) || '已移除的来源' }}<br />{{
                origin.source?.url || origin.source?.path
              }}</span
            >
            <Badge v-if="origin.current" tone="blue">当前来源</Badge>
          </div>
        </section>
        <section v-else style="margin-top: 16px" aria-label="来源记录">
          <h3 class="field-label">
            来源记录（当前实体 {{ currentEntityRecords.length }} 项<template
              v-if="otherEntityRecords.length"
              >，全库共 {{ allMembers.length }} 项</template
            >）
          </h3>
          <p class="subtle">来源记录用于追踪归集与上游历史，当前使用的内容以实体为准。</p>
          <dl class="detail-list">
            <div v-for="record in currentEntityRecords" :key="record.id" class="detail-row">
              <dt class="full-location">
                {{ record.sourceName }}
                <Badge v-if="record.isCurrent" tone="blue" style="margin-left: 6px">当前查看</Badge>
              </dt>
              <dd class="full-location">来源记录版本：{{ record.version }}</dd>
            </div>
          </dl>

          <div v-if="otherEntityRecords.length" style="margin-top: 14px">
            <div
              class="field-label"
              style="font-size: 12px; margin-bottom: 6px; color: var(--muted)"
            >
              其他实体版本的历史来源：
            </div>
            <dl class="detail-list">
              <div
                v-for="record in otherEntityRecords"
                :key="record.id"
                class="detail-row"
                style="opacity: 0.85"
              >
                <dt class="full-location">
                  {{ record.sourceName }}
                  <span class="mono subtle" style="margin-left: 6px; font-size: 11px"
                    >({{ record.entityLabel }})</span
                  >
                </dt>
                <dd class="full-location">
                  来源记录版本：{{ record.version }}
                  <button
                    class="link-button"
                    style="margin-left: 8px"
                    @click="activeSkillId = record.id"
                  >
                    切换查看
                  </button>
                </dd>
              </div>
            </dl>
          </div>
        </section>
      </TabsContent>
      <TabsContent class="skill-detail-content" value="content"
        ><div v-if="loadingContent" class="skeleton" style="height: 240px" />
        <div v-else class="markdown" v-html="rendered" />
        <div class="callout" style="margin-top: 16px">
          内容仅以净化后的 Markdown 预览，不执行其中的脚本或指令。
        </div></TabsContent
      >
      <TabsContent class="skill-detail-content" value="distribution">
        <div v-if="otherEntityBindings.length" class="callout warning" style="margin-bottom: 14px">
          <div style="font-weight: 600; margin-bottom: 4px">
            其他工具当前绑定了该 Skill 的不同实体版本：
          </div>
          <div
            v-for="ob in otherEntityBindings"
            :key="ob.id"
            class="subtle"
            style="font-size: 12px; margin-top: 2px"
          >
            • <strong>{{ ob.targetName }}</strong
            >：当前使用「{{ ob.sourceName }}」· {{ ob.entityLabel }}
          </div>
          <div style="margin-top: 8px">
            <Button
              size="sm"
              variant="primary"
              @click="currentSkill && emit('distribute', currentSkill.id)"
            >
              <Link2 />统一将所有工具切换为当前版本
            </Button>
          </div>
        </div>

        <div v-if="!bindings.length" class="callout">当前实体版本尚未分发到任何目标。</div>
        <div v-else class="list-stack">
          <div v-for="binding in pagedDistBindings" :key="binding.id" class="list-row">
            <div class="list-row-main">
              <div class="list-row-title">
                {{
                  app.targetName(
                    app.snapshot?.targets.find((target) => target.id === binding.targetId),
                  )
                }}
              </div>
              <div class="list-row-meta mono full-location">{{ binding.path }}</div>
              <SkillDirectoryActions :binding="binding" />
              <div class="actions" style="margin-top: 6px">
                <Badge
                  v-for="claim in binding.claims"
                  :key="claim"
                  :tone="claim === 'manual' ? 'neutral' : 'blue'"
                  >{{
                    claim === 'manual'
                      ? '手动添加'
                      : app.snapshot?.presets.find((p) => `preset:${p.id}` === claim)?.name || claim
                  }}</Badge
                >
              </div>
            </div>
            <Badge tone="neutral">分发已登记</Badge>
          </div>
          <AppPagination
            v-if="bindings.length > 10"
            v-model:page="distPage"
            v-model:page-size="distPageSize"
            :total="bindings.length"
            :page-sizes="[5, 10, 20]"
            compact
            :show-size-changer="false"
            style="margin-top: 12px"
          /></div
      ></TabsContent>
      <TabsContent class="skill-detail-content" value="updates">
        <ContentUpdateRecovery
          v-if="singleContent && currentSkill"
          :source-id="currentSkill.sourceId"
          :skill-id="currentSkill.id"
        />
        <template v-else
          ><div v-if="!bindings.length" class="callout">分发后可对每个目标设置跟随策略。</div>
          <div v-else class="list-stack">
            <div v-for="binding in pagedUpdatesBindings" :key="binding.id" class="list-row">
              <div class="list-row-main">
                <div class="list-row-title">
                  {{
                    app.targetName(
                      app.snapshot?.targets.find((target) => target.id === binding.targetId),
                    )
                  }}
                </div>
                <div class="list-row-meta">
                  {{
                    binding.externalPath
                      ? '跟随本地内容'
                      : binding.follow
                        ? '跟随来源更新'
                        : '固定版本'
                  }}
                  · 分发记录版本：{{ binding.version }}
                </div>
                <SkillDirectoryActions :binding="binding" />
              </div>
              <Button
                size="sm"
                :disabled="!!binding.externalPath || binding.claims.some((c) => c !== 'manual')"
                @click="toggleFollow(binding.id, !binding.follow)"
                >{{ binding.follow ? '固定版本' : '恢复跟随' }}</Button
              ><Button
                size="icon"
                variant="ghost"
                :disabled="!!binding.externalPath || binding.claims.some((c) => c !== 'manual')"
                title="选择历史版本"
                aria-label="回滚到上一个版本"
                @click="rollback(binding.id)"
                ><RotateCcw
              /></Button>
            </div>
            <AppPagination
              v-if="bindings.length > 10"
              v-model:page="updatesPage"
              v-model:page-size="updatesPageSize"
              :total="bindings.length"
              :page-sizes="[5, 10, 20]"
              compact
              :show-size-changer="false"
              style="margin-top: 12px"
            /></div
        ></template>
      </TabsContent>
    </TabsRoot>
  </AppSheet>
  <AppDialog
    v-model:open="historyOpen"
    title="回滚分发版本"
    description="选择保留的历史快照。回滚后会暂停该目标的跟随更新。"
    ><label class="field"
      ><span class="field-label">历史版本</span
      ><AppSelect v-model="rollbackDigest" aria-label="历史版本" :options="rollbackDigestOptions"
    /></label>
    <SkillDirectoryActions :skill="rollbackSkill" />
    <template #footer
      ><Button @click="historyOpen = false">取消</Button
      ><Button variant="primary" @click="confirmRollback">回滚到此版本</Button></template
    ></AppDialog
  >
  <AppDialog
    v-model:open="detailRepairOpen"
    title="重新收录当前内容"
    :description="`此来源包包含 ${detailRepairPreview?.skillCount || 0} 个 Skill，将以统一库当前文件生成新快照。`"
  >
    <p>
      所有受管目标将同步到重新收录的当前内容。 若本地 Skill
      文件已被编辑修改或受系统文件变动影响导致快照不匹配，重新收录后即可正常分发最新内容。
    </p>
    <p v-if="detailRepairError" class="field-error" role="alert">{{ detailRepairError }}</p>
    <template #footer>
      <Button :disabled="detailRepairing" @click="detailRepairOpen = false">取消</Button>
      <Button variant="primary" :disabled="detailRepairing" @click="confirmDetailRepair">
        <RefreshCw
          :class="{ spin: detailRepairing }"
          aria-hidden="true"
          style="margin-right: 4px"
        />
        {{ detailRepairing ? '正在重新收录…' : '确认重新收录' }}
      </Button>
    </template>
  </AppDialog>
</template>

<style scoped>
.full-location {
  white-space: normal;
  overflow-wrap: anywhere;
  word-break: normal;
  overflow: visible;
  text-overflow: clip;
}
.full-location .mono {
  margin-top: 5px;
  font-size: 12px;
  color: var(--muted);
}
.spin {
  animation: spin 1s linear infinite;
}
@keyframes spin {
  from {
    transform: rotate(0deg);
  }
  to {
    transform: rotate(360deg);
  }
}
</style>
