<script setup lang="ts">
import { Inbox } from 'lucide-vue-next'
defineProps<{ title: string; description: string }>()
</script>
<template>
  <div class="empty">
    <div>
      <div class="empty-icon"><Inbox /></div>
      <h3>{{ title }}</h3>
      <p>{{ description }}</p>
      <slot />
    </div>
  </div>
</template>
