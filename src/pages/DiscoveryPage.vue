<script setup lang="ts">
import SkillDirectoryActions from '@/components/SkillDirectoryActions.vue'
import { computed, onMounted, ref, watch } from 'vue'
import { Search, Globe2, Download, CheckCircle2, GitBranch, FolderInput } from 'lucide-vue-next'
import Button from '@/components/ui/Button.vue'
import Badge from '@/components/ui/Badge.vue'
import EmptyState from '@/components/ui/EmptyState.vue'
import AppSheet from '@/components/ui/AppSheet.vue'
import AppDialog from '@/components/ui/AppDialog.vue'
import AppPagination from '@/components/ui/AppPagination.vue'
import { api } from '@/services/api'
import { defaultCatalogSites, normalizeCatalogSites, catalogSiteKey } from '@/services/catalogSites'
import { useAppStore } from '@/stores/app'
import { catalogInstallSessions } from '@/stores/catalogInstall'
import type { CatalogItem } from '@/services/types'
import { usePagination } from '@/composables/usePagination'
import {
  catalogInstallComplete,
  catalogInstallErrors,
  catalogInstallKey,
  createCatalogInstallSession,
  runCatalogInstall,
} from '@/services/catalogInstall'

const app = useAppStore()
const query = ref('')
const sites = computed(() =>
  normalizeCatalogSites(app.snapshot?.settings.catalogSites || defaultCatalogSites).map((site) => ({
    label: site.name,
    value: site.url,
    status: 'configured',
  })),
)
const selectedSites = ref<string[]>(sites.value.map((site) => site.value))
watch(sites, (next, previous) => {
  const existing = new Set(previous.map((site) => site.value))
  selectedSites.value = next
    .filter((site) => selectedSites.value.includes(site.value) || !existing.has(site.value))
    .map((site) => site.value)
})
const results = ref<CatalogItem[]>([])
const errors = ref<string[]>([])
const searching = ref(false)
const detail = ref<CatalogItem | null>(null)
const detailOpen = ref(false)
const installOpen = ref(false)
const executingInstall = ref(false)
const installing = computed(
  () =>
    executingInstall.value ||
    Object.values(catalogInstallSessions.value).some((session) => session.running),
)
const installItem = ref<CatalogItem | null>(null)
const installSessions = catalogInstallSessions
const installSession = computed(() =>
  installItem.value ? installSessions.value[catalogInstallKey(installItem.value)] : undefined,
)
const unfinishedInstalls = computed(() =>
  Object.values(installSessions.value).filter((session) => !catalogInstallComplete(session)),
)
const presetIds = ref<string[]>([])
const targetIds = ref<string[]>([])
const gitOpen = ref(false)
const gitUrl = ref('')
const gitRef = ref('HEAD')
const gitSubdir = ref('')
const importingGit = ref(false)
const installed = computed(
  () =>
    new Set(
      app.snapshot?.sources
        .filter((s) => ['catalog', 'clawhub'].includes(s.kind))
        .map((s) => `${catalogSiteKey(s.url)}:${s.reference}`),
    ),
)
const siteLabel = (value: string) =>
  sites.value.find((site) => catalogSiteKey(site.value) === catalogSiteKey(value))?.label || value

const {
  page,
  pageSize,
  pagedItems: pagedResults,
  resetPage,
} = usePagination(results, { initialPageSize: 12 })

