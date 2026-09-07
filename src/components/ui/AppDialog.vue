<script setup lang="ts">
import {
  DialogRoot,
  DialogPortal,
  DialogOverlay,
  DialogContent,
  DialogTitle,
  DialogDescription,
} from 'reka-ui'

withDefaults(
  defineProps<{ open: boolean; title: string; description?: string; large?: boolean }>(),
  { description: '', large: false },
)
const emit = defineEmits<{ 'update:open': [value: boolean] }>()
</script>

<template>
  <DialogRoot :open="open" @update:open="emit('update:open', $event)">
    <DialogPortal>
      <DialogOverlay class="dialog-overlay" />
      <DialogContent class="dialog-content" :class="{ 'dialog-lg': large }">
        <header class="dialog-header">
          <DialogTitle class="dialog-title">{{ title }}</DialogTitle>
          <DialogDescription v-if="description" class="dialog-description">{{
            description
          }}</DialogDescription>
        </header>
        <div class="dialog-body"><slot /></div>
        <footer v-if="$slots.footer" class="dialog-footer"><slot name="footer" /></footer>
      </DialogContent>
    </DialogPortal>
  </DialogRoot>
</template>
