import { beforeEach, describe, expect, it, vi } from 'vitest'
import { DEFAULT_PREFERENCES } from '~/utils/preferences'
import { useConnection } from './useConnection'
import { usePreferences } from './usePreferences'

const tauri = vi.hoisted(() => ({
  invoke: vi.fn(),
  isTauri: vi.fn(() => true)
}))

vi.mock('@tauri-apps/api/core', () => tauri)

const CONNECTED_INFO = {
  database: 'ventas',
  user: 'ana',
  serverVersion: '16.3'
}

async function desktopConnection() {
  tauri.isTauri.mockReturnValue(true)
  const connection = useConnection()
  await connection.checkDesktop()
  return connection
}

beforeEach(() => {
  usePreferences().resetPreferences()
  tauri.invoke.mockReset()
  tauri.isTauri.mockReset()
  tauri.isTauri.mockReturnValue(true)
})

describe('checkDesktop', () => {
  it('marca isDesktop cuando Tauri está disponible', async () => {
    const { isDesktop, checkDesktop } = useConnection()

    await checkDesktop()

    expect(isDesktop.value).toBe(true)
  })

  it('deja isDesktop en false cuando no está disponible', async () => {
    tauri.isTauri.mockReturnValue(false)
    const { isDesktop, checkDesktop } = useConnection()

    await checkDesktop()

    expect(isDesktop.value).toBe(false)
  })
})

describe('connect', () => {
  it('no invoca al backend sin escritorio', async () => {
    tauri.isTauri.mockReturnValue(false)
    const { isDesktop, isConnecting, error, connect, checkDesktop } = useConnection()

    await checkDesktop()
    await connect()

    expect(isDesktop.value).toBe(false)
    expect(tauri.invoke).not.toHaveBeenCalled()
    expect(error.value?.message).toContain('pnpm tauri dev')
    expect(isConnecting.value).toBe(false)
  })

  it('rechaza campos obligatorios vacíos', async () => {
    const { fields, error, connect } = await desktopConnection()

    fields.host = '   '
    await connect()

    expect(tauri.invoke).not.toHaveBeenCalled()
    expect(error.value).not.toBeNull()

    fields.host = 'localhost'
    fields.database = ''
    await connect()

    expect(tauri.invoke).not.toHaveBeenCalled()
    expect(error.value).not.toBeNull()

    fields.database = 'postgres'
    fields.user = ''
    await connect()

    expect(tauri.invoke).not.toHaveBeenCalled()
    expect(error.value).not.toBeNull()
  })

  it('rechaza una URL vacía', async () => {
    const { mode, url, error, connect } = await desktopConnection()

    mode.value = 'url'
    url.value = '   '
    await connect()

    expect(tauri.invoke).not.toHaveBeenCalled()
    expect(error.value).not.toBeNull()
  })

  it('conecta, calcula la etiqueta y guarda las preferencias sin la contraseña real', async () => {
    tauri.invoke.mockImplementation((command: string) =>
      command === 'write_preferences'
        ? Promise.resolve(DEFAULT_PREFERENCES)
        : Promise.resolve(CONNECTED_INFO)
    )
    const { fields, connection, connectionLabel, isConnecting, error, connect } =
      await desktopConnection()

    fields.host = 'db.example.com'
    fields.port = '6543'
    fields.database = 'ventas'
    fields.user = 'ana'
    fields.password = 'secreto-real'
    fields.sslrootcert = '/etc/ssl/ca.pem'

    await connect()

    expect(tauri.invoke).toHaveBeenCalledWith('connect', {
      request: expect.objectContaining({
        mode: 'fields',
        host: 'db.example.com',
        port: 6543,
        database: 'ventas',
        user: 'ana',
        password: 'secreto-real',
        sslrootcert: '/etc/ssl/ca.pem'
      })
    })
    expect(connection.value).toEqual(CONNECTED_INFO)
    expect(connectionLabel.value).toBe('ana@db.example.com:6543/ventas')
    expect(error.value).toBeNull()
    expect(isConnecting.value).toBe(false)

    await vi.waitFor(() =>
      expect(tauri.invoke).toHaveBeenCalledWith('write_preferences', {
        preferences: expect.objectContaining({
          connection: expect.objectContaining({
            mode: 'fields',
            host: 'db.example.com',
            port: 6543,
            database: 'ventas',
            user: 'ana',
            sslrootcert: '/etc/ssl/ca.pem'
          })
        })
      })
    )
    const writeCall = tauri.invoke.mock.calls.find(([command]) => command === 'write_preferences')
    expect(writeCall).toBeDefined()
    expect(JSON.stringify(writeCall)).not.toContain('secreto-real')
  })

  it('normaliza un error con message y code', async () => {
    tauri.invoke.mockRejectedValueOnce({ message: 'relación no existe', code: '42P01' })
    const { error, isConnecting, connect } = await desktopConnection()

    await connect()

    expect(error.value).toEqual({ message: 'relación no existe', code: '42P01' })
    expect(isConnecting.value).toBe(false)
  })

  it('normaliza un error en forma de string', async () => {
    tauri.invoke.mockRejectedValueOnce('sin conexión')
    const { error, connect } = await desktopConnection()

    await connect()

    expect(error.value).toEqual({ message: 'sin conexión' })
  })

  it('no escribe preferencias si la conexión falla', async () => {
    tauri.invoke.mockRejectedValueOnce({ message: 'relación no existe', code: '42P01' })
    const { connect } = await desktopConnection()

    await connect()

    expect(tauri.invoke.mock.calls.some(([command]) => command === 'write_preferences')).toBe(false)
  })
})

