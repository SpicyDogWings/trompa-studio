import { reactive, ref } from 'vue'
import {
  DEFAULT_FIELDS,
  DEFAULT_URL,
  buildConnectRequest,
  connectionLabel as formatConnectionLabel,
  toDbError,
  validateConnection,
  type ConnectionFields,
  type ConnectionInfo,
  type ConnectionMode,
  type DbErrorPayload
} from '~/utils/connection'
import { connectionPreferences, fieldsFromConnection } from '~/utils/preferences'
import { usePreferences } from './usePreferences'

export function useConnection() {
  const isDesktop = ref(false)
  const isConnecting = ref(false)
  const showPassword = ref(false)
  const mode = ref<ConnectionMode>('fields')

  const fields = reactive<ConnectionFields>({ ...DEFAULT_FIELDS })
  const url = ref(DEFAULT_URL)

  const error = ref<DbErrorPayload | null>(null)
  const connection = ref<ConnectionInfo | null>(null)
  const connectionLabel = ref('')

  function restore() {
    // Se llama después de `load()` de preferencias: el estado ya está hidratado.
    // `fieldsFromConnection` nunca devuelve la contraseña guardada.
    const saved = fieldsFromConnection(usePreferences().current().connection)

    mode.value = saved.mode
    url.value = saved.url
    Object.assign(fields, saved.fields)
  }

  async function checkDesktop() {
    try {
      const { isTauri } = await import('@tauri-apps/api/core')
      isDesktop.value = isTauri()
    } catch {
      isDesktop.value = false
    }
  }

  async function connect() {
    error.value = null

    if (!isDesktop.value) {
      error.value = {
        message: 'La conexión se ejecuta en el backend Rust. Abre la app con pnpm tauri dev.'
      }
      return
    }

    const validation = validateConnection(mode.value, fields, url.value)
    if (validation) {
      error.value = { message: validation }
      return
    }

    isConnecting.value = true

    try {
      const { invoke } = await import('@tauri-apps/api/core')

      connection.value = await invoke<ConnectionInfo>('connect', {
        request: buildConnectRequest(mode.value, fields, url.value)
      })
      connectionLabel.value = formatConnectionLabel(mode.value, fields, url.value)

      // La contraseña no viaja: `connectionPreferences` la omite.
      void usePreferences().setConnection(connectionPreferences(mode.value, fields, url.value))
    } catch (cause) {
      error.value = toDbError(cause)
    } finally {
      isConnecting.value = false
    }
  }

  async function disconnect() {
    try {
      const { invoke } = await import('@tauri-apps/api/core')
      await invoke('disconnect')
    } catch {
      // Aunque falle en el backend, limpiamos el estado local.
    }

    connection.value = null
    error.value = null
  }

  return {
    isDesktop,
    isConnecting,
    showPassword,
    mode,
    fields,
    url,
    error,
    connection,
    connectionLabel,
    restore,
    checkDesktop,
    connect,
    disconnect
  }
}
