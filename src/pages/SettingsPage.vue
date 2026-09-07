<script setup lang="ts">
import AppSelect from '@/components/ui/AppSelect.vue'
import { ref, watch, onMounted } from 'vue'
import {
  HardDrive,
  Globe2,
  RefreshCcw,
  Palette,
  AppWindow,
  FolderOpen,
  Download,
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
const section = ref<'storage' | 'sites' | 'updates' | 'app' | 'general'>('storage')
const migrateOpen = ref(false)
const migrateConfirm = ref(false)
const newPath = ref('')
const busy = ref(false)
const appUpdate = ref<{
  configured: boolean
  available: boolean
  version?: string
  message: string
} | null>(null)
const catalogSites = ref('clawhub\nskillhub\nskills.sh')
const autoStart = ref(false)
const restartConfirm = ref(false)
const localTimezone = Intl.DateTimeFormat().resolvedOptions().timeZone
const closeToTray = ref(true)
const theme = ref('light')
const updateMode = ref('off')
const endpoint = ref('')
const publicKey = ref('')
watch(
  () => app.snapshot?.settings,
  (s) => {
    if (!s) return
    catalogSites.value = (s.catalogSites || ['clawhub', 'skillhub', 'skills.sh']).join('\n')
    closeToTray.value = s.closeToTray
    theme.value = s.theme
    updateMode.value = s.updateMode
    endpoint.value = s.updateEndpoint
    publicKey.value = s.updatePublicKey
  },
  { immediate: true },
)
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
  if (!app.isNative) {
    autoStart.value = !autoStart.value
    return
  }
  try {
    const { enable, disable, isEnabled } = await import('@tauri-apps/plugin-autostart')
    if (autoStart.value) await disable()
    else await enable()
    autoStart.value = await isEnabled()
  } catch (e) {
    app.error = String(e)
  }
}
async function saveSettings() {
  await app.mutate(
    () =>
      api.settings({
        catalogSites: catalogSites.value
          .split('\n')
          .map((s) => s.trim())
          .filter(Boolean),
        closeToTray: closeToTray.value,
        theme: theme.value,
        updateMode: updateMode.value,
        updateEndpoint: endpoint.value,
        updatePublicKey: publicKey.value,
      }),
    '设置已保存',
  )
}
function startMigrate() {
  newPath.value = app.snapshot?.storageRoot || ''
  migrateOpen.value = true
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
async function checkApp() {
  try {
    appUpdate.value = await api.checkAppUpdate()
  } catch (e) {
    app.error = e instanceof Error ? e.message : '无法检查应用更新'
  }
}
async function installApp() {
  if (!app.isNative) {
    app.error = '浏览器演示无法安装桌面应用更新，请在 Tauri 原生环境中执行'
    return
  }
  try {
    appUpdate.value = await api.installAppUpdate()
  } catch (e) {
    app.error = e instanceof Error ? e.message : '安装应用更新失败'
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
        <p class="page-subtitle">配置存储、来源、更新与桌面应用行为。</p>
      </div>
      <Button variant="primary" @click="saveSettings">保存设置</Button>
    </header>
    <div class="settings-layout">
      <nav class="panel settings-nav">
        <button :class="{ active: section === 'storage' }" @click="section = 'storage'">存储</button
        ><button :class="{ active: section === 'sites' }" @click="section = 'sites'">
          网站来源</button
        ><button :class="{ active: section === 'updates' }" @click="section = 'updates'">
          Skill 更新</button
        ><button :class="{ active: section === 'app' }" @click="section = 'app'">应用更新</button
        ><button :class="{ active: section === 'general' }" @click="section = 'general'">
          常规与外观
        </button>
      </nav>
      <section class="panel settings-section">
        <template v-if="section === 'storage'"
          ><div class="panel-header" style="padding: 0 0 13px">
            <h3 class="panel-title">
              <HardDrive style="width: 16px; display: inline; vertical-align: -3px" /> 统一存储
            </h3>
          </div>
          <div class="settings-row">
            <div>
              <h4>当前统一目录</h4>
              <p class="mono">{{ app.snapshot?.storageRoot || '尚未配置' }}</p>
            </div>
            <Button @click="startMigrate"><FolderOpen />修改位置</Button>
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
            修改目录会进入迁移预览。迁移会复制并校验内容，再更新已分发的软链。旧库保留供核验。
          </div></template
        >
        <template v-else-if="section === 'sites'"
          ><h3 class="panel-title">网站来源</h3>
          <p class="page-subtitle">配置可搜索的 Skill 目录，一行一个站点，最多 8 个。</p>
          <label class="field" style="margin-top: 20px"
            ><span class="field-label">搜索站点</span
            ><textarea
              v-model="catalogSites"
              class="textarea"
              rows="6"
              placeholder="clawhub&#10;https://your-catalog.example"
            />
          </label>
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
        <template v-else-if="section === 'app'"
          ><div class="panel-header" style="padding: 0 0 13px">
            <h3 class="panel-title">
              <AppWindow style="width: 16px; display: inline; vertical-align: -3px" /> 应用更新
            </h3>
          </div>
          <div class="settings-row">
            <div>
              <h4>SkillDock 0.1.0</h4>
              <p>应用升级与 Skill 内容更新相互独立。</p>
            </div>
            <Button @click="checkApp"><RefreshCcw />检查更新</Button>
          </div>
          <div v-if="appUpdate" class="callout" :class="{ warning: !appUpdate.configured }">
            {{ appUpdate.message }}
            <Button
              v-if="appUpdate.available"
              size="sm"
              variant="primary"
              style="margin-left: 8px"
              @click="restartConfirm = true"
              ><Download />安装并重启 {{ appUpdate.version }}</Button
            >
          </div>
          <div class="settings-row">
            <div>
              <h4>自定义更新端点</h4>
              <p>配置签名升级服务的 HTTPS 地址与验证公钥。留空时不启用应用升级。</p>
            </div>
          </div>
          <div class="form-grid">
            <label class="field"
              ><span class="field-label">更新端点</span
              ><input v-model="endpoint" class="input" placeholder="https://..." /></label
            ><label class="field"
              ><span class="field-label">更新公钥</span
              ><input v-model="publicKey" class="input" placeholder="发布服务提供的签名公钥"
            /></label></div
        ></template>
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
              aria-label="开机启动"
              @click="toggleAutoStart"
            />
          </div>
          <div class="callout warning">
            关闭窗口后可从状态栏或托盘重新打开。菜单中的“完全退出”会结束后台检查。
          </div></template
        >
      </section>
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
          :disabled="!newPath.trim() || newPath === app.snapshot?.storageRoot"
          @click="migrateConfirm = true"
          >继续确认</Button
        ></template
      ></AppDialog
    >
    <ConfirmDialog
      v-model:open="migrateConfirm"
      title="确认迁移存储"
      :description="`将中央库迁移到 ${newPath}，并更新 ${app.snapshot?.bindings.length || 0} 条软链关系。部分失败会保留可恢复任务。`"
      confirm-text="开始迁移"
      :busy="busy"
      @confirm="migrate"
    />
    <ConfirmDialog
      v-model:open="restartConfirm"
      title="安装应用更新并重启"
      description="请保存当前编辑。安装完成后 SkillDock 将重新启动。"
      confirm-text="安装并重启"
      @confirm="installApp"
    />
  </div>
</template>
