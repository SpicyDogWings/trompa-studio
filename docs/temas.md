# Temas

## Informe: cómo se compone el aspecto hoy

El color de la app se decide en tres capas:

1. **`app/app.config.ts`** fija la paleta semántica de Nuxt UI (`ui.colors = { primary: 'blue', neutral: 'slate' }`).
2. **Nuxt UI 4 genera esa paleta en runtime.** El plugin `@nuxt/ui` emite un `<style id="nuxt-ui-colors">` a partir de `useAppConfig().ui.colors`; por cada clave salen las variables `--ui-color-<clave>-<sombra>` y `--ui-<clave>` (sombra 500 en `.light`, 400 en `.dark`). Los valores deben ser nombres de `tailwindcss/colors`, porque lo que se emite es `var(--color-<nombre>-<sombra>, …)`: un literal `#ff0000` produce CSS inválido.
3. **El resto de tokens es estático** en `node_modules/@nuxt/ui/dist/runtime/index.css`, dentro de `@layer theme` y sin API de runtime: `--ui-radius`, `--ui-container`, `--ui-header-height` y, por modo, `--ui-bg`, `--ui-bg-muted`, `--ui-bg-elevated`, `--ui-bg-accented`, `--ui-bg-inverted`, `--ui-text*`, `--ui-border*` y `--ui-chart-1..6`.

Como esas reglas viven en una capa, una hoja **sin capa** las sobrescribe sin `!important`. Y como `@nuxtjs/color-mode` pone las clases `light`/`dark` en `<html>`, un bloque `.dark { … }` propio se aplica solo cuando toca.

Por eso un tema usa dos vías:

- **Paletas** (primary, neutral…) por `useAppConfig().ui.colors`, que es lo único reactivo.
- **Tokens de superficie** por un `<style id="trompa-theme">` sin capa, con bloques `.light`/`.dark`.

Las tres regiones de la interfaz (panel del esquema, cabecera y área de trabajo) no tienen token en Nuxt UI, así que el tema define variables propias: `--trompa-sidebar-bg`, `--trompa-sidebar-border`, `--trompa-header-bg` y `--trompa-content-bg`. Sus valores por defecto están en `app/assets/css/main.css` (`@layer base`) y apuntan a los `--ui-*`, de modo que sin tema las regiones se comportan como antes.

El modo claro/oscuro sigue siendo ortogonal: es el toggle existente (`ui.colorMode` en las preferencias). El tema fija los valores de ambos modos.

## Formato

Un tema es un archivo TOML. Todas las tablas y claves son opcionales salvo `name`; lo que falta deja el valor por defecto de Nuxt UI (o el de `app.config.ts` para la paleta). No hay herencia ni `extends`, y una clave desconocida es un **error**, no un silencio: un archivo con `[light.background] panel = "#fff"` se descarta entero.

El nombre visible va en `name`; el **`id` del tema es el nombre del archivo sin extensión** (para los integrados, el nombre del recurso: `trompa`, `papel`).

### Mapa clave → variable

| Sección | Clave | Variable CSS |
|---|---|---|
| `[<modo>.background]` | `default` / `muted` / `elevated` / `accented` / `inverted` | `--ui-bg` / `--ui-bg-muted` / `--ui-bg-elevated` / `--ui-bg-accented` / `--ui-bg-inverted` |
| `[<modo>.background.sidebar]` | `default` / `border` | `--trompa-sidebar-bg` / `--trompa-sidebar-border` |
| `[<modo>.background.header]` | `default` | `--trompa-header-bg` |
| `[<modo>.background.content]` | `default` | `--trompa-content-bg` |
| `[<modo>.text]` | `default` / `dimmed` / `muted` / `toned` / `highlighted` / `inverted` | `--ui-text` / `--ui-text-dimmed` / `--ui-text-muted` / `--ui-text-toned` / `--ui-text-highlighted` / `--ui-text-inverted` |
| `[<modo>.border]` | `default` / `muted` / `accented` / `inverted` | `--ui-border` / `--ui-border-muted` / `--ui-border-accented` / `--ui-border-inverted` |
| `[<modo>.chart]` | `1`…`6` | `--ui-chart-1`…`--ui-chart-6` |
| `[shape]` | `radius` / `container` / `headerHeight` | `--ui-radius` / `--ui-container` / `--ui-header-height` |
| `[palette]` | `primary`, `secondary`, `success`, `info`, `warning`, `error`, `neutral` | `useAppConfig().ui.colors` (no son variables: las genera Nuxt UI) |

`<modo>` es `light` o `dark`.

### Validación

| Clave | Regla |
|---|---|
| `name` | No vacío tras recortar, ≤ 48 caracteres |
| `description` | ≤ 160 caracteres |
| `[palette].*` | Un nombre de `tailwindcss/colors`: `slate`, `gray`, `zinc`, `neutral`, `stone`, `red`, `orange`, `amber`, `yellow`, `lime`, `green`, `emerald`, `teal`, `cyan`, `sky`, `blue`, `indigo`, `violet`, `purple`, `fuchsia`, `pink`, `rose` |
| `[<modo>.chart]` | Claves solo `1`…`6` |
| Colores | `#` + 3, 6 u 8 dígitos hex; o `oklch(…)`, `rgb(…)`, `rgba(…)`, `hsl(…)`, `hsla(…)` con cuerpo `[0-9A-Za-z.,%/ -]+`; o el literal `transparent` |
| `[shape].radius` / `container` / `headerHeight` | `^[0-9]+(\.[0-9]+)?(px\|rem\|em)$` |