async function search() {
  resetPage()
  if (app.isNative && !query.value.trim()) {
    errors.value = ['请输入搜索关键词；原生目录服务不支持空查询']
    return
  }
  searching.value = true
  try {
    const data = await api.searchCatalog(query.value, selectedSites.value)
    results.value = data.items
    errors.value = data.errors
  } catch (e) {
    errors.value = [e instanceof Error ? e.message : '搜索失败']
  } finally {
    searching.value = false
  }
}
function show(item: CatalogItem) {
  detail.value = item
  detailOpen.value = true
}
function startInstall(item: CatalogItem) {
  if (installing.value) return
  installItem.value = item
  detailOpen.value = false
  const session = installSession.value
  presetIds.value =
    session?.steps.filter((step) => step.kind === 'preset').map((step) => step.id) || []
  targetIds.value =
    session?.steps.filter((step) => step.kind === 'target').map((step) => step.id) || []
  installOpen.value = true
}
function closeInstall(open: boolean) {
  if (!installing.value) installOpen.value = open
}
function endInstall() {
  if (installing.value || !installItem.value) return
  delete installSessions.value[catalogInstallKey(installItem.value)]
  installOpen.value = false
}
async function install() {
  if (!installItem.value || !app.snapshot || installing.value || app.loading) return
  const key = catalogInstallKey(installItem.value)
  if (!installSessions.value[key])
    installSessions.value[key] = createCatalogInstallSession(
      installItem.value,
      presetIds.value,
      targetIds.value,
      app.snapshot,
    )
  const session = installSessions.value[key]!
  executingInstall.value = true
  app.loading = true
  app.notice = ''
  app.error = ''
  try {
    const complete = await runCatalogInstall(
      session,
      {
        ...api,
        snapshot: () => app.snapshot!,
        updateSnapshot: (next) => {
          app.snapshot = next
        },
      },
      !app.isNative,
    )
    if (complete) {
      app.notice = `${session.item.name} 已入库（${session.skillIds.length} 个 Skill），所选后续操作已完成`
      installOpen.value = false
      delete installSessions.value[key]
    } else app.error = catalogInstallErrors(session).join('；')
  } finally {
    executingInstall.value = false
    app.loading = false
  }
}
async function importGit() {
  if (importingGit.value || !gitUrl.value.trim()) return
  importingGit.value = true
  try {
    const ok = await app.mutate(
      () =>
        api.importGit(gitUrl.value.trim(), gitRef.value || 'HEAD', gitSubdir.value || undefined),
      'Git 来源已导入',
    )
    if (ok) gitOpen.value = false
  } finally {
    importingGit.value = false
  }
}
onMounted(() => {
  if (!app.isNative) search()
})
</script>

