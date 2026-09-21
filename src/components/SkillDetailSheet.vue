<script setup lang="ts">
import SkillDirectoryActions from '@/components/SkillDirectoryActions.vue'
import AppSelect from '@/components/ui/AppSelect.vue'
import AppPagination from './ui/AppPagination.vue'
import { computed, ref, watch } from 'vue'
import { TabsRoot, TabsList, TabsTrigger, TabsContent } from 'reka-ui'
import MarkdownIt from 'markdown-it'
import DOMPurify from 'dompurify'
import { Copy, Link2, RotateCcw } from 'lucide-vue-next'
import AppSheet from './ui/AppSheet.vue'
import AppDialog from './ui/AppDialog.vue'
import Button from './ui/Button.vue'
import Badge from './ui/Badge.vue'
import { useAppStore } from '@/stores/app'
import { api } from '@/services/api'
import { copyText, formatDate } from '@/lib/utils'
import type { Skill } from '@/services/types'
import { usePagination } from '@/composables/usePagination'
import { skillEntityKey, skillEntityLabel } from '@/services/librarySkills'

const props = defineProps<{ open: boolean; skill: Skill | null }>()
const emit = defineEmits<{
  'update:open': [value: boolean]
  distribute: [skillId: string]
}>()
const app = useAppStore()
const markdown = ref('')
const loadingContent = ref(false)
const copied = ref(false)
const historyOpen = ref(false)
const versions = ref<Skill[]>([])
const rollbackBinding = ref('')
const rollbackDigest = ref('')
const contentPath = computed(() =>
  app.snapshot && props.skill
    ? app.snapshot.skillEntityPaths?.[props.skill.id] ||
      props.skill.externalPath ||
      `${app.snapshot.storageRoot}/objects/${props.skill.bundleDigest}/tree/${props.skill.relativePath}`
    : '',
)
const entityLabel = computed(() =>
  app.snapshot && props.skill ? skillEntityLabel(props.skill, app.snapshot) : '实体位置未确认',
)
const entityRecords = computed(() => {
  if (!app.snapshot || !props.skill) return []
  const snapshot = app.snapshot
  const key = skillEntityKey(props.skill, snapshot)
  return snapshot.skills
    .filter(
      (member) =>
        member.name.toLocaleLowerCase() === props.skill!.name.toLocaleLowerCase() &&
        skillEntityKey(member, snapshot) === key,
    )
    .map((member) => ({
      id: member.id,
      sourceName:
        app.sourceName(snapshot.sources.find((source) => source.id === member.sourceId)) ||
        '独立来源',
      version: member.version || '未记录',
    }))
})
const bindings = computed(
  () => app.snapshot?.bindings.filter((item) => item.skillId === props.skill?.id) ?? [],
)
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
  app.snapshot?.sources.find((item) => item.id === props.skill?.sourceId),
)
const rendered = computed(() =>
  DOMPurify.sanitize(new MarkdownIt({ html: false, linkify: true }).render(markdown.value)),
)

watch(
  () => [props.open, props.skill?.id] as const,
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
  if (!props.skill || !app.snapshot) return
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
  if (!props.skill) return
  try {
    versions.value = await api.skillHistory(props.skill.id)
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
</script>

<template>
  <AppSheet
    :open="open"
    :title="skill?.name || 'Skill 详情'"
    :description="skill?.description"
    @update:open="emit('update:open', $event)"
  >
    <template #toolbar><SkillDirectoryActions :skill="skill" /></template>
    <TabsRoot default-value="overview" class="skill-detail-tabs">
      <TabsList class="tabs-list"
        ><TabsTrigger class="tab-trigger" value="overview">概览</TabsTrigger
        ><TabsTrigger class="tab-trigger" value="content">内容</TabsTrigger
        ><TabsTrigger class="tab-trigger" value="distribution">分发</TabsTrigger
        ><TabsTrigger class="tab-trigger" value="updates">更新</TabsTrigger></TabsList
      >
      <TabsContent class="skill-detail-content" value="overview">
        <div class="actions" style="margin-bottom: 14px">
          <Button variant="primary" @click="skill && emit('distribute', skill.id)"
            ><Link2 />分发</Button
          ><Badge tone="blue">{{ skill?.externalPath ? '跟随本地内容' : '统一库快照' }}</Badge>
        </div>
        <dl class="detail-list">
          <div class="detail-row">
            <dt>当前实体</dt>
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
            <dt>{{ skill?.externalPath ? '本地实体路径' : '中央库路径' }}</dt>
            <dd>
              <span class="mono">{{ contentPath }}</span
              ><button class="link-button" style="margin-left: 8px" @click="copyPath">
                <Copy style="width: 12px; display: inline" />
                {{ copied ? '已复制' : '复制' }}
              </button>
            </dd>
          </div>
          <div class="detail-row">
            <dt>整包快照摘要</dt>
            <dd class="mono">
              {{ skill?.externalPath ? '本地引用，不锁定内容摘要' : skill?.bundleDigest }}
            </dd>
          </div>
          <div class="detail-row">
            <dt>入库时间</dt>
            <dd>{{ formatDate(skill?.installedAt || '') }}</dd>
          </div>
          <div class="detail-row">
            <dt>适用环境</dt>
            <dd>请查阅 SKILL.md 中声明的系统与依赖要求</dd>
          </div>
        </dl>
        <section style="margin-top: 16px" aria-label="当前实体的来源记录">
          <h3 class="field-label">来源记录（{{ entityRecords.length }}）</h3>
          <p class="subtle">来源记录版本用于追溯，当前使用的内容以实体为准。</p>
          <dl class="detail-list">
            <div v-for="record in entityRecords" :key="record.id" class="detail-row">
              <dt class="full-location">{{ record.sourceName }}</dt>
              <dd class="full-location">来源记录版本：{{ record.version }}</dd>
            </div>
          </dl>
        </section>
      </TabsContent>
      <TabsContent class="skill-detail-content" value="content"
        ><div v-if="loadingContent" class="skeleton" style="height: 240px" />
        <div v-else class="markdown" v-html="rendered" />
        <div class="callout" style="margin-top: 16px">
          内容仅以净化后的 Markdown 预览，不执行其中的脚本或指令。
        </div></TabsContent
      >
      <TabsContent class="skill-detail-content" value="distribution"
        ><div v-if="!bindings.length" class="callout">尚未分发到任何目标。</div>
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
              <div class="list-row-meta mono full-location">
                {{ binding.path }}
              </div>
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
      <TabsContent class="skill-detail-content" value="updates"
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
      ></TabsContent>
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
</style>
