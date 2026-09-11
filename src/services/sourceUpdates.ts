import type { Source } from './types'

export function sourceUpdateState(source: Source) {
  const localReference = source.kind === 'local_reference'
  const needsSetup = !localReference && source.status === 'detached'
  const canCheck =
    !localReference &&
    !needsSetup &&
    ['git', 'local', 'folder', 'catalog', 'clawhub'].includes(source.kind)
  return {
    localReference,
    needsSetup,
    canCheck,
    action: needsSetup
      ? '配置更新来源'
      : localReference
        ? '跟随本地内容'
        : canCheck
          ? '检查更新'
          : '暂不支持更新',
  }
}
