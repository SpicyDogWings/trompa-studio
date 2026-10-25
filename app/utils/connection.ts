export type ConnectionMode = 'fields' | 'url'

export type SslMode = 'prefer' | 'require' | 'disable'

export interface ConnectionFields {
  host: string
  port: string
  database: string
  user: string
  password: string
  sslmode: SslMode
  sslrootcert: string
}

export interface ConnectionInfo {
  database: string
  user: string
  serverVersion: string
}

export interface DbErrorPayload {
  message: string
  code?: string
}

export type ConnectRequest =
  | {
      mode: 'fields'
      host: string
      port: number
      database: string
      user: string
      password: string
      sslmode: SslMode
      sslrootcert: string
    }
  | { mode: 'url'; url: string; sslrootcert: string }

export const DEFAULT_FIELDS: ConnectionFields = {
  host: 'localhost',
  port: '5432',
  database: 'postgres',
  user: 'postgres',
  password: '',
  sslmode: 'prefer',
  sslrootcert: ''
}

export const DEFAULT_URL = 'postgresql://postgres@localhost:5432/postgres'

const PORT_MESSAGE = 'El puerto debe ser un número entre 1 y 65535.'
const DEFAULT_PORT = 5432

function parsePort(value: string): number | null {
  const trimmed = value.trim()
  if (!/^\d+$/.test(trimmed)) return null

  const port = Number.parseInt(trimmed, 10)
  return port >= 1 && port <= 65535 ? port : null
}

export function validateConnection(
  mode: ConnectionMode,
  fields: ConnectionFields,
  url: string
): string | null {
  if (mode === 'fields') {
    if (!fields.host.trim() || !fields.database.trim() || !fields.user.trim()) {
      return 'Host, base de datos y usuario son obligatorios.'
    }
    if (parsePort(fields.port) === null) return PORT_MESSAGE
    return null
  }

  if (!url.trim()) return 'Pega una URL de conexión.'
  return null
}

export function buildConnectRequest(
  mode: ConnectionMode,
  fields: ConnectionFields,
  url: string
): ConnectRequest {
  if (mode === 'url') {
    return { mode: 'url', url: url.trim(), sslrootcert: fields.sslrootcert.trim() }
  }

  return {
    mode: 'fields',
    host: fields.host.trim(),
    port: parsePort(fields.port) ?? DEFAULT_PORT,
    database: fields.database.trim(),
    user: fields.user.trim(),
    password: fields.password,
    sslmode: fields.sslmode,
    sslrootcert: fields.sslrootcert.trim()
  }
}

export function connectionLabel(
  mode: ConnectionMode,
  fields: ConnectionFields,
  url: string
): string {
  if (mode === 'url') return url.trim()

  return `${fields.user.trim()}@${fields.host.trim()}:${fields.port.trim()}/${fields.database.trim()}`
}

export function toDbError(cause: unknown): DbErrorPayload {
  if (typeof cause === 'string') return { message: cause }

  if (cause && typeof cause === 'object') {
    const payload = cause as Partial<DbErrorPayload>
    if (typeof payload.message === 'string') {
      return { message: payload.message, code: payload.code }
    }
  }

  return { message: 'No se pudo conectar a PostgreSQL.' }
}
