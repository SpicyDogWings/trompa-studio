// Traducción de un tema del backend a CSS.
//
// Nuxt UI 4 genera los colores de la paleta en runtime desde
// `useAppConfig().ui.colors` (y no admite variables): esos se aplican por la
// vía del composable. El resto de tokens (`--ui-bg`, `--ui-text`, radios…) son
// estáticos en `@layer theme`, así que una hoja sin capa los sobrescribe sin
// `!important`. El modo claro/oscuro lo marca `@nuxtjs/color-mode` con las
// clases `light`/`dark` en `<html>`.

export interface ThemePalette {
  primary?: string | null
  secondary?: string | null
  success?: string | null
  info?: string | null
  warning?: string | null
  error?: string | null
  neutral?: string | null
}

export interface ThemeShape {
  radius?: string | null
  container?: string | null
  headerHeight?: string | null
}

export interface ThemePanel {
  default?: string | null
  border?: string | null
}

export interface ThemeSurface {
  default?: string | null
}

export interface ThemeBackground {
  default?: string | null
  muted?: string | null
  elevated?: string | null
  accented?: string | null
  inverted?: string | null
  sidebar?: ThemePanel
  header?: ThemeSurface
  content?: ThemeSurface
}

export interface ThemeText {
  default?: string | null
  dimmed?: string | null
  muted?: string | null
  toned?: string | null
  highlighted?: string | null
  inverted?: string | null
}

export interface ThemeBorder {
  default?: string | null
  muted?: string | null
  accented?: string | null
  inverted?: string | null
}

export interface ThemeMode {
  background?: ThemeBackground
  text?: ThemeText
  border?: ThemeBorder
  chart?: Record<string, string>
}

export interface ThemeDefinition {
  id: string
  name: string
  description: string
  builtin: boolean
  palette: ThemePalette
  shape: ThemeShape
  light: ThemeMode
  dark: ThemeMode
  path?: string | null
}

export interface ThemeErrorInfo {
  file: string
  message: string
}

export interface ThemeCatalog {
  themes: ThemeDefinition[]
  errors: ThemeErrorInfo[]
  directory: string
}

/** Paleta semántica del tema, sin claves vacías, lista para `ui.colors`. */
export function paletteOverrides(theme: ThemeDefinition | null): Record<string, string> {
  const palette: Record<string, string> = {}
  if (!theme) return palette

  for (const [key, value] of Object.entries(theme.palette ?? {})) {
    if (typeof value === 'string' && value.length > 0) palette[key] = value
  }

  return palette
}

type Declaration = readonly [name: string, value: string | null | undefined]

function declarations(entries: readonly Declaration[]): string {
  return entries
    .filter((entry): entry is readonly [string, string] => typeof entry[1] === 'string' && entry[1].length > 0)
    .map(([name, value]) => `${name}:${value}`)
    .join(';')
}

/** Tokens de un modo, en el orden documento del formato. */
function modeDeclarations(mode: ThemeMode | undefined): string {
  const background = mode?.background ?? {}
  const text = mode?.text ?? {}
  const border = mode?.border ?? {}
  const chart = mode?.chart ?? {}

  const entries: Declaration[] = [
    ['--ui-bg', background.default],
    ['--ui-bg-muted', background.muted],
    ['--ui-bg-elevated', background.elevated],
    ['--ui-bg-accented', background.accented],
    ['--ui-bg-inverted', background.inverted],
    ['--trompa-sidebar-bg', background.sidebar?.default],
    ['--trompa-sidebar-border', background.sidebar?.border],
    ['--trompa-header-bg', background.header?.default],
    ['--trompa-content-bg', background.content?.default],
    ['--ui-text', text.default],
    ['--ui-text-dimmed', text.dimmed],
    ['--ui-text-muted', text.muted],
    ['--ui-text-toned', text.toned],
    ['--ui-text-highlighted', text.highlighted],
    ['--ui-text-inverted', text.inverted],
    ['--ui-border', border.default],
    ['--ui-border-muted', border.muted],
    ['--ui-border-accented', border.accented],
    ['--ui-border-inverted', border.inverted]
  ]

  const charts = Object.keys(chart).sort((a, b) => Number(a) - Number(b))
  for (const key of charts) {
    entries.push([`--ui-chart-${key}`, chart[key]])
  }

  return declarations(entries)
}

/**
 * Hoja de un tema: `:root` para la forma, `:root,.light` y `.dark` para los
 * tokens de cada modo. Cada bloque solo aparece si tiene propiedades; un tema
 * sin forma ni tokens produce `''`.
 */
export function themeCss(theme: ThemeDefinition): string {
  const shape = declarations([
    ['--ui-radius', theme.shape?.radius],
    ['--ui-container', theme.shape?.container],
    ['--ui-header-height', theme.shape?.headerHeight]
  ])
  const light = modeDeclarations(theme.light)
  const dark = modeDeclarations(theme.dark)

  const blocks: string[] = []
  if (shape) blocks.push(`:root{${shape}}`)
  if (light) blocks.push(`:root,.light{${light}}`)
  if (dark) blocks.push(`.dark{${dark}}`)

  return blocks.join('')
}

/**
 * Crea, reemplaza o elimina `<style id="trompa-theme">`. Sin documento (SSR) no
 * hace nada.
 */
export function applyThemeStyleElement(css: string): void {
  if (typeof document === 'undefined') return

  const existing = document.getElementById('trompa-theme')

  if (css === '') {
    existing?.remove()
    return
  }

  if (existing) {
    existing.textContent = css
    return
  }

  const style = document.createElement('style')
  style.id = 'trompa-theme'
  style.textContent = css
  document.head.appendChild(style)
}
