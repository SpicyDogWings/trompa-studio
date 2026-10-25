import { beforeEach, describe, expect, it, vi } from 'vitest'
import { useSchema } from './useSchema'
import { usePreferences } from './usePreferences'
import { DEFAULT_PREFERENCES } from '~/utils/preferences'
import { buildSchemaTree, type SchemaCatalog } from '~/utils/schema'

const tauri = vi.hoisted(() => ({ invoke: vi.fn() }))

vi.mock('@tauri-apps/api/core', () => tauri)

const FIRST_CATALOG: SchemaCatalog = {
  database: 'trompa',
  schemas: [
    {
      name: 'public',
      tables: [{ oid: 1, name: 'clientes' }],
      views: [],
      functions: [],
      enums: []
    }
  ]
}

const SECOND_CATALOG: SchemaCatalog = {
  database: 'trompa',
  schemas: [
    {
      name: 'demo',
      tables: [{ oid: 2, name: 'tabla_demo' }],
      views: [],
      functions: [],
      enums: []
    }
  ]
}

const LEAF = buildSchemaTree(FIRST_CATALOG)[0]?.children?.[0]?.children?.[0]
const GROUP = buildSchemaTree(FIRST_CATALOG)[0]?.children?.[0]
const SCHEMA_NODE = buildSchemaTree(FIRST_CATALOG)[0]

beforeEach(() => {
  usePreferences().resetPreferences()
  tauri.invoke.mockReset()
})

describe('load', () => {
  it('carga el catálogo y limpia la selección', async () => {
    tauri.invoke.mockResolvedValue(FIRST_CATALOG)
    const schema = useSchema()

    schema.select(LEAF ?? null)
    await schema.load()

    expect(tauri.invoke).toHaveBeenCalledWith('list_schema')
    expect(schema.catalog.value).toEqual(FIRST_CATALOG)
    expect(schema.tree.value.map((item) => item.id)).toEqual(['schema:public'])
    expect(schema.selected.value).toBeNull()
    expect(schema.error.value).toBeNull()
    expect(schema.isLoading.value).toBe(false)
  })

  it('guarda el error con mensaje y código SQLSTATE', async () => {
    tauri.invoke.mockRejectedValue({ message: 'fallo del catálogo', code: '42P01' })
    const schema = useSchema()

    await schema.load()

    expect(schema.error.value).toEqual({ message: 'fallo del catálogo', code: '42P01' })
    expect(schema.catalog.value).toBeNull()
    expect(schema.selected.value).toBeNull()
    expect(schema.isLoading.value).toBe(false)
  })

  it('normaliza un error en forma de string', async () => {
    tauri.invoke.mockRejectedValue('sin conexión')
    const schema = useSchema()

    await schema.load()

    expect(schema.error.value).toEqual({ message: 'sin conexión' })
    expect(schema.catalog.value).toBeNull()
    expect(schema.isLoading.value).toBe(false)
  })

  it('ignora la respuesta vieja cuando llega después de una nueva carga', async () => {
    const firstRequest = Promise.withResolvers<SchemaCatalog>()
    tauri.invoke
      .mockImplementationOnce(() => firstRequest.promise)
      .mockResolvedValueOnce(SECOND_CATALOG)

    const schema = useSchema()

    const first = schema.load()
    const second = schema.load()

    await second
    expect(schema.catalog.value).toEqual(SECOND_CATALOG)
    expect(schema.isLoading.value).toBe(false)

    firstRequest.resolve(FIRST_CATALOG)
    await first
    expect(schema.catalog.value).toEqual(SECOND_CATALOG)
    expect(schema.isLoading.value).toBe(false)
  })

  it('ignora el error viejo cuando llega después de una nueva carga', async () => {
    const firstRequest = Promise.withResolvers<SchemaCatalog>()
    tauri.invoke
      .mockImplementationOnce(() => firstRequest.promise)
      .mockResolvedValueOnce(SECOND_CATALOG)

    const schema = useSchema()

    const first = schema.load()
    const second = schema.load()

    await second
    firstRequest.reject({ message: 'error viejo' })
    await first

    expect(schema.error.value).toBeNull()
    expect(schema.catalog.value).toEqual(SECOND_CATALOG)
    expect(schema.isLoading.value).toBe(false)
  })
})

describe('select', () => {
  it('acepta hojas y descarta nodos de esquema, grupo o null', () => {
    const schema = useSchema()

    expect(LEAF).toBeDefined()
    expect(GROUP).toBeDefined()
    expect(SCHEMA_NODE).toBeDefined()

    schema.select(LEAF ?? null)
    expect(schema.selected.value).toEqual(LEAF)

    schema.select(GROUP ?? null)
    expect(schema.selected.value).toBeNull()

    schema.select(SCHEMA_NODE ?? null)
    expect(schema.selected.value).toBeNull()

    schema.select(null)
    expect(schema.selected.value).toBeNull()
  })
})

describe('posición del sidebar', () => {
  it('guarda la posición en las preferencias', async () => {
    tauri.invoke.mockResolvedValue(DEFAULT_PREFERENCES)
    const schema = useSchema()

    schema.setPosition('left')

    expect(schema.position.value).toBe('left')
    await vi.waitFor(() =>
      expect(tauri.invoke).toHaveBeenCalledWith('write_preferences', {
        preferences: expect.objectContaining({ schema: { sidebarPosition: 'left' } })
      })
    )
  })

  it('restaura la posición de las preferencias cargadas', async () => {
    tauri.invoke.mockResolvedValueOnce({
      ...DEFAULT_PREFERENCES,
      schema: { sidebarPosition: 'left' }
    })
    await usePreferences().load()

    const schema = useSchema()

    schema.position.value = 'right'
    schema.restorePosition()

    expect(schema.position.value).toBe('left')
  })

  it('restaura right cuando no hay preferencias cargadas', () => {
    const schema = useSchema()

    schema.position.value = 'left'
    schema.restorePosition()

    expect(schema.position.value).toBe('right')
  })

  it('alterna la posición y la persiste', async () => {
    tauri.invoke.mockResolvedValue(DEFAULT_PREFERENCES)
    const schema = useSchema()

    expect(schema.position.value).toBe('right')

    schema.togglePosition()
    expect(schema.position.value).toBe('left')
    await vi.waitFor(() =>
      expect(tauri.invoke).toHaveBeenCalledWith('write_preferences', {
        preferences: expect.objectContaining({ schema: { sidebarPosition: 'left' } })
      })
    )

    schema.togglePosition()
    expect(schema.position.value).toBe('right')
    await vi.waitFor(() =>
      expect(tauri.invoke).toHaveBeenCalledWith('write_preferences', {
        preferences: expect.objectContaining({ schema: { sidebarPosition: 'right' } })
      })
    )
  })
})

describe('reset', () => {
  it('limpia catálogo, selección y error', async () => {
    tauri.invoke.mockRejectedValue({ message: 'fallo', code: '08006' })
    const schema = useSchema()

    await schema.load()
    expect(schema.error.value).not.toBeNull()

    tauri.invoke.mockResolvedValue(FIRST_CATALOG)
    await schema.load()
    schema.select(LEAF ?? null)
    expect(schema.catalog.value).toEqual(FIRST_CATALOG)
    expect(schema.selected.value).not.toBeNull()

    schema.reset()

    expect(schema.catalog.value).toBeNull()
    expect(schema.tree.value).toEqual([])
    expect(schema.selected.value).toBeNull()
    expect(schema.error.value).toBeNull()
    expect(schema.isLoading.value).toBe(false)
  })
})
