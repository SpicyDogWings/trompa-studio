import {
  DEFAULT_FIELDS,
  DEFAULT_URL,
  type ConnectionFields,
  type ConnectionMode,
  type SslMode
} from '~/utils/connection'

export type ColorModePreference = 'system' | 'light' | 'dark'

export type SchemaSidebarPosition = 'left' | 'right'

export interface UiPreferences {
  colorMode: ColorModePreference
  /** Nombre (normalizado a slug) o ruta absoluta a un `.toml`. */
  theme: string
}

export interface SchemaPreferences {
  sidebarPosition: SchemaSidebarPosition
}

export interface ConnectionPreferences {
  mode: ConnectionMode
  host: string
  port: number
  database: string
  user: string
  sslmode: SslMode
  sslrootcert: string
  url: string
}

export interface Preferences {
  ui: UiPreferences
  schema: SchemaPreferences
  connection: ConnectionPreferences
}

export const COLOR_MODES: readonly ColorModePreference[] = ['system', 'light', 'dark']

export const SIDEBAR_POSITIONS: readonly SchemaSidebarPosition[] = ['left', 'right']

const CONNECTION_MODES: readonly ConnectionMode[] = ['fields', 'url']

const SSL_MODES: readonly SslMode[] = ['prefer', 'require', 'disable']

const DEFAULT_PORT = 5432

export const DEFAULT_PREFERENCES: Preferences = {
  ui: { colorMode: 'system', theme: 'trompa' },
  schema: { sidebarPosition: 'right' },
  connection: {
    mode: 'fields',
    host: DEFAULT_FIELDS.host,
    port: DEFAULT_PORT,
    database: DEFAULT_FIELDS.database,
    user: DEFAULT_FIELDS.user,
    sslmode: DEFAULT_FIELDS.sslmode,
    sslrootcert: DEFAULT_FIELDS.sslrootcert,
    url: DEFAULT_URL
  }
}

function asRecord(value: unknown): Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : {}
}

function pickString(value: unknown, fallback: string): string {
  return typeof value === 'string' ? value : fallback
}

function pickOne<T extends string>(value: unknown, allowed: readonly T[], fallback: T): T {
  return typeof value === 'string' && (allowed as readonly string[]).includes(value)
    ? (value as T)
    : fallback
}

function pickTheme(value: unknown, fallback: string): string {
  if (typeof value !== 'string') return fallback
  const trimmed = value.trim()
  return trimmed.length === 0 || trimmed.length > 512 ? fallback : trimmed
}

function pickPort(value: unknown): number {
  return typeof value === 'number' && Number.isInteger(value) && value >= 1 && value <= 65535
    ? value
    : DEFAULT_PREFERENCES.connection.port
}

/**
 * Devuelve un objeto completo y nuevo: lo ausente o fuera de rango se rellena
 * con `DEFAULT_PREFERENCES` campo a campo, igual que `normalize` en el backend.
 */
export function normalizePreferences(raw: unknown): Preferences {
  const root = asRecord(raw)
  const ui = asRecord(root.ui)
  const schema = asRecord(root.schema)
  const connection = asRecord(root.connection)
  const defaults = DEFAULT_PREFERENCES

  return {
    ui: {
      colorMode: pickOne(ui.colorMode, COLOR_MODES, defaults.ui.colorMode),
      theme: pickTheme(ui.theme, defaults.ui.theme)
    },
    schema: {
      sidebarPosition: pickOne(
        schema.sidebarPosition,
        SIDEBAR_POSITIONS,
        defaults.schema.sidebarPosition
      )
    },
    connection: {
      mode: pickOne(connection.mode, CONNECTION_MODES, defaults.connection.mode),
      host: pickString(connection.host, defaults.connection.host),
      port: pickPort(connection.port),
      database: pickString(connection.database, defaults.connection.database),
      user: pickString(connection.user, defaults.connection.user),
      sslmode: pickOne(connection.sslmode, SSL_MODES, defaults.connection.sslmode),
      sslrootcert: pickString(connection.sslrootcert, defaults.connection.sslrootcert),
      url: pickString(connection.url, defaults.connection.url)
    }
  }
}

/** Preferencias de conexión sin la contraseña: nunca se persiste. */
export function connectionPreferences(
  mode: ConnectionMode,
  fields: ConnectionFields,
  url: string
): ConnectionPreferences {
  const port = Number.parseInt(fields.port, 10)

  return {
    mode,
    host: fields.host,
    port: Number.isInteger(port) ? port : DEFAULT_PORT,
    database: fields.database,
    user: fields.user,
    sslmode: fields.sslmode,
    sslrootcert: fields.sslrootcert,
    url
  }
}

/** Reconstruye el estado editable del formulario desde las preferencias. */
export function fieldsFromConnection(connection: ConnectionPreferences): {
  mode: ConnectionMode
  fields: ConnectionFields
  url: string
} {
  return {
    mode: connection.mode,
    fields: {
      host: connection.host,
      port: String(connection.port),
      database: connection.database,
      user: connection.user,
      password: '',
      sslmode: connection.sslmode,
      sslrootcert: connection.sslrootcert
    },
    url: connection.url
  }
}
