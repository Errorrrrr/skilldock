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
  defineProps<{
    open: boolean
    title: string
    description?: string
    large?: boolean
    fixedLayout?: boolean
  }>(),
  { description: '', large: false },
)
const emit = defineEmits<{ 'update:open': [value: boolean] }>()
</script>

<template>
  <DialogRoot :open="open" @update:open="emit('update:open', $event)">
    <DialogPortal>
      <DialogOverlay class="dialog-overlay" />
      <DialogContent
        class="dialog-content"
        :class="{ 'dialog-lg': large, 'dialog-fixed-layout': fixedLayout }"
      >
        <header class="dialog-header">
          <DialogTitle class="dialog-title">{{ title }}</DialogTitle>
          <DialogDescription v-if="description" class="dialog-description">{{
            description
          }}</DialogDescription>
        </header>
        <div v-if="$slots.notice" class="dialog-notice" role="status"><slot name="notice" /></div>
        <div class="dialog-body"><slot /></div>
        <div v-if="$slots.options" class="dialog-options"><slot name="options" /></div>
        <footer v-if="$slots.footer" class="dialog-footer"><slot name="footer" /></footer>
      </DialogContent>
    </DialogPortal>
  </DialogRoot>
</template>

<style scoped>
.dialog-fixed-layout {
  display: flex;
  flex-direction: column;
  height: calc(100dvh - 40px);
  overflow: hidden;
}
.dialog-fixed-layout .dialog-header,
.dialog-fixed-layout .dialog-footer {
  flex-shrink: 0;
}
.dialog-fixed-layout .dialog-body {
  padding: 16px 24px;
  display: flex;
  flex-direction: column;
  flex: 1;
  min-height: 0;
  overflow: hidden;
}
.dialog-fixed-layout .dialog-body > :deep(*) {
  flex-shrink: 0;
}
.dialog-fixed-layout .dialog-body > :deep(.git-package-import) {
  flex: 1;
  min-height: 0;
}
</style>

<style scoped>
.dialog-options {
  flex-shrink: 0;
  padding: 12px 24px;
  border-top: 1px solid var(--line);
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.dialog-options :deep(p) {
  margin: 0;
}
</style>

<style scoped>
.dialog-notice {
  flex-shrink: 0;
  max-height: 96px;
  overflow: auto;
  overscroll-behavior: contain;
  margin: 12px 24px 0;
  font-size: 12px;
}
.dialog-notice :deep(p) {
  margin: 0;
}
.dialog-notice :deep(.callout) {
  padding: 8px 10px;
}
</style>
