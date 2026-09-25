import assert from 'node:assert/strict'
import { test } from 'node:test'
import {
  catalogInstallErrors,
  createCatalogInstallSession,
  resolveCatalogSkillIds,
  runCatalogInstall,
} from '../src/services/catalogInstall.ts'
import { presetMembershipInput, presetSyncErrors } from '../src/services/presetMembership.ts'
import type { CatalogItem, Skill, Snapshot, Source } from '../src/services/types.ts'

const item: CatalogItem = {
  slug: 'workflow',
  name: 'Workflow',
  description: '',
  version: '1',
  site: 'https://skillhub.cn',
}
const skill = (id: string, sourceId: string): Skill => ({
  id,
  sourceId,
  name: id,
  description: '',
  bundleDigest: id,
  relativePath: id,
  version: '1',
  installedAt: '',
})
const source = (id: string, url = 'https://api.skillhub.cn'): Source => ({
  id,
  url,
  name: id,
  path: '',
  scanSubdir: '',
  reference: item.slug,
  kind: 'catalog',
  version: '1',
  policy: { mode: 'off', intervalHours: 24 },
  lastChecked: '',
  nextCheck: '',
  status: 'ready',
  error: '',
})
function fixture(): Snapshot {
  return {
    initialized: true,
    storageRoot: '/library',
    revision: 1,
    schemaVersion: 3,
    contentBackups: [],
    libraryEntries: {},
    unmanagedTargetPaths: [],
    skills: [skill('canonical', 'git-source'), skill('member-two', 'catalog-source')],
    sources: [source('catalog-source')],
    skillOrigins: { canonical: ['git-source', 'catalog-source'] },
    targets: ['one', 'two'].map((id) => ({
      id,
      name: id,
      tool: id,
      scope: 'user',
      path: `/tools/${id}`,
    })),
    presets: [
      {
        id: 'preset',
        name: 'My preset',
        description: 'Kept',
        skillIds: [],
        revision: 1,
        locks: {},
      },
    ],
    presetApplications: [],
    packages: [],
    presetPackages: [
      {
        presetId: 'preset',
        packageId: 'pkg',
        autoAdd: true,
        excludedIds: ['excluded'],
        selectedIds: [],
      },
    ],
    externalInstallations: [],
    bindings: [],
    tasks: [],
    settings: {
      networkProxy: { mode: 'system', url: '' },
      backupRetention: 3,
      agentProfiles: [],
      catalogSites: [],
      closeToTray: false,
      theme: 'light',
      updateMode: 'off',
      updateEndpoint: '',
      updatePublicKey: '',
    },
  }
}
function setup() {
  let snapshot = fixture()
  const calls: string[] = []
  const operations = {
    snapshot: () => snapshot,
    updateSnapshot: (next: Snapshot) => {
      snapshot = next
    },
    installCatalog: async () => {
      calls.push('install')
      return structuredClone(snapshot)
    },
    savePreset: async (input: ReturnType<typeof presetMembershipInput>) => {
      calls.push('preset')
      assert.equal(input.syncApplied, true)
      assert.deepEqual(input.packages, snapshot.presetPackages)
      const next = structuredClone(snapshot)
      Object.assign(next.presets[0]!, {
        skillIds: input.skillIds,
        revision: next.presets[0]!.revision + 1,
      })
      next.revision++
      return next
    },
    retryPresetSync: async () => {
      calls.push('sync')
      const next = structuredClone(snapshot)
      for (const application of next.presetApplications) {
        application.error = ''
        application.appliedRevision = next.presets[0]!.revision
      }
      return next
    },
    plan: async (ids: string[], targets: string[]) => {
      calls.push(`plan:${targets[0]}`)
      assert.deepEqual(ids, ['canonical', 'member-two'])
      return {
        revision: snapshot.revision,
        items: ids.map((skillId) => ({
          skillId,
          targetId: targets[0]!,
          path: `/tools/${targets[0]}/${skillId}`,
          action: 'create',
          error: '',
        })),
      }
    },
    distribute: async (ids: string[], targets: string[], revision: number) => {
      calls.push(`distribute:${targets[0]}`)
      assert.deepEqual(ids, ['canonical', 'member-two'])
      assert.equal(revision, snapshot.revision)
      const next = structuredClone(snapshot)
      next.revision++
      return next
    },
  }
  return { operations, calls }
}

