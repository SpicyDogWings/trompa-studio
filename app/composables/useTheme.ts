import { computed, ref, type ComputedRef, type Ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { toDbError, type DbErrorPayload } from '~/utils/connection'
import { usePreferences } from '~/composables/usePreferences'
import {
  applyThemeStyleElement,
  paletteOverrides,
  themeCss,
  type ThemeCatalog,
  type ThemeDefinition
} from '~/utils/theme'

// Estado a nivel de módulo: el tema es un singleton de aplicación, igual que
// las preferencias.
const catalog = ref<ThemeCatalog | null>(null)
const isLoading = ref(false)
const error = ref<DbErrorPayload | null>(null)

let loaded = false

// La paleta de `app.config.ts` es la base: el tema solo la sobrescribe y
// quitarlo debe devolver los valores originales.
let paletteBase: Record<string, string> | null = null

/**
 * Copia en TS de `themes::theme_slug`: minúsculas ASCII, diacríticos
 * españoles plegados y cualquier racha que no sea `[a-z0-9]` → `-`.
 */
export function themeSlug(reference: string): string {
  const folded = reference
    .trim()
    .replace(/[áàäâãÁÀÄÂÃ]/g, 'a')
    .replace(/[éèëêÉÈËÊ]/g, 'e')
    .replace(/[íìïîÍÌÏÎ]/g, 'i')
    .replace(/[óòöôõÓÒÖÔÕ]/g, 'o')
    .replace(/[úùüûÚÙÜÛ]/g, 'u')
    .replace(/[ñÑ]/g, 'n')
    .replace(/[çÇ]/g, 'c')
    .toLowerCase()

  return folded
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '')
}

export function useTheme(): {
  catalog: Ref<ThemeCatalog | null>
  isLoading: Ref<boolean>
  error: Ref<DbErrorPayload | null>
  active: ComputedRef<ThemeDefinition | null>
  load(force?: boolean): Promise<void>
  apply(): void
  resetTheme(): void
} {
  /**
   * Resuelve `[ui] theme`: por `id` (nombre normalizado a slug), por ruta
   * absoluta y, si nada coincide, por el primer integrado.
   */
  const active = computed<ThemeDefinition | null>(() => {
    const themes = catalog.value?.themes ?? []
    if (themes.length === 0) return null

    const reference = usePreferences().current().ui.theme
    const slug = themeSlug(reference)

    return (
      themes.find((theme) => theme.id === slug) ??
      themes.find((theme) => theme.path === reference) ??
      themes.find((theme) => theme.builtin) ??
      null
    )
  })

  async function load(force = false): Promise<void> {
    if (loaded && !force) return

    error.value = null
    isLoading.value = true

    try {
      catalog.value = await invoke<ThemeCatalog>('list_themes')
      loaded = true
    } catch (cause) {
      // El catálogo anterior se conserva: un fallo de disco no debe dejar la
      // app sin tema.
      error.value = toDbError(cause)
    } finally {
      isLoading.value = false
    }
  }

  function apply(): void {
    if (!catalog.value) return

    const appConfig = useAppConfig()
    paletteBase ??= { ...appConfig.ui.colors }
    Object.assign(appConfig.ui.colors, paletteBase, paletteOverrides(active.value))
    applyThemeStyleElement(active.value ? themeCss(active.value) : '')
  }

  /** Solo para tests: devuelve el singleton a su estado inicial. */
  function resetTheme(): void {
    catalog.value = null
    isLoading.value = false
    error.value = null
    loaded = false
    paletteBase = null
  }

  return { catalog, isLoading, error, active, load, apply, resetTheme }
}