describe('disconnect', () => {
  it('invoca disconnect y limpia el estado local', async () => {
    tauri.invoke.mockResolvedValue(undefined)
    const { connection, error, disconnect } = useConnection()

    connection.value = CONNECTED_INFO
    error.value = { message: 'error previo' }

    await disconnect()

    expect(tauri.invoke).toHaveBeenCalledWith('disconnect')
    expect(connection.value).toBeNull()
    expect(error.value).toBeNull()
  })

  it('limpia el estado local aunque el backend falle', async () => {
    tauri.invoke.mockRejectedValueOnce(new Error('backend caído'))
    const { connection, error, disconnect } = useConnection()

    connection.value = CONNECTED_INFO
    error.value = { message: 'error previo' }

    await disconnect()

    expect(connection.value).toBeNull()
    expect(error.value).toBeNull()
  })
})

describe('restore', () => {
  it('recupera modo y URL desde las preferencias', async () => {
    tauri.invoke.mockResolvedValueOnce({
      ...DEFAULT_PREFERENCES,
      connection: {
        ...DEFAULT_PREFERENCES.connection,
        mode: 'url',
        url: 'postgresql://ana@db.example.com:6543/ventas'
      }
    })
    await usePreferences().load()

    const { mode, url, restore } = useConnection()

    restore()

    expect(mode.value).toBe('url')
    expect(url.value).toBe('postgresql://ana@db.example.com:6543/ventas')
  })

  it('recupera los campos y deja la contraseña siempre vacía', async () => {
    tauri.invoke.mockResolvedValueOnce({
      ...DEFAULT_PREFERENCES,
      connection: {
        ...DEFAULT_PREFERENCES.connection,
        host: 'db.example.com',
        port: 6543,
        database: 'ventas',
        user: 'ana',
        sslmode: 'require',
        sslrootcert: '/etc/ssl/ca.pem'
      }
    })
    await usePreferences().load()

    const { mode, fields, restore } = useConnection()

    restore()

    expect(mode.value).toBe('fields')
    expect(fields.host).toBe('db.example.com')
    expect(fields.port).toBe('6543')
    expect(fields.database).toBe('ventas')
    expect(fields.user).toBe('ana')
    expect(fields.sslmode).toBe('require')
    expect(fields.sslrootcert).toBe('/etc/ssl/ca.pem')
    expect(fields.password).toBe('')
  })
})
