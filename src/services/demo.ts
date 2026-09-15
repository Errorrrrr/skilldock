import { sourceUpdateState } from './sourceUpdates'
import { cloneAgentProfiles } from './agentProfiles'
import { defaultCatalogSites, normalizeCatalogSites, catalogSiteKey } from './catalogSites'
import type {
  Binding,
  CatalogResult,
  DistributionPlan,
  Preset,
  ScanResult,
  Skill,
  Snapshot,
  Source,
  Target,
} from './types'

const DEMO_KEY = 'skilldock-demo-v4'
const now = () => new Date().toISOString()
const id = (prefix: string) =>
  `${prefix}-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 7)}`

const sources: Source[] = [
  {
    id: 'src-official',
    name: 'SkillDock Official',
    kind: 'git',
    path: '',
    url: 'https://github.com/skilldock-hq/official-skills',
    scanSubdir: '',
    reference: 'main',
    version: 'd81a7f2',
    policy: { mode: 'notify', intervalHours: 24 },
    lastChecked: '2026-09-07T02:20:00Z',
    nextCheck: '2026-09-08T02:20:00Z',
    status: 'healthy',
    error: '',
  },
  {
    id: 'src-team',
    name: 'Team Workflow Pack',
    kind: 'folder',
    path: '/Users/demo/Work/team-skills',
    url: '',
    scanSubdir: '',
    reference: '',
    version: 'local',
    policy: { mode: 'off', intervalHours: 12 },
    lastChecked: '2026-09-07T01:10:00Z',
    nextCheck: '',
    status: 'temporary_failure',
    error: '本地文件夹暂时离线，保留上次成功快照',
  },
  {
    id: 'src-community',
    name: 'Community Registry',
    kind: 'catalog',
    path: '',
    url: 'https://skills.example.dev',
    scanSubdir: '',
    reference: '',
    version: 'catalog',
    policy: { mode: 'off', intervalHours: 168 },
    lastChecked: '2026-09-06T09:10:00Z',
    nextCheck: '',
    status: 'auth_required',
    error: '访问令牌缺失，暂时无法检查更新',
  },
]

const skillSeed: Array<[string, string, string, string, string]> = [
  ['code-review', 'Code Review', '聚焦风险、回归与可维护性的代码审查流程', 'src-official', '2.4.1'],
  [
    'figma-to-code',
    'Figma to Code',
    '读取设计上下文并产出可维护的前端实现',
    'src-official',
    '1.9.0',
  ],
  ['release-notes', 'Release Notes', '从变更记录生成清晰的中文发布说明', 'src-team', '1.3.2'],
  ['api-testing', 'API Testing', '编排接口认证、边界和异常路径验证', 'src-team', '3.1.0'],
  ['git-commit', 'Git Commit', '生成范围明确、便于追溯的提交记录', 'src-official', '2.0.3'],
  ['incident-helper', 'Incident Helper', '线上故障信息收集与排查清单', 'src-team', '0.8.4'],
  ['docs-writer', 'Docs Writer', '技术文档结构化写作与交付检查', 'src-community', '1.2.0'],
  ['security-scan', 'Security Scan', '依赖风险与敏感信息检查流程', 'src-community', '1.7.1'],
  [
    'frontend-standards',
    'Frontend Standards',
    'Vue 与 TypeScript 工程约束和审查规则',
    'src-official',
    '4.0.0',
  ],
  ['research', 'Deep Research', '多来源检索、证据整理和结论输出', 'src-official', '2.8.0'],
  ['task-planner', 'Task Planner', '将复杂目标拆解为可执行工作计划', 'src-team', '1.5.3'],
  ['image-studio', 'Image Studio', '图像生成、编辑和视觉验收工作流', 'src-community', '0.9.2'],
]

const skills: Skill[] = skillSeed.map(([skillId, name, description, sourceId, version], index) => ({
  id: skillId,
  name,
  description,
  sourceId,
  version,
  bundleDigest: `sha256:${(index + 11).toString(16).repeat(8)}`,
  relativePath: `skills/${skillId}`,
  installedAt: new Date(Date.UTC(2026, 7, 20 + index)).toISOString(),
}))

