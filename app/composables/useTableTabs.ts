import { computed, onScopeDispose, ref, type ComputedRef, type Ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { toDbError, type DbErrorPayload } from '~/utils/connection'
import type { SchemaObjectKind, SchemaTreeItem } from '~/utils/schema'
import {
  FREEZE_AFTER_MS,
  FREEZE_TICK_MS,
  PAGE_SIZE,
  rowsToRecords,
  type TablePage,
  type TableRow
} from '~/utils/table'

export interface TableTab {
  /** El `id` de la hoja del árbol: `${kind}:${schema}:${oid}`. */
  id: string
  schema: string
  name: string
  kind: SchemaObjectKind
  /** `${schema}.${name}`. */
  label: string
  /** 1-based. */
  page: number
  /** PAGE_SIZE al crear. */
  pageSize: number
  total: number
  columns: string[]
  rows: TableRow[]
  status: 'idle' | 'loading' | 'ready' | 'error'
  error: DbErrorPayload | null
  frozen: boolean
  inactiveSince: number | null
}

export function useTableTabs(): {
  tabs: Ref<TableTab[]>
  activeId: Ref<string | null>
  activeTab: ComputedRef<TableTab | null>
  open(item: SchemaTreeItem): void
  close(id: string): void
  activate(id: string): void
  goToPage(page: number): void
  reload(): void
  resetTabs(): void
} {
  const tabs = ref<TableTab[]>([])
  const activeId = ref<string | null>(null)

  const activeTab = computed(() => tabs.value.find((tab) => tab.id === activeId.value) ?? null)

  /** Identifica la petición vigente por pestaña: las respuestas viejas se descartan. */
  const requests = new Map<string, number>()

  async function load(tab: TableTab) {
    const requestId = (requests.get(tab.id) ?? 0) + 1
    requests.set(tab.id, requestId)
    tab.status = 'loading'
    tab.error = null

    try {
      const data = await invoke<TablePage>('fetch_table_page', {
        schema: tab.schema,
        table: tab.name,
        page: tab.page,
        pageSize: tab.pageSize
      })
      if (requests.get(tab.id) !== requestId) return

      tab.columns = data.columns
      tab.rows = rowsToRecords(data.columns, data.rows)
      tab.total = data.total
      // El backend ya recorta la página, así que se guarda la que devolvió.
      tab.page = data.page
      tab.pageSize = data.pageSize
      tab.status = 'ready'
    } catch (cause) {
      if (requests.get(tab.id) !== requestId) return

      // Las filas anteriores se conservan para no vaciar la vista al reintentar.
      tab.status = 'error'
      tab.error = toDbError(cause)
    }
  }

  function activate(id: string) {
    const next = tabs.value.find((tab) => tab.id === id)
    if (!next) return

    if (activeId.value !== null && activeId.value !== id) {
      const previous = tabs.value.find((tab) => tab.id === activeId.value)
      if (previous) previous.inactiveSince = Date.now()
    }

    activeId.value = id
    next.inactiveSince = null

    const wasFrozen = next.frozen
    next.frozen = false

    if ((wasFrozen || next.status === 'idle') && (next.kind === 'table' || next.kind === 'view'))
      void load(next)
  }

  function open(item: SchemaTreeItem) {
    if (tabs.value.some((tab) => tab.id === item.id)) {
      activate(item.id)
      return
    }

    tabs.value.push({
      id: item.id,
      schema: item.schema,
      name: item.name,
      kind: item.kind as SchemaObjectKind,
      label: `${item.schema}.${item.name}`,
      page: 1,
      pageSize: PAGE_SIZE,
      total: 0,
      columns: [],
      rows: [],
      status: 'idle',
      error: null,
      frozen: false,
      inactiveSince: null
    })

    activate(item.id)
  }

  function goToPage(page: number) {
    const tab = activeTab.value
    if (!tab) return
    if (page === tab.page && tab.status === 'ready') return

    tab.page = page
    if (tab.kind === 'table' || tab.kind === 'view') void load(tab)
  }

  function reload() {
    const tab = activeTab.value
    if (!tab || (tab.kind !== 'table' && tab.kind !== 'view')) return

    void load(tab)
  }

  function close(id: string) {
    const index = tabs.value.findIndex((tab) => tab.id === id)
    if (index === -1) return

    requests.delete(id)
    tabs.value.splice(index, 1)

    if (activeId.value !== id) return

    // La siguiente pestaña ocupa el índice borrado; si no queda, la anterior.
    const next = tabs.value[index] ?? tabs.value[tabs.value.length - 1]
    if (next) activate(next.id)
    else activeId.value = null
  }

  function resetTabs() {
    tabs.value = []
    activeId.value = null
    requests.clear()
  }

  // El render en servidor no puede programar intervalos: solo se crea en el cliente.
  const timer =
    typeof window === 'undefined'
      ? undefined
      : setInterval(() => {
          const now = Date.now()

          for (const tab of tabs.value) {
            if (tab.id === activeId.value) continue
            if (tab.inactiveSince === null) continue
            if (tab.status === 'loading') continue
            if (now - tab.inactiveSince < FREEZE_AFTER_MS) continue

            tab.frozen = true
            tab.status = 'idle'
            tab.columns = []
            tab.rows = []
            tab.total = 0
            tab.error = null
            // La página abierta se conserva; se recarga al volver a enfocar.
            requests.delete(tab.id)
          }
        }, FREEZE_TICK_MS)

  onScopeDispose(() => clearInterval(timer))

  return {
    tabs,
    activeId,
    activeTab,
    open,
    close,
    activate,
    goToPage,
    reload,
    resetTabs
  }
}
