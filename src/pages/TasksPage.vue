<script setup lang="ts">
import { ref, type Component } from 'vue'
import {
  ListChecks,
  RotateCcw,
  ChevronDown,
  ChevronUp,
  CheckCircle2,
  AlertTriangle,
  LoaderCircle,
  XCircle,
  Search,
} from 'lucide-vue-next'
import Button from '@/components/ui/Button.vue'
import Badge from '@/components/ui/Badge.vue'
import { useAppStore } from '@/stores/app'
import { api } from '@/services/api'
import { formatDate } from '@/lib/utils'

const app = useAppStore()
const expanded = ref<string[]>([])
const filter = ref('')
type Tone = 'neutral' | 'blue' | 'green' | 'amber' | 'red'
const status = (value: string): [string, Tone, Component] =>
  value === 'success'
    ? ['完成', 'green', CheckCircle2]
    : value === 'needsRecovery'
      ? ['待恢复', 'amber', AlertTriangle]
      : value === 'running'
        ? ['进行中', 'blue', LoaderCircle]
        : ['失败', 'red', XCircle]
function toggle(id: string) {
  expanded.value = expanded.value.includes(id)
    ? expanded.value.filter((item) => item !== id)
    : [...expanded.value, id]
}
async function recover(id: string) {
  await app.mutate(() => api.recover(id), '任务恢复完成')
}
</script>

<template>
  <div class="page">
    <header class="page-heading">
      <div>
        <h1 class="page-title">任务与记录</h1>
        <p class="page-subtitle">查看操作进度、历史结果与需要恢复的任务。</p>
      </div>
      <Button @click="app.refresh()"><RotateCcw />刷新记录</Button>
    </header>
    <section class="panel">
      <div class="toolbar">
        <div class="search-field">
          <Search /><input v-model="filter" placeholder="搜索任务名称或结果" />
        </div>
        <Badge>{{ app.snapshot?.tasks.length || 0 }} 条记录</Badge>
      </div>
      <div class="list-stack" style="padding: 12px">
        <article
          v-for="task in app.snapshot?.tasks.filter(
            (t) => !filter || `${t.title}${t.message}`.includes(filter),
          )"
          :key="task.id"
          class="list-row"
          style="align-items: flex-start"
        >
          <div class="item-icon"><component :is="status(task.status)[2]" /></div>
          <div class="list-row-main">
            <div class="list-row-title">
              {{ task.title }} <Badge>{{ task.kind }}</Badge>
            </div>
            <div class="list-row-meta">{{ formatDate(task.createdAt) }} · {{ task.message }}</div>
            <div v-if="expanded.includes(task.id)" class="callout" style="margin-top: 10px">
              <strong>任务结果</strong><br />{{ task.message }}<br /><span class="mono"
                >task: {{ task.id }}</span
              >
            </div>
          </div>
          <Badge :tone="status(task.status)[1]">{{ status(task.status)[0] }}</Badge
          ><Button
            v-if="task.status === 'needsRecovery'"
            size="sm"
            variant="primary"
            @click="recover(task.id)"
            ><RotateCcw />恢复</Button
          ><Button
            size="icon"
            variant="ghost"
            :aria-label="expanded.includes(task.id) ? '收起详情' : '展开详情'"
            @click="toggle(task.id)"
            ><ChevronUp v-if="expanded.includes(task.id)" /><ChevronDown v-else
          /></Button>
        </article>
      </div>
    </section>
  </div>
</template>