const targets: Target[] = [
  {
    id: 'target-codex',
    name: 'Codex 用户目录',
    tool: 'Codex',
    scope: 'user',
    path: '/Users/demo/.codex/skills',
  },
  {
    id: 'target-claude',
    name: 'Claude Code',
    tool: 'Claude Code',
    scope: 'user',
    path: '/Users/demo/.claude/skills',
  },
  {
    id: 'target-cursor',
    name: 'Design Workspace',
    tool: 'Cursor',
    scope: 'project',
    path: '/Users/demo/Work/design/.cursor/skills',
  },
  {
    id: 'target-opencode',
    name: 'OpenCode 实验项目',
    tool: 'OpenCode',
    scope: 'project',
    path: '/Users/demo/Work/labs/.opencode/skills',
  },
]

const presets: Preset[] = [
  {
    id: 'preset-frontend',
    name: '前端交付',
    description: '设计还原、规范检查与发布说明',
    skillIds: ['figma-to-code', 'frontend-standards', 'code-review', 'release-notes'],
    revision: 3,
    locks: {},
  },
  {
    id: 'preset-quality',
    name: '质量守门',
    description: '接口、代码和安全检查的组合',
    skillIds: ['api-testing', 'code-review', 'security-scan'],
    revision: 2,
    locks: {},
  },
  {
    id: 'preset-research',
    name: '研究工作台',
    description: '研究、写作和任务规划',
    skillIds: ['research', 'docs-writer', 'task-planner'],
    revision: 1,
    locks: {},
  },
]

const bindingSeed: Array<[string, string, string[], boolean, string?]> = [
  [
    'code-review',
    'target-codex',
    ['manual', 'preset:preset-frontend', 'preset:preset-quality'],
    true,
  ],
  ['figma-to-code', 'target-codex', ['preset:preset-frontend'], true],
  ['frontend-standards', 'target-codex', ['preset:preset-frontend'], true],
  ['release-notes', 'target-codex', ['preset:preset-frontend'], true],
  ['api-testing', 'target-claude', ['preset:preset-quality'], true],
  ['code-review', 'target-claude', ['preset:preset-quality'], true],
  ['security-scan', 'target-claude', ['preset:preset-quality'], false],
  ['research', 'target-cursor', ['preset:preset-research'], true],
  ['docs-writer', 'target-cursor', ['preset:preset-research'], true],
  ['task-planner', 'target-cursor', ['preset:preset-research'], true],
  ['incident-helper', 'target-opencode', ['manual'], true, 'broken'],
]

const bindings: Binding[] = bindingSeed.map(([skillId, targetId, claims, follow, state], index) => {
  const skill = skills.find((item) => item.id === skillId)!
  const target = targets.find((item) => item.id === targetId)!
  return {
    id: `binding-${index + 1}`,
    skillId,
    targetId,
    path: state === 'broken' ? `${target.path}/${skillId}-missing` : `${target.path}/${skillId}`,
    version: skill.version,
    digest: skill.bundleDigest,
    relativePath: skill.relativePath,
    borrowed: false,
    claims,
    follow,
  }
})

