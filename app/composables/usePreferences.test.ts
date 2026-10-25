import { beforeEach, describe, expect, it, vi } from 'vitest'
import { DEFAULT_PREFERENCES, type Preferences } from '~/utils/preferences'
import { usePreferences } from './usePreferences'

const tauri = vi.hoisted(() => ({ invoke: vi.fn() }))

vi.mock('@tauri-apps/api/core', () => tauri)

const SAVED: Preferences = {
  ui: { colorMode: 'light', theme: 'papel' },
  schema: { sidebarPosition: 'left' },
  connection: {
    mode: 'url',
    host: 'db.local',
    port: 6543,
    database: 'ventas',
    user: 'ana',
    sslmode: 'require',
    sslrootcert: '/etc/ssl/ca.pem',
    url: 'postgresql://ana@db.local:6543/ventas'
  }
}

const OTHER: Preferences = {
  ...SAVED,
  ui: { colorMode: 'dark', theme: 'papel' }
}

beforeEach(() => {
  tauri.invoke.mockReset()
  usePreferences().resetPreferences()
})

describe('load', () => {
  it('hidrata el estado normalizado desde read_preferences', async () => {
    tauri.invoke.mockResolvedValueOnce(SAVED)
    const { preferences, isLoading, error, load } = usePreferences()

    await load()

    expect(tauri.invoke).toHaveBeenCalledWith('read_preferences')
    expect(preferences.value).toEqual(SAVED)
    expect(error.value).toBeNull()
    expect(isLoading.value).toBe(false)
  })

  it('normaliza un payload inválido del backend', async () => {
    tauri.invoke.mockResolvedValueOnce({ ui: { colorMode: 'neon' }, connection: { port: 0 } })
    const { preferences, load } = usePreferences()

    await load()

    expect(preferences.value).toEqual(DEFAULT_PREFERENCES)
  })

  it('guarda el error y deja los defaults si el backend falla', async () => {
    tauri.invoke.mockRejectedValueOnce({ message: 'sin preferences.toml', code: 'IO' })
    const { preferences, isLoading, error, load } = usePreferences()

    await load()

    expect(error.value).toEqual({ message: 'sin preferences.toml', code: 'IO' })
    expect(preferences.value).toEqual(DEFAULT_PREFERENCES)
    expect(isLoading.value).toBe(false)
  })

  it('comparte la misma promesa en cargas concurrentes', async () => {
    tauri.invoke.mockResolvedValue(SAVED)
    const { load } = usePreferences()

    await Promise.all([load(), load(), load()])

    expect(tauri.invoke).toHaveBeenCalledTimes(1)
  })
})

describe('update', () => {
  it('escribe el documento completo y aplica la respuesta del backend', async () => {
    tauri.invoke.mockResolvedValueOnce(OTHER)
    const { preferences, update } = usePreferences()

    await update({ ui: { ...DEFAULT_PREFERENCES.ui, colorMode: 'light' } })

    expect(tauri.invoke).toHaveBeenCalledWith('write_preferences', {
      preferences: { ...DEFAULT_PREFERENCES, ui: { ...DEFAULT_PREFERENCES.ui, colorMode: 'light' } }
    })
    // La respuesta del backend manda sobre el estado optimista.
    expect(preferences.value).toEqual(OTHER)
  })

  it('envía en serie dos updates y gana el último', async () => {
    tauri.invoke.mockResolvedValueOnce(SAVED)
    tauri.invoke.mockResolvedValueOnce(OTHER)
    const { preferences, update } = usePreferences()

    await Promise.all([
      update({ ui: { ...DEFAULT_PREFERENCES.ui, colorMode: 'light' } }),
      update({ ui: { ...DEFAULT_PREFERENCES.ui, colorMode: 'dark' } })
    ])

    expect(tauri.invoke).toHaveBeenNthCalledWith(1, 'write_preferences', {
      preferences: { ...DEFAULT_PREFERENCES, ui: { ...DEFAULT_PREFERENCES.ui, colorMode: 'light' } }
    })
    expect(tauri.invoke).toHaveBeenNthCalledWith(2, 'write_preferences', {
      preferences: { ...DEFAULT_PREFERENCES, ui: { ...DEFAULT_PREFERENCES.ui, colorMode: 'dark' } }
    })
    expect(preferences.value).toEqual(OTHER)
  })

  it('conserva el estado optimista si la escritura falla', async () => {
    tauri.invoke.mockRejectedValueOnce({ message: 'disco lleno' })
    const { preferences, error, update } = usePreferences()

    await update({ ui: { ...DEFAULT_PREFERENCES.ui, colorMode: 'dark' } })

    expect(error.value).toEqual({ message: 'disco lleno' })
    expect(preferences.value).toEqual({
      ...DEFAULT_PREFERENCES,
      ui: { ...DEFAULT_PREFERENCES.ui, colorMode: 'dark' }
    })
  })
})

describe('setters', () => {
  it('setColorMode cambia solo la sección ui', async () => {
    tauri.invoke.mockResolvedValue(SAVED)
    const { setColorMode } = usePreferences()

    await setColorMode('light')

    expect(tauri.invoke).toHaveBeenCalledWith('write_preferences', {
      preferences: { ...DEFAULT_PREFERENCES, ui: { ...DEFAULT_PREFERENCES.ui, colorMode: 'light' } }
    })
  })

  it('setColorMode conserva ui.theme', async () => {
    tauri.invoke.mockResolvedValueOnce(SAVED)
    const { load, setColorMode } = usePreferences()

    await load()
    tauri.invoke.mockResolvedValue(OTHER)
    await setColorMode('dark')

    expect(tauri.invoke).toHaveBeenLastCalledWith('write_preferences', {
      preferences: { ...SAVED, ui: { colorMode: 'dark', theme: 'papel' } }
    })
  })

  it('setSidebarPosition cambia solo la sección schema', async () => {
    tauri.invoke.mockResolvedValue(SAVED)
    const { setSidebarPosition } = usePreferences()

    await setSidebarPosition('left')

    expect(tauri.invoke).toHaveBeenCalledWith('write_preferences', {
      preferences: { ...DEFAULT_PREFERENCES, schema: { sidebarPosition: 'left' } }
    })
  })

  it('setConnection cambia solo la sección connection', async () => {
    tauri.invoke.mockResolvedValue(SAVED)
    const { setConnection } = usePreferences()

    await setConnection(SAVED.connection)

    expect(tauri.invoke).toHaveBeenCalledWith('write_preferences', {
      preferences: { ...DEFAULT_PREFERENCES, connection: SAVED.connection }
    })
  })
})
