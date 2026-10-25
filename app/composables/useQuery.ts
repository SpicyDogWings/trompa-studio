import { ref } from 'vue'
import { toDbError, type DbErrorPayload } from '~/utils/connection'

export type QueryCell = string | null

/** Eventos que emite `start_query` desde el backend, en orden. */
export type QueryEvent =
  | { type: 'started'; queryId: number }
  | { type: 'columns'; columns: string[] }
  | { type: 'rows'; rows: QueryCell[][] }
  | { type: 'done'; rowsAffected: number; elapsedMs: number }
  | { type: 'error'; message: string; code: string | null }

export function useQuery() {
  const isRunning = ref(false)
  const columns = ref<string[]>([])
  const rows = ref<QueryCell[][]>([])
  const rowsAffected = ref<number | null>(null)
  const elapsedMs = ref<number | null>(null)
  const error = ref<DbErrorPayload | null>(null)

  // Id de la consulta en curso, para poder cancelarla.
  let queryId: number | null = null

  function apply(event: QueryEvent) {
    switch (event.type) {
      case 'started':
        queryId = event.queryId
        break
      case 'columns':
        // Cada sentencia trae su propia descripción: se muestra la última,
        // igual que `run_query`.
        columns.value = event.columns
        rows.value = []
        break
      case 'rows':
        rows.value.push(...event.rows)
        break
      case 'done':
        rowsAffected.value = event.rowsAffected
        elapsedMs.value = event.elapsedMs
        isRunning.value = false
        break
      case 'error':
        error.value = event.code
          ? { message: event.message, code: event.code }
          : { message: event.message }
        isRunning.value = false
        break
    }
  }

  async function run(sql: string) {
    reset()
    isRunning.value = true

    try {
      const { Channel, invoke } = await import('@tauri-apps/api/core')
      const channel = new Channel<QueryEvent>()
      channel.onmessage = apply

      const started = await invoke<number>('start_query', { sql, onEvent: channel })
      // El evento `started` también trae el id; cualquiera de los dos vale.
      queryId = started ?? queryId
    } catch (cause) {
      error.value = toDbError(cause)
      isRunning.value = false
    }
  }

  async function cancel() {
    if (queryId === null) return

    try {
      const { invoke } = await import('@tauri-apps/api/core')
      await invoke('cancel_query', { queryId })
    } catch (cause) {
      error.value = toDbError(cause)
      isRunning.value = false
    }
  }

  function reset() {
    isRunning.value = false
    columns.value = []
    rows.value = []
    rowsAffected.value = null
    elapsedMs.value = null
    error.value = null
    queryId = null
  }

  return {
    isRunning,
    columns,
    rows,
    rowsAffected,
    elapsedMs,
    error,
    run,
    cancel,
    reset
  }
}
