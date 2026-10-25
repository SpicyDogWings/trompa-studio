import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { effectScope, type EffectScope } from 'vue'
import { SCHEMA_ICONS, type SchemaObjectKind, type SchemaTreeItem } from '~/utils/schema'
import { useTableTabs } from './useTableTabs'

const tauri = vi.hoisted(() => ({ invoke: vi.fn() }))

vi.mock('@tauri-apps/api/core', () => tauri)

function leaf(id: string, name: string, kind: SchemaObjectKind = 'table'): SchemaTreeItem {
  return { id, label: `public.${name}`, icon: SCHEMA_ICONS[kind], kind, schema: 'public', name }
}

const CLIENTES = leaf('table:public:1', 'clientes')
const PEDIDOS = leaf('table:public:2', 'pedidos')
const FACTURAS = leaf('table:public:3', 'facturas')
const FUNCION = leaf('function:public:7', 'calcular_total', 'function')

const scopes: EffectScope[] = []

/** Deja correr las promesas pendientes del mock sin depender de temporizadores reales. */
async function flush() {
  await Promise.resolve()
  await Promise.resolve()
  await Promise.resolve()
}

function mount() {
  const scope = effectScope()
  const tabs = scope.run(() => useTableTabs())!
  scopes.push(scope)
  return tabs
}

beforeEach(() => {
  tauri.invoke.mockReset()
  // La respuesta refleja la página pedida, como haría el backend al recortarla.
  tauri.invoke.mockImplementation(
    async (
      _command: string,
      args: { schema: string; table: string; page: number; pageSize: number }
    ) => ({
      columns: ['id'],
      rows: [['1']],
      total: 250,
      page: args.page,
      pageSize: args.pageSize
    })
  )
})

afterEach(() => {
  for (const scope of scopes.splice(0)) scope.stop()
  vi.useRealTimers()
})

describe('open', () => {
  it('crea la pestaña y carga la primera página', async () => {
    const { tabs, activeId, open } = mount()

    open(CLIENTES)

    expect(activeId.value).toBe('table:public:1')
    expect(tabs.value).toHaveLength(1)
    expect(tabs.value[0]?.label).toBe('public.clientes')
    expect(tauri.invoke).toHaveBeenCalledWith('fetch_table_page', {
      schema: 'public',
      table: 'clientes',
      page: 1,
      pageSize: 100
    })

    await vi.waitFor(() => expect(tabs.value[0]?.status).toBe('ready'))
    expect(tabs.value[0]?.rows).toEqual([{ id: '1' }])
    expect(tabs.value[0]?.total).toBe(250)
  })

  it('no duplica la pestaña ni repite la carga de una pestaña lista', async () => {
    const { tabs, open } = mount()

    open(CLIENTES)
    await vi.waitFor(() => expect(tabs.value[0]?.status).toBe('ready'))
    open(CLIENTES)

    expect(tabs.value).toHaveLength(1)
    expect(tauri.invoke).toHaveBeenCalledTimes(1)
  })

  it('no consulta el backend para funciones', async () => {
    const { tabs, activeId, open } = mount()

    open(FUNCION)

    expect(activeId.value).toBe('function:public:7')
    expect(tabs.value[0]?.status).toBe('idle')
    expect(tauri.invoke).not.toHaveBeenCalled()
  })

  it('guarda el error del backend con su código SQLSTATE', async () => {
    const { tabs, open } = mount()
    tauri.invoke.mockRejectedValueOnce({ message: 'boom', code: '42P01' })

    open(CLIENTES)

    await vi.waitFor(() => expect(tabs.value[0]?.status).toBe('error'))
    expect(tabs.value[0]?.error).toEqual({ message: 'boom', code: '42P01' })
  })
})

describe('goToPage', () => {
  it('recarga la página pedida de la pestaña activa', async () => {
    const { tabs, open, goToPage } = mount()

    open(CLIENTES)
    await vi.waitFor(() => expect(tabs.value[0]?.status).toBe('ready'))

    goToPage(3)

    expect(tauri.invoke).toHaveBeenLastCalledWith('fetch_table_page', {
      schema: 'public',
      table: 'clientes',
      page: 3,
      pageSize: 100
    })
    await vi.waitFor(() => expect(tabs.value[0]?.status).toBe('ready'))
    expect(tabs.value[0]?.page).toBe(3)
  })

  it('ignora la página actual cuando ya está cargada', async () => {
    const { tabs, open, goToPage } = mount()

    open(CLIENTES)
    await vi.waitFor(() => expect(tabs.value[0]?.status).toBe('ready'))
    goToPage(1)

    expect(tauri.invoke).toHaveBeenCalledTimes(1)
  })
})

describe('close', () => {
  it('activa la vecina y deja la vista sin pestaña al cerrar la última', () => {
    const { tabs, activeId, open, activate, close } = mount()

    open(CLIENTES)
    open(PEDIDOS)
    open(FACTURAS)
    activate('table:public:1')

    close('table:public:1')
    expect(activeId.value).toBe('table:public:2')

    close('table:public:2')
    expect(activeId.value).toBe('table:public:3')

    close('table:public:3')
    expect(tabs.value).toHaveLength(0)
    expect(activeId.value).toBeNull()
  })
})

describe('congelado', () => {
  it('suelta los datos de la pestaña inactiva y los recarga al enfocarla', async () => {
    vi.useFakeTimers()
    const { tabs, activeId, open, activate } = mount()

    open(CLIENTES)
    open(PEDIDOS)
    await flush()

    expect(tabs.value[0]?.rows.length).toBeGreaterThan(0)
    expect(tabs.value[0]?.frozen).toBe(false)

    await vi.advanceTimersByTimeAsync(30_000)
    expect(tabs.value[0]?.frozen).toBe(false)
    expect(tabs.value[0]?.rows.length).toBeGreaterThan(0)

    await vi.advanceTimersByTimeAsync(35_000)
    expect(tabs.value[0]?.frozen).toBe(true)
    expect(tabs.value[0]?.rows).toEqual([])
    expect(tabs.value[0]?.columns).toEqual([])
    expect(tabs.value[0]?.total).toBe(0)

    tauri.invoke.mockClear()
    activate('table:public:1')
    await flush()

    expect(activeId.value).toBe('table:public:1')
    expect(tauri.invoke).toHaveBeenCalledTimes(1)
    expect(tauri.invoke).toHaveBeenCalledWith('fetch_table_page', {
      schema: 'public',
      table: 'clientes',
      page: 1,
      pageSize: 100
    })
    expect(tabs.value[0]?.frozen).toBe(false)
    expect(tabs.value[0]?.rows).toEqual([{ id: '1' }])
  })
})

describe('resetTabs', () => {
  it('vacía las pestañas y la pestaña activa', () => {
    const { tabs, activeId, open, resetTabs } = mount()

    open(CLIENTES)
    resetTabs()

    expect(tabs.value).toEqual([])
    expect(activeId.value).toBeNull()
  })
})
