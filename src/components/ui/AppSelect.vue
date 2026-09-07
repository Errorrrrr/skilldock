<script setup lang="ts" generic="T extends string | number">
import { computed } from 'vue'
import {
  SelectRoot,
  SelectTrigger,
  SelectValue,
  SelectPortal,
  SelectContent,
  SelectViewport,
  SelectItem,
  SelectItemText,
  SelectItemIndicator,
  SelectScrollUpButton,
  SelectScrollDownButton,
} from 'reka-ui'
import { Check, ChevronDown, ChevronUp } from 'lucide-vue-next'

defineOptions({ inheritAttrs: false })
const props = withDefaults(
  defineProps<{
    options: readonly { value: string | number; label: string; disabled?: boolean }[]
    disabled?: boolean
    placeholder?: string
  }>(),
  { placeholder: '请选择' },
)
const model = defineModel<T>()
// Encode every value so empty-string filters remain selectable and numbers retain their type.
const encode = (value: string | number) => JSON.stringify([typeof value, value])
const value = computed(() => (model.value === undefined ? undefined : encode(model.value)))
function select(value: unknown) {
  const option = props.options.find((option) => encode(option.value) === value)
  if (option && !option.disabled) model.value = option.value as T
}
</script>

<template>
  <SelectRoot :model-value="value" :disabled="disabled" @update:model-value="select">
    <SelectTrigger v-bind="$attrs" class="select select-trigger">
      <SelectValue :placeholder="placeholder" class="select-value" />
      <ChevronDown class="select-chevron" :size="16" aria-hidden="true" />
    </SelectTrigger>
    <SelectPortal>
      <SelectContent
        class="select-content"
        position="popper"
        :side-offset="5"
        :collision-padding="12"
      >
        <SelectScrollUpButton class="select-scroll"><ChevronUp :size="14" /></SelectScrollUpButton>
        <SelectViewport class="select-viewport">
          <SelectItem
            v-for="option in options"
            :key="encode(option.value)"
            :value="encode(option.value)"
            :disabled="option.disabled"
            :text-value="option.label"
            class="select-item"
          >
            <SelectItemText>{{ option.label }}</SelectItemText>
            <SelectItemIndicator class="select-indicator"><Check :size="15" /></SelectItemIndicator>
          </SelectItem>
          <div v-if="!options.length" class="select-empty">暂无可选项</div>
        </SelectViewport>
        <SelectScrollDownButton class="select-scroll"
          ><ChevronDown :size="14"
        /></SelectScrollDownButton>
      </SelectContent>
    </SelectPortal>
  </SelectRoot>
</template>
