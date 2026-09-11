import { invoke } from '@tauri-apps/api/core'
import type {
  CatalogResult,
  DistributionPlan,
  ScanResult,
  Snapshot,
  Target,
  Skill,
  PresetPackage,
} from './types'
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

export interface BackupEntry {
  id: string
  createdAt: string
  status: string
  paths: string[]
  issues: string[]
}

export interface RestorePreview {
  revision: number
  items: { digest: string; path: string; status: string; reason: string; fingerprint: string }[]
}

export interface CleanupChoice {
  digest: string
  fingerprint: string
}
export interface CleanupReport {
  id: string
  status: string
  items: RestorePreview['items']
}

export interface PresetFolderPreview {
  packageId?: string | null
  removed?: { skillId: string; name: string; path: string }[]
  root: string
  revision: number
  items: {
    path: string
    name: string
    description: string
    existingId: string | null
    snapshotId: string | null
    change?: 'added' | 'changed' | 'unchanged'
    existingLinks: string[]
    sameName: boolean
    error?: string
    entryPath?: string
  }[]
  warnings: string[]
}

export interface SourceBindingInput {
  sourceId: string
  kind: 'git' | 'local'
  url: string
  path: string
  reference: string
  subdir: string
}

export const api = {
  importPackage: (
    path: string,
    selectedPaths: string[],
    revision: number,
  ): Promise<{ snapshot: Snapshot; packageId: string; skillIds: string[] }> =>
    isNative
      ? native('import_package', { path, selectedPaths, revision })
      : Promise.reject(new Error('请在桌面应用中同步本机包')),
  packageMigrationPreview: (
    path: string,
  ): Promise<{ items: { path: string; entity: string; status: string; reason: string }[] }> =>
    native('package_migration_preview', { path }),
  retryPresetSync: (): Promise<Snapshot> => native('retry_preset_sync'),
  setPresetFollow: (presetId: string, targetId: string, follow: boolean): Promise<Snapshot> =>
    native('set_preset_follow', { presetId, targetId, follow }),
  previewBindSource: (
    input: SourceBindingInput,
  ): Promise<{ revision: number; digest: string; memberNames: string[] }> =>
    isNative
      ? native('preview_bind_source', { ...input })
      : Promise.reject(new Error('请在桌面应用中核对真实更新来源')),
  bindSource: (
    input: SourceBindingInput,
    expectedRevision: number,
    digest: string,
  ): Promise<Snapshot> =>
    isNative
      ? native('bind_source', { ...input, expectedRevision, digest })
      : Promise.reject(new Error('请在桌面应用中配置真实更新来源')),

  previewSnapshotRefresh: (
    skillId: string,
  ): Promise<{ revision: number; contentDigest: string; skillCount: number; path: string }> =>
    isNative
      ? native('preview_snapshot_refresh', { skillId })
      : Promise.reject(new Error('请在桌面应用中重新收录')),
  refreshSnapshot: (
    skillId: string,
    expectedRevision: number,
    contentDigest: string,
  ): Promise<Snapshot> =>
    isNative
      ? native('refresh_snapshot', { skillId, expectedRevision, contentDigest })
      : Promise.reject(new Error('请在桌面应用中重新收录')),

  previewPresetFolder: async (path: string): Promise<PresetFolderPreview> => {
    if (isNative) return native('preview_preset_folder', { path })
    const [scan, state] = await Promise.all([demo.demoScan(path), demo.demoSnapshot()])
    return {
      root: path,
      revision: state.revision,
      warnings: scan.warnings,
      items: scan.items
        .filter((item) => ['ready', 'new', 'same'].includes(item.status))
        .map((item) => ({
          ...item,
          existingId: null,
          snapshotId: null,
          existingLinks: [],
          sameName: false,
        })),
    }
  },
  importPresetFolder: (
    path: string,
    selectedPaths: string[],
    mode: 'reference' | 'copy',
    revision: number,
  ): Promise<{ snapshot: Snapshot; skillIds: string[] }> =>
    isNative
      ? native('import_preset_folder', { path, selectedPaths, mode, revision })
      : Promise.reject(new Error('请在桌面应用中引用或导入本机 Skill 包')),

  openDirectory: (path: string, revealLink = false): Promise<void> =>
    isNative
      ? invoke('open_directory', { path, revealLink })
      : Promise.reject(new Error('请在桌面应用中打开本机目录')),
  previewObjectCleanup: (): Promise<RestorePreview> =>
    isNative
      ? native('preview_object_cleanup')
      : demo.demoSnapshot().then((state) => ({ revision: state.revision, items: [] })),
  listObjectCleanups: (): Promise<CleanupReport[]> =>
    isNative ? native('list_object_cleanups') : Promise.resolve([]),
  cleanupObjects: (expectedRevision: number, items: CleanupChoice[]): Promise<Snapshot> =>
    isNative
      ? native('cleanup_objects', { expectedRevision, items })
      : Promise.reject(new Error('请在桌面应用中清理本机实体')),
  retryObjectCleanup: (taskId: string): Promise<Snapshot> =>
    isNative
      ? native('retry_object_cleanup', { taskId })
      : Promise.reject(new Error('请在桌面应用中重试清理')),
  previewRestore: (backupId: string): Promise<RestorePreview> =>
    isNative
      ? native('preview_restore', { backupId })
      : demo.demoSnapshot().then((state) => ({ revision: state.revision, items: [] })),
  retryBackupCleanup: (): Promise<Snapshot> =>
    isNative ? native('retry_backup_cleanup') : demo.demoSnapshot(),
  listBackups: (): Promise<BackupEntry[]> =>
    isNative ? native('list_backups') : Promise.resolve([]),
  restoreBackup: (
    backupId: string,
    expectedRevision?: number,
    cleanupItems: CleanupChoice[] = [],
  ): Promise<Snapshot> =>
    isNative
      ? native('restore_backup', { backupId, expectedRevision, cleanupItems })
      : Promise.reject(new Error('请在桌面应用中恢复本机备份')),
  snapshot: (): Promise<Snapshot> => (isNative ? native('snapshot') : demo.demoSnapshot()),
  configure: (path: string): Promise<Snapshot> =>
    isNative ? native('configure', { path }) : demo.demoConfigure(path),
  scan: (path: string): Promise<ScanResult> =>
    isNative ? native('scan', { path }) : demo.demoScan(path),
  scanMany: (paths: string[]): Promise<ScanResult> =>
    isNative ? native('scan_many', { paths }) : demo.demoScanMany(paths),
  discover: (): Promise<Target[]> => (isNative ? native('discover') : demo.demoDiscover()),
  importBatch: async (
    groups: { path: string; selectedPaths: string[] }[],
    adopt: boolean,
  ): Promise<Snapshot> => {
    if (isNative) return native('import_batch', { groups, adopt })
    let state = await demo.demoSnapshot()
    for (const group of groups)
      state = await demo.demoImportFolder(group.path, group.selectedPaths, adopt)
    return state
  },
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
    takeover = false,
  ): Promise<Snapshot> =>
    isNative
      ? claim?.startsWith('preset:')
        ? native('apply_preset', {
            presetId: claim.slice(7),
            targetIds,
            expectedRevision,
            takeover,
          })
        : native('distribute', { skillIds, targetIds, claim, expectedRevision, takeover })
      : demo.demoDistribute(skillIds, targetIds, claim, expectedRevision),
  revoke: (bindingIds: string[], claim?: string): Promise<Snapshot> =>
    isNative ? native('revoke', { bindingIds, claim }) : demo.demoRevoke(bindingIds, claim),
  savePreset: (input: {
    id?: string
    name: string
    description: string
    skillIds: string[]
    packages?: PresetPackage[]
    syncApplied?: boolean
  }): Promise<Snapshot> => (isNative ? native('save_preset', input) : demo.demoSavePreset(input)),
  applyPreset: (
    presetId: string,
    targetIds: string[],
    expectedRevision: number,
    takeover = false,
  ): Promise<Snapshot> =>
    isNative
      ? native('apply_preset', { presetId, targetIds, expectedRevision, takeover })
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
