import { describe, expect, it } from 'vitest'
import {
  DEFAULT_FIELDS,
  DEFAULT_URL,
  STORAGE_KEY,
  buildConnectRequest,
  connectionLabel,
  parsePreferences,
  preferencesPayload,
  toDbError,
  validateConnection,
  type ConnectionFields
} from './connection'

function makeFields(overrides: Partial<ConnectionFields> = {}): ConnectionFields {
  return { ...DEFAULT_FIELDS, ...overrides }
}

describe('validateConnection', () => {
  it('exige host, base de datos y usuario en modo fields', () => {
    const message = 'Host, base de datos y usuario son obligatorios.'

    expect(validateConnection('fields', makeFields({ host: '   ' }), DEFAULT_URL)).toBe(message)
    expect(validateConnection('fields', makeFields({ database: '' }), DEFAULT_URL)).toBe(message)
    expect(validateConnection('fields', makeFields({ user: '\t' }), DEFAULT_URL)).toBe(message)
  })

  it('acepta campos completos en modo fields', () => {
    expect(validateConnection('fields', makeFields(), DEFAULT_URL)).toBeNull()
  })

  it('exige una URL no vacía en modo url', () => {
    expect(validateConnection('url', makeFields(), '   ')).toBe('Pega una URL de conexión.')
    expect(validateConnection('url', makeFields(), '')).toBe('Pega una URL de conexión.')
  })

  it('acepta una URL válida en modo url', () => {
    expect(validateConnection('url', makeFields({ host: '' }), DEFAULT_URL)).toBeNull()
  })
})

describe('buildConnectRequest', () => {
  it('recorta host, base de datos y usuario en modo fields', () => {
    const request = buildConnectRequest(
      'fields',
      makeFields({
        host: '  db.local  ',
        port: '6543',
        database: '  ventas  ',
        user: '  ana  '
      }),
      DEFAULT_URL
    )

    expect(request).toEqual({
      mode: 'fields',
      host: 'db.local',
      port: 6543,
      database: 'ventas',
      user: 'ana',
      password: '',
      sslmode: 'prefer'
    })
  })

  it('recorta la URL en modo url', () => {
    const request = buildConnectRequest('url', makeFields(), '  postgresql://ana@db:5432/ventas  ')

    expect(request).toEqual({ mode: 'url', url: 'postgresql://ana@db:5432/ventas' })
  })

  it.each(['abc', '', '0'])('usa el puerto 5432 cuando el valor es %j', (port) => {
    const request = buildConnectRequest('fields', makeFields({ port }), DEFAULT_URL)

    expect(request.mode).toBe('fields')
    expect(request).toMatchObject({ port: 5432 })
  })

  it('conserva un puerto válido', () => {
    const request = buildConnectRequest('fields', makeFields({ port: '6543' }), DEFAULT_URL)

    expect(request).toMatchObject({ port: 6543 })
  })

  it('no recorta la contraseña y transmite sslmode', () => {
    const request = buildConnectRequest(
      'fields',
      makeFields({ password: '  s3creta  ', sslmode: 'require' }),
      DEFAULT_URL
    )

    expect(request).toMatchObject({ password: '  s3creta  ', sslmode: 'require' })
  })
})

describe('connectionLabel', () => {
  it('usa la URL recortada en modo url', () => {
    expect(connectionLabel('url', makeFields(), '  postgresql://ana@db:5432/ventas  ')).toBe(
      'postgresql://ana@db:5432/ventas'
    )
  })

  it('compone usuario, host, puerto y base en modo fields', () => {
    const label = connectionLabel(
      'fields',
      makeFields({
        host: '  db.local ',
        port: '6543',
        database: ' ventas ',
        user: ' ana '
      }),
      DEFAULT_URL
    )

    expect(label).toBe('ana@db.local:6543/ventas')
  })
})

describe('preferencesPayload', () => {
  it('vacía la contraseña en modo fields', () => {
    const payload = preferencesPayload(
      'fields',
      makeFields({ host: 'db.local', user: 'ana', password: 's3creta', sslmode: 'require' }),
      DEFAULT_URL
    )

    expect(payload).toEqual({
      mode: 'fields',
      host: 'db.local',
      port: '5432',
      database: 'postgres',
      user: 'ana',
      password: '',
      sslmode: 'require'
    })
    expect(JSON.stringify(payload)).not.toContain('s3creta')
  })

  it('guarda solo mode y url en modo url', () => {
    const payload = preferencesPayload('url', makeFields({ password: 's3creta' }), DEFAULT_URL)

    expect(payload).toEqual({ mode: 'url', url: DEFAULT_URL })
  })
})

describe('parsePreferences', () => {
  it('restaura mode, url, campos y sslmode', () => {
    const raw = JSON.stringify({
      mode: 'fields',
      host: 'db.local',
      port: '6543',
      database: 'ventas',
      user: 'ana',
      sslmode: 'require'
    })

    expect(parsePreferences(raw)).toEqual({
      mode: 'fields',
      url: DEFAULT_URL,
      fields: {
        host: 'db.local',
        port: '6543',
        database: 'ventas',
        user: 'ana',
        password: '',
        sslmode: 'require'
      }
    })
  })

  it('restaura el modo url', () => {
    const parsed = parsePreferences(JSON.stringify({ mode: 'url', url: 'postgresql://ana@db:5432/ventas' }))

    expect(parsed?.mode).toBe('url')
    expect(parsed?.url).toBe('postgresql://ana@db:5432/ventas')
    expect(parsed?.fields).toEqual(DEFAULT_FIELDS)
  })

  it('ignora una contraseña guardada', () => {
    const parsed = parsePreferences(
      JSON.stringify({ mode: 'fields', user: 'ana', password: 's3creta' })
    )

    expect(parsed?.fields.password).toBe('')
    expect(parsed?.fields.user).toBe('ana')
  })

  it('devuelve null sin datos o con JSON corrupto', () => {
    expect(parsePreferences(null)).toBeNull()
    expect(parsePreferences('')).toBeNull()
    expect(parsePreferences('{no es json')).toBeNull()
    expect(parsePreferences('null')).toBeNull()
  })

  it('cae a prefer si el sslmode guardado no es válido', () => {
    const parsed = parsePreferences(JSON.stringify({ mode: 'fields', sslmode: 'verify-full' }))

    expect(parsed?.fields.sslmode).toBe('prefer')
  })
})

describe('toDbError', () => {
  it('acepta un string', () => {
    expect(toDbError('boom')).toEqual({ message: 'boom' })
  })

  it('acepta un objeto con message y code', () => {
    expect(toDbError({ message: 'relation does not exist', code: '42P01' })).toEqual({
      message: 'relation does not exist',
      code: '42P01'
    })
  })

  it('usa el mensaje genérico con un objeto desconocido', () => {
    expect(toDbError({ foo: 1 })).toEqual({ message: 'No se pudo conectar a PostgreSQL.' })
    expect(toDbError(42)).toEqual({ message: 'No se pudo conectar a PostgreSQL.' })
    expect(toDbError(undefined)).toEqual({ message: 'No se pudo conectar a PostgreSQL.' })
  })
})

describe('STORAGE_KEY', () => {
  it('mantiene la clave usada por index.vue', () => {
    expect(STORAGE_KEY).toBe('trompa:last-connection')
  })
})
