<script setup lang="ts">
import SingleContentSettings from '@/components/SingleContentSettings.vue'
import { libraryDirectory } from '@/services/librarySkills'
import AppUpdatePanel from '@/components/AppUpdatePanel.vue'
import { useAppUpdater } from '@/composables/useAppUpdater'
import AgentDirectories from '@/components/AgentDirectories.vue'
import { useSettingsEditor } from '@/stores/settingsEditor'
import { useRoute, useRouter } from 'vue-router'
import AppSelect from '@/components/ui/AppSelect.vue'
import { computed, ref, watch, onMounted } from 'vue'
import {
  Plus,
  Trash2,
  HardDrive,
  Globe2,
  RefreshCcw,
  Palette,
  FolderOpen,
  ShieldCheck,
  ExternalLink,
} from 'lucide-vue-next'
import Button from '@/components/ui/Button.vue'
import Badge from '@/components/ui/Badge.vue'
import AppDialog from '@/components/ui/AppDialog.vue'
import ConfirmDialog from '@/components/ui/ConfirmDialog.vue'
import DirectoryField from '@/components/DirectoryField.vue'
import { useAppStore } from '@/stores/app'
import { api } from '@/services/api'

const app = useAppStore()
const editor = useSettingsEditor()
const route = useRoute()
const router = useRouter()
const section = ref<
  'storage' | 'directories' | 'sites' | 'updates' | 'app' | 'general' | 'network'
>('storage')
const migrateOpen = ref(false)
const migrateConfirm = ref(false)
const newPath = ref('')
const busy = ref(false)
const { busy: appUpdateBusy, phase: appUpdatePhase } = useAppUpdater()
const catalogSites = computed({
  get: () => editor.draft.catalogSites,
  set: (value) => {
    editor.draft.catalogSites = value
  },
})
const siteError = computed(() => editor.errors.sites || '')
const proxyMode = computed({
  get: () => editor.draft.networkProxy.mode,
  set: (value) => {
    editor.draft.networkProxy.mode = value
  },
})
const proxyUrl = computed({
  get: () => editor.draft.networkProxy.url,
  set: (value) => {
    editor.draft.networkProxy.url = value
  },
})
const proxyTesting = ref(false)
const proxyMessage = ref('')
const proxyError = ref(false)
watch([proxyMode, proxyUrl], () => {
  proxyMessage.value = ''
})
async function testProxy() {
  proxyTesting.value = true
  proxyMessage.value = ''
  proxyError.value = false
  try {
    const result = await api.testProxy({ mode: proxyMode.value, url: proxyUrl.value.trim() })
    proxyMessage.value = result.message
  } catch (e) {
    proxyMessage.value = e instanceof Error ? e.message : String(e)
    proxyError.value = true
  } finally {
    proxyTesting.value = false
  }
}
const agentProfiles = computed({
  get: () => editor.draft.agentProfiles,
  set: (value) => {
    editor.draft.agentProfiles = value
  },
})
const autoStart = ref(false)
const autoStartBusy = ref(false)
const localTimezone = Intl.DateTimeFormat().resolvedOptions().timeZone
const closeToTray = computed({
  get: () => editor.draft.closeToTray,
  set: (value) => {
    editor.draft.closeToTray = value
  },
})
const theme = computed({
  get: () => editor.draft.theme,
  set: (value) => {
    editor.draft.theme = value
  },
})
const updateMode = computed({
  get: () => editor.draft.updateMode,
  set: (value) => {
    editor.draft.updateMode = value
  },
})
const endpoint = computed({
  get: () => editor.draft.updateEndpoint,
  set: (value) => {
    editor.draft.updateEndpoint = value
  },
})
const publicKey = computed({
  get: () => editor.draft.updatePublicKey,
  set: (value) => {
    editor.draft.updatePublicKey = value
  },
})
const saveErrors = computed(() => Object.values(editor.errors).filter(Boolean))
watch(
  () => route.query.section,
  (value) => {
    if (
      typeof value === 'string' &&
      ['storage', 'directories', 'sites', 'updates', 'app', 'general', 'network'].includes(value)
    )
      section.value = value as typeof section.value
  },
  { immediate: true },
)
watch(section, (value) => {
  if (route.query.section !== value)
    void router.replace({ query: { ...route.query, section: value } })
})
onMounted(async () => {
  if (app.isNative) {
    try {
      const { isEnabled } = await import('@tauri-apps/plugin-autostart')
      autoStart.value = await isEnabled()
    } catch (e) {
      app.error = String(e)
    }
  }
})
async function toggleAutoStart() {
  if (autoStartBusy.value) return
  if (!app.isNative) {
    autoStart.value = !autoStart.value
    return
  }
  try {
    autoStartBusy.value = true
    const { enable, disable, isEnabled } = await import('@tauri-apps/plugin-autostart')
    if (autoStart.value) await disable()
    else await enable()
    autoStart.value = await isEnabled()
  } catch (e) {
    app.error = String(e)
  } finally {
    autoStartBusy.value = false
  }
}
function startMigrate() {
  newPath.value = libraryDirectory(app.snapshot)
  migrateOpen.value = true
}
async function openSkillsFolder() {
  if (!app.snapshot?.storageRoot) return
  try {
    await api.openDirectory(libraryDirectory(app.snapshot))
  } catch (error) {
    app.error = error instanceof Error ? error.message : String(error)
  }
}
async function migrate() {
  busy.value = true
  const ok = await app.mutate(
    () => api.migrateStorage(newPath.value),
    `统一存储目录已迁移到 ${newPath.value}`,
  )
  busy.value = false
  if (ok) {
    migrateConfirm.value = false
    migrateOpen.value = false
  }
}
const updateModeOptions = [
  { value: 'off', label: '关闭' },
  { value: 'notify', label: '仅检查提醒' },
  { value: 'auto', label: '自动更新入库' },
]
const themeOptions = [
  { value: 'light', label: '浅色' },
  { value: 'system', label: '跟随系统' },
  { value: 'dark', label: '深色' },
]
</script>

