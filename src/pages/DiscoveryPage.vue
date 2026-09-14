<script setup lang="ts">
import SkillDirectoryActions from '@/components/SkillDirectoryActions.vue'
import { computed, onMounted, ref, watch } from 'vue'
import {
  Search,
  Globe2,
  Download,
  CheckCircle2,
  GitBranch,
  FolderInput,
  ExternalLink,
} from 'lucide-vue-next'
import Button from '@/components/ui/Button.vue'
import Badge from '@/components/ui/Badge.vue'
import EmptyState from '@/components/ui/EmptyState.vue'
import AppSheet from '@/components/ui/AppSheet.vue'
import AppDialog from '@/components/ui/AppDialog.vue'
import DirectoryField from '@/components/DirectoryField.vue'
import AppPagination from '@/components/ui/AppPagination.vue'
import { api } from '@/services/api'
import { defaultCatalogSites, normalizeCatalogSites, catalogSiteKey } from '@/services/catalogSites'
import { useAppStore } from '@/stores/app'
import type { CatalogItem } from '@/services/types'
import { usePagination } from '@/composables/usePagination'

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
const installing = ref(false)
const presetIds = ref<string[]>([])
const targetIds = ref<string[]>([])
const gitOpen = ref(false)
const gitUrl = ref('')
const gitRef = ref('HEAD')
const gitSubdir = ref('')
const folderOpen = ref(false)
const folder = ref(app.isNative ? '' : '/Users/demo/Downloads/community-skills')
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
  detail.value = item
  detailOpen.value = false
  presetIds.value = []
  targetIds.value = []
  installOpen.value = true
}
async function install() {
  if (!detail.value) return
  installing.value = true
  const ok = await app.mutate(
    () => api.installCatalog(detail.value!.slug, detail.value!.site),
    `${detail.value.name} 已安装到中央库`,
  )
  if (!ok) {
    installing.value = false
    return
  }
  const source = app.snapshot?.sources.find(
    (s) =>
      s.reference === detail.value!.slug &&
      ['catalog', 'clawhub'].includes(s.kind) &&
      catalogSiteKey(s.url) === catalogSiteKey(detail.value!.site),
  )
  const skill =
    app.snapshot?.skills.find((item) => item.sourceId === source?.id) ||
    (!app.isNative
      ? app.snapshot?.skills.find((item) => item.id === detail.value!.slug)
      : undefined)
  for (const presetId of presetIds.value) {
    const preset = app.snapshot?.presets.find((item) => item.id === presetId)
    if (preset && skill)
      await app.mutate(
        () =>
          api.savePreset({
            id: preset.id,
            name: preset.name,
            description: preset.description,
            skillIds: [...new Set([...preset.skillIds, skill.id])],
          }),
        `已加入预设「${preset.name}」`,
      )
  }
  if (skill && targetIds.value.length && app.snapshot) {
    try {
      const plan = await api.plan([skill.id], targetIds.value)
      if (plan.items.some((item) => item.error))
        throw new Error('目标存在冲突，Skill 已入库但未分发')
      await app.mutate(
        () => api.distribute([skill.id], targetIds.value, plan.revision),
        '安装并分发已完成',
      )
    } catch (e) {
      app.error = e instanceof Error ? e.message : '分发失败'
    }
  }
  installing.value = false
  installOpen.value = false
}
async function importGit() {
  const ok = await app.mutate(
    () => api.importGit(gitUrl.value, gitRef.value || 'HEAD', gitSubdir.value || undefined),
    'Git 来源已导入',
  )
  if (ok) gitOpen.value = false
}
async function importFolder() {
  try {
    const scan = await api.scan(folder.value)
    const paths = scan.items
      .filter((i) => ['ready', 'new', 'same'].includes(i.status))
      .map((i) => i.path)
    if (!paths.length) throw new Error('未发现可直接导入的 Skill；冲突内容请使用归集向导')
    const ok = await app.mutate(
      () => api.importFolder(folder.value, paths, false),
      `已从文件夹导入 ${paths.length} 项`,
    )
    if (ok) folderOpen.value = false
  } catch (e) {
    app.error = e instanceof Error ? e.message : '导入失败'
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
        ><Button @click="folderOpen = true"><FolderInput />复制文件夹入库</Button>
      </div>
    </header>
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
          ><Button v-else variant="primary" size="sm" @click="startInstall(item)"
            ><Download />安装</Button
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
      v-model:open="installOpen"
      title="确认安装"
      :description="`${detail?.name || ''} 将先安装到中央库；下面的分发与加入预设均为可选。`"
      large
      ><div class="form-grid">
        <section>
          <div class="field-label" style="margin-bottom: 8px">内容</div>
          <div class="choice">
            <div class="item-icon"><Download /></div>
            <div class="choice-main">
              <div class="choice-title">{{ detail?.name }}</div>
              <div class="choice-meta">
                {{ siteLabel(detail?.site || '') }} · v{{ detail?.version }}
              </div>
              <SkillDirectoryActions :catalog="detail" />
            </div>
          </div>
          <div class="callout" style="margin-top: 10px">
            安装内容先进入统一目录，可选择同时加入预设或分发。
          </div>
        </section>
        <section>
          <div class="field-label" style="margin-bottom: 8px">可选后续操作</div>
          <div class="choice-list">
            <label v-for="preset in app.snapshot?.presets" :key="preset.id" class="choice"
              ><input v-model="presetIds" class="checkbox" type="checkbox" :value="preset.id" />
              <div class="choice-main">
                <div class="choice-title">加入 {{ preset.name }}</div>
              </div></label
            ><label v-for="target in app.snapshot?.targets" :key="target.id" class="choice"
              ><input v-model="targetIds" class="checkbox" type="checkbox" :value="target.id" />
              <div class="choice-main">
                <div class="choice-title">分发到 {{ app.targetName(target) }}</div>
              </div></label
            >
          </div>
        </section>
      </div>
      <template #footer
        ><Button @click="installOpen = false">取消</Button
        ><Button variant="primary" :disabled="installing" @click="install">{{
          installing ? '安装中…' : '安装并继续'
        }}</Button></template
      ></AppDialog
    >
    <AppDialog
      v-model:open="gitOpen"
      title="从 Git 仓库导入"
      description="可指定分支、标签、提交号与仓库子目录。"
      ><div class="list-stack">
        <label class="field"
          ><span class="field-label">仓库地址</span><input v-model="gitUrl" class="input"
        /></label>
        <div class="form-grid">
          <label class="field"
            ><span class="field-label">引用</span><input v-model="gitRef" class="input" /></label
          ><label class="field"
            ><span class="field-label">子目录</span
            ><input v-model="gitSubdir" class="input" placeholder="可选"
          /></label>
        </div>
        <div class="callout">
          跟踪远端更新不会修改你的原仓库；来源包可包含多个 Skill 与共享资源。
        </div>
      </div>
      <template #footer
        ><Button @click="gitOpen = false">取消</Button
        ><Button variant="primary" @click="importGit">导入</Button></template
      ></AppDialog
    >
    <AppDialog
      v-model:open="folderOpen"
      title="复制文件夹入库"
      description="将内容复制到统一库，原文件夹保留；直接使用原目录请添加本地来源。"
      ><label class="field"
        ><span class="field-label">文件夹</span><DirectoryField v-model="folder"
      /></label>
      <div class="callout" style="margin-top: 12px">
        若需要接管目录并替换为软链，请改用“归集已有 Skill”向导。
      </div>
      <template #footer
        ><Button @click="folderOpen = false">取消</Button
        ><Button variant="primary" @click="importFolder">扫描并导入</Button></template
      ></AppDialog
    >
  </div>
</template>
