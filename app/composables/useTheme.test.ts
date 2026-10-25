import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { ThemeCatalog, ThemeDefinition } from '~/utils/theme'
import { themeSlug, useTheme } from './useTheme'
import { usePreferences } from './usePreferences'

const tauri = vi.hoisted(() => ({ invoke: vi.fn() }))

vi.mock('@tauri-apps/api/core', () => tauri)

function theme(id: string, overrides: Partial<ThemeDefinition> = {}): ThemeDefinition {
  return {
    id,
    name: id,
    description: '',
    builtin: false,
    palette: {},
    shape: {},
    light: {},
    dark: {},
    path: null,
    ...overrides
  }
}

const CATALOG: ThemeCatalog = {
  themes: [
    theme('trompa', { builtin: true, palette: { primary: 'blue', neutral: 'slate' } }),
    theme('papel', { name: 'Papel', palette: { primary: 'amber' }, path: '/temas/papel.toml' }),
    theme('otro', { name: 'Otro', path: '/ruta/absoluta/otro.toml' })
  ],
  errors: [],
  directory: '/temas'
}

/** Backend simulado: el catálogo y el eco de `write_preferences`. */
function mockBackend(catalog: ThemeCatalog = CATALOG): void {
  tauri.invoke.mockImplementation((command: string, args?: { preferences?: unknown }) =>
    Promise.resolve(command === 'list_themes' ? catalog : args?.preferences)
  )
}

async function useReference(reference: string): Promise<void> {
  const preferences = usePreferences()
  await preferences.update({ ui: { ...preferences.current().ui, theme: reference } })
}

beforeEach(() => {
  tauri.invoke.mockReset()
  useTheme().resetTheme()
  usePreferences().resetPreferences()
  mockBackend()
})

describe('load', () => {
  it('pide list_themes una vez y force repite', async () => {
    const theme = useTheme()

    await theme.load()

    expect(tauri.invoke).toHaveBeenCalledWith('list_themes')
    expect(tauri.invoke).toHaveBeenCalledTimes(1)
    expect(theme.catalog.value).toEqual(CATALOG)
    expect(theme.error.value).toBeNull()
    expect(theme.isLoading.value).toBe(false)

    await theme.load()
    expect(tauri.invoke).toHaveBeenCalledTimes(1)

    await theme.load(true)
    expect(tauri.invoke).toHaveBeenCalledTimes(2)
  })

  it('guarda el error y conserva el catálogo anterior', async () => {
    const theme = useTheme()
    await theme.load()

    tauri.invoke.mockRejectedValueOnce({ message: 'sin themes/' })
    await theme.load(true)

    expect(theme.catalog.value).toEqual(CATALOG)
    expect(theme.error.value?.message).toBe('sin themes/')
  })
})

describe('active', () => {
  it('es null antes de cargar el catálogo', () => {
    expect(useTheme().active.value).toBeNull()
  })

  it('resuelve por id con la referencia normalizada a slug', async () => {
    const theme = useTheme()
    await theme.load()

    await useReference('Papél')

    expect(theme.active.value?.id).toBe('papel')
  })

  it('resuelve por ruta absoluta', async () => {
    const theme = useTheme()
    await theme.load()

    await useReference('/ruta/absoluta/otro.toml')

    expect(theme.active.value?.id).toBe('otro')
  })

  it('cae al integrado cuando la referencia no existe', async () => {
    const theme = useTheme()
    await theme.load()

    await useReference('no-existe')

    expect(theme.active.value?.id).toBe('trompa')
    expect(theme.active.value?.builtin).toBe(true)
  })
})

describe('themeSlug', () => {
  it('pliega diacríticos y separa por rachas', () => {
    expect(themeSlug('Mi Tema Bonito')).toBe('mi-tema-bonito')
    expect(themeSlug('Papél')).toBe('papel')
    expect(themeSlug('  año  ñandú  ')).toBe('ano-nandu')
  })
})
