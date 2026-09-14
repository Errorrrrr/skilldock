import type { ExternalInstallation, Binding, CatalogItem, Skill, Snapshot } from './types'
import { catalogSiteKey } from './catalogSites'
import { targetDisplayName } from './agentProfiles'

export interface SkillDirectoryContext {
  members?: Skill[]
  skill?: Skill | null
  skillId?: string
  presetId?: string
  binding?: Binding | null
  installation?: ExternalInstallation
  path?: string
  catalog?: CatalogItem | null
}

function joinDirectory(base: string, ...parts: string[]): string {
  if (!base) return ''
  const root = base.replace(/\\/g, '/')
  const suffix = parts
    .map((part) => part.replace(/\\/g, '/').replace(/^\/+|\/+$/g, ''))
    .filter(Boolean)
    .join('/')
  return suffix ? `${root.replace(/\/+$/, '')}/${suffix}` : root
}

// Used in every Skill view so locked versions and nested local paths have the
// same meaning in lists, selectors, plans and details.
export function skillDirectories(snapshot: Snapshot | null, context: SkillDirectoryContext) {
  if (context.installation) {
    const { path, entityPath } = context.installation
    return {
      directory: path,
      original: entityPath,
      directories: [{ path, label: path }],
      directoryReason: '',
      originalReason: '',
    }
  }

  if (context.path)
    return {
      directory: context.path,
      original: context.path,
      directories: [{ path: context.path, label: context.path }],
      directoryReason: '',
      originalReason: '',
    }
  let skill = context.skill ?? undefined
  let bundleDirectory = ''
  let catalogMembers: Skill[] = []
  if (context.catalog && snapshot) {
    const catalog = context.catalog
    const sources = snapshot.sources.filter(
      (source) =>
        ['catalog', 'clawhub'].includes(source.kind) &&
        source.reference === catalog.slug &&
        catalogSiteKey(source.url) === catalogSiteKey(catalog.site),
    )
    const members = snapshot.skills.filter((member) =>
      sources.some((source) => source.id === member.sourceId),
    )
    catalogMembers = members
    if (members.length === 1) skill = members[0]
    else if (
      members.length > 1 &&
      new Set(members.map((member) => member.bundleDigest)).size === 1
    ) {
      bundleDirectory = joinDirectory(
        snapshot.storageRoot,
        'objects',
        members[0]!.bundleDigest,
        'tree',
      )
    }
  } else if (!skill && snapshot) {
    const skillId = context.binding?.skillId ?? context.skillId
    skill =
      (context.presetId
        ? snapshot.presets.find((preset) => preset.id === context.presetId)?.locks[skillId ?? '']
        : undefined) ?? snapshot.skills.find((member) => member.id === skillId)
  }
  const digest = context.binding?.digest ?? skill?.bundleDigest
  const relative = context.binding?.relativePath ?? skill?.relativePath ?? ''
  const original =
    context.binding?.externalPath ||
    skill?.externalPath ||
    bundleDirectory ||
    (snapshot && digest
      ? joinDirectory(snapshot.storageRoot, 'objects', digest, 'tree', relative)
      : '')
  // Only recorded bindings represent managed link locations. A local source
  // path may still be a separate physical copy and must not masquerade as a link.
  const members = context.members ?? (skill ? [skill] : catalogMembers)
  const bindings = context.binding
    ? [context.binding]
    : (snapshot?.bindings ?? []).flatMap((binding) =>
        members.flatMap((member) => {
          if (member.externalPath || binding.externalPath) {
            if (!member.externalPath || !binding.externalPath) return []
            if (member.id === binding.skillId && member.externalPath === binding.externalPath)
              return [binding]
            const prefix = `${binding.externalPath.replace(/\\/g, '/').replace(/\/+$/, '')}/`
            const actual = member.externalPath.replace(/\\/g, '/')
            return actual.startsWith(prefix)
              ? [{ ...binding, path: joinDirectory(binding.path, actual.slice(prefix.length)) }]
              : []
          }
          if (member.bundleDigest !== binding.digest) return []
          if (member.id === binding.skillId && member.relativePath === binding.relativePath)
            return [binding]
          // Nested Skills can be reached through a distributed parent Skill.
          const prefix = binding.relativePath ? `${binding.relativePath}/` : ''
          if (member.relativePath && member.relativePath.startsWith(prefix)) {
            const suffix = member.relativePath.slice(prefix.length)
            if (suffix) return [{ ...binding, path: joinDirectory(binding.path, suffix) }]
          }
          return []
        }),
      )
  const external = (snapshot?.externalInstallations ?? [])
    .filter((i) => members.some((m) => m.id === i.skillId))
    .map((i) => ({
      path: i.path,
      label: `${targetDisplayName(
        snapshot?.targets.find((t) => t.id === i.targetId),
        snapshot?.settings.agentProfiles,
      )} · 外部已安装`,
    }))
  const directories = [
    ...external,
    ...new Map(
      bindings
        .filter((binding) => binding.path)
        .map((binding) => [
          binding.path,
          {
            path: binding.path,
            label:
              targetDisplayName(
                snapshot?.targets.find((target) => target.id === binding.targetId),
                snapshot?.settings.agentProfiles,
              ) || binding.path,
          },
        ]),
    ).values(),
  ]
  const directory = directories[0]?.path || ''
  return {
    directory,
    directories,
    original,
    directoryReason: directory ? '' : '此版本尚无已记录的分发软链目录',
    originalReason: original
      ? ''
      : context.catalog
        ? '尚未安装或无法匹配本地实体，请从 Skill 库查看具体成员'
        : '未找到此 Skill 的本地实体记录',
  }
}
