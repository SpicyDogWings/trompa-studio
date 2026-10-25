<script setup lang="ts">
import type { TableTab } from '~/composables/useTableTabs'
import { SCHEMA_ICONS } from '~/utils/schema'

defineProps<{ tabs: TableTab[]; activeId: string | null }>()

const emit = defineEmits<{ select: [id: string]; close: [id: string] }>()
</script>

<template>
  <div class="flex min-w-0 items-center gap-1 overflow-x-auto">
    <p v-if="tabs.length === 0" class="px-2 text-xs text-muted">
      Selecciona una tabla en el esquema.
    </p>

    <div
      v-for="tab in tabs"
      :key="tab.id"
      class="flex shrink-0 items-center gap-1 rounded-md border px-2 py-1 text-xs"
      :class="
        tab.id === activeId
          ? 'border-primary bg-primary/10 text-highlighted'
          : 'border-default text-muted'
      "
    >
      <button
        type="button"
        class="flex min-w-0 items-center gap-1.5"
        @click="emit('select', tab.id)"
      >
        <UIcon :name="SCHEMA_ICONS[tab.kind]" class="size-3.5 shrink-0" />
        <span class="max-w-40 truncate">{{ tab.label }}</span>
        <UIcon
          v-if="tab.status === 'loading'"
          name="i-lucide-loader-circle"
          class="size-3 shrink-0 animate-spin"
        />
        <UIcon
          v-else-if="tab.frozen"
          name="i-lucide-snowflake"
          class="size-3 shrink-0"
        />
      </button>

      <UButton
        variant="ghost"
        size="xs"
        icon="i-lucide-x"
        :aria-label="`Cerrar ${tab.label}`"
        @click.stop="emit('close', tab.id)"
      />
    </div>
  </div>
</template>
