export const STORAGE_KEY = 'trompa:last-connection'

export type ConnectionMode = 'fields' | 'url'

export type SslMode = 'prefer' | 'require' | 'disable'

export interface ConnectionFields {
  host: string
  port: string
  database: string
  user: string
  password: string
  sslmode: SslMode
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
    }
  | { mode: 'url'; url: string }

export const DEFAULT_FIELDS: ConnectionFields = {
  host: 'localhost',
  port: '5432',
  database: 'postgres',
  user: 'postgres',
  password: '',
  sslmode: 'prefer'
}

export const DEFAULT_URL = 'postgresql://postgres@localhost:5432/postgres'

const SSL_MODES: readonly SslMode[] = ['prefer', 'require', 'disable']

function isSslMode(value: unknown): value is SslMode {
  return typeof value === 'string' && (SSL_MODES as readonly string[]).includes(value)
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
    return { mode: 'url', url: url.trim() }
  }

  return {
    mode: 'fields',
    host: fields.host.trim(),
    port: Number.parseInt(fields.port, 10) || 5432,
    database: fields.database.trim(),
    user: fields.user.trim(),
    password: fields.password,
    sslmode: fields.sslmode
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

export function preferencesPayload(
  mode: ConnectionMode,
  fields: ConnectionFields,
  url: string
): Record<string, unknown> {
  if (mode === 'url') return { mode: 'url', url }

  return { mode: 'fields', ...fields, password: '' }
}

export function parsePreferences(
  raw: string | null
): { mode: ConnectionMode; fields: ConnectionFields; url: string } | null {
  if (!raw) return null

  let saved: unknown
  try {
    saved = JSON.parse(raw)
  } catch {
    return null
  }

  if (!saved || typeof saved !== 'object') return null

  const data = saved as Record<string, unknown>
  const fields: ConnectionFields = { ...DEFAULT_FIELDS }
  const mode: ConnectionMode = data.mode === 'url' ? 'url' : 'fields'
  const url = typeof data.url === 'string' ? data.url : DEFAULT_URL

  for (const key of ['host', 'port', 'database', 'user'] as const) {
    const value = data[key]
    if (typeof value === 'string') fields[key] = value
  }

  fields.sslmode = isSslMode(data.sslmode) ? data.sslmode : 'prefer'

  return { mode, fields, url }
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
