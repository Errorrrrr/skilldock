import { api } from '@/services/api'
import { createAppUpdater } from '@/services/appUpdater'
import { version as buildVersion } from '../../package.json'

const updater = createAppUpdater(api, buildVersion)
export function useAppUpdater() {
  return updater
}
