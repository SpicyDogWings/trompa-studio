import { describe, expect, it } from 'vitest'
import { DEFAULT_FIELDS } from '~/utils/connection'
import {
  DEFAULT_PREFERENCES,
  connectionPreferences,
  fieldsFromConnection,
  normalizePreferences,
  type Preferences
} from '~/utils/preferences'

const COMPLETE: Preferences = {
  ui: { colorMode: 'dark', theme: 'papel' },
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

describe('normalizePreferences', () => {
  it('conserva un objeto completo', () => {
    expect(normalizePreferences(COMPLETE)).toEqual(COMPLETE)
  })

  it('devuelve los defaults para entradas que no son un objeto', () => {
    for (const raw of [null, undefined, 'texto', 42, ['ui']]) {
      expect(normalizePreferences(raw)).toEqual(DEFAULT_PREFERENCES)
    }
  })

  it('rellena cada campo inválido con su default', () => {
    const normalized = normalizePreferences({
      ui: { colorMode: 'neon' },
      schema: { sidebarPosition: 'top' },
      connection: { mode: 'x', sslmode: 'x', port: 0, host: 7 }
    })

    expect(normalized).toEqual(DEFAULT_PREFERENCES)
  })

  it('normaliza theme: recorta y cae al default si está vacío o es enorme', () => {
    expect(normalizePreferences({ ui: { theme: '  papel  ' } }).ui.theme).toBe('papel')
    expect(normalizePreferences({ ui: { theme: '   ' } }).ui.theme).toBe('trompa')
    expect(normalizePreferences({ ui: { theme: 7 } }).ui.theme).toBe('trompa')
    expect(normalizePreferences({ ui: { theme: 'x'.repeat(513) } }).ui.theme).toBe('trompa')
    expect(normalizePreferences({ ui: { theme: '/ruta/a/mi.toml' } }).ui.theme).toBe(
      '/ruta/a/mi.toml'
    )
  })

  it('rechaza un puerto en forma de string', () => {
    expect(normalizePreferences({ connection: { port: '5432' } }).connection.port).toBe(5432)
    expect(normalizePreferences({ connection: { port: 70000 } }).connection.port).toBe(5432)
    expect(normalizePreferences({ connection: { port: 6543 } }).connection.port).toBe(6543)
  })

  it('no muta DEFAULT_PREFERENCES', () => {
    const before = structuredClone(DEFAULT_PREFERENCES)

    const normalized = normalizePreferences({ ui: { colorMode: 'light' } })
    normalized.schema.sidebarPosition = 'left'
    normalized.connection.host = 'otro'
    delete (normalized.connection as Partial<Preferences['connection']>).url

    expect(DEFAULT_PREFERENCES).toEqual(before)
    expect(normalized).not.toBe(DEFAULT_PREFERENCES)
  })
})

describe('connectionPreferences', () => {
  it('omite la contraseña y convierte el puerto', () => {
    const payload = connectionPreferences(
      'fields',
      { ...DEFAULT_FIELDS, host: 'db.local', port: '6543', password: 'secreto' },
      DEFAULT_PREFERENCES.connection.url
    )

    expect(payload).not.toHaveProperty('password')
    expect(payload.port).toBe(6543)
    expect(payload.host).toBe('db.local')
  })

  it('usa 5432 cuando el puerto no es numérico', () => {
    const payload = connectionPreferences('fields', { ...DEFAULT_FIELDS, port: '' }, 'url')

    expect(payload.port).toBe(5432)
  })
})

describe('fieldsFromConnection', () => {
  it('reconstruye el formulario sin contraseña', () => {
    const restored = fieldsFromConnection(COMPLETE.connection)

    expect(restored.mode).toBe('url')
    expect(restored.url).toBe('postgresql://ana@db.local:6543/ventas')
    expect(restored.fields).toEqual({
      host: 'db.local',
      port: '6543',
      database: 'ventas',
      user: 'ana',
      password: '',
      sslmode: 'require',
      sslrootcert: '/etc/ssl/ca.pem'
    })
  })
})
