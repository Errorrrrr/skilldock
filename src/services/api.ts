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
import { version as appVersion } from '../../package.json'

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

export interface AppUpdateResult {
  configured: boolean
  available: boolean
  currentVersion: string
  endpoint: string
  custom: boolean
  message: string
  version?: string
  notes?: string
  token?: string
}
export interface AppUpdateProgress {
  token: string
  phase: 'downloading' | 'verifying' | 'installing' | 'restarting'
  downloaded: number
  total: number | null
}
const demoAppUpdate: AppUpdateResult = {
  configured: false,
  available: false,
  currentVersion: appVersion,
  endpoint: 'https://github.com/Errorrrrr/skilldock/releases/latest/download/latest.json',
  custom: false,
  message: '浏览器演示不检查或安装应用更新，请在桌面应用中操作',
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

export interface LocalSourcePreview extends PresetFolderPreview {
  contentDigest: string
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

export interface CollectionPreview {
  revision: number
  fingerprint: string
  mode: 'package' | 'individual'
  paths: string[]
  items: ScanResult['items']
  warnings: string[]
}

export interface GitPackagePreview {
  issues: string[]
  token: string
  revision: number
  url: string
  reference: string
  subdir: string
  commit: string
  name: string
  items: {
    path: string
    name: string
    description: string
    existingId: string | null
    change: string
  }[]
  removed: { skillId: string; name: string; path: string }[]
}
export interface GitPackageResult {
  snapshot: Snapshot
  packageId: string
  skillIds: string[]
  presetId: string
}

export interface SingleContentPreview {
  revision: number
  alreadyEnabled: boolean
  groups: { name: string; skills: Skill[]; identical: boolean }[]
  bindings: Snapshot['bindings']
  presets: Snapshot['presets']
  backupCount: number
}
export interface SingleContentChoice {
  name: string
  skillId?: string
  keepSeparate?: boolean
}

export const api = {
  replaceCurrentContent: (
    skillId: string,
    replacementId: string,
    expectedRevision: number,
  ): Promise<Snapshot> =>
    isNative
      ? native('replace_current_content', { skillId, replacementId, expectedRevision })
      : demo.demoReplaceCurrentContent(skillId, replacementId, expectedRevision),
  previewSingleContent: (): Promise<SingleContentPreview> =>
    isNative ? native('preview_single_content') : demo.demoPreviewSingleContent(),
  enableSingleContent: (
    expectedRevision: number,
    choices: SingleContentChoice[],
  ): Promise<Snapshot> =>
    isNative
      ? native('enable_single_content', { expectedRevision, choices })
      : demo.demoEnableSingleContent(expectedRevision, choices),
  arrangeLibrary: (): Promise<Snapshot> =>
    isNative ? native('arrange_library') : demo.demoArrangeLibrary(),
  undoContentUpdate: (sourceId: string, expectedRevision: number): Promise<Snapshot> =>
    isNative
      ? native('undo_content_update', { sourceId, expectedRevision })
      : demo.demoUndoContentUpdate(sourceId, expectedRevision),
  previewGitPackage: (url: string, reference: string, subdir: string): Promise<GitPackagePreview> =>
    native('preview_git_package', { url, reference, subdir }),
  importGitPackage: (input: {
    token: string
    revision: number
    selectedPaths: string[]
    presetId?: string
    presetName?: string
    autoAdd: boolean
    removedIds?: string[]
  }): Promise<GitPackageResult> => native('import_git_package', input),
  previewLocalSource: (path: string, sourceId?: string): Promise<LocalSourcePreview> =>
    isNative
      ? native('preview_local_source', { path, sourceId })
      : Promise.reject(new Error('请在桌面应用中扫描本地来源')),
  saveLocalSource: (input: {
    path: string
    name: string
    selectedPaths: string[]
    revision: number
    contentDigest: string
    sourceId?: string
    migrate?: boolean
    keepConflicts?: boolean
  }): Promise<{ snapshot: Snapshot; sourceId: string }> =>
    isNative
      ? native('save_local_source', input)
      : Promise.reject(new Error('请在桌面应用中配置本地来源')),
  revokeLocalSource: (
    sourceId: string,
    targetIds: string[],
    expectedRevision: number,
  ): Promise<Snapshot> =>
    isNative
      ? native('revoke_local_source', { sourceId, targetIds, expectedRevision })
      : Promise.reject(new Error('请在桌面应用中管理本地来源')),
  removeLocalSource: (sourceId: string, expectedRevision: number): Promise<Snapshot> =>
    isNative
      ? native('remove_local_source', { sourceId, targetIds: [], expectedRevision })
      : Promise.reject(new Error('请在桌面应用中管理本地来源')),
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
  previewCollection: async (
    paths: string[],
    mode: 'package' | 'individual',
    adopt: boolean,
  ): Promise<CollectionPreview> => {
    if (isNative) return native('preview_collection', { paths, mode, adopt })
    const [scan, snapshot] = await Promise.all([demo.demoScanMany(paths), demo.demoSnapshot()])
    return { ...scan, paths, mode, revision: snapshot.revision, fingerprint: 'demo' }
  },
  collectSkills: async (input: {
    paths: string[]
    mode: 'package' | 'individual'
    adopt: boolean
    expectedRevision: number
    fingerprint: string
    selectedPaths: string[]
    resolutions: Record<string, string>
  }): Promise<Snapshot> => {
    if (isNative) return native('collect_skills', input)
    let state = await demo.demoSnapshot()
    for (const path of input.paths) {
      const selected = input.selectedPaths.filter((p) => p === path || p.startsWith(`${path}/`))
      if (selected.length) state = await demo.demoImportFolder(path, selected, input.adopt)
    }
    return state
  },
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
  removeUpdateSource: (sourceId: string, expectedRevision: number): Promise<Snapshot> =>
    isNative
      ? native('remove_update_source', { sourceId, expectedRevision })
      : demo.demoRemoveUpdateSource(sourceId, expectedRevision),
  removeTarget: (targetId: string, expectedRevision: number): Promise<Snapshot> =>
    isNative
      ? native('remove_target', { targetId, expectedRevision })
      : demo.demoRemoveTarget(targetId, expectedRevision),
  addTarget: (input: {
    name: string
    tool: string
    scope: string
    path: string
  }): Promise<Snapshot> => (isNative ? native('add_target', input) : demo.demoAddTarget(input)),
  plan: (
    skillIds: string[],
    targetIds: string[],
    claim?: string,
    adoptExisting = false,
    replaceBindingIds: string[] = [],
  ): Promise<DistributionPlan> =>
    isNative
      ? native('plan', { skillIds, targetIds, claim, adoptExisting, replaceBindingIds })
      : demo.demoPlan(skillIds, targetIds, adoptExisting, replaceBindingIds, claim),
  distribute: (
    skillIds: string[],
    targetIds: string[],
    expectedRevision: number,
    claim?: string,
    takeover = false,
    adoptExisting = false,
    replaceBindingIds: string[] = [],
  ): Promise<Snapshot> =>
    isNative
      ? claim?.startsWith('source:')
        ? native('apply_local_source', {
            sourceId: claim.slice(7),
            targetIds,
            expectedRevision,
            takeover,
            adoptExisting,
            replaceBindingIds,
          })
        : claim?.startsWith('preset:')
          ? native('apply_preset', {
              presetId: claim.slice(7),
              targetIds,
              expectedRevision,
              takeover,
              adoptExisting,
              replaceBindingIds,
            })
          : native('distribute', {
              skillIds,
              targetIds,
              claim,
              expectedRevision,
              takeover,
              adoptExisting,
              replaceBindingIds,
            })
      : demo.demoDistribute(
          skillIds,
          targetIds,
          claim,
          expectedRevision,
          adoptExisting,
          replaceBindingIds,
        ),
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
    dailyTime?: string,
  ): Promise<Snapshot> =>
    isNative
      ? native('set_policy', { sourceId, mode, intervalHours, dailyTime })
      : demo.demoSetPolicy(sourceId, mode, intervalHours, dailyTime),
  checkSource: (sourceId: string, apply: boolean): Promise<Snapshot> =>
    isNative ? native('check_source', { sourceId, apply }) : demo.demoCheckSource(sourceId, apply),
  setFollow: (bindingId: string, follow: boolean): Promise<Snapshot> =>
    isNative ? native('set_follow', { bindingId, follow }) : demo.demoSetFollow(bindingId, follow),
  diagnose: (): Promise<{ issues: string[] }> =>
    isNative ? native('diagnose') : demo.demoDiagnose(),
  recover: (taskId: string): Promise<Snapshot> =>
    isNative ? native('recover', { taskId }) : demo.demoRecover(taskId),
  testProxy: (networkProxy: Snapshot['settings']['networkProxy']): Promise<{ message: string }> =>
    isNative
      ? native('test_proxy', { networkProxy })
      : Promise.reject(new Error('请在桌面应用中测试真实网络连接')),
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
  appUpdateInfo: (): Promise<AppUpdateResult> =>
    isNative ? native('app_update_info') : Promise.resolve({ ...demoAppUpdate }),
  checkAppUpdate: (): Promise<AppUpdateResult> =>
    isNative ? native('check_app_update') : Promise.resolve({ ...demoAppUpdate }),
  installAppUpdate: (token: string): Promise<void> =>
    isNative
      ? native('install_app_update', { allowRestart: true, token })
      : Promise.reject(new Error('浏览器演示无法安装应用更新')),
  async onAppUpdateProgress(callback: (progress: AppUpdateProgress) => void): Promise<() => void> {
    if (!isNative) return () => {}
    const { listen } = await import('@tauri-apps/api/event')
    return listen<AppUpdateProgress>('skilldock:app-update', (event) => callback(event.payload))
  },
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
