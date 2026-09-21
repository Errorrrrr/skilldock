<script setup lang="ts">
import { distributionActionLabel } from '@/services/distributionLabels'
import SkillDirectoryActions from '@/components/SkillDirectoryActions.vue'
import { computed, ref, watch } from 'vue'
import { Link2, AlertTriangle, ArrowRight, FolderSymlink } from 'lucide-vue-next'
import AppDialog from './ui/AppDialog.vue'
import Button from './ui/Button.vue'
import Badge from './ui/Badge.vue'
import AppSelect from './ui/AppSelect.vue'
import { api } from '@/services/api'
import { useAppStore } from '@/stores/app'
import type { DistributionPlan } from '@/services/types'
import { skillDistributionChoices, skillEntityLabel } from '@/services/librarySkills'

const props = withDefaults(
  defineProps<{
    open: boolean
    skillIds: string[]
    claim?: string
    title?: string
    initialTargetIds?: string[]
  }>(),
  { claim: 'manual', title: '分发 Skill', initialTargetIds: () => [] },
)
const emit = defineEmits<{ 'update:open': [value: boolean]; completed: [] }>()
const app = useAppStore()
const targetIds = ref<string[]>([])
const selectedIds = ref<string[]>([])
const replaceBindingIds = ref<string[]>([])
const plan = ref<DistributionPlan | null>(null)
const busy = ref(false)
const takeover = ref(false)
const adoptExisting = ref(false)
const adoptableCount = computed(
  () =>
    plan.value?.items.filter((item) => item.action === 'borrow' || item.action === 'adopt')
      .length ?? 0,
)
const localError = ref('')
const selectedSkills = computed(
  () => app.snapshot?.skills.filter((skill) => selectedIds.value.includes(skill.id)) ?? [],
)

const entityLabels = computed(() =>
  Object.fromEntries(
    selectedSkills.value.map((skill) => [
      skill.id,
      app.snapshot ? skillEntityLabel(skill, app.snapshot) : '实体位置未确认',
    ]),
  ),
)

const sourceChoices = computed(() =>
  Object.fromEntries(
    selectedSkills.value.map((skill) => [
      skill.id,
      (app.snapshot
        ? skillDistributionChoices(
            [
              skill,
              ...app.snapshot.skills.filter(
                (other) => other.name.toLocaleLowerCase() === skill.name.toLocaleLowerCase(),
              ),
            ],
            app.snapshot,
          )
        : []
      ).map((other) => ({
        value: other.id,
        label: `${sourceName(other.id)} · ${skillEntityLabel(other, app.snapshot!)}`,
      })),
    ]),
  ),
)
function sourceName(skillId: string) {
  const skill = app.snapshot?.skills.find((s) => s.id === skillId)
  return (
    app.sourceName(app.snapshot?.sources.find((source) => source.id === skill?.sourceId)) ||
    '未关联来源'
  )
}
function claimName(claim: string) {
  if (claim === 'manual') return '手动分发'
  return (
    app.snapshot?.presets.find((p) => `preset:${p.id}` === claim)?.name ||
    app.snapshot?.sources.find((s) => `source:${s.id}` === claim)?.name ||
    claim
  )
}
function selectSource(skillId: string, next: string | number | undefined) {
  if (next === undefined) return
  selectedIds.value = selectedIds.value.map((id) => (id === skillId ? String(next) : id))
  replaceBindingIds.value = []
}
const switchableIds = computed(
  () =>
    plan.value?.items
      .filter(
        (item) =>
          item.replacement &&
          (item.action === 'replace' || item.error === '目标已使用其他来源，请确认切换来源'),
      )
      .map((item) => item.replacement!.bindingId) ?? [],
)
const allSwitchesSelected = computed(
  () =>
    switchableIds.value.length > 0 &&
    switchableIds.value.every((id) => replaceBindingIds.value.includes(id)),
)
const planItems = computed(
  () =>
    plan.value?.items.map((item) => ({
      ...item,
      skillName: app.snapshot?.skills.find((skill) => skill.id === item.skillId)?.name,
      targetName: app.targetName(
        app.snapshot?.targets.find((target) => target.id === item.targetId),
      ),
      statusLabel: item.error
        ? item.error === '目标已使用其他来源，请确认切换来源'
          ? '待确认切换'
          : '需处理冲突'
        : distributionActionLabel(item.action),
      awaitingConfirmation: item.error === '目标已使用其他来源，请确认切换来源',
      sourceName: sourceName(item.skillId),
      previousSource: item.replacement ? sourceName(item.replacement.skillId) : '',
      claimsText: item.replacement?.claims.map(claimName).join('、') || '',
      blockingText: item.replacement?.blockingClaims.map(claimName).join('、') || '',
      contentLabel:
        item.replacement?.contentEqual === true
          ? 'Skill 目录内容一致（不含包级共享资源）'
          : item.replacement?.contentEqual === false
            ? 'Skill 目录内容存在差异'
            : 'Skill 目录内容无法完整比较',
    })) ?? [],
)
async function changeReplacements(event: Event) {
  replaceBindingIds.value = (event.target as HTMLInputElement).checked
    ? [...switchableIds.value]
    : []
  await preview()
}

