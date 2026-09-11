import type { Binding, Skill, Snapshot } from './types'
import { targetDisplayName } from './agentProfiles'

export interface LibrarySkill extends Skill {
  members: Skill[]
  memberIds: string[]
  memberSummary: string
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

// Display identity is the Skill name; source identities remain intact for mutations.
export function librarySkillCount(snapshot: Snapshot | null): number {
  return new Set(snapshot?.skills.map((skill) => skill.name)).size
}

// Group installation records without deleting snapshots or changing existing links.
export function librarySkills(snapshot: Snapshot | null): LibrarySkill[] {
  if (!snapshot) return []
  const groups = new Map<string, Skill[]>()
  for (const skill of snapshot.skills) {
    const members = groups.get(skill.name) ?? []
    members.push(skill)
    groups.set(skill.name, members)
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
    return {
      ...members[0]!,
      members,
      memberIds,
      memberSummary: members
        .map((member) => {
          const source = snapshot.sources.find((source) => source.id === member.sourceId)
          return `${source?.path || source?.url || source?.name || '统一库'} · ${member.version}`
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
              ? '由预设分发'
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
          hint: `${[...new Set([...matches.map((b) => b.path), ...external.map((i) => i.path)])].join('\n') || target.path}\n${actionLabel}${retained ? '；预设引用需到预设页取消' : ''}`,
        }
      }),
    }
  })
}
