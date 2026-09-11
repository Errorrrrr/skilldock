<script setup lang="ts">
import { distributionActionLabel } from '@/services/distributionLabels'
import SkillDirectoryActions from '@/components/SkillDirectoryActions.vue'
import { computed, ref, watch } from 'vue'
import { Link2, AlertTriangle } from 'lucide-vue-next'
import AppDialog from './ui/AppDialog.vue'
import Button from './ui/Button.vue'
import Badge from './ui/Badge.vue'
import { api } from '@/services/api'
import { useAppStore } from '@/stores/app'
import type { DistributionPlan } from '@/services/types'

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
const plan = ref<DistributionPlan | null>(null)
const busy = ref(false)
const takeover = ref(false)
const localError = ref('')
const selectedSkills = computed(
  () => app.snapshot?.skills.filter((skill) => props.skillIds.includes(skill.id)) ?? [],
)

watch(
  () => props.open,
  (open) => {
    if (open) {
      targetIds.value = [...props.initialTargetIds]
      plan.value = null
      takeover.value = false
      localError.value = ''
    }
  },
)
async function preview() {
  if (!targetIds.value.length) {
    localError.value = '请至少选择一个分发目标'
    return
  }
  busy.value = true
  localError.value = ''
  try {
    plan.value = await api.plan(props.skillIds, targetIds.value, props.claim)
  } catch (error) {
    localError.value = error instanceof Error ? error.message : '无法生成分发计划'
  } finally {
    busy.value = false
  }
}
async function submit() {
  if (!plan.value) return
  if (plan.value.items.some((i) => i.action === 'takeover') && !takeover.value) {
    localError.value = '请确认接管已有链接'
    return
  }
  busy.value = true
  const ok = await app.mutate(
    () =>
      api.distribute(
        props.skillIds,
        targetIds.value,
        plan.value!.revision,
        props.claim,
        takeover.value,
      ),
    '分发任务已完成',
  )
  busy.value = false
  if (ok) {
    emit('completed')
    emit('update:open', false)
  }
}
</script>

<template>
  <AppDialog
    :open="open"
    :title="title"
    description="先选择目标，再确认实际链接计划。计划过期时会要求重新预览。"
    large
    @update:open="emit('update:open', $event)"
  >
    <div v-if="!plan" class="form-grid">
      <section>
        <div class="field-label" style="margin-bottom: 8px">
          将分发 {{ selectedSkills.length }} 个 Skill
        </div>
        <div class="choice-list">
          <div v-for="skill in selectedSkills" :key="skill.id" class="choice">
            <div class="item-icon"><Link2 /></div>
            <div class="choice-main">
              <div class="choice-title">{{ skill.name }}</div>
              <div class="choice-meta">版本 {{ skill.version }}</div>
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
    <section v-else>
      <div
        v-if="plan.items.some((item) => item.error)"
        class="callout warning"
        style="margin-bottom: 12px"
      >
        <AlertTriangle
          style="width: 15px; vertical-align: -3px; margin-right: 5px"
        />存在冲突的条目不会被静默覆盖，请返回调整选择。
      </div>
      <div class="list-stack">
        <div v-for="item in plan.items" :key="`${item.skillId}-${item.targetId}`" class="list-row">
          <div class="list-row-main">
            <div class="list-row-title">
              {{ app.snapshot?.skills.find((skill) => skill.id === item.skillId)?.name }} →
              {{
                app.targetName(app.snapshot?.targets.find((target) => target.id === item.targetId))
              }}
            </div>
            <div class="list-row-meta mono">{{ item.path }}</div>
            <SkillDirectoryActions :skill-id="item.skillId" />
          </div>
          <Badge :tone="item.error ? 'red' : item.action === 'keep' ? 'neutral' : 'blue'">{{
            item.error || distributionActionLabel(item.action)
          }}</Badge>
        </div>
      </div>
      <div class="callout" style="margin-top: 12px">
        完成仅表示软链计划已执行；工具是否发现、实际运行是否验证，需要分别确认。
      </div>
    </section>
    <p v-if="localError" class="field-error">{{ localError }}</p>
    <label v-if="plan?.items.some((i) => i.action === 'takeover')" class="choice"
      ><input v-model="takeover" class="checkbox" type="checkbox" /><span
        >接管已有外部链接，切换到统一库版本；取消全部引用时恢复原链接。</span
      ></label
    >
    <template #footer
      ><Button v-if="plan" @click="plan = null">返回选择</Button
      ><Button v-else @click="emit('update:open', false)">取消</Button
      ><Button
        variant="primary"
        :disabled="busy || !!plan?.items.some((item) => item.error)"
        @click="plan ? submit() : preview()"
        >{{ busy ? '处理中…' : plan ? '确认执行' : '预览分发计划' }}</Button
      ></template
    >
  </AppDialog>
</template>
