export const PAGE_SIZE = 100
export const FREEZE_AFTER_MS = 60_000
export const FREEZE_TICK_MS = 5_000

export type TableCell = string | null
export type TableRow = Record<string, string | null>

export interface TablePage {
  columns: string[]
  rows: TableCell[][]
  total: number
  page: number
  pageSize: number
}

export interface TableColumnDefinition {
  accessorKey: string
  header: string
  cell: (context: { getValue: () => unknown }) => string
}

/** El backend recorta la página con este mismo cálculo, así que un total de 0 sigue siendo la página 1. */
export function totalPages(total: number, pageSize: number): number {
  return Math.max(1, Math.ceil(total / pageSize))
}

export function clampPage(page: number, total: number, pageSize: number): number {
  return Math.min(Math.max(page, 1), totalPages(total, pageSize))
}

export function rowsToRecords(columns: string[], rows: TableCell[][]): TableRow[] {
  return rows.map((row) => {
    const record: TableRow = {}

    columns.forEach((column, index) => {
      record[column] = row[index] ?? null
    })

    return record
  })
}

/** UTable pinta `null` y `''` como espacio duro, así que la celda se declara para mostrar `NULL`. */
export function tableColumns(columns: string[]): TableColumnDefinition[] {
  return columns.map((column) => ({
    accessorKey: column,
    header: column,
    cell: ({ getValue }) => {
      const value = getValue()
      return value === null || value === undefined ? 'NULL' : String(value)
    }
  }))
}

export function pageRange(
  page: number,
  pageSize: number,
  total: number
): { from: number; to: number } {
  if (total === 0) return { from: 0, to: 0 }

  return {
    from: (page - 1) * pageSize + 1,
    to: Math.min(page * pageSize, total)
  }
}
