<script setup lang="ts">
import { FolderOpen } from 'lucide-vue-next'
import Button from './ui/Button.vue'
import { api } from '@/services/api'

const model = defineModel<string>({ required: true })
const props = withDefaults(defineProps<{ placeholder?: string; disabled?: boolean }>(), {
  placeholder: '选择或输入目录',
})
async function pick() {
  if (props.disabled) return
  const selected = await api.pickDirectory(model.value)
  if (selected) model.value = selected
}
</script>
<template>
  <div class="path-field">
    <input
      v-model="model"
      class="input mono"
      :disabled="props.disabled"
      :placeholder="props.placeholder"
    /><Button aria-label="选择目录" :disabled="props.disabled" @click="pick"
      ><FolderOpen />选择</Button
    >
  </div>
</template>
