import { describe, expect, it } from 'vitest'
import {
  SCHEMA_GROUP_DEFINITIONS,
  SCHEMA_GROUP_ORDER,
  SCHEMA_ICONS,
  buildSchemaTree,
  functionLabel,
  isSchemaLeaf,
  type SchemaCatalog,
  type SchemaInfo,
  type SchemaTreeItem
} from './schema'

const PUBLIC_SCHEMA: SchemaInfo = {
  name: 'public',
  tables: [{ oid: 1, name: 'clientes' }],
  views: [{ oid: 2, name: 'vista_clientes' }],
  functions: [
    { oid: 3, name: 'fn_total', signature: 'integer' },
    { oid: 4, name: 'fn_ping', signature: '' }
  ],
  enums: [{ oid: 5, name: 'estado' }]
}

const DEMO_SCHEMA: SchemaInfo = {
  name: 'demo',
  tables: [
    { oid: 10, name: 'zeta' },
    { oid: 11, name: 'alfa' }
  ],
  views: [{ oid: 12, name: 'v_demo' }],
  functions: [{ oid: 13, name: 'fn_demo', signature: 'integer, text' }],
  enums: [{ oid: 14, name: 'estado_demo' }]
}

const TABLES_ONLY_SCHEMA: SchemaInfo = {
  name: 'solo_tablas',
  tables: [{ oid: 20, name: 't' }],
  views: [],
  functions: [],
  enums: []
}

const CATALOG: SchemaCatalog = {
  database: 'trompa',
  schemas: [PUBLIC_SCHEMA, DEMO_SCHEMA, TABLES_ONLY_SCHEMA]
}

function findItem(items: SchemaTreeItem[], id: string): SchemaTreeItem {
  const found = items.find((item) => item.id === id)
  if (!found) throw new Error(`No se encontró el nodo ${id}`)
  return found
}

describe('buildSchemaTree', () => {
  it('devuelve un árbol vacío sin catálogo o con esquemas vacíos', () => {
    expect(buildSchemaTree(null)).toEqual([])
    expect(buildSchemaTree({ database: 'trompa', schemas: [] })).toEqual([])
  })

  it('preserva el orden de esquemas y crea un nodo por esquema', () => {
    const tree = buildSchemaTree(CATALOG)

    expect(tree.map((item) => item.id)).toEqual([
      'schema:public',
      'schema:demo',
      'schema:solo_tablas'
    ])
    expect(tree[0]).toMatchObject({
      id: 'schema:public',
      label: 'public',
      icon: SCHEMA_ICONS.schema,
      kind: 'schema',
      schema: 'public',
      name: 'public',
      defaultExpanded: true
    })
  })

  it('crea los grupos en el orden fijo Tablas, Vistas, Funciones y Enums', () => {
    const tree = buildSchemaTree(CATALOG)
    const demo = findItem(tree, 'schema:demo')

    expect(demo.children?.map((item) => item.name)).toEqual([...SCHEMA_GROUP_ORDER])
    expect(demo.children?.every((item) => item.kind === 'group')).toBe(true)
    expect(demo.children?.map((item) => item.id)).toEqual([
      'group:demo:table',
      'group:demo:view',
      'group:demo:function',
      'group:demo:enum'
    ])
    expect(demo.children?.map((item) => item.label)).toEqual([
      'Tablas',
      'Vistas',
      'Funciones',
      'Enums'
    ])
    expect(demo.children?.map((item) => item.icon)).toEqual([
      SCHEMA_GROUP_DEFINITIONS.table.icon,
      SCHEMA_GROUP_DEFINITIONS.view.icon,
      SCHEMA_GROUP_DEFINITIONS.function.icon,
      SCHEMA_GROUP_DEFINITIONS.enum.icon
    ])
    expect(demo.children?.every((item) => item.defaultExpanded === true)).toBe(true)
    expect(demo.children?.every((item) => item.schema === 'demo')).toBe(true)
  })

  it('omite los grupos sin elementos', () => {
    const tree = buildSchemaTree(CATALOG)
    const soloTablas = findItem(tree, 'schema:solo_tablas')

    expect(soloTablas.children?.map((item) => item.id)).toEqual(['group:solo_tablas:table'])
  })

  it('construye las hojas de tabla, vista, enum y función con sus ids e iconos', () => {
    const tree = buildSchemaTree(CATALOG)
    const demo = findItem(tree, 'schema:demo')

    const tables = demo.children?.find((item) => item.id === 'group:demo:table')?.children
    const views = demo.children?.find((item) => item.id === 'group:demo:view')?.children
    const functions = demo.children?.find((item) => item.id === 'group:demo:function')?.children
    const enums = demo.children?.find((item) => item.id === 'group:demo:enum')?.children

    expect(tables?.map((item) => item.name)).toEqual(['zeta', 'alfa'])
    expect(tables?.[0]).toEqual({
      id: 'table:demo:10',
      label: 'zeta',
      icon: 'i-lucide-table',
      kind: 'table',
      schema: 'demo',
      name: 'zeta',
      oid: 10
    })
    expect(views?.[0]).toEqual({
      id: 'view:demo:12',
      label: 'v_demo',
      icon: 'i-lucide-eye',
      kind: 'view',
      schema: 'demo',
      name: 'v_demo',
      oid: 12
    })
    expect(functions?.[0]).toEqual({
      id: 'function:demo:13',
      label: 'fn_demo(integer, text)',
      icon: 'i-lucide-square-function',
      kind: 'function',
      schema: 'demo',
      name: 'fn_demo',
      oid: 13
    })
    expect(enums?.[0]).toEqual({
      id: 'enum:demo:14',
      label: 'estado_demo',
      icon: 'i-lucide-list',
      kind: 'enum',
      schema: 'demo',
      name: 'estado_demo',
      oid: 14
    })
  })

  it('etiqueta las funciones con y sin firma', () => {
    const tree = buildSchemaTree(CATALOG)
    const publicSchema = findItem(tree, 'schema:public')
    const functions = publicSchema.children?.find(
      (item) => item.id === 'group:public:function'
    )?.children

    expect(functions?.map((item) => item.id)).toEqual([
      'function:public:3',
      'function:public:4'
    ])
    expect(functions?.map((item) => item.label)).toEqual(['fn_total(integer)', 'fn_ping'])
  })
})

describe('functionLabel', () => {
  it('incluye los paréntesis solo cuando hay firma', () => {
    expect(functionLabel({ oid: 1, name: 'fn', signature: 'integer' })).toBe('fn(integer)')
    expect(functionLabel({ oid: 2, name: 'fn', signature: '' })).toBe('fn')
  })
})

describe('isSchemaLeaf', () => {
  it('es verdadero solo para hojas', () => {
    const tree = buildSchemaTree(CATALOG)
    const demo = findItem(tree, 'schema:demo')
    const group = findItem(demo.children ?? [], 'group:demo:table')
    const leaf = findItem(group.children ?? [], 'table:demo:10')

    expect(isSchemaLeaf(leaf)).toBe(true)
    expect(isSchemaLeaf(group)).toBe(false)
    expect(isSchemaLeaf(demo)).toBe(false)
    expect(isSchemaLeaf(null)).toBe(false)
    expect(isSchemaLeaf(undefined)).toBe(false)
  })
})