function freshSnapshot(): Snapshot {
  return {
    initialized: true,
    schemaVersion: 1,
    packages: [],
    presetPackages: [],
    presetApplications: [],
    externalInstallations: [],
    storageRoot: '/Users/demo/SkillDock',
    revision: 7,
    skills: structuredClone(skills),
    sources: structuredClone(sources),
    targets: structuredClone(targets),
    bindings: structuredClone(bindings),
    presets: structuredClone(presets),
    tasks: [
      {
        id: 'task-3',
        kind: 'update',
        title: '检查 Official 来源',
        status: 'success',
        message: '发现 2 个可用更新，未自动应用',
        createdAt: '2026-09-07T02:20:00Z',
      },
      {
        id: 'task-2',
        kind: 'distribute',
        title: '应用「前端交付」',
        status: 'success',
        message: '4 个 Skill 已分发到 Codex 用户目录',
        createdAt: '2026-09-06T08:40:00Z',
      },
      {
        id: 'task-1',
        kind: 'migrate',
        title: '迁移统一存储目录',
        status: 'needsRecovery',
        message: '11 项成功，1 项链接待恢复',
        createdAt: '2026-09-05T04:12:00Z',
      },
    ],
    unmanagedTargetPaths: [],
    settings: {
      networkProxy: { mode: 'system', url: '' },
      backupRetention: 3,
      agentProfiles: cloneAgentProfiles(),
      catalogSites: normalizeCatalogSites(defaultCatalogSites),
      closeToTray: true,
      theme: 'light',
      updateMode: 'off',
      updateEndpoint: '',
      updatePublicKey: '',
    },
  }
}

let state = load()
function load(): Snapshot {
  try {
    const saved = JSON.parse(localStorage.getItem(DEMO_KEY) || '') as Snapshot
    saved.settings.agentProfiles = cloneAgentProfiles(saved.settings.agentProfiles)
    saved.settings.catalogSites = normalizeCatalogSites(
      saved.settings.catalogSites || defaultCatalogSites,
    )
    saved.packages ??= []
    saved.presetPackages ??= []
    saved.presetApplications ??= []
    saved.externalInstallations ??= []
    for (const source of saved.sources) {
      if (
        source.kind === 'git' &&
        saved.packages.some(
          (p) => !p.remote && p.scopes.some((scope) => scope.sourceId === source.id),
        )
      )
        source.kind = 'local'
      if (!sourceUpdateState(source).canCheck) {
        source.policy.mode = 'off'
        source.nextCheck = ''
      }
    }
    return saved
  } catch {
    return freshSnapshot()
  }
}
function save(next: Snapshot) {
  state = next
  localStorage.setItem(DEMO_KEY, JSON.stringify(next))
  return structuredClone(next)
}
function task(kind: string, title: string, message: string, status = 'success') {
  state.tasks.unshift({ id: id('task'), kind, title, status, message, createdAt: now() })
  state.revision += 1
}
function findSkill(skillId: string) {
  return state.skills.find((item) => item.id === skillId)
}