watch(
  () => props.open,
  (open) => {
    if (open) {
      targetIds.value = [...props.initialTargetIds]
      selectedIds.value = [...props.skillIds]
      replaceBindingIds.value = []
      plan.value = null
      takeover.value = false
      adoptExisting.value = false
      localError.value = ''
    }
  },
)
async function preview() {
  if (busy.value) return
  if (!targetIds.value.length) {
    localError.value = '请至少选择一个分发目标'
    return
  }
  busy.value = true
  localError.value = ''
  try {
    plan.value = await api.plan(
      selectedIds.value,
      targetIds.value,
      props.claim,
      adoptExisting.value,
      replaceBindingIds.value,
    )
  } catch (error) {
    plan.value = null
    replaceBindingIds.value = []
    adoptExisting.value = false
    localError.value = error instanceof Error ? error.message : '无法生成分发计划'
  } finally {
    busy.value = false
  }
}
function changeOpen(open: boolean) {
  if (!busy.value) emit('update:open', open)
}
async function changeAdoption(event: Event) {
  adoptExisting.value = (event.target as HTMLInputElement).checked
  await preview()
}
function backToSelection() {
  plan.value = null
  replaceBindingIds.value = []
  takeover.value = false
  adoptExisting.value = false
  localError.value = ''
}
async function submit() {
  if (!plan.value || busy.value) return
  if (plan.value.items.some((i) => i.action === 'takeover') && !takeover.value) {
    localError.value = '请确认接管已有链接'
    return
  }
  busy.value = true
  const ok = await app.mutate(
    () =>
      api.distribute(
        selectedIds.value,
        targetIds.value,
        plan.value!.revision,
        props.claim,
        takeover.value,
        adoptExisting.value,
        replaceBindingIds.value,
      ),
    '分发任务已完成',
  )
  busy.value = false
  if (ok) {
    emit('completed')
    emit('update:open', false)
  } else localError.value = app.error || '分发失败，请返回重新预览'
}
</script>

