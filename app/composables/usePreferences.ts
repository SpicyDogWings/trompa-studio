import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { toDbError, type DbErrorPayload } from '~/utils/connection'
import {
  DEFAULT_PREFERENCES,
  normalizePreferences,
  type ColorModePreference,
  type ConnectionPreferences,
  type Preferences,
  type SchemaSidebarPosition
} from '~/utils/preferences'

// Estado a nivel de módulo: `useConnection` y `useSchema` comparten el mismo
// documento, así que el estado no puede vivir por instancia del composable.
// `localStorage` deja de existir como fuente de verdad: manda el TOML del backend.
const preferences = ref<Preferences>(structuredClone(DEFAULT_PREFERENCES))
const isLoading = ref(false)
const error = ref<DbErrorPayload | null>(null)

let loading: Promise<void> | null = null

// Serializa las escrituras: un toggle rápido no puede adelantar a otro.
// `writeId` descarta la respuesta de una escritura que ya quedó obsoleta por
// un update optimista posterior.
let saveQueue: Promise<void> = Promise.resolve()
let writeId = 0

export function usePreferences() {
  function load(): Promise<void> {
    if (loading) return loading

    error.value = null
    isLoading.value = true

    loading = invoke<Preferences>('read_preferences')
      .then((payload) => {
        preferences.value = normalizePreferences(payload)
      })
      .catch((cause) => {
        error.value = toDbError(cause)
        preferences.value = structuredClone(DEFAULT_PREFERENCES)
      })
      .finally(() => {
        isLoading.value = false
        loading = null
      })

    return loading
  }

  function current(): Preferences {
    return preferences.value
  }

  function update(patch: Partial<Preferences>): Promise<void> {
    const next = normalizePreferences({ ...preferences.value, ...patch })

    error.value = null
    preferences.value = next

    saveQueue = saveQueue
      .then(async () => {
        const id = ++writeId
        const saved = await invoke<Preferences>('write_preferences', { preferences: next })

        if (id === writeId) preferences.value = normalizePreferences(saved)
      })
      .catch((cause) => {
        // El estado optimista se conserva: la UI no debe rebotar por un fallo
        // de disco; el error queda visible para reintentar.
        error.value = toDbError(cause)
      })

    return saveQueue
  }

  function setColorMode(colorMode: ColorModePreference): Promise<void> {
    return update({ ui: { colorMode } })
  }

  function setSidebarPosition(sidebarPosition: SchemaSidebarPosition): Promise<void> {
    return update({ schema: { sidebarPosition } })
  }

  function setConnection(connection: ConnectionPreferences): Promise<void> {
    return update({ connection })
  }

  /** Solo para tests: devuelve el singleton a su estado inicial. */
  function resetPreferences(): void {
    preferences.value = structuredClone(DEFAULT_PREFERENCES)
    isLoading.value = false
    error.value = null
    loading = null
    saveQueue = Promise.resolve()
    writeId = 0
  }

  return {
    preferences,
    isLoading,
    error,
    load,
    current,
    update,
    setColorMode,
    setSidebarPosition,
    setConnection,
    resetPreferences
  }
}
