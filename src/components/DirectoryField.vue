<script setup lang="ts">
import { FolderOpen } from 'lucide-vue-next'
import Button from './ui/Button.vue'
import { api } from '@/services/api'

const model = defineModel<string>({ required: true })
const props = withDefaults(defineProps<{ placeholder?: string }>(), {
  placeholder: '选择或输入目录',
})
async function pick() {
  const selected = await api.pickDirectory(model.value)
  if (selected) model.value = selected
}
</script>
<template>
  <div class="path-field">
    <input v-model="model" class="input mono" :placeholder="props.placeholder" /><Button
      aria-label="选择目录"
      @click="pick"
      ><FolderOpen />选择</Button
    >
  </div>
</template>
