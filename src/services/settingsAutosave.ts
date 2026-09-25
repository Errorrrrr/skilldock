import { computed, reactive, ref, watch } from 'vue'
import type { Settings } from './types'
import { catalogSiteKey } from './catalogSites'

const groups = {
  network: ['networkProxy'],
  sites: ['catalogSites'],
  directories: ['agentProfiles'],
  app: ['updateEndpoint', 'updatePublicKey'],
  theme: ['theme'],
  tray: ['closeToTray'],
  updates: ['updateMode'],
} satisfies Record<string, Array<keyof Settings>>
export type SettingsGroup = keyof typeof groups
const clone = <T>(value: T): T => JSON.parse(JSON.stringify(value))
const equal = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b)
function pick(settings: Settings, group: SettingsGroup): Partial<Settings> {
  return Object.fromEntries(groups[group].map((key) => [key, clone(settings[key])]))
}
export function validateSettingsPatch(input: Partial<Settings>): Partial<Settings> {
  const patch = clone(input)
  if (patch.networkProxy) {
    const proxy = patch.networkProxy
    proxy.url = proxy.url.trim()
    if (proxy.mode === 'manual') {
      let url: URL
      try {
        url = new URL(proxy.url)
      } catch {
        throw new Error('请填写完整的 HTTP / HTTPS 代理地址')
      }
      if (
        !['http:', 'https:'].includes(url.protocol) ||
        !url.hostname ||
        url.username ||
        url.password ||
        url.search ||
        url.hash ||
        (url.pathname && url.pathname !== '/')
      )
        throw new Error('代理地址需使用 HTTP / HTTPS，且不能包含账号密码、路径或查询参数')
    }
  }
  if ('updateEndpoint' in patch) {
    patch.updateEndpoint = patch.updateEndpoint!.trim()
    patch.updatePublicKey = patch.updatePublicKey!.trim()
    if (!!patch.updateEndpoint !== !!patch.updatePublicKey)
      throw new Error('请同时填写更新地址和公钥，或同时清空以使用官方源')
    if (patch.updateEndpoint) {
      let url: URL
      try {
        url = new URL(patch.updateEndpoint)
      } catch {
        throw new Error('请填写完整的 HTTPS 更新地址')
      }
      if (url.protocol !== 'https:' || !url.hostname || url.username || url.password || url.hash)
        throw new Error('更新地址需使用 HTTPS，且不能包含账号密码或片段')
    }
  }
  for (const profile of patch.agentProfiles ?? []) {
    for (const paths of [profile.userPaths, profile.projectPaths]) {
      const normalized = paths.map((path) => path.trim().replace(/[\\/]+$/, ''))
      if (normalized.some((path) => !path) || new Set(normalized).size !== normalized.length)
        throw new Error(`${profile.name} 的目录不能为空或重复`)
    }
  }
  if (patch.catalogSites) {
    const urls = new Set<string>()
    if (!patch.catalogSites.length || patch.catalogSites.length > 8)
      throw new Error('请配置 1 至 8 个网站')
    for (const [index, site] of patch.catalogSites.entries()) {
      site.name = site.name.trim()
      site.url = site.url.trim()
      if (!site.name || site.name.length > 100)
        throw new Error(`第 ${index + 1} 个网站请填写名称（最多 100 个字符）`)
      let url: URL
      try {
        url = new URL(site.url)
      } catch {
        throw new Error(`第 ${index + 1} 个网站请填写完整的 HTTPS 网址`)
      }
      if (
        url.protocol !== 'https:' ||
        !url.hostname ||
        url.username ||
        url.password ||
        url.search ||
        url.hash
      )
        throw new Error(`第 ${index + 1} 个网站请填写完整的 HTTPS 网址，不含凭据、查询参数或片段`)
      site.url = url.href.replace(/\/+$/, '')
      const key = catalogSiteKey(site.url)
      if (urls.has(key)) throw new Error(`第 ${index + 1} 个网站的网址已存在`)
      urls.add(key)
    }
  }
  return patch
}

// Owned by an app-lifetime store: navigating away cannot cancel pending writes.
export function createSettingsAutosave(
  initial: Settings,
  options: {
    save: (patch: Partial<Settings>) => Promise<Settings>
    blocked: () => boolean
    delay?: number
  },
) {
  const draft = reactive(clone(initial))
  const saved = reactive(clone(initial))
  const errors = reactive<Partial<Record<SettingsGroup, string>>>({})
  const saving = ref(false)
  const keys = Object.keys(groups) as SettingsGroup[]
  const failed = new Map<SettingsGroup, string>()
  let timer: ReturnType<typeof setTimeout> | undefined
  let running: Promise<void> | undefined
  let inFlight: SettingsGroup | undefined
  const dirty = computed(() => keys.some((group) => !equal(pick(draft, group), pick(saved, group))))
  function receive(next: Settings) {
    for (const group of keys) {
      const clean = equal(pick(draft, group), pick(saved, group))
      Object.assign(saved, pick(next, group))
      if (clean && inFlight !== group) Object.assign(draft, pick(next, group))
    }
  }
  function schedule() {
    if (timer) clearTimeout(timer)
    timer = setTimeout(() => {
      void flush()
    }, options.delay ?? 800)
  }
  async function drain() {
    if (options.blocked()) return
    saving.value = true
    try {
      for (const group of keys) {
        if (options.blocked()) break
        const raw = pick(draft, group)
        if (equal(raw, pick(saved, group))) {
          delete errors[group]
          continue
        }
        if (failed.get(group) === JSON.stringify(raw)) continue
        try {
          const patch = validateSettingsPatch(raw)
          inFlight = group
          const next = await options.save(patch)
          // A response to older input must not replace edits typed during the request.
          const unchanged = equal(raw, pick(draft, group))
          receive(next)
          if (unchanged) Object.assign(draft, pick(next, group))
          failed.delete(group)
          delete errors[group]
        } catch (reason) {
          if (equal(raw, pick(draft, group))) {
            errors[group] = reason instanceof Error ? reason.message : String(reason)
            failed.set(group, JSON.stringify(raw))
          }
        } finally {
          inFlight = undefined
        }
      }
    } finally {
      saving.value = false
    }
  }
  async function flush() {
    if (timer) {
      clearTimeout(timer)
      timer = undefined
    }
    if (running) {
      await running
      return flush()
    }
    running = drain()
    try {
      await running
    } finally {
      running = undefined
    }
    if (
      dirty.value &&
      keys.some(
        (key) =>
          !equal(pick(draft, key), pick(saved, key)) &&
          failed.get(key) !== JSON.stringify(pick(draft, key)),
      ) &&
      !options.blocked()
    )
      schedule()
  }
  const stop = watch(
    draft,
    () => {
      for (const group of keys) {
        if (failed.get(group) !== JSON.stringify(pick(draft, group))) delete errors[group]
      }
      if (dirty.value) schedule()
    },
    { deep: true },
  )
  return {
    draft,
    errors,
    saving,
    dirty,
    receive,
    flush,
    retry() {
      failed.clear()
      schedule()
    },
    dispose() {
      stop()
      if (timer) clearTimeout(timer)
    },
  }
}
