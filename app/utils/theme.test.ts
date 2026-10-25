import { afterEach, describe, expect, it } from 'vitest'
import {
  applyThemeStyleElement,
  paletteOverrides,
  themeCss,
  type ThemeDefinition
} from '~/utils/theme'

function theme(overrides: Partial<ThemeDefinition> = {}): ThemeDefinition {
  return {
    id: 'x',
    name: 'X',
    description: '',
    builtin: false,
    palette: {},
    shape: {},
    light: {},
    dark: {},
    path: null,
    ...overrides
  }
}

afterEach(() => {
  document.getElementById('trompa-theme')?.remove()
})

describe('themeCss', () => {
  it('genera los tres bloques en el orden del formato', () => {
    const css = themeCss(
      theme({
        shape: { radius: '0.375rem' },
        light: {
          background: {
            default: '#faf7f2',
            sidebar: { default: '#f1e9dc', border: '#e0d3bf' },
            header: { default: '#faf7f2' },
            content: { default: 'transparent' }
          },
          text: { default: '#3b2f24', highlighted: '#1f1710' },
          border: { default: '#e0d3bf' },
          chart: { '2': '#0f766e', '1': '#b45309' }
        },
        dark: { background: { default: '#201a14' } }
      })
    )

    expect(css).toBe(
      ':root{--ui-radius:0.375rem}' +
        ':root,.light{--ui-bg:#faf7f2;--trompa-sidebar-bg:#f1e9dc;--trompa-sidebar-border:#e0d3bf;--trompa-header-bg:#faf7f2;--trompa-content-bg:transparent;--ui-text:#3b2f24;--ui-text-highlighted:#1f1710;--ui-border:#e0d3bf;--ui-chart-1:#b45309;--ui-chart-2:#0f766e}' +
        '.dark{--ui-bg:#201a14}'
    )
  })

  it('devuelve una hoja vacía para un tema solo de paleta', () => {
    expect(themeCss(theme({ palette: { primary: 'blue', neutral: 'slate' } }))).toBe('')
  })

  it('omite claves nulas o vacías', () => {
    expect(
      themeCss(
        theme({
          shape: { radius: null, container: '' },
          light: { background: { default: null }, chart: {} },
          dark: {}
        })
      )
    ).toBe('')
  })

  it('emite solo el bloque del modo que tenga tokens', () => {
    expect(themeCss(theme({ dark: { text: { dimmed: '#9c8b76' } } }))).toBe(
      '.dark{--ui-text-dimmed:#9c8b76}'
    )
  })
})

describe('paletteOverrides', () => {
  it('devuelve un objeto vacío sin tema', () => {
    expect(paletteOverrides(null)).toEqual({})
  })

  it('copia solo las paletas definidas', () => {
    const overrides = paletteOverrides(
      theme({ palette: { primary: 'amber', secondary: null, neutral: '' } })
    )

    expect(overrides).toEqual({ primary: 'amber' })
  })
})

describe('applyThemeStyleElement', () => {
  it('crea, reemplaza y elimina la hoja del tema', () => {
    applyThemeStyleElement(':root{--ui-bg:#fff}')

    expect(document.querySelectorAll('#trompa-theme')).toHaveLength(1)
    expect(document.getElementById('trompa-theme')?.textContent).toBe(':root{--ui-bg:#fff}')

    applyThemeStyleElement(':root{--ui-bg:#000}')

    expect(document.querySelectorAll('#trompa-theme')).toHaveLength(1)
    expect(document.getElementById('trompa-theme')?.textContent).toBe(':root{--ui-bg:#000}')

    applyThemeStyleElement('')

    expect(document.getElementById('trompa-theme')).toBeNull()
  })

  it('eliminar una hoja que no existe no falla', () => {
    expect(() => applyThemeStyleElement('')).not.toThrow()
  })
})
