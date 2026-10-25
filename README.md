# Trompa Studio 🐘

Cliente de escritorio para **PostgreSQL**, inspirado en [Beekeeper Studio](https://www.beekeeperstudio.io/): conexiones guardadas, explorador de esquema, editor SQL con resultados en vivo y edición de datos, todo en una app nativa y ligera.

Construido con **Tauri 2** (backend Rust) + **Nuxt 4** (frontend Vue 3) + **Nuxt UI 4** sobre Tailwind CSS 4.

> **Estado:** proyecto en fase de diseño. La base (Tauri + Nuxt + entorno Devbox + MCPs de Zed) ya está montada; las funcionalidades de base de datos están en la hoja de ruta de abajo.

---

## 🎯 Qué queremos construir

Una alternativa de escritorio, rápida y multiplataforma, a los clientes de PostgreSQL tipo DBeaver o Beekeeper Studio, con estas prioridades:

- **Rápido al abrir.** Sin JVM ni Electron: binario nativo pequeño y arranque inmediato.
- **Seguro por defecto.** Las credenciales nunca tocan el frontend; el backend Rust es el único que habla con la base de datos.
- **Cómodo para el día a día.** Conectar, explorar el esquema, escribir SQL, ver y editar resultados sin fricción.

## 🧭 Estado y hoja de ruta

### Base del proyecto

- [x] Plantilla Nuxt 4 + Tauri 2 + Nuxt UI 4
- [x] Entorno reproducible con Devbox (Node, pnpm, Rust y librerías de Tauri)
- [x] Tareas de Zed y plantilla de MCP (GitHub + Playwright) en `.zed/`

### MVP (siguiente)

- [ ] Gestión de conexiones: crear, editar y probar perfiles (host, puerto, usuario, base de datos, SSL)
- [ ] Credenciales en el **keyring del sistema** (nunca en texto plano ni en el frontend)
- [ ] Explorador de esquema: bases de datos → esquemas → tablas, vistas y columnas
- [ ] Editor SQL con resaltado de sintaxis y ejecución (`Ctrl/Cmd+Enter`), ejecutando también la selección
- [ ] Resultados en tabla con paginación y copia al portapapeles
- [ ] Historial de consultas de la sesión
- [ ] Conexión a través de **túnel SSH**

### Después del MVP

- [ ] Edición de datos en la cuadrícula (INSERT/UPDATE/DELETE con vista previa del SQL)
- [ ] Cliente de transacciones (commit/rollback manual)
- [ ] Autocompletado de tablas, columnas y funciones del catálogo
- [ ] Exportar resultados a CSV / JSON
- [ ] Múltiples pestañas de conexión y de consulta
- [ ] Diagrama de relaciones (FK)
- [ ] Tema claro/oscuro y atajos configurables

### Fuera de alcance (por ahora)

- Otros motores (MySQL, SQLite…) — el diseño no los impide, pero el foco es PostgreSQL.
- Extensiones o plugins de terceros.

## 🏗️ Stack

| Capa | Tecnología |
| --- | --- |
| Escritorio | Tauri 2 (Rust, edition 2024) |
| Frontend | Nuxt 4, Vue 3, TypeScript |
| UI | Nuxt UI 4 + Tailwind CSS 4 |
| Iconos | Colecciones de Iconify empaquetadas offline |
| Base de datos | Driver PostgreSQL en Rust (previsto: `sqlx` / `tokio-postgres`) |
| Entorno de desarrollo | Devbox (Nix) |

## 🧊 Entorno de desarrollo con Devbox (recomendado)

El proyecto incluye un entorno reproducible con [Devbox](https://www.jetify.com/devbox) (`devbox.json` + `devbox.lock`), así no hay que instalar ni versionar a mano Node, pnpm, Rust ni las librerías de WebKit/GTK que necesita Tauri en Linux.

Incluye: **Node.js 26.12.0**, **pnpm 12.9.0**, **Rust 1.99.0** (`cargo` + `rustc`), `pkg-config`, OpenSSL, GTK 3, WebKitGTK 4.1, libsoup 3, librsvg, gdk-pixbuf, glib, cairo, pango, harfbuzz, at-spi2, gobject-introspection, libayatana-appindicator, `xdotool` y `patchelf`.

```bash
# 1. Instala el entorno (la primera vez descarga los paquetes en Nix)
devbox install

# 2. Ejecuta directamente los scripts
devbox run setup   # pnpm install
devbox run dev     # pnpm tauri dev (web + ventana nativa)
devbox run build   # pnpm tauri build
devbox run check   # cargo check del backend
```

También puedes entrar al shell con `devbox shell`. Opcional con direnv: `devbox generate direnv && direnv allow` carga el entorno al entrar al directorio.

En **Zed**, los mismos comandos están disponibles como tareas en `.zed/tasks.json`: abre `task: spawn` (`ctrl-shift-p` → “task: spawn”) y elige `devbox: dev (escritorio)`, `devbox: build`, etc. Las tareas de larga duración (`dev`, `dev:web`, `shell`) usan terminal propio, y `build`/`check`/`setup` se ocultan solas si terminan bien.

> `devbox.lock` sí se versiona; `.devbox/` y `.direnv/` están ignorados.

### Pruebas

```bash
# Backend: los tests de integración necesitan el PostgreSQL de devbox.
devbox run db:start
TROMPA_TEST_DATABASE_URL='postgresql://postgres@127.0.0.1:5432/postgres?sslmode=disable' \
  cargo test --lib --manifest-path src-tauri/Cargo.toml

# Frontend
pnpm vitest run
```

`TROMPA_TEST_DATABASE_URL` es obligatoria para los tests de integración: si falta, fallan con un mensaje explícito en vez de omitirse en silencio. Cubren el camino real de la UI (`Fields` con `sslmode=prefer`) y la propagación del mensaje y el SQLSTATE de PostgreSQL al frontend.

## 🛠 Requisitos previos (instalación manual)

Si prefieres no usar Devbox:

- **Node.js** `^22.22.3`, `^24.15.0` o `>=26` (requisito de Nuxt 4.6)
- **pnpm** (recomendado) o npm/yarn
- **Rust** `1.90+` con `cargo` (requisito de Tauri 2.12, que usa edition 2024)
- Dependencias del sistema para Tauri según tu SO ([guía oficial](https://tauri.app/start/prerequisites/)): en Linux, `libwebkit2gtk-4.1-dev`, `libgtk-3-dev`, `libsoup-3.0-dev`, `libayatana-appindicator3-dev`, `librsvg2-dev`, `libxdo-dev`, `libssl-dev`, `build-essential`, `pkg-config`.

## 🚀 Instalación y desarrollo

```bash
git clone https://github.com/SpicyDogWings/trompa-studio.git
cd trompa-studio

pnpm install
pnpm tauri dev     # o: devbox run dev
```

- **Web** (solo frontend): `pnpm dev` → `http://localhost:3000`
- **Escritorio**: se abre la ventana de Tauri apuntando a ese servidor

## 🔨 Compilación para producción

```bash
pnpm tauri build   # o: devbox run build
```

Tauri ejecuta `pnpm generate` (definido en `beforeBuildCommand`) y empaqueta el sitio estático en `.output/public`. Los binarios quedan en `src-tauri/target/release/` y los instaladores en `src-tauri/target/release/bundle/`.

> El frontend se sirve como sitio estático generado por Nuxt. Si la app necesita rutas completamente dinámicas en cliente, se puede activar SPA con `ssr: false` en `nuxt.config.ts`.

## 📦 Estructura del proyecto

```
trompa-studio/
├── app/                    # Frontend Nuxt
│   ├── app.vue             # Página principal (a reemplazar por la UI del cliente)
│   ├── app.config.ts       # Tema de Nuxt UI (colores primary/neutral)
│   └── assets/css/         # Estilos globales (Tailwind + Nuxt UI)
├── public/                 # Archivos públicos estáticos
├── src-tauri/              # Backend Rust
│   ├── src/                # main.rs / lib.rs (comandos de Tauri)
│   ├── capabilities/       # Permisos de Tauri
│   ├── icons/              # Iconos de la app
│   └── tauri.conf.json     # Configuración de Tauri
├── .zed/                   # Tareas y plantilla de MCP para Zed
├── devbox.json             # Entorno reproducible
├── nuxt.config.ts          # Configuración de Nuxt
├── package.json            # Dependencias de Node
└── README.md
```

Estructura prevista a medida que avance el MVP:

```
├── app/
│   ├── components/         # ConnectionDialog, SchemaTree, QueryEditor, ResultGrid…
│   ├── composables/        # useConnections, useQuery, useSchema…
│   ├── pages/              # Editor, conexiones, ajustes
│   └── stores/             # Estado de conexiones y pestañas
└── src-tauri/src/
    ├── db/                 # Pool de conexiones, introspección del catálogo, queries
    ├── commands/           # Comandos expuestos al frontend (connect, query, schema…)
    └── secrets/            # Integración con el keyring del sistema
```

## 🔐 Arquitectura y seguridad

- **El frontend nunca ejecuta SQL directamente ni recibe credenciales.** Todo pasa por comandos de Tauri (`#[tauri::command]`) que viven en Rust.
- **Contraseñas en el keyring del SO** (Secret Service en Linux, Keychain en macOS, Credential Manager en Windows) mediante un plugin de keyring; en la app solo se guarda una referencia al perfil.
- **Consultas parametrizadas** para todo lo que no sea SQL libre del usuario.
- **Timeouts y cancelación** de consultas desde el backend para no bloquear la UI.
- **CSP** de Tauri configurada en producción (hoy `null` en la plantilla; se endurecerá antes de la primera release).
- **MCP de Zed**: la plantilla está en `.zed/settings.example.json` y el archivo real `.zed/settings.json` está en `.gitignore`. GitHub requiere un token fine-grained (Contents/Metadata: Read, más Issues/Pull requests si los usas).

## 🤝 Contribución

Por ahora el proyecto no acepta contribuciones externas. Cuando la base esté estable se publicará una guía.

## 📜 Licencia

MIT License — ver [LICENSE](LICENSE).
