import { catalogSiteKey } from './catalogSites.ts'
import {
  presetMembershipErrors,
  presetMembershipInput,
  presetSyncErrors,
} from './presetMembership.ts'
import type { CatalogItem, DistributionPlan, Snapshot } from './types'

type StepStatus = 'pending' | 'running' | 'succeeded' | 'failed'
export type CatalogInstallStep = {
  kind: 'install' | 'resolve' | 'preset' | 'target'
  id: string
  label: string
  status: StepStatus
  error: string
  // A saved preset may still have failed target synchronization. Retry only synchronization.
  saved?: boolean
}
export type CatalogInstallSession = {
  item: CatalogItem
  steps: CatalogInstallStep[]
  skillIds: string[]
  running: boolean
}

export function catalogInstallKey(item: Pick<CatalogItem, 'site' | 'slug'>): string {
  return `${catalogSiteKey(item.site)}:${item.slug}`
}

export function resolveCatalogSkillIds(
  snapshot: Snapshot,
  item: Pick<CatalogItem, 'site' | 'slug'>,
  demo = false,
): string[] {
  const sourceIds = new Set(
    snapshot.sources
      .filter(
        (source) =>
          ['catalog', 'clawhub'].includes(source.kind) &&
          source.reference === item.slug &&
          catalogSiteKey(source.url) === catalogSiteKey(item.site),
      )
      .map((source) => source.id),
  )
  const ids = snapshot.skills
    .filter(
      (skill) =>
        sourceIds.has(skill.sourceId) ||
        snapshot.skillOrigins[skill.id]?.some((id) => sourceIds.has(id)),
    )
    .map((skill) => skill.id)
  if (!ids.length && demo && snapshot.skills.some((skill) => skill.id === item.slug))
    ids.push(item.slug)
  return [...new Set(ids)]
}

export function createCatalogInstallSession(
  item: CatalogItem,
  presetIds: string[],
  targetIds: string[],
  snapshot: Snapshot,
): CatalogInstallSession {
  const steps: Array<Pick<CatalogInstallStep, 'kind' | 'id' | 'label'>> = [
    { kind: 'install', id: 'install', label: '收录到统一库' },
    { kind: 'resolve', id: 'resolve', label: '确认来源的全部 Skill 成员' },
    ...[...new Set(presetIds)].map((id) => ({
      kind: 'preset' as const,
      id,
      label: `加入预设「${snapshot.presets.find((item) => item.id === id)?.name || id}」并同步跟随目标`,
    })),
    ...[...new Set(targetIds)].map((id) => ({
      kind: 'target' as const,
      id,
      label: `分发到「${snapshot.targets.find((item) => item.id === id)?.name || id}」`,
    })),
  ]
  return {
    item: { ...item },
    steps: steps.map((step) => ({ ...step, status: 'pending', error: '' })),
    skillIds: [],
    running: false,
  }
}

export function catalogInstallComplete(session: CatalogInstallSession): boolean {
  return session.steps.every((step) => step.status === 'succeeded')
}

export function catalogInstallErrors(session: CatalogInstallSession): string[] {
  return session.steps
    .filter((step) => step.status === 'failed')
    .map((step) => `${step.label}：${step.error}`)
}

type Operations = {
  snapshot: () => Snapshot
  updateSnapshot: (snapshot: Snapshot) => void
  installCatalog: (slug: string, site: string) => Promise<Snapshot>
  savePreset: (input: ReturnType<typeof presetMembershipInput>) => Promise<Snapshot>
  retryPresetSync: () => Promise<Snapshot>
  plan: (skillIds: string[], targetIds: string[]) => Promise<DistributionPlan>
  distribute: (skillIds: string[], targetIds: string[], revision: number) => Promise<Snapshot>
}

export async function runCatalogInstall(
  session: CatalogInstallSession,
  operations: Operations,
  demo = false,
): Promise<boolean> {
  if (session.running) return false
  session.running = true
  try {
    const snapshot = operations.snapshot()
    if (session.skillIds.some((id) => !snapshot.skills.some((skill) => skill.id === id))) {
      const resolve = session.steps.find((step) => step.kind === 'resolve')!
      resolve.status = 'failed'
      resolve.error = '本次安装的部分 Skill 已从库中移除。请检查统一库，或结束本次操作后重新安装。'
      return false
    }
    // A route change may let the user edit a preset or delete a target before retrying.
    // Revalidate completed steps without overwriting those later choices.
    for (const step of session.steps) {
      if (step.status !== 'succeeded') continue
      if (
        (step.kind === 'preset' && presetSyncErrors(snapshot, step.id, session.skillIds).length) ||
        (step.kind === 'target' && !snapshot.targets.some((target) => target.id === step.id))
      )
        step.status = 'pending'
    }
    for (const step of session.steps) {
      if (step.status === 'succeeded') continue
      step.status = 'running'
      step.error = ''
      try {
        if (step.kind === 'install') {
          operations.updateSnapshot(
            await operations.installCatalog(session.item.slug, session.item.site),
          )
        } else if (step.kind === 'resolve') {
          session.skillIds = resolveCatalogSkillIds(operations.snapshot(), session.item, demo)
          if (!session.skillIds.length)
            throw new Error('内容已入库，但未找到对应成员。可重试确认，或到统一库检查来源。')
        } else if (step.kind === 'preset') {
          if (step.saved) {
            const errors = presetMembershipErrors(operations.snapshot(), step.id, session.skillIds)
            if (errors.length) throw new Error(errors.join('；'))
          }
          const next = step.saved
            ? await operations.retryPresetSync()
            : await operations.savePreset(
                presetMembershipInput(operations.snapshot(), step.id, session.skillIds),
              )
          operations.updateSnapshot(next)
          step.saved = true
          const errors = presetSyncErrors(next, step.id, session.skillIds)
          if (errors.length) throw new Error(`成员已保存，目标同步未完成：${errors.join('；')}`)
        } else {
          if (!operations.snapshot().targets.some((target) => target.id === step.id))
            throw new Error('此工具目标已被删除，请结束本次操作并重新选择目标')
          const plan = await operations.plan(session.skillIds, [step.id])
          const errors = plan.items.filter((item) => item.error).map((item) => item.error)
          if (errors.length) throw new Error([...new Set(errors)].join('；'))
          operations.updateSnapshot(
            await operations.distribute(session.skillIds, [step.id], plan.revision),
          )
        }
        step.status = 'succeeded'
      } catch (error) {
        step.status = 'failed'
        step.error = error instanceof Error ? error.message : '操作失败'
        // Later steps depend on the import and on canonical member IDs.
        if (step.kind === 'install' || step.kind === 'resolve') break
      }
    }
    return catalogInstallComplete(session)
  } finally {
    session.running = false
  }
}