El conjunto cerrado de valores de color impide inyectar CSS arbitrario desde un archivo.

### Ejemplo completo

`src-tauri/themes/papel.toml` (integrado) usa todas las claves:

```toml
name = "Papel"
description = "Papel cálido con tinta sepia, cómodo con luz ambiente."

[palette]
primary = "amber"
secondary = "orange"
success = "green"
info = "sky"
warning = "yellow"
error = "red"
neutral = "stone"

[shape]
radius = "0.375rem"

[light.background]
default = "#faf7f2"
muted = "#f3ece1"
elevated = "#efe5d8"
accented = "#e4d7c3"
inverted = "#2b2118"

[light.background.sidebar]
default = "#f1e9dc"
border = "#e0d3bf"

[light.background.header]
default = "#faf7f2"

[light.background.content]
default = "transparent"

[light.text]
default = "#3b2f24"
dimmed = "#8a7a67"
muted = "#6b5b49"
toned = "#52443a"
highlighted = "#1f1710"
inverted = "#faf7f2"

[light.border]
default = "#e0d3bf"
muted = "#e6dccb"
accented = "#cdbca4"
inverted = "#3b2f24"

[light.chart]
1 = "#b45309"
2 = "#0f766e"
3 = "#7c3aed"
4 = "#be123c"
5 = "#0369a1"
6 = "#4d7c0f"

[dark.background]
default = "#201a14"
muted = "#1a1511"
elevated = "#2a221b"
accented = "#3a2f25"
inverted = "#faf7f2"

[dark.background.sidebar]
default = "#1c1712"
border = "#3a2f25"

[dark.background.header]
default = "#201a14"

[dark.background.content]
default = "transparent"

[dark.text]
default = "#e7ddcf"
dimmed = "#9c8b76"
muted = "#bcab95"
toned = "#d3c4b0"
highlighted = "#faf7f2"
inverted = "#201a14"

[dark.border]
default = "#3a2f25"
muted = "#332a21"
accented = "#4a3c2f"
inverted = "#faf7f2"

[dark.chart]
1 = "#f59e0b"
2 = "#2dd4bf"
3 = "#a78bfa"
4 = "#fb7185"
5 = "#38bdf8"
6 = "#a3e635"
```

## Uso

### Dónde vive cada tema

- **Integrados** (`builtin: true`, `path: null`), compilados en el binario: `trompa` (replica el aspecto por defecto: azul y pizarra) y `papel`.
- **Del usuario**: `<directorio de configuración>/themes/*.toml`. En Linux es `~/.config/trompa-studio/themes/`; en macOS y Windows, el que devuelve `config_dir()` con el sufijo `trompa-studio/themes`. La carpeta se crea sola al listar, y un archivo inválido no impide arrancar.

### Elegir el tema

No hay panel de ajustes todavía: el tema se elige en `preferences.toml`, bajo `[ui]`:

```toml
[ui]
colorMode = "system"
theme = "papel"
```

`theme` acepta:

- Un **nombre** que se normaliza a slug (`"Mi Tema"` → `mi-tema`), que es el `id` del tema: el nombre del archivo sin `.toml` para los de la carpeta, o el nombre del integrado. Mayúsculas y acentos pliegan: `"Papél"` resuelve `papel`.
- Una **ruta a un `.toml`** (con separador de directorios o extensión `.toml`), absoluta o relativa al directorio de trabajo, que también puede estar fuera de `themes/`.

Precedencia de `id` al cargar: integrados → archivo referenciado en `preferences.toml` → archivos de la carpeta ordenados por nombre. El primero gana; un duplicado se reporta y se omite. Si la referencia no resuelve, se aplica el integrado `trompa` y la preferencia **se conserva** para cuando el archivo reaparezca.

Los temas se leen al arrancar: no hay watcher del sistema de archivos ni botón de recarga; para aplicar cambios, reinicia la app. El panel de ajustes, cuando exista, será la vía para cambiarlo sin editar el TOML.

### Archivos inválidos

Un archivo con TOML roto o una clave fuera de las reglas de validación no rompe el arranque: se omite y queda listado en `errors` del catálogo que devuelve `list_themes`, con el nombre del archivo y el mensaje (visible en la consola de desarrollo del webview). El resto de temas se siguen sirviendo.

## Límites

- No hay overrides por componente: el tema solo controla paleta, forma, superficies, texto, bordes y gráficas.
- Los nombres de `[palette]` tienen que existir en Tailwind; un color inventado se rechaza.
- Un color semántico nuevo (por ejemplo `primary` extra) exigiría rebuild del CSS de Tailwind; el formato no lo permite.
- El tema se aplica después de hidratar (dos llamadas IPC: preferencias y catálogo), así que el primer frame usa los colores por defecto.
- Las regiones del usuario dependen de `app/assets/css/main.css`; el valor por defecto de `--trompa-sidebar-bg` usa `color-mix(in oklab, …)`, soportado por WebKitGTK ≥ 2.40. Si el sidebar saliera transparente en una plataforma vieja, basta con fijar `--trompa-sidebar-bg: transparent` ahí.
- Fuera de alcance: temas junto al ejecutable, editor de temas y recarga en caliente.
