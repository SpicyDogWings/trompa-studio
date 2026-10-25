export type SchemaObjectKind = 'table' | 'view' | 'function' | 'enum'
export type SchemaNodeKind = 'schema' | 'group' | SchemaObjectKind

export interface SchemaObject {
  oid: number
  name: string
}

export interface SchemaFunction extends SchemaObject {
  signature: string
}

export interface SchemaInfo {
  name: string
  tables: SchemaObject[]
  views: SchemaObject[]
  functions: SchemaFunction[]
  enums: SchemaObject[]
}

export interface SchemaCatalog {
  database: string
  schemas: SchemaInfo[]
}

export interface SchemaTreeItem {
  id: string
  label: string
  icon: string
  kind: SchemaNodeKind
  schema: string
  name: string
  oid?: number
  defaultExpanded?: boolean
  children?: SchemaTreeItem[]
}

export const SCHEMA_ICONS = {
  schema: 'i-lucide-folder',
  table: 'i-lucide-table',
  view: 'i-lucide-eye',
  function: 'i-lucide-square-function',
  enum: 'i-lucide-list'
} as const

export const SCHEMA_GROUP_ORDER = ['table', 'view', 'function', 'enum'] as const

export type SchemaGroupKind = (typeof SCHEMA_GROUP_ORDER)[number]

export const SCHEMA_GROUP_DEFINITIONS: Record<
  SchemaGroupKind,
  { label: string; icon: string }
> = {
  table: { label: 'Tablas', icon: 'i-lucide-table-2' },
  view: { label: 'Vistas', icon: 'i-lucide-eye' },
  function: { label: 'Funciones', icon: 'i-lucide-square-function' },
  enum: { label: 'Enums', icon: 'i-lucide-list' }
}

export const SCHEMA_LEAF_KINDS: readonly SchemaObjectKind[] = [
  'table',
  'view',
  'function',
  'enum'
]

const LEAF_KIND_SET: ReadonlySet<SchemaNodeKind> = new Set(SCHEMA_LEAF_KINDS)

function isSchemaObjectKind(kind: SchemaNodeKind): kind is SchemaObjectKind {
  return LEAF_KIND_SET.has(kind)
}

export function functionLabel(object: SchemaFunction): string {
  return object.signature ? `${object.name}(${object.signature})` : object.name
}

function buildLeaf(
  schema: string,
  kind: SchemaObjectKind,
  object: SchemaObject
): SchemaTreeItem {
  return {
    id: `${kind}:${schema}:${object.oid}`,
    label: object.name,
    icon: SCHEMA_ICONS[kind],
    kind,
    schema,
    name: object.name,
    oid: object.oid
  }
}

function groupLeaves(schema: SchemaInfo, kind: SchemaGroupKind): SchemaTreeItem[] {
  switch (kind) {
    case 'table':
      return schema.tables.map((table) => buildLeaf(schema.name, 'table', table))
    case 'view':
      return schema.views.map((view) => buildLeaf(schema.name, 'view', view))
    case 'function':
      return schema.functions.map((fn) => ({
        ...buildLeaf(schema.name, 'function', fn),
        label: functionLabel(fn)
      }))
    case 'enum':
      return schema.enums.map((item) => buildLeaf(schema.name, 'enum', item))
  }
}

export function buildSchemaTree(catalog: SchemaCatalog | null): SchemaTreeItem[] {
  if (!catalog) return []

  return catalog.schemas.map((schema) => {
    const children: SchemaTreeItem[] = []

    for (const kind of SCHEMA_GROUP_ORDER) {
      const leaves = groupLeaves(schema, kind)
      if (leaves.length === 0) continue

      const definition = SCHEMA_GROUP_DEFINITIONS[kind]

      children.push({
        id: `group:${schema.name}:${kind}`,
        label: definition.label,
        icon: definition.icon,
        kind: 'group',
        schema: schema.name,
        name: kind,
        defaultExpanded: true,
        children: leaves
      })
    }

    return {
      id: `schema:${schema.name}`,
      label: schema.name,
      icon: SCHEMA_ICONS.schema,
      kind: 'schema',
      schema: schema.name,
      name: schema.name,
      defaultExpanded: true,
      children
    }
  })
}

export function isSchemaLeaf(
  item: SchemaTreeItem | null | undefined
): item is SchemaTreeItem {
  return !!item && isSchemaObjectKind(item.kind)
}
