<script setup lang="ts">
import { computed, ref } from 'vue'
import {
  MonitorCog,
  Plus,
  Link2,
  Search,
  AlertTriangle,
  ExternalLink,
  Unlink,
  PackageOpen,
} from 'lucide-vue-next'
import Button from '@/components/ui/Button.vue'
import Badge from '@/components/ui/Badge.vue'
import AppSheet from '@/components/ui/AppSheet.vue'
import AppDialog from '@/components/ui/AppDialog.vue'
import ConfirmDialog from '@/components/ui/ConfirmDialog.vue'
import TargetEditorDialog from '@/components/TargetEditorDialog.vue'
import DistributionDialog from '@/components/DistributionDialog.vue'
import { api } from '@/services/api'
import { useAppStore } from '@/stores/app'
import type { Binding, Target } from '@/services/types'

const app = useAppStore()
const addOpen = ref(false)
const detail = ref<Target | null>(null)
const detailOpen = ref(false)
const chooseSkillsOpen = ref(false)
const distributeOpen = ref(false)
const distributeIds = ref<string[]>([])
const revokeBinding = ref<Binding | null>(null)
const revokeOpen = ref(false)
const issues = ref<string[]>([])
const groups = computed(() => [
  {
    key: 'user',
    label: '用户级目录',
    items: app.snapshot?.targets.filter((t) => t.scope === 'user') ?? [],
  },
  {
    key: 'project',
    label: '项目级目录',
    items: app.snapshot?.targets.filter((t) => t.scope !== 'user') ?? [],
  },
])
const bindings = computed(
  () => app.snapshot?.bindings.filter((b) => b.targetId === detail.value?.id) ?? [],
)
const count = (targetId: string) =>
  app.snapshot?.bindings.filter((b) => b.targetId === targetId).length || 0
function show(target: Target) {
  detail.value = target
  detailOpen.value = true
}
function openDistribute() {
  distributeIds.value = []
  chooseSkillsOpen.value = true
}
function continueDistribute() {
  chooseSkillsOpen.value = false
  distributeOpen.value = true
}
async function diagnose() {
  try {
    issues.value = (await api.diagnose()).issues
    if (!issues.value.length) app.notice = '未发现断链或目录异常'
  } catch (e) {
    app.error = e instanceof Error ? e.message : '诊断失败'
  }
}
async function revoke() {
  if (!revokeBinding.value) return
  const claim = 'manual'
  const ok = await app.mutate(
    () => api.revoke([revokeBinding.value!.id], claim),
    '已取消指定来源的分发关系',
  )
  if (ok) revokeOpen.value = false
}
function requestRevoke(binding: Binding) {
  revokeBinding.value = binding
  revokeOpen.value = true
}
</script>

