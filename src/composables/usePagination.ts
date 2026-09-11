import { computed, ref, watch, type ComputedRef, type Ref } from 'vue'

export interface PaginationOptions {
  initialPage?: number
  initialPageSize?: number
}

export function usePagination<T>(
  source: Ref<T[]> | ComputedRef<T[]> | (() => T[]),
  options: PaginationOptions = {},
) {
  const items = typeof source === 'function' ? computed(source) : source
  const safeInitialPageSize = Math.max(1, Math.floor(Number(options.initialPageSize) || 10))
  const safeInitialPage = Math.max(1, Math.floor(Number(options.initialPage) || 1))

  const page = ref(safeInitialPage)
  const pageSize = ref(safeInitialPageSize)

  const total = computed(() => items.value.length)
  const totalPages = computed(() => {
    const size = Math.max(1, Math.floor(Number(pageSize.value) || 1))
    return Math.max(1, Math.ceil(total.value / size))
  })

  // Ensure pageSize and page are always positive numbers and within valid range
  watch(
    [total, pageSize, page],
    () => {
      // Validate pageSize: must be positive integer >= 1
      if (
        typeof pageSize.value !== 'number' ||
        !Number.isFinite(pageSize.value) ||
        pageSize.value < 1
      ) {
        pageSize.value = Math.max(1, Math.floor(Number(pageSize.value) || 10))
      } else if (!Number.isInteger(pageSize.value)) {
        pageSize.value = Math.floor(pageSize.value)
      }

      // Validate page: must be between 1 and totalPages
      if (page.value > totalPages.value) {
        page.value = totalPages.value
      }
      if (typeof page.value !== 'number' || !Number.isFinite(page.value) || page.value < 1) {
        page.value = 1
      }
    },
    { flush: 'sync' },
  )

  const pagedItems = computed(() => {
    const size = Math.max(1, Math.floor(Number(pageSize.value) || 1))
    const validPage = Math.max(1, Math.min(page.value, totalPages.value))
    const start = (validPage - 1) * size
    return items.value.slice(start, start + size)
  })

  function resetPage() {
    page.value = 1
  }

  function setPage(newPage: number) {
    const validPage = Math.max(1, Math.floor(Number(newPage) || 1))
    page.value = Math.max(1, Math.min(validPage, totalPages.value))
  }

  function setPageSize(newSize: number) {
    const validSize = Math.max(1, Math.floor(Number(newSize) || 1))
    pageSize.value = validSize
  }

  return {
    page,
    pageSize,
    total,
    totalPages,
    pagedItems,
    resetPage,
    setPage,
    setPageSize,
  }
}