<template>
  <div class="page">
    <header class="page-heading">
      <div>
        <h1 class="page-title">设置</h1>
        <p class="page-subtitle">编辑完成后自动保存。迁移目录与安装更新仍需单独确认。</p>
      </div>
      <span class="subtle" role="status" aria-live="polite">{{
        editor.saving
          ? '正在自动保存…'
          : saveErrors.length
            ? '部分设置未保存'
            : editor.dirty
              ? appUpdateBusy
                ? '更新结束后自动保存'
                : '等待自动保存…'
              : '所有设置已保存'
      }}</span>
    </header>
    <div v-if="saveErrors.length" class="callout warning" role="alert" style="margin-bottom: 12px">
      <p v-for="message in saveErrors" :key="message">{{ message }}</p>
      <p>未保存的编辑会保留；请补全或修正后自动保存，网络或文件错误可重试。</p>
      <Button size="sm" :disabled="editor.saving || appUpdateBusy" @click="editor.retry"
        >重试自动保存</Button
      >
    </div>
    <div class="settings-layout">
      <nav class="panel settings-nav">
        <button :class="{ active: section === 'network' }" @click="section = 'network'">
          网络代理
        </button>
        <button :class="{ active: section === 'storage' }" @click="section = 'storage'">存储</button
        ><button :class="{ active: section === 'sites' }" @click="section = 'sites'">
          网站来源</button
        ><button :class="{ active: section === 'directories' }" @click="section = 'directories'">
          工具目录</button
        ><button :class="{ active: section === 'updates' }" @click="section = 'updates'">
          Skill 更新</button
        ><button :class="{ active: section === 'app' }" @click="section = 'app'">应用更新</button
        ><button :class="{ active: section === 'general' }" @click="section = 'general'">
          常规与外观
        </button>
      </nav>
      <fieldset
        class="panel settings-section"
        :disabled="appUpdateBusy && appUpdatePhase !== 'checking'"
      >
        <AgentDirectories v-if="section === 'directories'" v-model="agentProfiles" />
        <template v-else-if="section === 'network'">
          <div class="panel-header" style="padding: 0 0 13px">
            <h3 class="panel-title">网络代理</h3>
          </div>
          <p class="choice-meta">
            用于 Git 导入、网站检索、Skill
            下载和来源更新。自动保存后新请求立即生效，不修改系统代理。
          </p>
          <label class="field"
            ><span class="field-label">连接方式</span>
            <AppSelect
              v-model="proxyMode"
              :disabled="proxyTesting"
              :options="[
                { value: 'system', label: '跟随系统代理' },
                { value: 'direct', label: '直连' },
                { value: 'manual', label: '自定义代理' },
              ]"
            />
          </label>
          <label v-if="proxyMode === 'manual'" class="field" style="margin-top: 16px"
            ><span class="field-label">代理地址</span>
            <input
              v-model="proxyUrl"
              :disabled="proxyTesting"
              class="input"
              placeholder="http://127.0.0.1:7897"
            />
            <span class="choice-meta">支持 HTTP / HTTPS 代理；暂不支持带账号密码的地址。</span>
          </label>
          <p v-if="proxyMode === 'system'" class="choice-meta">
            读取系统已启用的 HTTP/HTTPS 代理及绕过列表，不依赖终端环境。未启用时直连；PAC
            自动代理暂不支持，可改用自定义代理。
          </p>
          <Button style="margin-top: 16px" :disabled="proxyTesting" @click="testProxy">{{
            proxyTesting ? '正在测试 HTTP 与 Git…' : '测试连接'
          }}</Button>
          <p class="choice-meta">测试当前填写的配置；分别检查 GitHub 的 HTTP 和 Git 连接。</p>
          <p v-if="proxyMessage" :class="proxyError ? 'field-error' : 'callout'" role="status">
            {{ proxyMessage }}
          </p>
        </template>
        <template v-else-if="section === 'storage'"
          ><div class="panel-header" style="padding: 0 0 13px">
            <h3 class="panel-title">
              <HardDrive style="width: 16px; display: inline; vertical-align: -3px" /> 统一存储
            </h3>
          </div>
          <div class="settings-row">
            <div>
              <SingleContentSettings />
              <h4>当前统一目录</h4>
              <p class="mono">{{ libraryDirectory(app.snapshot) || '尚未配置' }}</p>
              <p
                class="field-hint"
                style="margin-top: 4px; font-size: 13px; color: var(--text-muted)"
              >
                每个 Skill 按名称直接浏览，所有工具共用当前内容；内部存储与恢复记录由应用管理。
              </p>
            </div>
            <div style="display: flex; gap: 8px; align-items: center; flex-wrap: wrap">
              <Button
                v-if="app.snapshot?.storageRoot"
                variant="ghost"
                title="在访达或文件管理器中打开直观的技能目录"
                @click="openSkillsFolder"
              >
                <FolderOpen />打开技能目录
              </Button>
              <Button @click="startMigrate"><FolderOpen />修改位置</Button>
            </div>
          </div>
          <div class="settings-row">
            <div>
              <h4>存储概况</h4>
              <p>
                {{ app.snapshot?.skills.length || 0 }} 个 Skill ·
                {{ app.snapshot?.sources.length || 0 }} 个来源
              </p>
            </div>
          </div>
          <div class="callout">
            修改目录会进入迁移预览。迁移会复制并校验内容，再更新已分发的软链。迁移成功后自动清理旧目录。
          </div></template
        >
        <template v-else-if="section === 'sites'"
          ><h3 class="panel-title">网站来源</h3>
          <p class="page-subtitle">
            为每个网站填写名称和网址，最多添加 8 个。名称将显示在搜索页面。
          </p>
          <div class="catalog-sites-editor">
            <div v-for="(site, index) in catalogSites" :key="index" class="catalog-site-row">
              <label class="field">
                <span class="field-label">网站名称</span>
                <input
                  v-model="site.name"
                  class="input"
                  :aria-label="`网站 ${index + 1} 名称`"
                  maxlength="100"
                  placeholder="例如：团队 Skills"
                />
              </label>
              <label class="field">
                <span class="field-label">网址</span>
                <input
                  v-model="site.url"
                  class="input"
                  type="url"
                  :aria-label="`网站 ${index + 1} 网址`"
                  placeholder="https://example.com"
                  spellcheck="false"
                />
              </label>
              <Button
                variant="ghost"
                size="icon"
                :disabled="catalogSites.length <= 1"
                :aria-label="`删除网站 ${index + 1}`"
                @click="catalogSites.splice(index, 1)"
              >
                <Trash2 :size="16" />
              </Button>
            </div>
            <p v-if="siteError" class="catalog-site-error" role="alert">{{ siteError }}</p>
            <Button
              variant="secondary"
              :disabled="catalogSites.length >= 8"
              @click="catalogSites.push({ name: '', url: '' })"
            >
              <Plus :size="16" /> 添加网站
            </Button>
          </div>
          <div class="callout" style="margin-top: 14px">
            内置 ClawHub、SkillHub 和 Skills.sh。自定义站点需兼容 ClawHub HTTP API，也可通过公开 Git
            仓库导入。
          </div></template
        >
        <template v-else-if="section === 'updates'"
          ><div class="panel-header" style="padding: 0 0 13px">
            <h3 class="panel-title">
              <RefreshCcw style="width: 16px; display: inline; vertical-align: -3px" /> 默认 Skill
              更新策略
            </h3>
          </div>
          <div class="settings-row">
            <div>
              <h4>默认策略</h4>
              <p>新来源使用此策略；已有来源可在更新中心单独调整。</p>
            </div>
            <AppSelect
              v-model="updateMode"
              style="width: 170px"
              aria-label="默认策略"
              :options="updateModeOptions"
            />
          </div>
          <div class="settings-row">
            <div>
              <h4>默认时区</h4>
              <p>计划时间按本机时区显示。</p>
            </div>
            <Badge>{{ localTimezone }}</Badge>
          </div>
          <div class="callout">
            检查间隔可在更新中心按来源设置。完全退出应用后，定时检查停止。
          </div></template
        >
        <AppUpdatePanel
          v-else-if="section === 'app'"
          :settings-pending="editor.dirty || editor.saving"
          v-model:endpoint="endpoint"
          v-model:public-key="publicKey"
        />
        <template v-else
          ><div class="panel-header" style="padding: 0 0 13px">
            <h3 class="panel-title">
              <Palette style="width: 16px; display: inline; vertical-align: -3px" /> 常规与外观
            </h3>
          </div>
          <div class="settings-row">
            <div>
              <h4>关闭窗口后继续运行</h4>
              <p>保留托盘入口和定时检查；完全退出会停止调度。</p>
            </div>
            <button
              class="switch"
              :class="{ on: closeToTray }"
              :aria-pressed="closeToTray"
              aria-label="关闭窗口后继续运行"
              @click="closeToTray = !closeToTray"
            />
          </div>
          <div class="settings-row">
            <div>
              <h4>主题</h4>
              <p>选择浅色、深色或跟随系统。</p>
            </div>
            <AppSelect
              v-model="theme"
              aria-label="主题"
              style="width: 150px"
              :options="themeOptions"
            />
          </div>
          <div class="settings-row">
            <div>
              <h4>开机启动</h4>
              <p>登录系统后自动运行 SkillDock。</p>
            </div>
            <button
              class="switch"
              :class="{ on: autoStart }"
              :aria-pressed="autoStart"
              :disabled="autoStartBusy"
              aria-label="开机启动"
              @click="toggleAutoStart"
            />
          </div>
          <div class="callout warning">
            关闭窗口后可从状态栏或托盘重新打开。菜单中的“完全退出”会结束后台检查。
          </div></template
        >
      </fieldset>
    </div>
    <AppDialog
      v-model:open="migrateOpen"
      title="迁移统一存储目录"
      description="先预览影响，再确认执行。"
      ><label class="field"
        ><span class="field-label">新目录</span><DirectoryField v-model="newPath"
      /></label>
      <div class="list-stack" style="margin-top: 14px">
        <div class="list-row">
          <div class="list-row-main">
            <div class="list-row-title">迁移中央库内容</div>
            <div class="list-row-meta">
              {{ app.snapshot?.skills.length || 0 }} 个 Skill 将复制并校验
            </div>
          </div>
          <Badge tone="blue">第 1 阶段</Badge>
        </div>
        <div class="list-row">
          <div class="list-row-main">
            <div class="list-row-title">更新目标软链</div>
            <div class="list-row-meta">
              {{ app.snapshot?.bindings.length || 0 }} 条关系可能受影响，失败会生成待恢复任务
            </div>
          </div>
          <Badge tone="amber">第 2 阶段</Badge>
        </div>
      </div>
      <template #footer
        ><Button @click="migrateOpen = false">取消</Button
        ><Button
          variant="primary"
          :disabled="!newPath.trim() || newPath === libraryDirectory(app.snapshot)"
          @click="migrateConfirm = true"
          >继续确认</Button
        ></template
      ></AppDialog
    >
    <ConfirmDialog
      v-model:open="migrateConfirm"
      title="确认迁移存储"
      :description="`将中央库迁移到 ${newPath}，并更新 ${app.snapshot?.bindings.length || 0} 条软链关系。成功后自动删除旧目录及其内容；清理失败可从任务记录重试。`"
      confirm-text="开始迁移"
      :busy="busy"
      @confirm="migrate"
    />
  </div>
</template>

<style scoped>
.settings-section {
  min-width: 0;
  margin: 0;
}
.catalog-sites-editor {
  display: grid;
  gap: 16px;
  margin-top: 24px;
  justify-items: start;
}
.catalog-site-row {
  display: grid;
  grid-template-columns: minmax(140px, 1fr) minmax(240px, 2fr) 32px;
  gap: 12px;
  align-items: end;
  width: 100%;
}
.catalog-site-row .field {
  min-width: 0;
  margin: 0;
}
.catalog-site-row .input {
  width: 100%;
}
.catalog-site-error {
  color: var(--danger, #dc2626);
  font-size: 13px;
}
@media (max-width: 760px) {
  .catalog-site-row {
    grid-template-columns: minmax(0, 1fr) 32px;
  }
  .catalog-site-row .field:first-child {
    grid-column: 1 / -1;
  }
}
</style>
