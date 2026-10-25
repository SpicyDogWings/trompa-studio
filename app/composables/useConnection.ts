import { reactive, ref } from 'vue'
import {
  DEFAULT_FIELDS,
  DEFAULT_URL,
  STORAGE_KEY,
  buildConnectRequest,
  connectionLabel as formatConnectionLabel,
  parsePreferences,
  preferencesPayload,
  toDbError,
  validateConnection,
  type ConnectionFields,
  type ConnectionInfo,
  type ConnectionMode,
  type DbErrorPayload
} from '~/utils/connection'

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
    try {
      const raw = localStorage.getItem(STORAGE_KEY)
      const saved = parsePreferences(raw)
      if (!saved) return

      mode.value = saved.mode
      url.value = saved.url
      // `parsePreferences` nunca devuelve la contraseña guardada.
      Object.assign(fields, saved.fields)
    } catch {
      // Si el almacenamiento está corrupto, empezamos de cero.
    }
  }

  async function checkDesktop() {
    try {
      const { isTauri } = await import('@tauri-apps/api/core')
      isDesktop.value = isTauri()
    } catch {
      isDesktop.value = false
    }
  }

  function persist() {
    const payload = preferencesPayload(mode.value, fields, url.value)

    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(payload))
    } catch {
      // Sin persistencia la app sigue funcionando.
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

      persist()
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
