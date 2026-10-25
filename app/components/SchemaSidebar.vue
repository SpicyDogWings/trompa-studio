<script setup lang="ts">
import type { SchemaSidebarPosition } from '~/composables/useSchema'
import type { DbErrorPayload } from '~/utils/connection'
import { isSchemaLeaf, type SchemaTreeItem } from '~/utils/schema'

const props = defineProps<{
  database: string
  items: SchemaTreeItem[]
  selected: SchemaTreeItem | null
  loading: boolean
  error: DbErrorPayload | null
  position: SchemaSidebarPosition
}>()

const emit = defineEmits<{
  'update:selected': [item: SchemaTreeItem | null]
  'update:position': [position: SchemaSidebarPosition]
  retry: []
}>()

function getKey(item: SchemaTreeItem): string {
  return item.id
}

function onSelect(value: SchemaTreeItem | undefined): void {
  emit('update:selected', isSchemaLeaf(value) ? value : null)
}

function togglePosition(): void {
  emit('update:position', props.position === 'right' ? 'left' : 'right')
}
</script>

<template>
  <aside class="flex h-full w-80 shrink-0 flex-col bg-elevated/20">
    <header class="flex shrink-0 items-center gap-2 px-3 py-2.5">
      <UIcon name="i-lucide-folder-tree" class="size-4 shrink-0 text-muted" />
      <div class="min-w-0 flex-1">
        <p class="text-sm font-medium text-highlighted">
          Esquema
        </p>
        <p class="truncate text-xs text-muted" :title="database">
          {{ database }}
        </p>
      </div>
      <UButton
        color="neutral"
        variant="ghost"
        size="xs"
        :icon="position === 'right' ? 'i-lucide-panel-left' : 'i-lucide-panel-right'"
        :aria-label="position === 'right' ? 'Mover esquema a la izquierda' : 'Mover esquema a la derecha'"
        @click="togglePosition"
      />
    </header>

    <div class="min-h-0 flex-1 overflow-y-auto px-2 py-2">
      <div v-if="loading" class="space-y-2">
        <USkeleton v-for="n in 6" :key="n" class="h-6 w-full" />
      </div>

      <div v-else-if="error" class="space-y-3">
        <UAlert
          color="error"
          variant="subtle"
          icon="i-lucide-circle-alert"
          :title="error.message"
          :description="error.code ? `SQLSTATE ${error.code}` : undefined"
        />
        <UButton
          label="Reintentar"
          icon="i-lucide-refresh-cw"
          size="xs"
          color="neutral"
          variant="subtle"
          @click="emit('retry')"
        />
      </div>

      <p v-else-if="items.length === 0" class="px-2 py-4 text-sm text-muted">
        No se encontraron tablas, vistas, funciones ni enums.
      </p>

      <UTree
        v-else
        :items="items"
        :model-value="selected ?? undefined"
        :get-key="getKey"
        selection-behavior="replace"
        @update:model-value="onSelect"
      />
    </div>

    <footer v-if="selected" class="shrink-0 border-t border-default px-3 py-2">
      <p class="truncate font-mono text-xs text-muted">
        Seleccionado: {{ selected.schema }}.{{ selected.name }}
      </p>
    </footer>
  </aside>
</template>
