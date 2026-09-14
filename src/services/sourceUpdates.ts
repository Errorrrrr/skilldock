import type { Source } from './types'

export function sourceUpdateState(source: Source) {
  const removed = source.updatesRemoved === true
  const localReference = source.kind === 'local_reference'
  // 归集批次记录安装位置，不代表成员共享一个上游仓库。
  const collected = source.kind === 'local' && source.status === 'detached'
  const needsSetup = !removed && source.kind === 'git' && source.status === 'detached'
  const remote =
    source.kind === 'git'
      ? /^(https:\/\/|ssh:\/\/|[^/\s]+@[^:]+:)/.test(source.url)
      : ['catalog', 'clawhub'].includes(source.kind) && !!source.url && !!source.reference
  const canCheck = !removed && !collected && !needsSetup && remote
  const manual =
    !collected &&
    (['local', 'folder'].includes(source.kind) ||
      (source.kind === 'git' && source.url.startsWith('/')))

  return {
    removed,
    manual,
    localReference,
    collected,
    needsSetup,
    canCheck,
    action: removed
      ? '已移除更新管理'
      : collected
        ? '未关联原始来源'
        : needsSetup
          ? '配置更新来源'
          : localReference
            ? '跟随本地内容'
            : manual
              ? '手动同步本地文件夹'
              : canCheck
                ? '检查更新'
                : '暂不支持更新',
  }
}
