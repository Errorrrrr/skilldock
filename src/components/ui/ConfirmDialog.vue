<script setup lang="ts">
import AppDialog from './AppDialog.vue'
import Button from './Button.vue'

withDefaults(
  defineProps<{
    open: boolean
    title: string
    description: string
    confirmText?: string
    danger?: boolean
    busy?: boolean
  }>(),
  { confirmText: '确认', danger: false, busy: false },
)
const emit = defineEmits<{ 'update:open': [value: boolean]; confirm: [] }>()
</script>

<template>
  <AppDialog
    :open="open"
    :title="title"
    :description="description"
    @update:open="emit('update:open', $event)"
  >
    <div class="callout" :class="{ warning: danger }"><slot>请确认操作范围后继续。</slot></div>
    <template #footer
      ><Button @click="emit('update:open', false)">取消</Button
      ><Button :variant="danger ? 'danger' : 'primary'" :disabled="busy" @click="emit('confirm')">{{
        busy ? '处理中…' : confirmText
      }}</Button></template
    >
  </AppDialog>
</template>
