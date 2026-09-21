<script setup lang="ts">
import { computed } from 'vue'
import { LoaderCircle } from 'lucide-vue-next'
import { cva, type VariantProps } from 'class-variance-authority'
import { cn } from '@/lib/utils'

const styles = cva('btn', {
  variants: {
    variant: {
      primary: 'btn-primary',
      secondary: 'btn-secondary',
      ghost: 'btn-ghost',
      danger: 'btn-danger',
    },
    size: { default: '', sm: 'btn-sm', icon: 'btn-icon' },
  },
  defaultVariants: { variant: 'secondary', size: 'default' },
})
type ButtonVariants = VariantProps<typeof styles>
const props = withDefaults(
  defineProps<{
    variant?: ButtonVariants['variant']
    size?: ButtonVariants['size']
    class?: string
    type?: 'button' | 'submit'
    loading?: boolean
    disabled?: boolean
  }>(),
  { variant: 'secondary', size: 'default', type: 'button' },
)
const classes = computed(() =>
  cn(styles({ variant: props.variant, size: props.size }), props.class),
)
</script>

<template>
  <button
    :type="type"
    :class="classes"
    :disabled="disabled || loading"
    :aria-busy="loading || undefined"
  >
    <LoaderCircle v-if="loading" class="button-spinner" aria-hidden="true" />
    <slot />
  </button>
</template>

<style scoped>
.button-spinner {
  animation: button-spin 1s linear infinite;
}
@keyframes button-spin {
  to {
    transform: rotate(360deg);
  }
}
@media (prefers-reduced-motion: reduce) {
  .button-spinner {
    animation: none;
  }
}
</style>
