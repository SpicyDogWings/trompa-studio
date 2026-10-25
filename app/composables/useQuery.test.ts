import { beforeEach, describe, expect, it, vi } from 'vitest'
import { useQuery, type QueryEvent } from './useQuery'

const tauri = vi.hoisted(() => {
  const channels: FakeChannel[] = []

  class FakeChannel {
    onmessage: ((message: QueryEvent) => void) | null = null

    constructor() {
      channels.push(this)
    }

    emit(message: QueryEvent) {
      this.onmessage?.(message)
    }
  }

  return { channels, FakeChannel, invoke: vi.fn() }
})

interface FakeChannel {
  onmessage: ((message: QueryEvent) => void) | null
  emit(message: QueryEvent): void
}

vi.mock('@tauri-apps/api/core', () => ({
  invoke: tauri.invoke,
  Channel: tauri.FakeChannel
}))

function lastChannel(): FakeChannel {
  const channel = tauri.channels.at(-1)
  if (!channel) throw new Error('no se creó ningún Channel')
  return channel
}

beforeEach(() => {
  tauri.channels.length = 0
  tauri.invoke.mockReset()
  tauri.invoke.mockResolvedValue(1)
})

describe('run', () => {
  it('crea un Channel y lo pasa a start_query', async () => {
    const { run } = useQuery()

    await run('SELECT 1')

    expect(tauri.channels).toHaveLength(1)
    expect(tauri.invoke).toHaveBeenCalledWith('start_query', {
      sql: 'SELECT 1',
      onEvent: tauri.channels[0]
    })
  })

  it('marca isRunning mientras la consulta está en curso', async () => {
    let resolveInvoke: ((id: number) => void) | undefined
    tauri.invoke.mockReturnValue(
      new Promise<number>((resolve) => {
        resolveInvoke = resolve
      })
    )

    const { isRunning, run } = useQuery()
    const pending = run('SELECT 1')

    expect(isRunning.value).toBe(true)

    resolveInvoke?.(1)
    await pending

    expect(isRunning.value).toBe(true)

    lastChannel().emit({ type: 'done', rowsAffected: 0, elapsedMs: 1 })

    expect(isRunning.value).toBe(false)
  })

  it('acumula columnas, lotes de filas y el resumen final', async () => {
    const { isRunning, columns, rows, rowsAffected, elapsedMs, error, run } = useQuery()

    await run('SELECT generate_series(1,3)')

    expect(isRunning.value).toBe(true)
    expect(error.value).toBeNull()

    const channel = lastChannel()
    channel.emit({ type: 'started', queryId: 7 })
    channel.emit({ type: 'columns', columns: ['n'] })
    channel.emit({ type: 'rows', rows: [['1'], ['2']] })
    channel.emit({ type: 'rows', rows: [['3']] })
    channel.emit({ type: 'done', rowsAffected: 3, elapsedMs: 12 })

    expect(columns.value).toEqual(['n'])
    expect(rows.value).toEqual([['1'], ['2'], ['3']])
    expect(rowsAffected.value).toBe(3)
    expect(elapsedMs.value).toBe(12)
    expect(isRunning.value).toBe(false)
    expect(error.value).toBeNull()
  })

  it('guarda los nulos de las filas tal cual', async () => {
    const { rows, run } = useQuery()

    await run('SELECT NULL')

    lastChannel().emit({ type: 'columns', columns: ['vacio'] })
    lastChannel().emit({ type: 'rows', rows: [[null]] })

    expect(rows.value).toEqual([[null]])
  })

  it('normaliza el error que devuelve el backend al lanzar la consulta', async () => {
    tauri.invoke.mockRejectedValueOnce({ message: 'sin conexión', code: '08006' })

    const { isRunning, error, run } = useQuery()

    await run('SELECT 1')

    expect(isRunning.value).toBe(false)
    expect(error.value).toEqual({ message: 'sin conexión', code: '08006' })
  })
})

describe('cancel', () => {
  it('invoca cancel_query con el id que devuelve start_query', async () => {
    tauri.invoke.mockResolvedValueOnce(7)

    const { cancel, run } = useQuery()
    await run('SELECT pg_sleep(30)')

    tauri.invoke.mockResolvedValueOnce(undefined)
    await cancel()

    expect(tauri.invoke).toHaveBeenLastCalledWith('cancel_query', { queryId: 7 })
  })

  it('usa el id del evento started cuando start_query aún no ha respondido', async () => {
    let resolveInvoke: ((id: number) => void) | undefined
    tauri.invoke.mockReturnValueOnce(
      new Promise<number>((resolve) => {
        resolveInvoke = resolve
      })
    )
    tauri.invoke.mockResolvedValueOnce(undefined)

    const { cancel, run } = useQuery()
    const pending = run('SELECT pg_sleep(30)')

    await vi.waitFor(() => expect(tauri.channels).toHaveLength(1))
    lastChannel().emit({ type: 'started', queryId: 9 })
    await cancel()

    expect(tauri.invoke).toHaveBeenLastCalledWith('cancel_query', { queryId: 9 })

    resolveInvoke?.(9)
    await pending
  })

  it('refleja el error 57014 que llega tras la cancelación', async () => {
    tauri.invoke.mockResolvedValueOnce(7)

    const { isRunning, error, cancel, run } = useQuery()
    await run('SELECT pg_sleep(30)')

    tauri.invoke.mockResolvedValueOnce(undefined)
    await cancel()

    expect(isRunning.value).toBe(true)

    lastChannel().emit({
      type: 'error',
      message: 'canceling statement due to user request',
      code: '57014'
    })

    expect(isRunning.value).toBe(false)
    expect(error.value).toEqual({
      message: 'canceling statement due to user request',
      code: '57014'
    })
  })

  it('no invoca el backend si no hay consulta en curso', async () => {
    const { cancel } = useQuery()

    await cancel()

    expect(tauri.invoke).not.toHaveBeenCalled()
  })

  it('expone un error legible si el backend no puede cancelar', async () => {
    tauri.invoke.mockResolvedValueOnce(7)

    const { isRunning, error, cancel, run } = useQuery()
    await run('SELECT pg_sleep(30)')

    tauri.invoke.mockRejectedValueOnce('no hay ninguna consulta en ejecución')
    await cancel()

    expect(isRunning.value).toBe(false)
    expect(error.value).toEqual({ message: 'no hay ninguna consulta en ejecución' })
  })
})

describe('reset', () => {
  it('limpia todo el estado y olvida el id', async () => {
    tauri.invoke.mockResolvedValueOnce(7)

    const { isRunning, columns, rows, rowsAffected, elapsedMs, error, reset, cancel, run } =
      useQuery()

    await run('SELECT 1')
    lastChannel().emit({ type: 'started', queryId: 7 })
    lastChannel().emit({ type: 'columns', columns: ['n'] })
    lastChannel().emit({ type: 'rows', rows: [['1']] })
    lastChannel().emit({ type: 'done', rowsAffected: 1, elapsedMs: 3 })

    error.value = { message: 'previo' }
    reset()

    expect(isRunning.value).toBe(false)
    expect(columns.value).toEqual([])
    expect(rows.value).toEqual([])
    expect(rowsAffected.value).toBeNull()
    expect(elapsedMs.value).toBeNull()
    expect(error.value).toBeNull()

    tauri.invoke.mockClear()
    await cancel()

    expect(tauri.invoke).not.toHaveBeenCalled()
  })
})
