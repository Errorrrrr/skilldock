import type { CatalogItem, CatalogSite } from './types'

const countFormatter = new Intl.NumberFormat('zh-CN')

export function catalogUsage(item: CatalogItem): string {
  if (
    typeof item.downloads === 'number' &&
    Number.isSafeInteger(item.downloads) &&
    item.downloads >= 0
  )
    return `下载量 ${countFormatter.format(item.downloads)}`
  if (
    typeof item.installs === 'number' &&
    Number.isSafeInteger(item.installs) &&
    item.installs >= 0
  )
    return `安装量 ${countFormatter.format(item.installs)}`
  return '下载量未提供'
}

export function catalogHomepage(item: CatalogItem | null): string | undefined {
  if (!item?.homepageUrl) return undefined
  try {
    const url = new URL(item.homepageUrl)
    if (url.protocol === 'https:' && !url.username && !url.password) return url.href
  } catch {
    // Older catalog responses may not include a usable homepage.
  }
  return undefined
}

export const defaultCatalogSites: CatalogSite[] = [
  { name: 'ClawHub', url: 'https://clawhub.ai' },
  { name: 'SkillHub', url: 'https://skillhub.cn' },
  { name: 'Skills.sh', url: 'https://skills.sh' },
]

// Catalog responses use canonical API hosts; settings show the website address.
export function catalogSiteKey(value: string): string {
  const url = value.trim().replace(/\/+$/, '').toLowerCase()
  if (['clawhub', 'clawhub.ai', 'https://clawhub.ai'].includes(url)) return 'https://clawhub.ai'
  if (['skillhub', 'skillhub.cn', 'https://skillhub.cn', 'https://api.skillhub.cn'].includes(url))
    return 'https://skillhub.cn'
  if (['skills.sh', 'https://skills.sh', 'https://www.skills.sh'].includes(url))
    return 'https://skills.sh'
  return value.trim().replace(/\/+$/, '')
}

export function normalizeCatalogSites(sites: Array<CatalogSite | string>): CatalogSite[] {
  return sites.map((site) => {
    if (typeof site !== 'string') return { ...site }
    const url = catalogSiteKey(site)
    return { name: defaultCatalogSites.find((entry) => entry.url === url)?.name || site, url }
  })
}
