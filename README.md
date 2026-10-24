# Plantilla Nuxt + Tauri 🚀

Una plantilla moderna para aplicaciones de escritorio que combina lo mejor de **Nuxt 4**, **Tauri 2** y **Nuxt UI 4**. Perfecta para crear aplicaciones rápidas, seguras y con interfaz profesional.

## 🎯 Características

✅ **Nuxt 4** - Última versión con Vue 3, enrutamiento basado en archivos y potentes convenciones
✅ **Tauri 2** - Aplicaciones de escritorio seguras con backend en Rust y tamaño mínimo
✅ **Nuxt UI 4** - Componentes de interfaz profesionales sobre Tailwind CSS 4
✅ **Tailwind CSS 4** - Estilos modernos y personalizables
✅ **Vite** - Compilación rápida y desarrollo ágil
✅ **TypeScript** - Soporte completo para tipado estático
✅ **Iconos offline** - Colecciones de Iconify empaquetadas en el bundle, sin depender de la red

## 🧊 Entorno de desarrollo con Devbox (recomendado)

El proyecto incluye un entorno reproducible con [Devbox](https://www.jetify.com/devbox) (`devbox.json` + `devbox.lock`), así no tienes que instalar ni versionar a mano Node, pnpm, Rust ni las librerías de WebKit/GTK que necesita Tauri en Linux.

Incluye: **Node.js 26.12.0**, **pnpm 12.9.0**, **Rust 1.99.0** (`cargo` + `rustc`), `pkg-config`, GTK 3, WebKitGTK 4.1, libsoup 3, librsvg, gdk-pixbuf, glib, cairo, pango, harfbuzz, at-spi2, gobject-introspection, libayatana-appindicator, `xdotool` y `patchelf`.

```bash
# 1. Instala el entorno (la primera vez descarga los paquetes en Nix)
devbox install

# 2. Entra al shell con todo disponible
devbox shell

# 3. O ejecuta directamente los scripts
devbox run setup   # pnpm install
devbox run dev     # pnpm tauri dev
devbox run build   # pnpm tauri build
devbox run check   # cargo check del backend
```

Opcional con direnv: `devbox generate direnv && direnv allow` carga el entorno al entrar al directorio.

En **Zed**, los mismos comandos están disponibles como tareas en `.zed/tasks.json`: abre `task: spawn` (`ctrl-shift-p` → “task: spawn”) y elige `devbox: dev (escritorio)`, `devbox: build`, etc. Las tareas de larga duración (`dev`, `dev:web`, `shell`) usan terminal propio, y `build`/`check`/`setup` se ocultan solas si terminan bien.

> `devbox.lock` sí se versiona; `.devbox/` y `.direnv/` están ignorados.

## 🛠 Requisitos Previos (instalación manual)

Si prefieres no usar Devbox, necesitas:

- **Node.js** `^22.22.3`, `^24.15.0` o `>=26` (requisito de Nuxt 4.6)
- **pnpm** (recomendado) o npm/yarn
- **Rust** `1.90+` con `cargo` (requisito de Tauri 2.12, que usa edition 2024)
- Dependencias del sistema para Tauri según tu SO ([guía oficial](https://tauri.app/start/prerequisites/)): en Linux, `libwebkit2gtk-4.1-dev`, `libgtk-3-dev`, `libsoup-3.0-dev`, `libayatana-appindicator3-dev`, `librsvg2-dev`, `libxdo-dev`, `libssl-dev`, `build-essential`, `pkg-config`.

## 🚀 Instalación

1. Clona este repositorio:

```bash
git clone https://github.com/SpicyDogWings/Nuxtri.git
cd Nuxtri
```

2. Instala las dependencias:

```bash
# Con pnpm (recomendado)
pnpm install
```

## 📱 Desarrollo

Inicia el servidor de desarrollo con Tauri:

```bash
# Desarrollo web + escritorio
devbox run dev     # o: pnpm tauri dev
```

El servidor se iniciará en:
- **Web**: `http://localhost:3000`
- **Desktop**: Ventana de Tauri

Si solo quieres trabajar el frontend en el navegador, usa `pnpm dev` (`devbox run dev:web`).

> El entorno de Devbox está orientado a **Linux**. En macOS y Windows, Tauri usa los frameworks del sistema y no hacen falta los paquetes GTK/WebKit; instala solo Node, pnpm y Rust.

## 🔨 Compilación para Producción

```bash
# Genera el frontend estático y empaqueta la app de escritorio
devbox run build   # o: pnpm tauri build
```

Tauri ejecuta automáticamente `pnpm generate` (definido en `beforeBuildCommand`) antes de compilar, produciendo el sitio estático en `.output/public`.

Los binarios se generan en `src-tauri/target/release/` (y los instaladores en `src-tauri/target/release/bundle/`).

> **Nota:** el frontend se sirve como sitio estático generado por Nuxt. Si tu app necesita rutas totalmente dinámicas en cliente, puedes activar el modo SPA añadiendo `ssr: false` en `nuxt.config.ts`.

## 📦 Estructura del Proyecto

```
Nuxtri/
├── app/                    # Código de Nuxt (frontend)
│   ├── app.vue             # Landing page principal
│   ├── app.config.ts       # Tema de Nuxt UI (colores primary/neutral)
│   └── assets/css/         # Estilos globales (Tailwind + Nuxt UI)
├── public/                 # Archivos públicos estáticos
├── src-tauri/              # Código de Tauri (backend Rust)
│   ├── src/                # Código Rust (main.rs / lib.rs)
│   ├── capabilities/       # Permisos de Tauri
│   ├── icons/              # Iconos de la app
│   └── tauri.conf.json     # Configuración de Tauri
├── .zed/                   # MCPs del proyecto para Zed (GitHub + Playwright)
├── devbox.json             # Entorno reproducible (Node, pnpm, Rust, libs de Tauri)
├── devbox.lock             # Lock del entorno Devbox
├── nuxt.config.ts          # Configuración de Nuxt
├── package.json            # Dependencias de Node.js
└── README.md               # Este archivo
```

## 🎨 Personalización

### Cambiar el nombre de la aplicación:

1. Edita `package.json`:
```json
{
  "name": "mi-nueva-app",
  "version": "1.0.0"
}
```

2. Edita `src-tauri/tauri.conf.json` (formato de Tauri 2, claves en la raíz):
```json
{
  "productName": "MiNuevaApp",
  "version": "1.0.0",
  "identifier": "com.tuempresa.miapp"
}
```

> El `identifier` debe ser único y en formato de dominio inverso. Tauri lo usa como ID de la aplicación al empaquetar.

3. Opcionalmente ajusta el nombre del crate en `src-tauri/Cargo.toml`:

```toml
[package]
name = "mi_nueva_app"
```

### Cambiar los iconos:

```bash
pnpm tauri icon ruta/a/tu/icono.png
```

### Cambiar la landing page:

La home (`app/app.vue`) está construida con los componentes de Nuxt UI: layout (`UApp`, `UHeader`, `UNavigationMenu`, `UMain`, `UFooter`), página (`UPageHero`, `UPageSection`, `UPageGrid`, `UPageCard` con `spotlight`, `UPageCTA`) y primitivos (`UButton`, `UBadge`, `UIcon`, `UColorModeButton`). Usa los tokens semánticos (`bg-default`, `text-muted`, `bg-elevated`, `ring-default`…) para que el modo claro/oscuro salga solo. Edita ese archivo para cambiar textos y secciones, y `app/app.config.ts` para los colores.

### Colores del tema:

Edita `app/app.config.ts` para cambiar los colores de Nuxt UI:

```ts
export default defineAppConfig({
  ui: {
    colors: {
      primary: 'emerald',
      neutral: 'zinc'
    }
  }
})
```

## 🔌 Usar la API de Tauri desde Nuxt

El paquete `@tauri-apps/api` ya está instalado. En `app/app.vue` tienes un ejemplo que detecta si la app corre dentro de Tauri y muestra su versión:

```ts
const { isTauri } = await import('@tauri-apps/api/core')
if (isTauri()) {
  const { getTauriVersion } = await import('@tauri-apps/api/app')
  console.log(await getTauriVersion())
}
```

Para exponer comandos propios, añade funciones `#[tauri::command]` en `src-tauri/src/lib.rs` e invócalas con `invoke()` desde el frontend.

## 📚 Documentación

- [Documentación de Nuxt](https://nuxt.com/docs)
- [Documentación de Tauri](https://tauri.app/)
- [Componentes Nuxt UI](https://ui.nuxt.com/)
- [Tailwind CSS](https://tailwindcss.com/docs)

## 🤝 Contribución

No soportadas por el momento

## 📜 Licencia

MIT License - Usa este template libremente para tus proyectos personales y comerciales.

---

**¡Gracias por usar esta plantilla!** 🚀
