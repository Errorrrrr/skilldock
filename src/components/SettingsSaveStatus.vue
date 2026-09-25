<script setup lang="ts">
import { computed } from 'vue'
import { useRoute } from 'vue-router'
import { useSettingsEditor } from '@/stores/settingsEditor'
const editor = useSettingsEditor()
const route = useRoute()
const failed = computed(() => Object.values(editor.errors).some(Boolean))
</script>
<template>
  <div
    v-if="route.path !== '/settings' && editor.dirty"
    class="callout"
    :class="{ warning: failed }"
    role="status"
    style="margin-bottom: 12px"
  >
    {{ failed ? '有设置未保存，编辑内容已保留。' : '设置正在自动保存…' }}
    <RouterLink to="/settings">{{ failed ? '查看并处理' : '查看设置' }}</RouterLink>
  </div>
</template>