<template>
  <AppDialog
    :open="open"
    :title="title"
    description="先选择目标，再确认实际链接计划。计划过期时会要求重新预览。"
    large
    fixed-layout
    @update:open="changeOpen"
  >
    <div v-if="!plan" class="form-grid distribution-selection">
      <section>
        <div class="field-label" style="margin-bottom: 8px">
          将分发 {{ selectedSkills.length }} 个 Skill
        </div>
        <div class="choice-list">
          <div v-for="skill in selectedSkills" :key="skill.id" class="choice">
            <div class="item-icon"><Link2 /></div>
            <div class="choice-main">
              <div class="choice-title">{{ skill.name }}</div>
              <div class="choice-meta">当前实体：{{ entityLabels[skill.id] }}</div>
              <AppSelect
                v-if="claim === 'manual' && (sourceChoices[skill.id]?.length ?? 0) > 1"
                :model-value="skill.id"
                :options="sourceChoices[skill.id] ?? []"
                :disabled="busy"
                :aria-label="`${skill.name} 的分发来源`"
                @update:model-value="selectSource(skill.id, $event)"
              />
              <SkillDirectoryActions :skill="skill" />
            </div>
          </div>
        </div>
      </section>
      <section>
        <div class="field-label" style="margin-bottom: 8px">选择目标</div>
        <div class="choice-list">
          <label v-for="target in app.snapshot?.targets" :key="target.id" class="choice"
            ><input v-model="targetIds" class="checkbox" type="checkbox" :value="target.id" />
            <div class="choice-main">
              <div class="choice-title">{{ app.targetName(target) }}</div>
              <div class="choice-meta mono">{{ target.path }}</div>
            </div></label
          >
        </div>
      </section>
    </div>
    <section v-else class="distribution-preview" aria-label="分发计划">
      <div class="distribution-plan-list">
        <article
          v-for="item in planItems"
          :key="`${item.skillId}-${item.targetId}`"
          class="distribution-plan-card"
        >
          <header class="plan-card-heading">
            <div class="plan-card-identity">
              <h3>{{ item.skillName }}</h3>
              <span class="plan-target"><ArrowRight />{{ item.targetName }}</span>
            </div>
            <Badge
              :tone="
                item.error
                  ? item.awaitingConfirmation
                    ? 'amber'
                    : 'red'
                  : item.action === 'keep'
                    ? 'neutral'
                    : 'blue'
              "
            >
              {{ item.statusLabel }}
            </Badge>
          </header>
          <div class="plan-destination">
            <FolderSymlink aria-hidden="true" />
            <span class="mono">{{ item.path }}</span>
          </div>
          <template v-if="item.replacement">
            <div class="source-comparison">
              <div class="source-panel">
                <span class="source-caption">当前来源</span>
                <strong>{{ item.previousSource }}</strong>
                <span class="source-version">分发记录版本：{{ item.replacement.version }}</span>
                <p class="source-path mono">
                  {{ item.replacement.entityPath }}
                </p>
              </div>
              <div class="source-panel source-panel-next">
                <span class="source-caption">切换到</span>
                <strong>{{ item.sourceName }}</strong>
                <span class="source-version">来源记录版本：{{ item.replacement.nextVersion }}</span>
                <p class="source-path mono">
                  {{ item.replacement.nextEntityPath }}
                </p>
              </div>
            </div>
            <div class="plan-comparison-meta">
              <p>{{ item.contentLabel }}</p>
              <p v-if="item.claimsText"><span>当前引用</span>{{ item.claimsText }}</p>
            </div>
          </template>
          <div v-if="item.error && !item.awaitingConfirmation" class="plan-conflict">
            <AlertTriangle aria-hidden="true" />
            <div>
              <p>{{ item.error }}</p>
              <p v-if="item.blockingText" class="plan-conflict-claims">
                需先解除：{{ item.blockingText }}
              </p>
            </div>
          </div>
          <p v-else-if="item.replacement" class="plan-behavior">
            仅切换此工具的引用，旧实体和备份保留。{{
              item.replacement.restoresOriginal
                ? '全部取消后恢复原外部链接。'
                : '全部取消后移除受管软链。'
            }}
          </p>
          <p v-if="item.action === 'borrow'" class="plan-behavior">
            继续使用原目录；取消分发后保留此软链。
          </p>
          <p v-else-if="item.action === 'adopt'" class="plan-behavior">
            继续使用原目录；全部分发引用取消后删除此软链，实体目录保留。
          </p>
          <footer class="plan-card-footer">
            <SkillDirectoryActions :skill-id="item.skillId" />
          </footer>
        </article>
      </div>
    </section>
    <template #notice v-if="localError || plan?.items.some((item) => item.error)">
      <div
        v-if="plan?.items.some((item) => item.error)"
        class="callout warning distribution-notice"
      >
        <AlertTriangle aria-hidden="true" />
        <span>部分分发需确认来源切换或处理冲突，请查看条目标记。</span>
      </div>
      <p v-if="localError" class="field-error" role="alert">{{ localError }}</p>
    </template>
    <template
      #options
      v-if="
        switchableIds.length || adoptableCount || plan?.items.some((i) => i.action === 'takeover')
      "
    >
      <label v-if="switchableIds.length" class="choice">
        <input
          type="checkbox"
          class="checkbox"
          :checked="allSwitchesSelected"
          :disabled="busy"
          @change="changeReplacements"
        />
        <div class="choice-main">
          <div class="choice-title">确认将 {{ switchableIds.length }} 项已有分发切换到所选来源</div>
          <div class="choice-meta">
            旧实体和备份保留；未选择的工具及其他预设不变。勾选后重新预览，确认执行才生效。
          </div>
        </div>
      </label>
      <label v-if="adoptableCount" class="choice">
        <input
          :checked="adoptExisting"
          :disabled="busy"
          class="checkbox"
          type="checkbox"
          @change="changeAdoption"
        />
        <div class="choice-main">
          <div class="choice-title">接管这 {{ adoptableCount }} 条已有软链的管理</div>
          <div class="choice-meta">
            勾选并确认执行后，最后一个分发引用取消时会删除软链，原始实体目录保留。不勾选则仅复用，取消时保留软链。
          </div>
        </div>
      </label>

      <label v-if="plan?.items.some((i) => i.action === 'takeover')" class="choice"
        ><input v-model="takeover" :disabled="busy" class="checkbox" type="checkbox" /><span
          >接管已有外部链接，切换到统一库版本；取消全部引用时恢复原链接。</span
        ></label
      >
    </template>
    <template #footer
      ><Button v-if="plan" :disabled="busy" @click="backToSelection">返回选择</Button
      ><Button v-else :disabled="busy" @click="changeOpen(false)">取消</Button
      ><Button
        variant="primary"
        :disabled="busy || !!plan?.items.some((item) => item.error)"
        @click="plan ? submit() : preview()"
        >{{ busy ? '处理中…' : plan ? '确认执行' : '预览分发计划' }}</Button
      ></template
    >
  </AppDialog>
