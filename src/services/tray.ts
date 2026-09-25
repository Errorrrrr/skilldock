import { invoke } from '@tauri-apps/api/core'
import { isNative } from './api'
import { version as appVersion } from '../../package.json'
import { librarySkillCount } from './librarySkills'

export interface TrayStatus {
  version: string
  paused: boolean
  active: boolean
  skills: number
  sources: number
  scheduled: number
  automatic: number
  theme: string
}
let demoPaused = false
export async function trayAction(
  action: 'ready' | 'status' | 'open' | 'check' | 'configure' | 'pause' | 'quit' | 'hide',
): Promise<TrayStatus> {
  if (isNative) return invoke('tray_action', { action })
  if (action === 'pause') demoPaused = !demoPaused
  const { demoSnapshot } = await import('./demo')
  const snapshot = await demoSnapshot()
  return {
    version: appVersion,
    paused: demoPaused,
    active: false,
    skills: librarySkillCount(snapshot),
    sources: snapshot.sources.length,
    scheduled: snapshot.sources.filter((source) => ['notify', 'auto'].includes(source.policy.mode))
      .length,
    automatic: snapshot.sources.filter((source) => source.policy.mode === 'auto').length,
    theme: snapshot.settings.theme,
  }
}
