import defaults from '../../shared/agent-profiles.json'
import type { AgentProfile, Source, Target } from './types'
export const defaultAgentProfiles: AgentProfile[] = defaults

export function targetDisplayName(
  target: Pick<Target, 'name' | 'tool' | 'path'> | undefined,
  profiles = defaultAgentProfiles,
): string {
  if (!target) return ''
  const path = target.path.replace(/\\/g, '/').replace(/\/+$/, '')
  const tool = target.tool.trim()
  const knownProfiles = [...profiles, ...defaultAgentProfiles]
  const profile =
    knownProfiles.find((item) =>
      [item.id, item.name].some((value) => value.toLowerCase() === tool.toLowerCase()),
    ) ||
    knownProfiles.find((item) =>
      [...item.userPaths, ...item.projectPaths].some((configured) => {
        const normalized = configured.replace(/\\/g, '/').replace(/\/+$/, '')
        if (path === normalized) return true
        const suffix = normalized.replace(/^~\//, '')
        return !suffix.startsWith('/') && suffix.includes('/') && path.endsWith(`/${suffix}`)
      }),
    )
  const toolName = profile?.name || (/^(custom|unknown)?$/i.test(tool) ? '' : tool)
  const name = target.name.trim()
  if (toolName) {
    if (name.toLowerCase().includes(toolName.toLowerCase())) return name
    return `${toolName} · ${!name || /^skills$/i.test(name) ? 'Skills' : name}`
  }
  if (name && !/^skills$/i.test(name)) return name
  return `${path.split('/').slice(-2, -1)[0] || '自定义目录'} · Skills`
}

export function sourceDisplayName(
  source: Source | null | undefined,
  profiles = defaultAgentProfiles,
): string {
  if (!source) return ''
  if (!['folder', 'local', 'local_reference'].includes(source.kind)) return source.name
  return targetDisplayName({ name: source.name, path: source.path, tool: 'custom' }, profiles)
}

export function cloneAgentProfiles(profiles = defaultAgentProfiles): AgentProfile[] {
  return profiles
    .filter((profile) => profile.id.toLowerCase() !== 'qclaw')
    .map((profile) => ({
      ...profile,
      userPaths: [...profile.userPaths],
      projectPaths: [...profile.projectPaths],
    }))
}

export interface ResolvedTool {
  name: string
  paths: string[]
}

export function resolveToolNameFromPath(
  linkPath: string,
  targets: Target[] = [],
  profiles = defaultAgentProfiles,
): string {
  if (!linkPath) return ''
  const normalized = linkPath.replace(/\\/g, '/').replace(/\/+$/, '')

  const matchedTarget = targets.find((t) => {
    const tPath = t.path.replace(/\\/g, '/').replace(/\/+$/, '')
    return normalized === tPath || normalized.startsWith(`${tPath}/`)
  })
  if (matchedTarget) {
    const tool = matchedTarget.tool?.trim() || ''
    const knownProfiles = [...profiles, ...defaultAgentProfiles]
    const profile = knownProfiles.find((item) =>
      [item.id, item.name].some((val) => val.toLowerCase() === tool.toLowerCase()),
    )
    if (profile?.name) return profile.name
    if (tool && !/^(custom|unknown)$/i.test(tool)) {
      return tool.charAt(0).toUpperCase() + tool.slice(1)
    }
    const name = matchedTarget.name?.trim()
    if (name && !/^skills$/i.test(name)) {
      return name
    }
  }

  const knownProfiles = [...profiles, ...defaultAgentProfiles]
  for (const profile of knownProfiles) {
    const matched = [...profile.userPaths, ...profile.projectPaths].some((configured) => {
      const confNorm = configured.replace(/\\/g, '/').replace(/\/+$/, '')
      const suffix = confNorm.replace(/^~\//, '')
      if (!suffix || /^skills$/i.test(suffix)) return false
      return (
        normalized === confNorm ||
        normalized.endsWith(`/${suffix}`) ||
        normalized.includes(`/${suffix}/`)
      )
    })
    if (matched) return profile.name
  }

  const patterns: [RegExp, string][] = [
    [/(?:^|\/)\.gemini\/(?:antigravity|config)(?:\/|$)/i, 'Antigravity'],
    [/(?:^|\/)\.cursor(?:\/|$)/i, 'Cursor'],
    [/(?:^|\/)\.claude(?:\/|$)/i, 'Claude Code'],
    [/(?:^|\/)\.codex(?:\/|$)/i, 'Codex'],
    [/(?:^|\/)\.gemini(?:\/|$)/i, 'Gemini CLI'],
    [/(?:^|\/)\.agents(?:\/|$)/i, '通用 Agent'],
    [/(?:^|\/)\.opencode(?:\/|$)/i, 'OpenCode'],
    [/(?:^|\/)\.openclaw(?:\/|$)/i, 'OpenClaw'],
    [/(?:^|\/)\.(?:workbuddy|codebuddy)(?:\/|$)/i, 'WorkBuddy'],
    [/(?:^|\/)\.trae(?:\/|$)/i, 'Trae'],
    [/(?:^|\/)\.windsurf(?:\/|$)/i, 'Windsurf'],
  ]
  for (const [regex, name] of patterns) {
    if (regex.test(normalized)) return name
  }

  const parts = normalized.split('/').filter(Boolean)
  if (parts.length >= 2) {
    const parent = parts[parts.length - 2]
    if (parent && !/^skills$/i.test(parent)) {
      return parent.replace(/^\./, '')
    }
    if (parts.length >= 3) {
      return parts[parts.length - 3].replace(/^\./, '')
    }
  }
  return '自定义工具'
}

export function resolveToolsFromLinks(
  links: string[],
  targets: Target[] = [],
  profiles = defaultAgentProfiles,
): ResolvedTool[] {
  const map = new Map<string, string[]>()
  for (const link of links) {
    const toolName = resolveToolNameFromPath(link, targets, profiles)
    if (!map.has(toolName)) {
      map.set(toolName, [])
    }
    map.get(toolName)!.push(link)
  }
  return Array.from(map.entries()).map(([name, paths]) => ({ name, paths }))
}
