import { computed, ref, type Ref } from 'vue'
import { api } from '@/services/api'
import { useAppStore } from '@/stores/app'
import type { LibrarySkill } from '@/services/librarySkills'

export function useLibraryDistribution(rows: Ref<LibrarySkill[]>) {
  const app = useAppStore()
  const distributionBusy = ref(false)
  const batchOpen = ref(false)
  const batchMode = ref<'distribute' | 'revoke'>('distribute')
  const batchTargetIds = ref<string[]>([])
  const batchRowIds = ref<string[]>([])
  const batchError = ref('')
  const repairSkillId = ref('')
  const repairOpen = ref(false)
  const repairPreview = ref<Awaited<ReturnType<typeof api.previewSnapshotRefresh>> | null>(null)
  const operationError = ref('')
  const batchSummary = computed(
    () =>
      `已选 ${batchRowIds.value.length} 个 Skill、${batchTargetIds.value.length} 个工具。` +
      (batchMode.value === 'distribute'
        ? '从统一库分发，已分发的项目自动跳过。'
        : '取消所选工具的手动分发，统一库内容和预设引用保留。'),
  )

  function openBatch(ids: string[], mode: 'distribute' | 'revoke') {
    batchRowIds.value = [...ids]
    batchMode.value = mode
    batchTargetIds.value = []
    batchError.value = ''
    batchOpen.value = true
  }
  function closeBatch(open: boolean) {
    if (!distributionBusy.value) batchOpen.value = open
  }

  async function apply(ids: string[], targets: string[], mode: 'distribute' | 'revoke') {
    if (distributionBusy.value || app.loading) return false
    distributionBusy.value = true
    app.error = ''
    operationError.value = ''
    repairSkillId.value = ''
    let completed = 0
    try {
      if (mode === 'revoke') {
        const bindings = rows.value
          .filter((row) => ids.includes(row.id))
          .flatMap((row) => row.bindings)
          .filter((binding) => targets.includes(binding.targetId))
        const manual = bindings.filter((binding) => binding.claims.includes('manual'))
        const retained = bindings.filter((binding) =>
          binding.claims.some((claim) => claim !== 'manual'),
        ).length
        const borrowed = manual.filter((binding) => binding.borrowed).length
        if (!manual.length) {
          app.notice = retained
            ? '所选分发由预设或本地来源管理，请到对应页面取消'
            : '所选工具没有需要取消的分发'
          return true
        }
        return await app.mutate(
          () => api.revoke(manual.map((binding) => binding.id)),
          `已取消 ${manual.length} 项手动分发` +
            (retained ? `；${retained} 项仍由预设或本地来源使用` : '') +
            (borrowed ? `；${borrowed} 项原有链接保留` : ''),
        )
      }
      // Different tools may already bind different copies of the same Skill.
      // Submit only missing rows per target, with a fresh revision for every transaction.
      for (const targetId of targets) {
        const current = rows.value.filter((row) => ids.includes(row.id))
        const missing = current.filter(
          (row) => !row.tools.some((tool) => tool.id === targetId && tool.active),
        )
        if (!missing.length) continue
        const skillIds = missing.map((row) => row.id)
        const plan = await api.plan(skillIds, [targetId])
        const conflicts = plan.items.filter((item) => item.error)
        if (conflicts.length)
          throw new Error(conflicts.map((item) => `${item.path}：${item.error}`).join('；'))
        const ok = await app.mutate(
          () => api.distribute(skillIds, [targetId], plan.revision),
          '分发完成',
        )
        if (!ok) throw new Error(app.error)
        completed += missing.length
      }
      app.notice = completed ? `已完成 ${completed} 项分发` : '所选 Skill 已分发到这些工具'
      return true
    } catch (error) {
      app.error =
        (completed ? `已完成 ${completed} 项分发，其余未完成：` : '') +
        (error instanceof Error ? error.message : '分发失败，请重试')
      operationError.value = app.error
      if (
        app.error.includes('快照') &&
        (app.error.includes('变化') || app.error.includes('修改'))
      ) {
        repairSkillId.value =
          rows.value.find((row) => ids.includes(row.id) && app.error.includes(row.bundleDigest))
            ?.id ?? (ids.length === 1 ? ids[0]! : '')
      }
      await app.refresh(true)
      return false
    } finally {
      distributionBusy.value = false
    }
  }

  async function toggle(rowId: string, targetId: string) {
    const row = rows.value.find((row) => row.id === rowId)
    if (!row) return
    const tool = row.tools.find((tool) => tool.id === targetId)
    if (!tool || tool.protected) return
    await apply([rowId], [targetId], tool.active ? 'revoke' : 'distribute')
  }
  async function submitBatch() {
    if (!batchTargetIds.value.length) return
    batchError.value = ''
    const ok = await apply([...batchRowIds.value], [...batchTargetIds.value], batchMode.value)
    if (ok) batchOpen.value = false
    else batchError.value = app.error
  }

  async function previewRepair() {
    if (!repairSkillId.value || distributionBusy.value) return
    distributionBusy.value = true
    try {
      repairPreview.value = await api.previewSnapshotRefresh(repairSkillId.value)
      repairOpen.value = true
    } catch (error) {
      operationError.value = error instanceof Error ? error.message : '无法预览重新收录'
    } finally {
      distributionBusy.value = false
    }
  }
  async function repair() {
    if (!repairPreview.value || distributionBusy.value) return
    distributionBusy.value = true
    const preview = repairPreview.value
    const ok = await app.mutate(
      () => api.refreshSnapshot(repairSkillId.value, preview.revision, preview.contentDigest),
      '已重新收录当前内容，请再次点击工具分发',
    )
    distributionBusy.value = false
    if (ok) {
      repairOpen.value = false
      repairSkillId.value = ''
      operationError.value = ''
      batchError.value = ''
    } else operationError.value = app.error
  }

  return {
    operationError,
    repairSkillId,
    repairOpen,
    repairPreview,
    previewRepair,
    repair,
    distributionBusy,
    batchOpen,
    batchMode,
    batchTargetIds,
    batchError,
    batchSummary,
    openBatch,
    closeBatch,
    submitBatch,
    toggle,
  }
}