test('catalog source resolution includes merged canonical IDs and all package members', () => {
  const state = fixture()
  state.sources.push(source('unrelated', 'https://clawhub.ai'))
  state.skills.push(skill('different-site', 'unrelated'))
  assert.deepEqual(resolveCatalogSkillIds(state, item), ['canonical', 'member-two'])
  state.skillOrigins.canonical!.push('catalog-source')
  assert.deepEqual(resolveCatalogSkillIds(state, item), ['canonical', 'member-two'])
})

test('slug fallback is confined to the demo and never selects a native record by name', () => {
  const state = fixture()
  state.sources = []
  state.skills.push(skill('workflow', 'other-source'))
  assert.deepEqual(resolveCatalogSkillIds(state, item), [])
  assert.deepEqual(resolveCatalogSkillIds(state, item, true), ['workflow'])
})

test('a failed preset stays visible after successful target distribution', async () => {
  const { operations, calls } = setup()
  const originalSave = operations.savePreset
  operations.savePreset = async () => {
    calls.push('preset-failure')
    throw new Error('保存失败')
  }
  const session = createCatalogInstallSession(item, ['preset'], ['one'], operations.snapshot())
  assert.equal(await runCatalogInstall(session, operations), false)
  assert.deepEqual(calls, ['install', 'preset-failure', 'plan:one', 'distribute:one'])
  assert.equal(session.steps.find((step) => step.kind === 'target')!.status, 'succeeded')
  assert.match(catalogInstallErrors(session).join(''), /保存失败/)
  operations.savePreset = originalSave
  assert.equal(await runCatalogInstall(session, operations), true)
  assert.deepEqual(calls, ['install', 'preset-failure', 'plan:one', 'distribute:one', 'preset'])
  assert.deepEqual(operations.snapshot().presets[0]!.skillIds, ['canonical', 'member-two'])
})

test('successful import is not repeated when canonical member resolution needs retry', async () => {
  const { operations, calls } = setup()
  const original = structuredClone(operations.snapshot())
  operations.snapshot().skills = []
  const session = createCatalogInstallSession(item, [], ['one'], operations.snapshot())
  assert.equal(await runCatalogInstall(session, operations), false)
  assert.deepEqual(calls, ['install'])
  assert.equal(session.steps[2]!.status, 'pending')
  operations.updateSnapshot(original)
  assert.equal(await runCatalogInstall(session, operations), true)
  assert.deepEqual(calls, ['install', 'plan:one', 'distribute:one'])
})

test('retry after one target conflict only distributes to the unfinished target', async () => {
  const { operations, calls } = setup()
  const originalPlan = operations.plan
  operations.plan = async (ids, targets) => {
    const plan = await originalPlan(ids, targets)
    if (targets[0] === 'two') plan.items[0]!.error = '名称被占用'
    return plan
  }
  const session = createCatalogInstallSession(item, [], ['one', 'two'], operations.snapshot())
  assert.equal(await runCatalogInstall(session, operations), false)
  operations.plan = originalPlan
  assert.equal(await runCatalogInstall(session, operations), true)
  assert.deepEqual(calls, [
    'install',
    'plan:one',
    'distribute:one',
    'plan:two',
    'plan:two',
    'distribute:two',
  ])
})

test('saved membership with a sync error retries synchronization without saving again', async () => {
  const { operations, calls } = setup()
  operations.snapshot().presetApplications = [
    { presetId: 'preset', targetId: 'one', follow: true, appliedRevision: 1, error: '存在冲突' },
  ]
  const session = createCatalogInstallSession(item, ['preset'], ['two'], operations.snapshot())
  assert.equal(await runCatalogInstall(session, operations), false)
  assert.equal(session.steps.find((step) => step.kind === 'preset')!.saved, true)
  assert.match(catalogInstallErrors(session).join(''), /成员已保存.*存在冲突/)
  assert.equal(await runCatalogInstall(session, operations), true)
  assert.deepEqual(calls, ['install', 'preset', 'plan:two', 'distribute:two', 'sync'])
})

test('membership addition deduplicates members and preserves package subscriptions', () => {
  const state = fixture()
  state.presets[0]!.skillIds = ['canonical']
  const input = presetMembershipInput(state, 'preset', ['canonical', 'member-two'])
  assert.equal(input.description, 'Kept')
  assert.equal(input.syncApplied, true)
  assert.deepEqual(input.skillIds, ['canonical', 'member-two'])
  assert.deepEqual(input.packages, state.presetPackages)
  assert.deepEqual(state.presets[0]!.skillIds, ['canonical'])
})

