import { ref } from 'vue'
import type { CatalogInstallSession } from '@/services/catalogInstall'

// Keep unfinished steps while the user visits target or preset pages to resolve conflicts.
export const catalogInstallSessions = ref<Record<string, CatalogInstallSession>>({})
