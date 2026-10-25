import { describe, expect, it } from 'vitest'
import { clampPage, pageRange, rowsToRecords, tableColumns, totalPages } from './table'

describe('totalPages', () => {
  it('redondea hacia arriba y nunca baja de una página', () => {
    expect(totalPages(0, 100)).toBe(1)
    expect(totalPages(250, 100)).toBe(3)
  })
})

describe('clampPage', () => {
  it('recorta la página al rango válido', () => {
    expect(clampPage(9, 250, 100)).toBe(3)
    expect(clampPage(0, 250, 100)).toBe(1)
    expect(clampPage(2, 0, 100)).toBe(1)
  })

  it('conserva una página válida', () => {
    expect(clampPage(2, 250, 100)).toBe(2)
  })
})

describe('rowsToRecords', () => {
  it('rellena con null las celdas que faltan en la fila', () => {
    expect(rowsToRecords(['a', 'b'], [['x', null], ['y']])).toEqual([
      { a: 'x', b: null },
      { a: 'y', b: null }
    ])
  })
})

describe('tableColumns', () => {
  it('usa la columna como acceso y cabecera', () => {
    const [column] = tableColumns(['id'])

    expect(column?.accessorKey).toBe('id')
    expect(column?.header).toBe('id')
  })

  it('muestra NULL en lugar de espacio duro para valores ausentes', () => {
    const [column] = tableColumns(['id'])

    expect(column?.cell({ getValue: () => null })).toBe('NULL')
    expect(column?.cell({ getValue: () => '7' })).toBe('7')
  })
})

describe('pageRange', () => {
  it('calcula el rango visible de la página', () => {
    expect(pageRange(3, 100, 250)).toEqual({ from: 201, to: 250 })
    expect(pageRange(1, 100, 0)).toEqual({ from: 0, to: 0 })
  })
})
