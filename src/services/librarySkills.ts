import type { Binding, Skill, Snapshot } from './types'
import { targetDisplayName } from './agentProfiles'

export interface LibrarySkill extends Skill {
  members: Skill[]
  memberIds: string[]
  memberSummary: string
  entityCount: number
  needsSourceChoice: boolean
  sourceSummary: string
  bindings: Binding[]
  tools: {
    id: string
    name: string
    active: boolean
    external: boolean
    protected: boolean
    actionLabel: string
    hint: string
  }[]
}

export function skillEntityKey(skill: Skill, snapshot: Snapshot): string {
  const resolved = snapshot.skillEntityPaths?.[skill.id]
  if (resolved) return JSON.stringify(['path', resolved])
  // An unresolved native path is not evidence that two records share an entity.
  if (snapshot.skillEntityPaths) return JSON.stringify(['unknown', skill.id])
  // Compatibility with older backends and the browser demo.
  if (skill.externalPath) return JSON.stringify(['external', skill.externalPath])
  if (skill.bundleDigest)
    return JSON.stringify(['snapshot', skill.bundleDigest, skill.relativePath])
  return JSON.stringify(['unknown', skill.id])
}

export function skillDistributionChoices(members: Skill[], snapshot: Snapshot): Skill[] {
  const entities = new Map<string, Skill>()
  for (const skill of members) {
    const key = skillEntityKey(skill, snapshot)
    if (!entities.has(key)) entities.set(key, skill)
  }
  return [...entities.values()]
}

export function skillEntityLabel(skill: Skill, snapshot: Snapshot): string {
  if (snapshot.schemaVersion >= 3) return skill.externalPath ? '本地引用' : '统一库当前内容'
  const key = skillEntityKey(skill, snapshot)
  const managed = snapshot.skills.find(
    (member) => !member.externalPath && skillEntityKey(member, snapshot) === key,
  )
  if (snapshot.skillEntityPaths && !snapshot.skillEntityPaths[skill.id]) return '实体位置未确认'
  if (managed?.bundleDigest)
    return `快照 ${managed.bundleDigest.slice(0, 12)} · ${managed.relativePath}`
  return `本地目录 · ${snapshot.skillEntityPaths?.[skill.id] || skill.externalPath || '位置未知'}`
}

// Display identity is the Skill name; source identities remain intact for mutations.
export function librarySkillCount(snapshot: Snapshot | null): number {
  return new Set(snapshot?.skills.map((skill) => skill.name)).size
}

// Group installation records without deleting snapshots or changing existing links.
export function librarySkills(snapshot: Snapshot | null): LibrarySkill[] {
  if (!snapshot) return []
  const groups = new Map<string, Skill[]>()
  for (const skill of snapshot.skills) {
    const key = snapshot.schemaVersion >= 3 ? skill.id : skill.name
    const members = groups.get(key) ?? []
    members.push(skill)
    groups.set(key, members)
  }
  return [...groups.values()].map((members) => {
    // Prefer a managed library snapshot; keep the representative stable across refreshes.
    members.sort(
      (a, b) =>
        Number(!!a.externalPath) - Number(!!b.externalPath) ||
        a.installedAt.localeCompare(b.installedAt) ||
        a.id.localeCompare(b.id),
    )
    const memberIds = members.map((member) => member.id)
    const bindings = snapshot.bindings.filter((binding) => memberIds.includes(binding.skillId))
    const entityCount = skillDistributionChoices(members, snapshot).length
    return {
      ...members[0]!,
      members,
      memberIds,
      entityCount,
      needsSourceChoice: entityCount > 1,
      sourceSummary:
        snapshot.schemaVersion >= 3
          ? `当前内容 · ${snapshot.skillOrigins[members[0]!.id]?.length || 1} 个收录来源`
          : `${members.length} 条来源记录 · ${entityCount === 1 ? '同一实体' : `${entityCount} 个分发候选`}`,
      memberSummary: members
        .map((member) => {
          const source = snapshot.sources.find((source) => source.id === member.sourceId)
          return `${source?.path || source?.url || source?.name || '统一库'} · 来源记录版本：${member.version || '未记录'}`
        })
        .join('\n'),
      bindings,
      tools: snapshot.targets.map((target) => {
        const matches = bindings.filter((binding) => binding.targetId === target.id)
        const external = (snapshot.externalInstallations ?? []).filter(
          (i) => memberIds.includes(i.skillId) && i.targetId === target.id,
        )
        const active = matches.length > 0 || external.length > 0
        const protectedByPreset =
          active && !matches.some((binding) => binding.claims.includes('manual'))
        const actionLabel =
          !matches.length && external.length
            ? '外部已安装 · 使用原目录'
            : protectedByPreset
              ? '由预设或本地来源分发'
              : active
                ? '取消分发'
                : '分发'
        const retained = matches.some((binding) =>
          binding.claims.some((claim) => claim !== 'manual'),
        )
        return {
          id: target.id,
          name: targetDisplayName(target, snapshot.settings.agentProfiles),
          active,
          external: !matches.length && external.length > 0,
          protected: protectedByPreset,
          actionLabel,
          hint: `${[...new Set([...matches.map((b) => b.path), ...external.map((i) => i.path)])].join('\n') || target.path}\n${actionLabel}${retained ? '；请到对应预设或本地来源取消引用' : ''}`,
        }
      }),
    }
  })
}

export function libraryDirectory(snapshot: Snapshot | null): string {
  if (!snapshot) return ''
  return snapshot.storageRoot.replace(/[\\/]\.skilldock$/, '')
}
