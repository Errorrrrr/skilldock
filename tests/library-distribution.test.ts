import assert from 'node:assert/strict'
import { test } from 'node:test'
import { computed } from 'vue'
import { createPinia, setActivePinia } from 'pinia'
import { librarySkills } from '../src/services/librarySkills'
import { useLibraryDistribution } from '../src/composables/useLibraryDistribution'
import { useAppStore } from '../src/stores/app'
import { api } from '../src/services/api'
import type { Snapshot, Skill, Binding } from '../src/services/types'

Object.assign(globalThis, { window: { setTimeout: () => 0, clearTimeout: () => {} } })
const skill = (id: string, name = 'review', externalPath?: string): Skill => ({
  id,
  name,
  externalPath,
  sourceId: `source-${id}`,
  bundleDigest: id,
  relativePath: name,
  description: name,
  version: id,
  installedAt: '2026-09-09',
})
const binding = (id: string, skillId: string, targetId: string, claims = ['manual']): Binding => ({
  id,
  skillId,
  targetId,
  claims,
  path: `/tools/${targetId}/review`,
  version: skillId,
  digest: skillId,
  relativePath: 'review',
  borrowed: false,
  follow: false,
})
function fixture(): Snapshot {
  return {
    initialized: true,
    storageRoot: '/library',
    revision: 1,
    schemaVersion: 1,
    contentBackups: [], skillOrigins: {}, libraryEntries: {},
    packages: [], presetPackages: [], presetApplications: [], externalInstallations: [],
    skills: [skill('b'), skill('a'), skill('c', 'other')],
    sources: [],
    presets: [],
    tasks: [],
    targets: ['one', 'two', 'three'].map((id) => ({
      id,
      name: id,
      tool: id,
      scope: 'user',
      path: `/tools/${id}`,
    })),
    bindings: [binding('b-one', 'b', 'one'), binding('a-two', 'a', 'two')],
    settings: {
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
  setActivePinia(createPinia())
  const app = useAppStore()
  app.snapshot = fixture()
  const rows = computed(() => librarySkills(app.snapshot))
  const calls: { action: string; ids: string[]; target?: string }[] = []
  api.snapshot = async () => structuredClone(JSON.parse(JSON.stringify(app.snapshot)))
  api.plan = async (ids, targets) => {
    calls.push({ action: 'plan', ids, target: targets[0] })
    return {
      revision: app.snapshot!.revision,
      items: ids.map((id) => ({
        skillId: id,
        targetId: targets[0]!,
        path: `/tools/${targets[0]}/${id}`,
        action: 'create',
        error: '',
      })),
    }
  }
  api.distribute = async (ids, targets, revision) => {
    assert.equal(revision, app.snapshot!.revision)
    calls.push({ action: 'distribute', ids, target: targets[0] })
    const next = await api.snapshot()
    next.bindings.push(...ids.map((id) => binding(`${id}-${targets[0]}`, id, targets[0]!)))
    next.revision++
    return next
  }
  api.revoke = async (ids) => {
    calls.push({ action: 'revoke', ids })
    const next = await api.snapshot()
    next.bindings = next.bindings.flatMap((b) => {
      if (!ids.includes(b.id)) return [b]
      b.claims = b.claims.filter((claim) => claim !== 'manual')
      return b.claims.length ? [b] : []
    })
    next.revision++
    return next
  }
  return { app, rows, calls, actions: useLibraryDistribution(rows) }
}
test('one row per name aggregates installations while retaining source records', () => {
  const state = fixture()
  const before = JSON.stringify(state)
  const rows = librarySkills(state)
  assert.equal(rows.length, 2)
  assert.equal(rows[0]!.id, 'a')
  assert.deepEqual(
    rows[0]!.tools.map((tool) => tool.active),
    [true, true, false],
  )
  assert.equal(JSON.stringify(state), before)
  state.skills.unshift(skill('0', 'review', '/external'))
  assert.equal(librarySkills(state)[0]!.id, 'a')
  assert.equal(librarySkills(null).length, 0)
})
test('toggle revokes the actual copy and redistributes from the library representative', async () => {
  const { app, actions, calls } = setup()
  await actions.toggle('a', 'one')
  assert.deepEqual(calls[0], { action: 'revoke', ids: ['b-one'] })
  await actions.toggle('a', 'one')
  assert.equal(app.snapshot!.bindings.find((b) => b.targetId === 'one')!.skillId, 'a')
})
test('batch distribution skips existing copies and submits fresh revisions per target', async () => {
  const { actions, calls } = setup()
  actions.openBatch(['a', 'c'], 'distribute')
  actions.batchTargetIds.value = ['one', 'two', 'three']
  await actions.submitBatch()
  assert.deepEqual(
    calls.filter((call) => call.action === 'distribute').map((call) => call.ids),
    [['c'], ['c'], ['a', 'c']],
  )
  assert.equal(actions.batchOpen.value, false)
})
test('batch revoke includes every copy but preserves preset claims and unselected tools', async () => {
  const { app, actions, calls, rows } = setup()
  app.snapshot!.bindings.push(binding('extra', 'b', 'two', ['manual', 'preset:p']))
  app.snapshot!.bindings.push(binding('protected', 'c', 'three', ['preset:p']))
  actions.openBatch(['a', 'c'], 'revoke')
  actions.batchTargetIds.value = ['one', 'two']
  await actions.submitBatch()
  assert.deepEqual(calls[0]!.ids.sort(), ['a-two', 'b-one', 'extra'])
  assert.deepEqual(
    app.snapshot!.bindings.map((b) => b.id),
    ['extra', 'protected'],
  )
  assert.equal(rows.value[0]!.tools[1]!.protected, true)
  await actions.toggle('a', 'two')
  assert.equal(calls.length, 1)
})
test('a conflict keeps the batch open and reports partial completion without duplicate retries', async () => {
  const { actions, calls } = setup()
  const originalPlan = api.plan
  api.plan = async (ids, targets) => {
    const plan = await originalPlan(ids, targets)
    if (targets[0] === 'three') plan.items[0]!.error = '目标已被占用'
    return plan
  }
  actions.openBatch(['c'], 'distribute')
  actions.batchTargetIds.value = ['one', 'three']
  await actions.submitBatch()
  assert.equal(actions.batchOpen.value, true)
  assert.match(actions.batchError.value, /已完成 1 项分发.*目标已被占用/)
  api.plan = originalPlan
  await actions.submitBatch()
  assert.deepEqual(
    calls.filter((call) => call.action === 'distribute').map((call) => call.target),
    ['one', 'three'],
  )
})
test('rapid repeated toggle does not submit duplicate operations', async () => {
  const { actions, calls } = setup()
  await Promise.all([actions.toggle('a', 'three'), actions.toggle('a', 'three')])
  assert.equal(calls.filter((call) => call.action === 'distribute').length, 1)
})

test('snapshot failures offer repair for the failing package and clear the error after repair', async () => {
  const { app, actions } = setup()
  api.distribute = async () => {
    throw new Error('统一库快照内容已变化；快照 c')
  }
  actions.openBatch(['a', 'c'], 'distribute')
  actions.batchTargetIds.value = ['three']
  await actions.submitBatch()
  assert.equal(actions.repairSkillId.value, 'c')
  assert.match(actions.operationError.value, /快照内容已变化/)
  api.previewSnapshotRefresh = async (id) => {
    assert.equal(id, 'c')
    return {
      revision: app.snapshot!.revision,
      contentDigest: 'new-content',
      skillCount: 1,
      path: '/library',
    }
  }
  api.refreshSnapshot = async (id, revision, digest) => {
    assert.equal(id, 'c')
    assert.equal(revision, app.snapshot!.revision)
    assert.equal(digest, 'new-content')
    return await api.snapshot()
  }
  await actions.previewRepair()
  assert.equal(actions.repairOpen.value, true)
  await actions.repair()
  assert.equal(actions.repairOpen.value, false)
  assert.equal(actions.operationError.value, '')
})

test('update actions distinguish detached sources, local references and configured sources', async () => {
  const { sourceUpdateState } = await import('../src/services/sourceUpdates')
  const base = {
    id: 's',
    name: 's',
    kind: 'local',
    path: '/source',
    url: '',
    scanSubdir: '',
    reference: '',
    version: '',
    policy: { mode: 'off' as const, intervalHours: 24 },
    lastChecked: '',
    nextCheck: '',
    status: 'detached',
    error: '',
  }
  const collected = sourceUpdateState(base)
  assert.equal(collected.action, '未关联原始来源')
  assert.equal(collected.needsSetup, false)
  assert.equal(collected.canCheck, false)
  assert.equal(sourceUpdateState({ ...base, kind: 'local_reference' }).action, '跟随本地内容')
  const local = sourceUpdateState({ ...base, status: 'temporary_failure' })
  assert.equal(local.action, '手动同步本地文件夹')
  assert.equal(local.canCheck, false)
  const remote = { ...base, kind: 'git', url: 'https://example.com/skills.git' }
  assert.equal(sourceUpdateState(remote).needsSetup, true)
  assert.equal(sourceUpdateState(remote).canCheck, false)
  assert.equal(sourceUpdateState({ ...remote, status: 'available' }).canCheck, true)
  assert.equal(sourceUpdateState({ ...remote, status: 'temporary_failure' }).canCheck, true)
  assert.equal(sourceUpdateState({ ...remote, status: 'available', url: '' }).canCheck, false)
  assert.equal(
    sourceUpdateState({ ...remote, status: 'available', updatesRemoved: true }).canCheck,
    false,
  )
})

test('directory labels identify tools for legacy names without changing targets', async () => {
  const { targetDisplayName } = await import('../src/services/agentProfiles')
  const target = {
    id: 'legacy',
    name: 'skills',
    tool: 'custom',
    scope: 'user',
    path: '/Users/demo/.codex/skills',
  }
  assert.equal(targetDisplayName(target), 'Codex · Skills')
  assert.equal(
    targetDisplayName({ ...target, path: 'C:\\Users\\demo\\.cursor\\skills\\' }),
    'Cursor · Skills',
  )
  assert.equal(
    targetDisplayName({ ...target, path: '/Users/demo/.agents/skills' }),
    '通用 Agent Skills · Skills',
  )
  assert.equal(targetDisplayName({ ...target, path: '/workspace/team/skills' }), 'team · Skills')
  assert.equal(
    targetDisplayName({ ...target, tool: 'Codex', name: 'Codex · 用户级' }),
    'Codex · 用户级',
  )
  assert.equal(
    targetDisplayName({ ...target, tool: 'Cursor', name: '工作项目' }),
    'Cursor · 工作项目',
  )
  assert.equal(
    targetDisplayName(target, [
      { id: 'custom-agent', name: '团队工具', userPaths: [target.path], projectPaths: [] },
    ]),
    '团队工具 · Skills',
  )
  const snapshot = fixture()
  snapshot.targets = [target]
  assert.equal(librarySkills(snapshot)[0]!.tools[0]!.name, 'Codex · Skills')
  assert.equal(target.name, 'skills')
})

test('update source labels identify local tool directories and preserve remote names', async () => {
  const { sourceDisplayName } = await import('../src/services/agentProfiles')
  const source = {
    id: 'source',
    name: 'skills',
    kind: 'local',
    path: '/Users/demo/.codex/skills',
    url: '',
    scanSubdir: '',
    reference: '',
    version: '',
    policy: { mode: 'off' as const, intervalHours: 24 },
    lastChecked: '',
    nextCheck: '',
    status: 'detached',
    error: '',
  }
  assert.equal(sourceDisplayName(source), 'Codex · Skills')
  assert.equal(
    sourceDisplayName({ ...source, kind: 'folder', path: '/Users/demo/.claude/skills' }),
    'Claude Code · Skills',
  )
  assert.equal(
    sourceDisplayName({ ...source, kind: 'local_reference', path: '/Users/demo/.agents/skills' }),
    '通用 Agent Skills · Skills',
  )
  assert.equal(
    sourceDisplayName({ ...source, kind: 'git', url: 'https://example.com/skills.git' }),
    'skills',
  )
  assert.equal(sourceDisplayName({ ...source, kind: 'catalog' }), 'skills')
  assert.equal(sourceDisplayName(null), '')
  const { app } = setup()
  assert.equal(app.sourceName(source), 'Codex · Skills')
  assert.equal(source.name, 'skills')
})
