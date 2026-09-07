<script setup lang="ts">
import {
  DialogRoot,
  DialogPortal,
  DialogOverlay,
  DialogContent,
  DialogTitle,
  DialogDescription,
  DialogClose,
} from 'reka-ui'
import { X } from 'lucide-vue-next'

defineProps<{ open: boolean; title: string; description?: string }>()
const emit = defineEmits<{ 'update:open': [value: boolean] }>()
</script>

<template>
  <DialogRoot :open="open" @update:open="emit('update:open', $event)">
    <DialogPortal>
      <DialogOverlay class="dialog-overlay" />
      <DialogContent class="sheet-content">
        <header class="sheet-header">
          <div>
            <DialogTitle class="dialog-title">{{ title }}</DialogTitle
            ><DialogDescription v-if="description" class="dialog-description">{{
              description
            }}</DialogDescription>
          </div>
          <DialogClose class="sheet-close" aria-label="关闭详情"><X /></DialogClose>
        </header>
        <div class="sheet-body"><slot /></div>
      </DialogContent>
    </DialogPortal>
  </DialogRoot>
</template>
