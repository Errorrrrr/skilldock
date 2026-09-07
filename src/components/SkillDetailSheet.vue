<script setup lang="ts">
import AppSelect from '@/components/ui/AppSelect.vue'
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

const props = defineProps<{ open: boolean; skill: Skill | null }>()
const emit = defineEmits<{ 'update:open': [value: boolean]; distribute: [skillId: string] }>()
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
    ? `${app.snapshot.storageRoot}/objects/${props.skill.bundleDigest}/tree/${props.skill.relativePath}`
    : '',
)
const bindings = computed(
  () => app.snapshot?.bindings.filter((item) => item.skillId === props.skill?.id) ?? [],
)
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

const rollbackDigestOptions = computed(() =>
  versions.value.map((version) => ({
    value: version.bundleDigest,
    label: `${version.version} · ${version.bundleDigest.slice(0, 12)}`,
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
    <TabsRoot default-value="overview">
      <TabsList class="tabs-list"
        ><TabsTrigger class="tab-trigger" value="overview">概览</TabsTrigger
        ><TabsTrigger class="tab-trigger" value="content">内容</TabsTrigger
        ><TabsTrigger class="tab-trigger" value="distribution">分发</TabsTrigger
        ><TabsTrigger class="tab-trigger" value="updates">更新</TabsTrigger></TabsList
      >
      <TabsContent value="overview">
        <div class="actions" style="margin-bottom: 14px">
          <Button variant="primary" @click="skill && emit('distribute', skill.id)"
            ><Link2 />分发</Button
          ><Badge tone="blue">v{{ skill?.version }}</Badge>
        </div>
        <dl class="detail-list">
          <div class="detail-row">
            <dt>来源</dt>
            <dd>{{ source?.name || '独立来源' }}</dd>
          </div>
          <div class="detail-row">
            <dt>中央库路径</dt>
            <dd>
              <span class="mono">{{ contentPath }}</span
              ><button class="link-button" style="margin-left: 8px" @click="copyPath">
                <Copy style="width: 12px; display: inline" /> {{ copied ? '已复制' : '复制' }}
              </button>
            </dd>
          </div>
          <div class="detail-row">
            <dt>内容摘要</dt>
            <dd class="mono">{{ skill?.bundleDigest }}</dd>
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
      </TabsContent>
      <TabsContent value="content"
        ><div v-if="loadingContent" class="skeleton" style="height: 240px" />
        <div v-else class="markdown" v-html="rendered" />
        <div class="callout" style="margin-top: 16px">
          内容仅以净化后的 Markdown 预览，不执行其中的脚本或指令。
        </div></TabsContent
      >
      <TabsContent value="distribution"
        ><div v-if="!bindings.length" class="callout">尚未分发到任何目标。</div>
        <div v-else class="list-stack">
          <div v-for="binding in bindings" :key="binding.id" class="list-row">
            <div class="list-row-main">
              <div class="list-row-title">
                {{ app.snapshot?.targets.find((target) => target.id === binding.targetId)?.name }}
              </div>
              <div class="list-row-meta mono">{{ binding.path }}</div>
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
        </div></TabsContent
      >
      <TabsContent value="updates"
        ><div v-if="!bindings.length" class="callout">分发后可对每个目标设置跟随策略。</div>
        <div class="list-stack">
          <div v-for="binding in bindings" :key="binding.id" class="list-row">
            <div class="list-row-main">
              <div class="list-row-title">
                {{ app.snapshot?.targets.find((target) => target.id === binding.targetId)?.name }}
              </div>
              <div class="list-row-meta">
                {{ binding.follow ? '跟随来源更新' : '固定版本' }} · 当前 {{ binding.version }}
              </div>
            </div>
            <Button
              size="sm"
              :disabled="binding.claims.some((c) => c !== 'manual')"
              @click="toggleFollow(binding.id, !binding.follow)"
              >{{ binding.follow ? '固定版本' : '恢复跟随' }}</Button
            ><Button
              size="icon"
              variant="ghost"
              :disabled="binding.claims.some((c) => c !== 'manual')"
              title="选择历史版本"
              aria-label="回滚到上一个版本"
              @click="rollback(binding.id)"
              ><RotateCcw
            /></Button>
          </div></div
      ></TabsContent>
    </TabsRoot>
  </AppSheet>
  <AppDialog
    v-model:open="historyOpen"
    title="回滚分发版本"
    description="选择保留的历史快照。回滚后会暂停该目标的跟随更新。"
    ><label class="field"
      ><span class="field-label">历史版本</span
      ><AppSelect
        v-model="rollbackDigest"
        aria-label="历史版本"
        :options="rollbackDigestOptions" /></label
    ><template #footer
      ><Button @click="historyOpen = false">取消</Button
      ><Button variant="primary" @click="confirmRollback">回滚到此版本</Button></template
    ></AppDialog
  >
</template>
