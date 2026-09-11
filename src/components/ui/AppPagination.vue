<script setup lang="ts">
import { computed } from 'vue'
import {
  PaginationRoot,
  PaginationList,
  PaginationListItem,
  PaginationPrev,
  PaginationNext,
  PaginationEllipsis,
} from 'reka-ui'
import { ChevronLeft, ChevronRight } from 'lucide-vue-next'
import AppSelect from './AppSelect.vue'
import Button from './Button.vue'

const props = withDefaults(
  defineProps<{
    total: number
    page?: number
    pageSize?: number
    pageSizeOptions?: number[]
    pageSizes?: number[]
    showSizeChanger?: boolean
    showTotal?: boolean
    hideOnSinglePage?: boolean
    compact?: boolean
  }>(),
  {
    page: 1,
    pageSize: 10,
    pageSizeOptions: () => [10, 20, 50],
    showSizeChanger: true,
    showTotal: true,
    hideOnSinglePage: false,
    compact: false,
  },
)

const emit = defineEmits<{
  'update:page': [page: number]
  'update:pageSize': [size: number]
}>()

// Ensure pageSize is strictly a positive integer >= 1
const safePageSize = computed(() => Math.max(1, Math.floor(Number(props.pageSize) || 10)))

// Total pages must be at least 1
const totalPages = computed(() =>
  Math.max(1, Math.ceil(Math.max(0, props.total) / safePageSize.value)),
)

// Ensure page is strictly a positive integer within [1, totalPages]
const safePage = computed(() =>
  Math.max(1, Math.min(Math.floor(Number(props.page) || 1), totalPages.value)),
)

const rangeStart = computed(() =>
  props.total <= 0 ? 0 : Math.min(props.total, (safePage.value - 1) * safePageSize.value + 1),
)
const rangeEnd = computed(() =>
  Math.min(Math.max(0, props.total), safePage.value * safePageSize.value),
)

// Ensure all size options are strictly positive numbers (> 0)
const sizeOptions = computed(() => {
  const sourceList = props.pageSizes ?? props.pageSizeOptions ?? [10, 20, 50]
  const positiveSizes = sourceList
    .map((s) => Math.floor(Number(s)))
    .filter((s) => Number.isFinite(s) && s > 0)

  // Ensure current safePageSize is in the list
  const uniqueSizes = Array.from(new Set([...positiveSizes, safePageSize.value])).sort(
    (a, b) => a - b,
  )
  return uniqueSizes.map((size) => ({
    value: size,
    label: `${size} 条/页`,
  }))
})

function onPageChange(newPage: number) {
  const validPage = Math.max(1, Math.floor(Number(newPage) || 1))
  emit('update:page', validPage)
}

function onPageSizeChange(newSize: number | undefined) {
  if (newSize !== undefined) {
    const validSize = Math.max(1, Math.floor(Number(newSize) || 10))
    emit('update:pageSize', validSize)
    emit('update:page', 1)
  }
}
</script>

<template>
  <nav
    v-if="!hideOnSinglePage || totalPages > 1"
    class="pagination-bar"
    :class="{ 'pagination-compact': compact }"
    aria-label="分页导航"
  >
    <div v-if="showTotal" class="pagination-info">
      <span class="pagination-summary">
        共 <strong>{{ Math.max(0, total) }}</strong> 条
        <template v-if="total > 0 && !compact"> · 第 {{ rangeStart }}-{{ rangeEnd }} 项 </template>
      </span>
      <div v-if="showSizeChanger && total > 0 && !compact" class="pagination-sizer">
        <AppSelect
          :model-value="safePageSize"
          :options="sizeOptions"
          aria-label="每页显示条数"
          class="pagination-select"
          @update:model-value="onPageSizeChange"
        />
      </div>
    </div>
    <div v-else-if="showSizeChanger && total > 0 && !compact" class="pagination-sizer">
      <AppSelect
        :model-value="safePageSize"
        :options="sizeOptions"
        aria-label="每页显示条数"
        class="pagination-select"
        @update:model-value="onPageSizeChange"
      />
    </div>

    <div v-if="totalPages > 1" class="pagination-nav">
      <PaginationRoot
        :page="safePage"
        :items-per-page="safePageSize"
        :total="Math.max(0, total)"
        :sibling-count="1"
        @update:page="onPageChange"
      >
        <PaginationList v-slot="{ items }" class="pagination-list">
          <PaginationPrev as-child>
            <Button
              variant="ghost"
              size="sm"
              class="pagination-btn pagination-nav-btn"
              :disabled="page <= 1"
              aria-label="上一页"
              title="上一页"
            >
              <ChevronLeft :size="15" />
            </Button>
          </PaginationPrev>

          <template v-for="(item, index) in items" :key="index">
            <PaginationListItem v-if="item.type === 'page'" :value="item.value" as-child>
              <Button
                size="sm"
                :class="
                  item.value === page ? 'pagination-btn pagination-btn-active' : 'pagination-btn'
                "
                :variant="item.value === page ? 'primary' : 'ghost'"
                :aria-current="item.value === page ? 'page' : undefined"
                :aria-label="`第 ${item.value} 页`"
              >
                {{ item.value }}
              </Button>
            </PaginationListItem>
            <PaginationEllipsis v-else as-child>
              <span class="pagination-ellipsis" aria-hidden="true">…</span>
            </PaginationEllipsis>
          </template>

          <PaginationNext as-child>
            <Button
              variant="ghost"
              size="sm"
              class="pagination-btn pagination-nav-btn"
              :disabled="page >= totalPages"
              aria-label="下一页"
              title="下一页"
            >
              <ChevronRight :size="15" />
            </Button>
          </PaginationNext>
        </PaginationList>
      </PaginationRoot>
    </div>
  </nav>
</template>
