import { describe, expect, it } from 'vitest'
import {
  DEFAULT_FIELDS,
  DEFAULT_URL,
  buildConnectRequest,
  connectionLabel,
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

  it.each(['', '0', '-1', '70000', '5432.5', 'abc'])(
    'rechaza el puerto inválido %j en modo fields',
    (port) => {
      expect(validateConnection('fields', makeFields({ port }), DEFAULT_URL)).toBe(
        'El puerto debe ser un número entre 1 y 65535.'
      )
    }
  )

  it('acepta un puerto válido rodeado de espacios en modo fields', () => {
    expect(validateConnection('fields', makeFields({ port: ' 5432 ' }), DEFAULT_URL)).toBeNull()
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
      sslmode: 'prefer',
      sslrootcert: ''
    })
  })

  it('recorta la URL en modo url', () => {
    const request = buildConnectRequest('url', makeFields(), '  postgresql://ana@db:5432/ventas  ')

    expect(request).toEqual({
      mode: 'url',
      url: 'postgresql://ana@db:5432/ventas',
      sslrootcert: ''
    })
  })

  it.each(['abc', '', '0', '-1', '70000'])('usa el puerto 5432 cuando el valor es %j', (port) => {
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

  it('recorta la ruta de la CA y la transmite en ambos modos', () => {
    const fieldsRequest = buildConnectRequest(
      'fields',
      makeFields({ sslrootcert: '  /etc/ssl/ca.pem  ' }),
      DEFAULT_URL
    )
    expect(fieldsRequest).toMatchObject({ sslrootcert: '/etc/ssl/ca.pem' })

    const urlRequest = buildConnectRequest(
      'url',
      makeFields({ sslrootcert: '  /etc/ssl/ca.pem  ' }),
      DEFAULT_URL
    )
    expect(urlRequest).toMatchObject({ sslrootcert: '/etc/ssl/ca.pem' })
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
