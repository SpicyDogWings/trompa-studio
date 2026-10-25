import { computed, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { toDbError, type DbErrorPayload } from '~/utils/connection'
import {
  buildSchemaTree,
  isSchemaLeaf,
  type SchemaCatalog,
  type SchemaTreeItem
} from '~/utils/schema'

export type SchemaSidebarPosition = 'left' | 'right'

export const SCHEMA_SIDEBAR_STORAGE_KEY = 'trompa:schema-sidebar-position'

function isSchemaSidebarPosition(value: unknown): value is SchemaSidebarPosition {
  return value === 'left' || value === 'right'
}

export function useSchema() {
  const catalog = ref<SchemaCatalog | null>(null)
  const selected = ref<SchemaTreeItem | null>(null)
  const isLoading = ref(false)
  const error = ref<DbErrorPayload | null>(null)
  const position = ref<SchemaSidebarPosition>('right')

  const tree = computed(() => buildSchemaTree(catalog.value))

  // Identifica la petición vigente: las respuestas de peticiones anteriores se descartan.
  let requestId = 0

  async function load() {
    const id = ++requestId
    isLoading.value = true
    error.value = null

    try {
      const data = await invoke<SchemaCatalog>('list_schema')
      if (id !== requestId) return

      catalog.value = data
      selected.value = null
    } catch (cause) {
      if (id !== requestId) return

      error.value = toDbError(cause)
      catalog.value = null
      selected.value = null
    } finally {
      if (id === requestId) isLoading.value = false
    }
  }

  function reset() {
    requestId += 1
    catalog.value = null
    selected.value = null
    error.value = null
    isLoading.value = false
  }

  function select(item: SchemaTreeItem | null) {
    selected.value = item && isSchemaLeaf(item) ? item : null
  }

  function restorePosition() {
    try {
      const stored = localStorage.getItem(SCHEMA_SIDEBAR_STORAGE_KEY)
      position.value = isSchemaSidebarPosition(stored) ? stored : 'right'
    } catch {
      position.value = 'right'
    }
  }

  function setPosition(next: SchemaSidebarPosition) {
    position.value = next

    try {
      localStorage.setItem(SCHEMA_SIDEBAR_STORAGE_KEY, next)
    } catch {
      // Si el almacenamiento no está disponible, la posición sigue en memoria.
    }
  }

  function togglePosition() {
    setPosition(position.value === 'right' ? 'left' : 'right')
  }

  return {
    catalog,
    tree,
    selected,
    isLoading,
    error,
    position,
    load,
    reset,
    select,
    restorePosition,
    setPosition,
    togglePosition
  }
}