</template>

<style scoped>
.distribution-selection {
  flex: 1;
  min-height: 0;
  overflow: hidden;
}
.distribution-selection > section {
  display: flex;
  flex-direction: column;
  min-height: 0;
  overflow: hidden;
}
.distribution-selection .choice-list {
  flex: 1;
  min-height: 0;
  overflow: auto;
  overscroll-behavior: contain;
}
.distribution-preview {
  container-type: inline-size;
}
.distribution-plan-list {
  display: grid;
  gap: 12px;
}
.distribution-plan-card {
  min-width: 0;
  padding: 16px;
  border: 1px solid var(--line);
  border-radius: 10px;
  background: var(--panel);
}
.plan-card-heading {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 12px;
}
.plan-card-identity {
  min-width: 0;
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 4px 10px;
}
.plan-card-heading h3 {
  margin: 0;
  font-size: 14px;
  line-height: 22px;
  overflow-wrap: anywhere;
}
.plan-card-heading .badge {
  flex-shrink: 0;
  font-size: 11px;
}
.plan-target {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  color: var(--muted);
  font-size: 12px;
  line-height: 22px;
}
.plan-target svg,
.plan-destination svg,
.distribution-notice svg,
.plan-conflict > svg {
  width: 14px;
  height: 14px;
  flex-shrink: 0;
}
.plan-destination {
  display: flex;
  align-items: flex-start;
  gap: 7px;
  margin-top: 6px;
  font-size: 12px;
  line-height: 19px;
  color: var(--muted);
}
.plan-destination svg {
  margin-top: 2px;
}
.plan-destination .mono {
  min-width: 0;
  overflow-wrap: anywhere;
}
.source-comparison {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 10px;
  margin-top: 14px;
}
.source-panel {
  min-width: 0;
  padding: 12px;
  border: 1px solid var(--line);
  border-radius: 7px;
  background: var(--surface);
}
.source-panel-next {
  background: color-mix(in srgb, var(--blue-soft) 45%, var(--panel));
}
.source-caption,
.source-panel strong,
.source-version {
  display: block;
  overflow-wrap: anywhere;
}
.source-caption {
  margin-bottom: 6px;
  color: var(--muted);
  font-size: 11px;
  font-weight: 500;
}
.source-panel-next .source-caption {
  color: var(--blue);
}
.source-panel strong {
  font-size: 13px;
  font-weight: 600;
  line-height: 20px;
}
.source-version {
  margin-top: 2px;
  font-size: 11px;
  color: var(--muted);
}
.source-path {
  margin: 9px 0 0;
  padding-top: 8px;
  border-top: 1px solid var(--line);
  color: var(--muted);
  font-size: 11px;
  line-height: 17px;
  overflow-wrap: anywhere;
  text-wrap: wrap;
}
.plan-comparison-meta {
  display: grid;
  gap: 4px;
  margin-top: 10px;
  color: var(--muted);
  font-size: 11px;
  line-height: 18px;
  overflow-wrap: anywhere;
}
.plan-comparison-meta p,
.plan-conflict p {
  margin: 0;
}
.plan-comparison-meta span {
  margin-right: 8px;
}
.plan-conflict {
  display: flex;
  align-items: flex-start;
  gap: 8px;
  padding: 10px 12px;
  margin-top: 12px;
  border-radius: 6px;
  background: #fff3f0;
  color: #b43a2b;
  font-size: 12px;
  line-height: 19px;
  overflow-wrap: anywhere;
}
.plan-conflict > svg {
  margin-top: 2px;
}
.plan-conflict .plan-conflict-claims {
  margin-top: 3px;
  font-weight: 600;
}
.plan-behavior {
  margin: 10px 0 0;
  color: var(--muted);
  font-size: 11px;
  line-height: 18px;
}
.plan-card-footer {
  margin-top: 12px;
  padding-top: 8px;
  border-top: 1px solid var(--line);
}
.plan-card-footer .skill-directory-actions {
  margin-top: 0;
}
.distribution-notice {
  display: flex;
  align-items: flex-start;
  gap: 8px;
}
.distribution-notice svg {
  margin-top: 3px;
}
@container (max-width: 520px) {
  .source-comparison {
    grid-template-columns: minmax(0, 1fr);
  }
  .distribution-plan-card {
    padding: 12px;
  }
}
</style>