<template>
  <div class="page">
    <header class="page-heading">
      <div>
        <h1 class="page-title">分发目标</h1>
        <p class="page-subtitle">管理工具与项目中的 Skill 链接。</p>
      </div>
      <div class="actions">
        <Button @click="diagnose"><Search />检查断链</Button
        ><Button variant="primary" @click="addOpen = true"><Plus />添加目标</Button>
      </div>
    </header>
    <div v-if="issues.length" class="recovery-bar">
      <AlertTriangle style="width: 16px" />发现 {{ issues.length }} 个问题：{{
        issues.join('；')
      }}。
    </div>
    <section v-for="group in groups" :key="group.key" class="panel">
      <div class="panel-header">
        <h3 class="panel-title">{{ group.label }}</h3>
        <Badge>{{ group.items.length }} 个目标</Badge>
      </div>
      <div class="list-stack" style="padding: 12px">
        <div v-for="target in group.items" :key="target.id" class="list-row">
          <div class="item-icon"><MonitorCog /></div>
          <div class="list-row-main">
            <div class="list-row-title">
              {{ target.name }} <Badge tone="blue">{{ target.tool }}</Badge>
            </div>
            <div class="list-row-meta mono">{{ target.path }}</div>
          </div>
          <div style="text-align: right">
            <div class="item-name">{{ count(target.id) }}</div>
            <div class="item-desc">已分发</div>
          </div>
          <Button size="sm" @click="show(target)">查看详情</Button>
        </div>
      </div>
    </section>
    <TargetEditorDialog v-model:open="addOpen" />
    <AppSheet
      v-model:open="detailOpen"
      :title="detail?.name || '目标详情'"
      :description="detail?.path"
      ><div class="actions" style="margin-bottom: 16px">
        <Button variant="primary" @click="openDistribute"><Link2 />分发 Skill</Button
        ><Button @click="diagnose"><Search />检查断链</Button>
      </div>
      <dl class="detail-list">
        <div class="detail-row">
          <dt>工具</dt>
          <dd>{{ detail?.tool }}</dd>
        </div>
        <div class="detail-row">
          <dt>作用域</dt>
          <dd>{{ detail?.scope === 'user' ? '用户级' : '项目级' }}</dd>
        </div>
        <div class="detail-row">
          <dt>读取关系</dt>
          <dd>同一物理目录只登记一次；工具可能共享此目录。</dd>
        </div>
      </dl>
      <h3 class="panel-title" style="margin: 20px 0 10px">已分发 Skill</h3>
      <div v-if="!bindings.length" class="callout">此目标还没有分发关系。</div>
      <div v-else class="list-stack">
        <div v-for="binding in bindings" :key="binding.id" class="list-row">
          <div class="item-icon"><PackageOpen /></div>
          <div class="list-row-main">
            <div class="list-row-title">
              {{ app.snapshot?.skills.find((s) => s.id === binding.skillId)?.name }}
            </div>
            <div class="list-row-meta">
              v{{ binding.version }} ·
              {{
                binding.claims
                  .map((c) =>
                    c === 'manual'
                      ? '手动'
                      : app.snapshot?.presets.find((p) => `preset:${p.id}` === c)?.name || c,
                  )
                  .join(' + ')
              }}
            </div>
          </div>
          <Badge
            :tone="binding.path.includes('missing') ? 'red' : binding.follow ? 'green' : 'amber'"
            >{{
              binding.path.includes('missing')
                ? '软链断开'
                : binding.follow
                  ? '跟随更新'
                  : '固定版本'
            }}</Badge
          ><Button
            size="icon"
            variant="ghost"
            :disabled="!binding.claims.includes('manual')"
            title="取消手动分发；预设请在预设页面整体取消"
            aria-label="取消分发"
            @click="requestRevoke(binding)"
            ><Unlink
          /></Button>
        </div>
      </div>
      <div class="callout" style="margin-top: 14px">
        分发完成后，请在对应工具中刷新或重新加载 Skill。
      </div></AppSheet
    >
    <DistributionDialog
      v-model:open="distributeOpen"
      :skill-ids="distributeIds"
      :initial-target-ids="detail ? [detail.id] : []"
    />
    <AppDialog
      v-model:open="chooseSkillsOpen"
      title="选择要分发的 Skill"
      description="从中央库选择一个或多个 Skill。"
      large
      ><div class="choice-list" style="max-height: 360px; overflow: auto">
        <label v-for="skill in app.snapshot?.skills" :key="skill.id" class="choice"
          ><input v-model="distributeIds" class="checkbox" type="checkbox" :value="skill.id" />
          <div class="choice-main">
            <div class="choice-title">{{ skill.name }}</div>
            <div class="choice-meta">v{{ skill.version }}</div>
          </div></label
        >
      </div>
      <template #footer
        ><Button @click="chooseSkillsOpen = false">取消</Button
        ><Button variant="primary" :disabled="!distributeIds.length" @click="continueDistribute"
          >继续选择目标</Button
        ></template
      ></AppDialog
    >
    <ConfirmDialog
      v-model:open="revokeOpen"
      title="取消分发来源"
      :description="`将从此关系移除「${revokeBinding?.claims.includes('manual') ? '手动添加' : '预设'}」来源。若还有其他预设引用，软链会保留。`"
      confirm-text="确认取消"
      danger
      @confirm="revoke"
    />
  </div>
</template>
