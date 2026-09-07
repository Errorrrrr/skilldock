<script setup lang="ts">
withDefaults(defineProps<{ tone?: 'neutral' | 'blue' | 'green' | 'amber' | 'red' }>(), {
  tone: 'neutral',
})
</script>
<template>
  <span class="badge" :class="`badge-${tone}`"><slot /></span>
</template>