<template>
  <div class="page">
    <header class="page-heading">
      <div>
        <h1 class="page-title">发现与安装</h1>
        <p class="page-subtitle">搜索已配置的网站，也可以从 Git 仓库或本地文件夹安装。</p>
      </div>
      <div class="actions">
        <Button @click="$router.push('/local-sources')">本地来源</Button>
        <Button @click="gitOpen = true"><GitBranch />Git 仓库</Button
        ><Button @click="$router.push('/local-sources?add=true')"
          ><FolderInput />本地文件夹（复制入库）</Button
        >
      </div>
    </header>
    <div
      v-for="session in unfinishedInstalls"
      :key="catalogInstallKey(session.item)"
      class="recovery-bar"
    >
      <span
        >{{ session.item.name }}：{{
          session.running ? '正在处理' : '部分操作尚未完成，已完成步骤会保留'
        }}</span
      >
      <Button size="sm" :disabled="installing" @click="startInstall(session.item)"
        >查看并继续</Button
      >
    </div>
    <section class="panel">
      <div class="toolbar">
        <div class="search-field" style="max-width: none">
          <Search /><input
            v-model="query"
            aria-label="搜索 Skill 名称、能力或作者"
            placeholder="搜索 Skill 名称、能力或作者…"
            @keyup.enter="search"
          />
        </div>
        <Button variant="primary" :disabled="searching" @click="search">{{
          searching ? '搜索中…' : '搜索'
        }}</Button>
      </div>
      <div class="stats-line">
        <strong>站点</strong
        ><label
          v-for="site in sites"
          :key="site.value"
          class="choice"
          style="padding: 5px 8px"
          :title="site.status === 'unsupported' ? '协议不兼容，当前核心不支持搜索 Skills.sh' : ''"
          ><input
            v-model="selectedSites"
            class="checkbox"
            type="checkbox"
            :value="site.value"
            :disabled="site.status === 'unsupported'"
          /><span>{{ site.label }}</span
          ><Badge v-if="site.status === 'unsupported'">协议不兼容</Badge></label
        ><RouterLink class="link-button" to="/settings">管理网站来源</RouterLink>
      </div>
      <div v-if="errors.length" class="recovery-bar" style="margin: 12px">
        <Globe2 style="width: 16px" />部分来源暂不可用：{{
          errors.join('；')
        }}。其他站点结果仍可使用。
      </div>
      <EmptyState
        v-if="!searching && !results.length"
        title="没有找到结果"
        description="尝试更换关键词、启用更多站点，或使用 Git 仓库地址导入。"
      />
      <div v-else class="list-stack" style="padding: 14px">
        <div v-for="item in pagedResults" :key="`${item.site}-${item.slug}`" class="list-row">
          <div class="item-icon"><Globe2 /></div>
          <div class="list-row-main">
            <div class="list-row-title">
              <button class="item-name-button" @click="show(item)">{{ item.name }}</button>
              <Badge>{{ siteLabel(item.site) }}</Badge>
            </div>
            <div class="list-row-meta">{{ item.description }} · 版本 {{ item.version }}</div>
            <SkillDirectoryActions :catalog="item" />
          </div>
          <Badge v-if="installed.has(`${catalogSiteKey(item.site)}:${item.slug}`)" tone="green"
            ><CheckCircle2 />已入库</Badge
          ><Button
            v-if="
              !installed.has(`${catalogSiteKey(item.site)}:${item.slug}`) ||
              installSessions[catalogInstallKey(item)]
            "
            variant="primary"
            size="sm"
            :disabled="installing"
            @click="startInstall(item)"
            ><Download />{{
              installSessions[catalogInstallKey(item)] ? '继续处理' : '安装'
            }}</Button
          ><Button size="sm" variant="ghost" @click="show(item)">详情</Button>
        </div>
      </div>
      <AppPagination
        v-if="results.length"
        v-model:page="page"
        v-model:page-size="pageSize"
        :total="results.length"
        :page-size-options="[12, 24, 48]"
      />
    </section>
    <AppSheet
      v-model:open="detailOpen"
      :title="detail?.name || 'Skill 详情'"
      :description="detail?.description"
      ><div class="actions" style="margin-bottom: 18px">
        <Badge tone="blue">{{ siteLabel(detail?.site || '') }}</Badge
        ><Badge>v{{ detail?.version }}</Badge>
      </div>
      <SkillDirectoryActions :catalog="detail" style="margin-bottom: 14px" />
      <dl class="detail-list">
        <div class="detail-row">
          <dt>来源网站</dt>
          <dd>{{ siteLabel(detail?.site || '') }}</dd>
        </div>
        <div class="detail-row">
          <dt>标识</dt>
          <dd class="mono">{{ detail?.slug }}</dd>
        </div>
        <div class="detail-row">
          <dt>上游仓库</dt>
          <dd>此站点未提供仓库地址。</dd>
        </div>
        <div class="detail-row">
          <dt>文件预览</dt>
          <dd>安装后可查看 SKILL.md 和来源信息。</dd>
        </div>
      </dl>
      <div class="callout" style="margin-top: 16px">可用版本由站点返回；安装默认只进入中央库。</div>
      <Button
        v-if="detail && !installed.has(`${catalogSiteKey(detail.site)}:${detail.slug}`)"
        variant="primary"
        style="margin-top: 16px"
        @click="startInstall(detail)"
        ><Download />安装此 Skill</Button
      ></AppSheet
    >
    <AppDialog
      :open="installOpen"
      title="确认安装"
      :description="`${installItem?.name || ''} 将先安装到中央库；下面的分发与加入预设均为可选。`"
      large
      @update:open="closeInstall"
      ><div class="form-grid">
        <section>
          <div class="field-label" style="margin-bottom: 8px">内容</div>
          <div class="choice">
            <div class="item-icon"><Download /></div>
            <div class="choice-main">
              <div class="choice-title">{{ installItem?.name }}</div>
              <div class="choice-meta">
                {{ siteLabel(installItem?.site || '') }} · v{{ installItem?.version }}
              </div>
              <SkillDirectoryActions :catalog="installItem" />
            </div>
          </div>
          <div class="callout" style="margin-top: 10px">
            安装内容先进入统一目录；如果来源包含多个 Skill，所选预设与目标会包含全部成员。
          </div>
        </section>
        <section>
          <div class="field-label" style="margin-bottom: 8px">可选后续操作</div>
          <div class="choice-list">
            <label v-for="preset in app.snapshot?.presets" :key="preset.id" class="choice"
              ><input
                v-model="presetIds"
                class="checkbox"
                type="checkbox"
                :value="preset.id"
                :disabled="!!installSession"
              />
              <div class="choice-main">
                <div class="choice-title">加入 {{ preset.name }}</div>
              </div></label
            ><label v-for="target in app.snapshot?.targets" :key="target.id" class="choice"
              ><input
                v-model="targetIds"
                class="checkbox"
                type="checkbox"
                :value="target.id"
                :disabled="!!installSession"
              />
              <div class="choice-main">
                <div class="choice-title">分发到 {{ app.targetName(target) }}</div>
              </div></label
            >
          </div>
        </section>
      </div>
      <div v-if="installSession" class="list-stack" style="margin-top: 16px" aria-live="polite">
        <div v-for="step in installSession.steps" :key="`${step.kind}:${step.id}`" class="choice">
          <div class="choice-main">
            <div class="choice-title">{{ step.label }}</div>
            <p v-if="step.error" class="field-error" role="alert">{{ step.error }}</p>
          </div>
          <Badge
            :tone="
              step.status === 'succeeded' ? 'green' : step.status === 'failed' ? 'amber' : 'neutral'
            "
          >
            {{
              { pending: '待处理', running: '处理中', succeeded: '已完成', failed: '需重试' }[
                step.status
              ]
            }}
          </Badge>
        </div>
        <p v-if="installSession.skillIds.length" class="subtle">
          本次来源共 {{ installSession.skillIds.length }} 个 Skill；重试会跳过已经完成的操作。
        </p>
        <p v-if="!installing" class="subtle">
          结束本次操作只清除进度，已入库、已加入预设和已分发的内容会保留。
        </p>
      </div>
      <template #footer
        ><Button :disabled="installing" @click="closeInstall(false)">{{
          installSession ? '关闭' : '取消'
        }}</Button
        ><Button v-if="installSession" :disabled="installing" @click="endInstall"
          >结束本次操作</Button
        ><Button
          variant="primary"
          :disabled="installing || app.loading"
          :loading="installing"
          @click="install"
          >{{ installing ? '处理中…' : installSession ? '重试未完成步骤' : '安装并继续' }}</Button
        ></template
      ></AppDialog
    >
    <AppDialog
      v-model:open="gitOpen"
      title="从 Git 仓库导入"
      description="可指定分支、标签、提交号与仓库子目录。"
      ><div class="list-stack">
        <label class="field"
          ><span class="field-label">仓库地址</span
          ><input v-model="gitUrl" class="input" :disabled="importingGit"
        /></label>
        <div class="form-grid">
          <label class="field"
            ><span class="field-label">引用</span
            ><input v-model="gitRef" class="input" :disabled="importingGit" /></label
          ><label class="field"
            ><span class="field-label">子目录</span
            ><input v-model="gitSubdir" class="input" placeholder="可选" :disabled="importingGit"
          /></label>
        </div>
        <div class="callout">
          跟踪远端更新不会修改你的原仓库；来源包可包含多个 Skill 与共享资源。
        </div>
      </div>
      <template #footer
        ><Button :disabled="importingGit" @click="gitOpen = false">取消</Button
        ><Button
          variant="primary"
          :disabled="importingGit || !gitUrl.trim()"
          :loading="importingGit"
          @click="importGit"
          >{{ importingGit ? '导入中…' : '导入' }}</Button
        ></template
      ></AppDialog
    >
  </div>
</template>
