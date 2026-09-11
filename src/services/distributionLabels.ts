const labels: Record<string, string> = {
  create: '创建软链',
  reuse: '复用安装',
  keep: '保留现有关系',
  borrow: '保留外部安装',
  takeover: '接管并切换版本',
  update: '更新版本',
  conflict: '需要处理冲突',
}
export const distributionActionLabel = (action: string) => labels[action] ?? action