export async function demoSnapshot() {
  return structuredClone(state)
}
export async function demoConfigure(path: string) {
  state.initialized = true
  state.storageRoot = path
  task('configure', '配置统一存储目录', `已使用演示目录 ${path}`)
  return save(state)
}
export async function demoDiscover(): Promise<Target[]> {
  const candidates = state.settings.agentProfiles.flatMap((profile) =>
    profile.userPaths.map((path) => ({
      id: `discovered-${profile.id}-${path}`,
      name: profile.name,
      tool: profile.id,
      scope: 'user',
      path: path.replace('~/', '/Users/demo/'),
    })),
  )
  return candidates
}
export async function demoScan(path: string): Promise<ScanResult> {
  return {
    root: path,
    warnings: path.includes('restricted') ? ['一个子目录没有读取权限，已跳过'] : [],
    items: [
      {
        path: `${path}/code-review`,
        name: 'Code Review',
        description: '与中央库内容相同，可复用现有版本',
        status: 'same',
        error: '',
      },
      {
        path: `${path}/team-helper`,
        name: 'Team Helper',
        description: '可归集的新 Skill',
        status: 'new',
        error: '',
      },
      {
        path: `${path}/research`,
        name: 'Deep Research',
        description: '同名但内容不同，需要选择处理方式',
        status: 'conflict',
        error: '中央库已存在同名 Skill',
      },
      {
        path: `${path}/git-workflow`,
        name: 'Git Workflow',
        description: '指向：~/.local/share/skilldock/skills/git-workflow',
        status: 'linked',
        error:
          '外部已有软链（指向 ~/.local/share/skilldock/skills/git-workflow）。SkillDock 完整保留原样，不自动接管；如需归集请选择原始实体目录。',
      },
      {
        path: `${path}/legacy-link`,
        name: 'Legacy Link',
        description: '指向：~/.local/share/old-skills/legacy（已丢失）',
        status: 'broken',
        error: '软链目标不存在（指向 ~/.local/share/old-skills/legacy），链接已失效。',
      },
      {
        path: `${path}/builtin`,
        name: 'Tool Built-in',
        description: '工具自带内容，由外部管理',
        status: 'external',
        error: '',
      },
    ],
  }
}
export async function demoImportFolder(path: string, selectedPaths: string[], adopt: boolean) {
  for (const selected of selectedPaths) {
    const slug = selected.split('/').pop() || id('skill')
    if (!state.skills.some((item) => item.id === slug))
      state.skills.push({
        id: slug,
        name: slug === 'team-helper' ? 'Team Helper' : slug,
        description: '从本地文件夹归集的 Skill',
        sourceId: 'src-team',
        bundleDigest: `sha256:${id('digest')}`,
        relativePath: `skills/${slug}`,
        version: '1.0.0',
        installedAt: now(),
      })
  }
  task(
    'import',
    '归集已有 Skill',
    `${selectedPaths.length} 项已入库${adopt ? '并替换为软链' : '，原目录保持不变'}`,
  )
  return save(state)
}
export async function demoRemoveTarget(targetId: string, expectedRevision: number) {
  if (state.revision !== expectedRevision) throw new Error('资料库已变化，请重新确认')
  const target = state.targets.find((t) => t.id === targetId)
  if (!target) throw new Error('目标不存在')
  state.unmanagedTargetPaths ??= []
  if (!state.unmanagedTargetPaths.includes(target.path))
    state.unmanagedTargetPaths.push(target.path)
  state.targets = state.targets.filter((t) => t.id !== targetId)
  state.bindings = state.bindings.filter((b) => b.targetId !== targetId)
  state.presetApplications = state.presetApplications.filter((a) => a.targetId !== targetId)
  state.externalInstallations = state.externalInstallations.filter((i) => i.targetId !== targetId)
  task('remove_target', '移除分发目标', '保留目录和链接')
  return save(state)
}
export async function demoAddTarget(input: Omit<Target, 'id'>) {
  state.targets.push({ id: id('target'), ...input })
  task('target', '添加分发目标', input.name)
  return save(state)
}
export async function demoPlan(
  skillIds: string[],
  targetIds: string[],
  adoptExisting = false,
  replaceBindingIds: string[] = [],
  claim = 'manual',
): Promise<DistributionPlan> {
  return {
    revision: state.revision,
    items: skillIds.flatMap((skillId) =>
      targetIds.map((targetId) => {
        const skill = findSkill(skillId)
        const target = state.targets.find((t) => t.id === targetId)
        const binding =
          state.bindings.find((b) => b.targetId === targetId && b.skillId === skillId) ||
          state.bindings.find(
            (b) =>
              b.targetId === targetId &&
              findSkill(b.skillId)?.name.toLowerCase() === skill?.name.toLowerCase(),
          )
        const path = binding?.path || `${target?.path}/${skillId}`
        const incomingPath =
          skill?.externalPath ||
          `${state.storageRoot}/objects/${skill?.bundleDigest}/tree/${skill?.relativePath}`
        if (binding && binding.skillId !== skillId) {
          const blockingClaims = binding.claims.filter((c) => c !== 'manual' && c !== claim)
          const confirmed = replaceBindingIds.includes(binding.id)
          return {
            skillId,
            targetId,
            path,
            action: confirmed && !blockingClaims.length ? 'replace' : 'conflict',
            error: blockingClaims.length
              ? '旧来源仍被其他预设或本地来源引用，请先解除这些引用'
              : confirmed
                ? ''
                : '目标已使用其他来源，请确认切换来源',
            replacement: {
              bindingId: binding.id,
              skillId: binding.skillId,
              version: binding.version,
              nextVersion: skill!.version,
              nextEntityPath: incomingPath,
              entityPath:
                binding.externalPath ||
                `${state.storageRoot}/objects/${binding.digest}/tree/${binding.relativePath}`,
              claims: binding.claims,
              blockingClaims,
              contentEqual: null,
              restoresOriginal: binding.borrowed || !!binding.originalLink,
            },
          }
        }
        const borrowed = binding?.borrowed && skill?.externalPath
        return {
          skillId,
          targetId,
          path,
          action: borrowed ? (adoptExisting ? 'adopt' : 'borrow') : binding ? 'keep' : 'create',
          error: !skill || !target ? '对象不存在' : '',
        }
      }),
    ),
  }
}
export async function demoDistribute(
  skillIds: string[],
  targetIds: string[],
  claim = 'manual',
  expectedRevision: number,
  adoptExisting = false,
  replaceBindingIds: string[] = [],
) {
  if (expectedRevision !== state.revision) throw new Error('分发计划已过期，请重新预览')
  const plan = await demoPlan(skillIds, targetIds, adoptExisting, replaceBindingIds, claim)
  const errors = plan.items.filter((item) => item.error)
  if (errors.length) throw new Error(errors.map((item) => item.error).join('；'))
  for (const item of plan.items.filter((item) => item.action === 'replace')) {
    const binding = state.bindings.find((b) => b.id === item.replacement?.bindingId)!
    const skill = findSkill(item.skillId)!
    if (binding.borrowed) binding.originalLink = item.replacement!.entityPath
    Object.assign(binding, {
      skillId: skill.id,
      version: skill.version,
      digest: skill.bundleDigest,
      relativePath: skill.relativePath,
      externalPath: skill.externalPath,
      borrowed: false,
      follow: false,
    })
  }
  for (const skillId of skillIds)
    for (const targetId of targetIds) {
      const existing = state.bindings.find(
        (item) => item.skillId === skillId && item.targetId === targetId,
      )
      if (existing) {
        if (adoptExisting && existing.borrowed && findSkill(skillId)?.externalPath) {
          existing.borrowed = false
          existing.originalLink = undefined
        }
        if (!existing.claims.includes(claim)) existing.claims.push(claim)
        continue
      }
      const skill = findSkill(skillId)!
      const target = state.targets.find((item) => item.id === targetId)!
      state.bindings.push({
        id: id('binding'),
        skillId,
        targetId,
        path: `${target.path}/${skillId}`,
        version: skill.version,
        digest: skill.bundleDigest,
        relativePath: skill.relativePath,
        borrowed: false,
        claims: [claim],
        follow: true,
      })
    }
  task(
    'distribute',
    '分发 Skill',
    `${skillIds.length} 个 Skill 已处理到 ${targetIds.length} 个目标`,
  )
  return save(state)
}
export async function demoRevoke(bindingIds: string[], claim = 'manual') {
  state.bindings = state.bindings.flatMap((binding) => {
    if (!bindingIds.includes(binding.id)) return [binding]
    const claims = binding.claims.filter((item) => item !== claim)
    return claims.length ? [{ ...binding, claims }] : []
  })
  task('revoke', '取消分发', `已处理 ${bindingIds.length} 条使用关系`)
  return save(state)
}
export async function demoSavePreset(input: {
  id?: string
  name: string
  description: string
  skillIds: string[]
}) {
  const existing = input.id && state.presets.find((item) => item.id === input.id)
  if (existing) Object.assign(existing, input, { revision: existing.revision + 1 })
  else
    state.presets.push({
      id: id('preset'),
      name: input.name,
      description: input.description,
      skillIds: [...new Set(input.skillIds)],
      revision: 1,
      locks: {},
    })
  task('preset', existing ? '编辑预设' : '创建预设', input.name)
  return save(state)
}
export async function demoApplyPreset(
  presetId: string,
  targetIds: string[],
  expectedRevision: number,
) {
  const preset = state.presets.find((item) => item.id === presetId)
  if (!preset) throw new Error('预设不存在')
  return demoDistribute(preset.skillIds, targetIds, `preset:${presetId}`, expectedRevision)
}
export async function demoRevokePreset(presetId: string, targetIds: string[]) {
  const ids = state.bindings
    .filter(
      (item) => targetIds.includes(item.targetId) && item.claims.includes(`preset:${presetId}`),
    )
    .map((item) => item.id)
  return demoRevoke(ids, `preset:${presetId}`)
}
export async function demoDeletePreset(presetId: string) {
  if (state.bindings.some((item) => item.claims.includes(`preset:${presetId}`)))
    throw new Error('此预设仍应用在目标上，请先取消分发')
  state.presets = state.presets.filter((item) => item.id !== presetId)
  task('preset', '删除预设', '预设定义已删除')
  return save(state)
}
export async function demoRemoveSkill(skillId: string) {
  if (
    state.bindings.some((item) => item.skillId === skillId) ||
    state.presets.some((item) => item.skillIds.includes(skillId))
  )
    throw new Error('此 Skill 仍被目标或预设引用，请先解除引用')
  state.skills = state.skills.filter((item) => item.id !== skillId)
  task('uninstall', '从库中卸载', skillId)
  return save(state)
}
export async function demoReadSkill(skillId: string) {
  const skill = findSkill(skillId)
  if (!skill) throw new Error('Skill 不存在')
  return `# ${skill.name}\n\n${skill.description}\n\n## 使用方式\n\n1. 明确输入与目标。\n2. 按工作流执行并记录结果。\n3. 对关键步骤进行验证。\n\n> 演示模式仅展示内容，不会执行 Skill 中的任何指令。`
}
export async function demoSearchCatalog(query: string, sites: string[]): Promise<CatalogResult> {
  const all = [
    {
      slug: 'accessibility-audit',
      name: 'Accessibility Audit',
      description: '检查键盘操作、语义结构与可读性',
      version: '1.4.0',
      site: 'clawhub',
    },
    {
      slug: 'vue-performance',
      name: 'Vue Performance',
      description: '定位 Vue 页面渲染与包体性能问题',
      version: '2.1.0',
      site: 'clawhub',
    },
    {
      slug: 'product-copy',
      name: 'Product Copy',
      description: '生成清晰、一致的中文产品文案',
      version: '1.1.3',
      site: 'https://skills.example.dev',
    },
    {
      slug: 'release-checklist',
      name: 'Release Checklist',
      description: '发布前风险确认与交付状态核对',
      version: '3.0.0',
      site: 'https://skills.example.dev',
    },
  ]
  const normalized = query.trim().toLowerCase()
  return {
    items: all.filter(
      (item) =>
        (!normalized || `${item.name} ${item.description}`.toLowerCase().includes(normalized)) &&
        (!sites.length || sites.some((site) => catalogSiteKey(site) === catalogSiteKey(item.site))),
    ),
    errors: sites.includes('https://skills.example.dev')
      ? ['团队兼容站：临时限流，仅展示缓存结果']
      : [],
  }
}
export async function demoInstallCatalog(slug: string, site: string) {
  if (!state.skills.some((item) => item.id === slug))
    state.skills.push({
      id: slug,
      name: slug
        .split('-')
        .map((part) => part[0].toUpperCase() + part.slice(1))
        .join(' '),
      description: `从 ${site} 安装的 Skill`,
      sourceId: 'src-community',
      bundleDigest: `sha256:${id('digest')}`,
      relativePath: `skills/${slug}`,
      version: '1.0.0',
      installedAt: now(),
    })
  task('install', '从网站安装', `${slug} 已安装到统一库`)
  return save(state)
}
export async function demoImportGit(url: string, reference: string, subdir?: string) {
  const slug = (subdir || url.split('/').pop() || 'git-skill').replace(/\.git$/, '')
  if (!state.skills.some((item) => item.id === slug))
    state.skills.push({
      id: slug,
      name: slug,
      description: `来自 ${url} 的 Git Skill`,
      sourceId: 'src-official',
      bundleDigest: `sha256:${id('digest')}`,
      relativePath: `skills/${slug}`,
      version: reference,
      installedAt: now(),
    })
  task('import', '导入 Git 仓库', `${url} · ${reference}`)
  return save(state)
}
export async function demoSetPolicy(
  sourceId: string,
  mode: 'off' | 'notify' | 'auto',
  intervalHours: number,
) {
  const source = state.sources.find((item) => item.id === sourceId)
  if (!source) throw new Error('来源不存在')
  if (source.updatesRemoved) throw new Error('此来源已移除更新管理')
  if (mode !== 'off' && !sourceUpdateState(source).canCheck)
    throw new Error('本地文件夹仅支持手动同步')
  source.policy = { mode, intervalHours }
  task('settings', '更新来源策略', source.name)
  return save(state)
}
export async function demoCheckSource(sourceId: string, apply: boolean) {
  const source = state.sources.find((item) => item.id === sourceId)
  if (!source) throw new Error('来源不存在')
  if (source.updatesRemoved) throw new Error('此来源已移除更新管理')
  source.lastChecked = now()
  source.nextCheck = new Date(Date.now() + source.policy.intervalHours * 3600000).toISOString()
  if (source.status !== 'healthy' && source.error) throw new Error(source.error)
  task(
    'update',
    apply ? '更新来源内容' : '检查来源更新',
    `${source.name}${apply ? ' 已更新，跟随目标同步完成' : ' 已完成检查'}`,
  )
  return save(state)
}
export async function demoSetFollow(bindingId: string, follow: boolean) {
  const binding = state.bindings.find((item) => item.id === bindingId)
  if (!binding) throw new Error('分发关系不存在')
  binding.follow = follow
  return save(state)
}
export async function demoDiagnose() {
  return {
    issues: state.bindings
      .filter((item) => item.path.includes('missing'))
      .map((item) => `断链：${item.path}`),
  }
}
export async function demoRecover(taskId: string) {
  const target = state.tasks.find((item) => item.id === taskId)
  if (!target) throw new Error('任务不存在')
  target.status = 'success'
  target.message = '恢复已完成，链接重新验证通过'
  return save(state)
}
export async function demoSettings(input: Partial<Snapshot['settings']>) {
  state.settings = { ...state.settings, ...input }
  task('settings', '保存应用设置', '设置已在演示环境保存')
  return save(state)
}
export async function demoMigrateStorage(path: string) {
  state.storageRoot = path
  task('migrate', '迁移统一存储目录', `演示迁移完成：${path}`)
  return save(state)
}
export async function demoImportPreset() {
  const next = {
    id: id('preset'),
    name: '导入的预设',
    description: '从便携预设包导入',
    skillIds: ['code-review', 'task-planner'],
    revision: 1,
    locks: {},
  }
  state.presets.push(next)
  task('preset', '导入预设', next.name)
  return save(state)
}
export async function demoRollback(bindingId: string, digest: string) {
  const binding = state.bindings.find((item) => item.id === bindingId)
  if (!binding) throw new Error('分发关系不存在')
  binding.digest = digest
  binding.follow = false
  task('rollback', '回滚分发版本', binding.path)
  return save(state)
}

export async function demoScanMany(paths: string[]): Promise<ScanResult> {
  const results = await Promise.all(
    [...new Set(paths.map((path) => path.replace(/\/+$/, '')))].map(demoScan),
  )
  const items = results.flatMap((result) => result.items)
  return {
    root: '',
    items: items.filter(
      (item, index) => items.findIndex((other) => other.path === item.path) === index,
    ),
    warnings: results.flatMap((result) => result.warnings),
  }
}

export async function demoRemoveUpdateSource(sourceId: string, expectedRevision: number) {
  if (state.revision !== expectedRevision) throw new Error('数据已变化，请重新确认移除更新管理')
  const source = state.sources.find((item) => item.id === sourceId)
  if (!source) throw new Error('来源不存在')
  source.updatesRemoved = true
  source.policy.mode = 'off'
  source.nextCheck = ''
  task('remove_update_source', '移除更新管理', source.name)
  return save(state)
}
