import type { Snapshot } from './types'

export function presetMembershipInput(snapshot: Snapshot, presetId: string, skillIds: string[]) {
  const preset = snapshot.presets.find((item) => item.id === presetId)
  if (!preset) throw new Error('预设已不存在，请重新选择')
  return {
    id: preset.id,
    name: preset.name,
    description: preset.description,
    skillIds: [...new Set([...preset.skillIds, ...skillIds])],
    packages: snapshot.presetPackages.filter((item) => item.presetId === preset.id),
    syncApplied: true,
  }
}

export function presetMembershipErrors(
  snapshot: Snapshot,
  presetId: string,
  requiredSkillIds: string[],
): string[] {
  const preset = snapshot.presets.find((item) => item.id === presetId)
  if (!preset) return ['预设已不存在']
  const missingIds = requiredSkillIds.filter((id) => !preset.skillIds.includes(id))
  if (missingIds.length)
    return ['预设成员已被修改，本次安装的部分成员已移除。请检查预设；重试不会自动加回。']
  if (requiredSkillIds.some((id) => !snapshot.skills.some((skill) => skill.id === id)))
    return ['本次添加的部分 Skill 已从库中移除，请检查统一库']
  return []
}

export function presetSyncErrors(
  snapshot: Snapshot,
  presetId: string,
  requiredSkillIds: string[] = [],
): string[] {
  const errors = presetMembershipErrors(snapshot, presetId, requiredSkillIds)
  if (errors.length) return errors
  const preset = snapshot.presets.find((item) => item.id === presetId)!
  return snapshot.presetApplications
    .filter((item) => item.presetId === presetId && item.follow)
    .flatMap((item) => {
      const reason =
        item.error || (item.appliedRevision !== preset.revision ? '目标尚未同步到当前预设' : '')
      if (!reason) return []
      const target = snapshot.targets.find((target) => target.id === item.targetId)
      return [`${target?.name || item.targetId}：${reason}`]
    })
}
