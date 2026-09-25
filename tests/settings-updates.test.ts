import { test } from 'node:test'
import assert from 'node:assert/strict'
import { nextTick } from 'vue'
import { createSettingsAutosave } from '../src/services/settingsAutosave'
import { createAppUpdater } from '../src/services/appUpdater'
import type { Settings } from '../src/services/types'
import type { AppUpdateResult } from '../src/services/api'
const settings = (): Settings => ({
  networkProxy: { mode: 'system', url: '' },
  backupRetention: 0,
  agentProfiles: [],
  catalogSites: [{ name: 'ClawHub', url: 'https://clawhub.ai' }],
  closeToTray: true,
  theme: 'light',
  updateMode: 'off',
  updateEndpoint: '',
  updatePublicKey: '',
})
function deferred<T>() {
  let resolve!: (value: T) => void
  const promise = new Promise<T>((r) => {
    resolve = r
  })
  return { promise, resolve }
}

test('autosave sends only changed groups; incomplete site cannot block theme', async (t) => {
  let state = settings()
  const writes: Partial<Settings>[] = []
  const editor = createSettingsAutosave(state, {
    delay: 60_000,
    blocked: () => false,
    save: async (patch) => {
      writes.push(patch)
      state = { ...state, ...patch }
      return state
    },
  })
  t.after(editor.dispose)
  editor.draft.catalogSites.push({ name: '', url: '' })
  editor.draft.theme = 'dark'
  await nextTick()
  await editor.flush()
  assert.deepEqual(writes, [{ theme: 'dark' }])
  assert.match(editor.errors.sites!, /名称/)
  assert.equal(editor.draft.catalogSites.length, 2)
})
test('typing during a request and reverting to the old saved value survives response and polling', async (t) => {
  const first = deferred<Settings>()
  let writes = 0
  const editor = createSettingsAutosave(settings(), {
    delay: 60_000,
    blocked: () => false,
    save: async (patch) => {
      writes++
      return writes === 1 ? first.promise : { ...settings(), ...patch }
    },
  })
  t.after(editor.dispose)
  editor.draft.theme = 'dark'
  await nextTick()
  const saving = editor.flush()
  editor.draft.theme = 'light'
  await nextTick()
  editor.receive({ ...settings(), theme: 'dark' })
  first.resolve({ ...settings(), theme: 'dark' })
  await saving
  assert.equal(editor.draft.theme, 'light')
  await editor.flush()
  assert.equal(writes, 2)
  assert.equal(editor.dirty.value, false)
})
test('rapid edits are debounced and partial updater credentials never persist', async (t) => {
  const writes: Partial<Settings>[] = []
  let state = settings()
  const editor = createSettingsAutosave(state, {
    delay: 10,
    blocked: () => false,
    save: async (patch) => {
      writes.push(patch)
      state = { ...state, ...patch }
      return state
    },
  })
  t.after(editor.dispose)
  editor.draft.theme = 'dark'
  editor.draft.theme = 'system'
  editor.draft.updateEndpoint = 'https://example.com/latest.json'
  await new Promise((r) => setTimeout(r, 40))
  assert.deepEqual(writes, [{ theme: 'system' }])
  assert.match(editor.errors.app!, /同时/)
  editor.draft.updatePublicKey = 'public-key'
  await nextTick()
  await editor.flush()
  assert.deepEqual(writes[1], {
    updateEndpoint: 'https://example.com/latest.json',
    updatePublicKey: 'public-key',
  })
})
test('failed saves retain drafts, avoid retry storms, and retry explicitly', async (t) => {
  let fails = true
  let writes = 0
  const editor = createSettingsAutosave(settings(), {
    delay: 60_000,
    blocked: () => false,
    save: async (patch) => {
      writes++
      if (fails) throw new Error('目录不可写')
      return { ...settings(), ...patch }
    },
  })
  t.after(editor.dispose)
  editor.draft.theme = 'dark'
  await nextTick()
  await editor.flush()
  await editor.flush()
  assert.equal(writes, 1)
  assert.equal(editor.draft.theme, 'dark')
  assert.equal(editor.errors.theme, '目录不可写')
  fails = false
  editor.retry()
  await editor.flush()
  assert.equal(writes, 2)
  assert.equal(editor.dirty.value, false)
})
test('polling does not overwrite a dirty group and does update a clean one', async (t) => {
  const editor = createSettingsAutosave(settings(), {
    delay: 60_000,
    blocked: () => true,
    save: async () => {
      throw new Error('unexpected')
    },
  })
  t.after(editor.dispose)
  editor.draft.networkProxy = { mode: 'manual', url: 'http://127.0.0.1:7897' }
  editor.receive({ ...settings(), theme: 'dark' })
  await editor.flush()
  assert.equal(editor.draft.networkProxy.mode, 'manual')
  assert.equal(editor.draft.theme, 'dark')
  assert.equal(editor.dirty.value, true)
})
test('saving serializes groups and newer input; normalization does not loop', async (t) => {
  let active = 0
  let peak = 0
  let writes = 0
  let state = settings()
  const editor = createSettingsAutosave(state, {
    delay: 60_000,
    blocked: () => false,
    save: async (patch) => {
      peak = Math.max(peak, ++active)
      await Promise.resolve()
      active--
      writes++
      state = { ...state, ...patch }
      return state
    },
  })
  t.after(editor.dispose)
  editor.draft.catalogSites[0]!.name = '  Edited  '
  editor.draft.theme = 'dark'
  await nextTick()
  await Promise.all([editor.flush(), editor.flush()])
  await nextTick()
  await editor.flush()
  assert.equal(peak, 1)
  assert.equal(writes, 2)
  assert.equal(editor.draft.catalogSites[0]!.name, 'Edited')
  assert.equal(editor.dirty.value, false)
})
const available = (): AppUpdateResult => ({
  configured: true,
  available: true,
  currentVersion: '0.3.1',
  version: '0.3.2',
  endpoint: 'https://example.com/latest.json',
  custom: false,
  message: '发现新版本',
  token: 'token',
})
function updaterApi(check = async () => available()) {
  return {
    checkAppUpdate: check,
    appUpdateInfo: async () => ({ ...available(), available: false, token: undefined }),
    installAppUpdate: async () => ({}),
    onAppUpdateProgress: async () => () => {},
  }
}
test('automatic checks are throttled and opening the update page preserves release and token', async () => {
  let checks = 0
  const updater = createAppUpdater(
    updaterApi(async () => {
      checks++
      return available()
    }),
    '0.3.1',
  )
  await updater.automaticCheck()
  await updater.automaticCheck()
  await updater.loadInfo()
  assert.equal(checks, 1)
  assert.equal(updater.result.value?.token, 'token')
  assert.equal(updater.result.value?.available, true)
})
test('configuration changes discard stale in-flight responses', async () => {
  const pending = deferred<AppUpdateResult>()
  const updater = createAppUpdater(
    updaterApi(() => pending.promise),
    '0.3.1',
  )
  const checking = updater.check()
  updater.invalidate()
  pending.resolve(available())
  await checking
  assert.equal(updater.result.value, null)
  assert.equal(updater.busy.value, false)
})
test('network failure keeps notification but discards invalid native install token', async () => {
  let fail = false
  let installs = 0
  const api = updaterApi(async () => {
    if (fail) throw new Error('network')
    return available()
  })
  api.installAppUpdate = async () => {
    installs++
    return {}
  }
  const updater = createAppUpdater(api, '0.3.1')
  await updater.check()
  fail = true
  await updater.check()
  await updater.install()
  assert.equal(updater.result.value?.available, true)
  assert.equal(updater.result.value?.token, undefined)
  assert.equal(installs, 0)
})
test('expired install preview refreshes and requires a new confirmation', async (t) => {
  const realNow = Date.now
  let now = realNow()
  Date.now = () => now
  t.after(() => {
    Date.now = realNow
  })
  let checks = 0
  let installs = 0
  const api = updaterApi(async () => {
    checks++
    return available()
  })
  api.installAppUpdate = async () => {
    installs++
    return {}
  }
  const updater = createAppUpdater(api, '0.3.1')
  await updater.check()
  now += 26 * 60_000
  await updater.install()
  assert.equal(checks, 2)
  assert.equal(installs, 0)
  await updater.install()
  assert.equal(installs, 1)
})

test('automatic checks do not replace a release while installation confirmation is open', async () => {
  let checks = 0
  const updater = createAppUpdater(
    updaterApi(async () => {
      checks++
      return available()
    }),
    '0.3.1',
  )
  updater.confirming.value = true
  await updater.automaticCheck()
  assert.equal(checks, 0)
  updater.confirming.value = false
  await updater.automaticCheck()
  assert.equal(checks, 1)
})
