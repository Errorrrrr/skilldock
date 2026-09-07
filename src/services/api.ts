import { invoke } from '@tauri-apps/api/core'
import type { CatalogResult, DistributionPlan, ScanResult, Snapshot, Target, Skill } from './types'
import * as demo from './demo'

export const isNative = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window

async function native<T>(action: string, args: Record<string, unknown> = {}): Promise<T> {
  try {
    return await invoke<T>('execute', { request: { action, ...args } })
  } catch (error) {
    throw new Error(
      typeof error === 'string'
        ? error
        : error instanceof Error
          ? error.message
          : '原生服务调用失败',
    )
  }
}

export const api = {
  snapshot: (): Promise<Snapshot> => (isNative ? native('snapshot') : demo.demoSnapshot()),
  configure: (path: string): Promise<Snapshot> =>
    isNative ? native('configure', { path }) : demo.demoConfigure(path),
  scan: (path: string): Promise<ScanResult> =>
    isNative ? native('scan', { path }) : demo.demoScan(path),
  discover: (): Promise<Target[]> => (isNative ? native('discover') : demo.demoDiscover()),
  importFolder: (path: string, selectedPaths: string[], adopt: boolean): Promise<Snapshot> =>
    isNative
      ? native('import_folder', { path, selectedPaths, adopt })
      : demo.demoImportFolder(path, selectedPaths, adopt),
  addTarget: (input: {
    name: string
    tool: string
    scope: string
    path: string
  }): Promise<Snapshot> => (isNative ? native('add_target', input) : demo.demoAddTarget(input)),
  plan: (skillIds: string[], targetIds: string[], claim?: string): Promise<DistributionPlan> =>
    isNative ? native('plan', { skillIds, targetIds, claim }) : demo.demoPlan(skillIds, targetIds),
  distribute: (
    skillIds: string[],
    targetIds: string[],
    expectedRevision: number,
    claim?: string,
  ): Promise<Snapshot> =>
    isNative
      ? claim?.startsWith('preset:')
        ? native('apply_preset', { presetId: claim.slice(7), targetIds, expectedRevision })
        : native('distribute', { skillIds, targetIds, claim, expectedRevision })
      : demo.demoDistribute(skillIds, targetIds, claim, expectedRevision),
  revoke: (bindingIds: string[], claim?: string): Promise<Snapshot> =>
    isNative ? native('revoke', { bindingIds, claim }) : demo.demoRevoke(bindingIds, claim),
  savePreset: (input: {
    id?: string
    name: string
    description: string
    skillIds: string[]
  }): Promise<Snapshot> => (isNative ? native('save_preset', input) : demo.demoSavePreset(input)),
  applyPreset: (
    presetId: string,
    targetIds: string[],
    expectedRevision: number,
  ): Promise<Snapshot> =>
    isNative
      ? native('apply_preset', { presetId, targetIds, expectedRevision })
      : demo.demoApplyPreset(presetId, targetIds, expectedRevision),
  revokePreset: (presetId: string, targetIds: string[]): Promise<Snapshot> =>
    isNative
      ? native('revoke_preset', { presetId, targetIds })
      : demo.demoRevokePreset(presetId, targetIds),
  deletePreset: (presetId: string): Promise<Snapshot> =>
    isNative ? native('delete_preset', { presetId }) : demo.demoDeletePreset(presetId),
  removeSkill: (skillId: string): Promise<Snapshot> =>
    isNative ? native('remove_skill', { skillId }) : demo.demoRemoveSkill(skillId),
  skillHistory: (skillId: string): Promise<Skill[]> =>
    isNative
      ? native('skill_history', { skillId })
      : demo.demoSnapshot().then((s) => s.skills.filter((item) => item.id === skillId)),
  readSkill: (skillId: string): Promise<string> =>
    isNative ? native('read_skill', { skillId }) : demo.demoReadSkill(skillId),
  searchCatalog: (query: string, sites: string[] = []): Promise<CatalogResult> =>
    isNative ? native('search_catalog', { query, sites }) : demo.demoSearchCatalog(query, sites),
  installCatalog: (slug: string, site: string): Promise<Snapshot> =>
    isNative ? native('install_catalog', { slug, site }) : demo.demoInstallCatalog(slug, site),
  importGit: (url: string, reference: string, subdir?: string): Promise<Snapshot> =>
    isNative
      ? native('import_git', { url, reference, subdir })
      : demo.demoImportGit(url, reference, subdir),
  setPolicy: (
    sourceId: string,
    mode: 'off' | 'notify' | 'auto',
    intervalHours: number,
  ): Promise<Snapshot> =>
    isNative
      ? native('set_policy', { sourceId, mode, intervalHours })
      : demo.demoSetPolicy(sourceId, mode, intervalHours),
  checkSource: (sourceId: string, apply: boolean): Promise<Snapshot> =>
    isNative ? native('check_source', { sourceId, apply }) : demo.demoCheckSource(sourceId, apply),
  setFollow: (bindingId: string, follow: boolean): Promise<Snapshot> =>
    isNative ? native('set_follow', { bindingId, follow }) : demo.demoSetFollow(bindingId, follow),
  diagnose: (): Promise<{ issues: string[] }> =>
    isNative ? native('diagnose') : demo.demoDiagnose(),
  recover: (taskId: string): Promise<Snapshot> =>
    isNative ? native('recover', { taskId }) : demo.demoRecover(taskId),
  settings: (input: Partial<Snapshot['settings']>): Promise<Snapshot> =>
    isNative ? native('settings', input) : demo.demoSettings(input),
  migrateStorage: (path: string): Promise<Snapshot> =>
    isNative ? native('migrate_storage', { path }) : demo.demoMigrateStorage(path),
  exportPreset: (presetId: string, path: string): Promise<{ path: string }> =>
    isNative ? native('export_preset', { presetId, path }) : Promise.resolve({ path }),
  importPreset: (path: string): Promise<Snapshot> =>
    isNative ? native('import_preset', { path }) : demo.demoImportPreset(),
  rollback: (bindingId: string, digest: string): Promise<Snapshot> =>
    isNative ? native('rollback', { bindingId, digest }) : demo.demoRollback(bindingId, digest),
  checkAppUpdate: (): Promise<{
    configured: boolean
    available: boolean
    version?: string
    message: string
  }> => (isNative ? native('check_app_update') : demo.demoCheckAppUpdate()),
  installAppUpdate: (): Promise<{
    configured: boolean
    available: boolean
    version?: string
    message: string
  }> => native('install_app_update', { allowRestart: true }),
  async pickDirectory(defaultPath?: string) {
    if (!isNative) return defaultPath || '/Users/demo/SkillDock'
    const { open } = await import('@tauri-apps/plugin-dialog')
    const selected = await open({ directory: true, multiple: false, defaultPath })
    return typeof selected === 'string' ? selected : null
  },
  async pickFile() {
    if (!isNative) return '/Users/demo/Downloads/skilldock-preset.skilldock.zip'
    const { open } = await import('@tauri-apps/plugin-dialog')
    const selected = await open({
      directory: false,
      multiple: false,
      filters: [{ name: 'SkillDock 预设', extensions: ['zip'] }],
    })
    return typeof selected === 'string' ? selected : null
  },
  async saveFile(defaultPath: string) {
    if (!isNative) return defaultPath
    const { save } = await import('@tauri-apps/plugin-dialog')
    return save({ defaultPath, filters: [{ name: 'SkillDock 预设', extensions: ['zip'] }] })
  },
}
