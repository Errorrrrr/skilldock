import { invoke } from '@tauri-apps/api/core'
import { isNative } from './api'

export interface TrayStatus {
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
  return {
    paused: demoPaused,
    active: false,
    skills: 12,
    sources: 3,
    scheduled: 0,
    automatic: 0,
    theme: 'light',
  }
}