test('sync verification catches outdated follow targets and ignores deliberately pinned targets', () => {
  const state = fixture()
  state.presets[0]!.revision = 2
  state.presetApplications = [
    { presetId: 'preset', targetId: 'one', follow: true, appliedRevision: 1, error: '' },
    { presetId: 'preset', targetId: 'two', follow: false, appliedRevision: 1, error: '旧错误' },
  ]
  assert.deepEqual(presetSyncErrors(state, 'preset'), ['one：目标尚未同步到当前预设'])
  assert.deepEqual(presetSyncErrors(state, 'removed'), ['预设已不存在'])
})

test('repeated submissions share a running guard', async () => {
  const { operations, calls } = setup()
  const session = createCatalogInstallSession(item, [], [], operations.snapshot())
  const first = runCatalogInstall(session, operations)
  assert.equal(await runCatalogInstall(session, operations), false)
  assert.equal(await first, true)
  assert.deepEqual(calls, ['install'])
})

test('retry does not restore preset members the user removed after a failed sync', async () => {
  const { operations, calls } = setup()
  operations.snapshot().presetApplications = [
    {
      presetId: 'preset',
      targetId: 'one',
      follow: true,
      appliedRevision: 1,
      error: '存在冲突',
    },
  ]
  const session = createCatalogInstallSession(item, ['preset'], [], operations.snapshot())
  assert.equal(await runCatalogInstall(session, operations), false)
  operations.snapshot().presets[0]!.skillIds = ['member-two']
  const before = structuredClone(operations.snapshot())
  assert.equal(await runCatalogInstall(session, operations), false)
  assert.deepEqual(calls, ['install', 'preset'])
  assert.deepEqual(operations.snapshot(), before)
  assert.match(catalogInstallErrors(session).join(''), /预设成员已被修改.*不会自动加回/)
})

test('retry revalidates an earlier successful preset without undoing the user edit', async () => {
  const { operations, calls } = setup()
  const originalPlan = operations.plan
  operations.plan = async (ids, targets) => {
    const plan = await originalPlan(ids, targets)
    plan.items[0]!.error = '分发失败'
    return plan
  }
  const session = createCatalogInstallSession(item, ['preset'], ['one'], operations.snapshot())
  assert.equal(await runCatalogInstall(session, operations), false)
  assert.equal(session.steps.find((step) => step.kind === 'preset')!.status, 'succeeded')
  operations.snapshot().presets[0]!.skillIds = ['member-two']
  operations.plan = originalPlan
  assert.equal(await runCatalogInstall(session, operations), false)
  assert.deepEqual(calls, ['install', 'preset', 'plan:one', 'plan:one', 'distribute:one'])
  assert.deepEqual(operations.snapshot().presets[0]!.skillIds, ['member-two'])
  assert.match(catalogInstallErrors(session).join(''), /预设成员已被修改/)
})

test('retry stops when imported content was removed instead of silently narrowing the package', async () => {
  const { operations, calls } = setup()
  operations.savePreset = async () => {
    calls.push('preset-failure')
    throw new Error('保存失败')
  }
  const session = createCatalogInstallSession(item, ['preset'], [], operations.snapshot())
  assert.equal(await runCatalogInstall(session, operations), false)
  operations.snapshot().skills = operations
    .snapshot()
    .skills.filter((skill) => skill.id !== 'canonical')
  assert.equal(await runCatalogInstall(session, operations), false)
  assert.deepEqual(calls, ['install', 'preset-failure'])
  assert.deepEqual(session.skillIds, ['canonical', 'member-two'])
  assert.match(catalogInstallErrors(session).join(''), /部分 Skill 已从库中移除/)
})

test('retry reports a deleted target instead of treating an empty plan as completion', async () => {
  const { operations, calls } = setup()
  operations.snapshot().targets = []
  const session = createCatalogInstallSession(item, [], ['one'], operations.snapshot())
  assert.equal(await runCatalogInstall(session, operations), false)
  assert.deepEqual(calls, ['install'])
  assert.match(catalogInstallErrors(session).join(''), /目标已被删除/)
})
