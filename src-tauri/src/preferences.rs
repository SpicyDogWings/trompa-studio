//! Preferencias de la aplicación persistidas en un TOML.
//!
//! El archivo vive en `<config_dir>/trompa-studio/` (`~/.config/trompa-studio`
//! en Linux) y es propiedad del backend: el frontend solo pide y envía valores
//! completos por IPC. Las claves del TOML y del JSON de IPC son las mismas
//! (camelCase), sin capa de conversión.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::Manager;

use crate::db::DbError;

/// Nombre del archivo dentro del directorio de configuración.
const PREFERENCES_FILE: &str = "preferences.toml";

/// Directorio propio dentro de `config_dir()` (`~/.config/trompa-studio` en Linux).
const CONFIG_DIRECTORY: &str = "trompa-studio";

const COLOR_MODES: [&str; 3] = ["system", "light", "dark"];
const SIDEBAR_POSITIONS: [&str; 2] = ["left", "right"];
const CONNECTION_MODES: [&str; 2] = ["fields", "url"];
const SSL_MODES: [&str; 3] = ["prefer", "require", "disable"];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct UiPreferences {
    pub color_mode: String,
    /// Nombre (`id` normalizado a slug) o ruta absoluta a un `.toml`.
    /// La resolución la hace `themes::load_themes`.
    pub theme: String,
}

impl Default for UiPreferences {
    fn default() -> Self {
        Self {
            color_mode: "system".to_owned(),
            theme: "trompa".to_owned(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SchemaPreferences {
    pub sidebar_position: String,
}

impl Default for SchemaPreferences {
    fn default() -> Self {
        Self {
            sidebar_position: "right".to_owned(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ConnectionPreferences {
    pub mode: String,
    pub host: String,
    pub port: u16,
    pub database: String,
    pub user: String,
    pub sslmode: String,
    pub sslrootcert: String,
    pub url: String,
}

impl Default for ConnectionPreferences {
    fn default() -> Self {
        Self {
            mode: "fields".to_owned(),
            host: "localhost".to_owned(),
            port: 5432,
            database: "postgres".to_owned(),
            user: "postgres".to_owned(),
            sslmode: "prefer".to_owned(),
            sslrootcert: String::new(),
            url: "postgresql://postgres@localhost:5432/postgres".to_owned(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Preferences {
    pub ui: UiPreferences,
    pub schema: SchemaPreferences,
    pub connection: ConnectionPreferences,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            ui: UiPreferences::default(),
            schema: SchemaPreferences::default(),
            connection: ConnectionPreferences::default(),
        }
    }
}

/// Repara valores fuera de rango campo a campo.
///
/// No recorta strings ni valida host, base ni usuario: eso lo hace el
/// frontend al conectar, donde hay contexto para mostrar el error.
fn normalize(mut preferences: Preferences) -> Preferences {
    if !COLOR_MODES.contains(&preferences.ui.color_mode.as_str()) {
        preferences.ui.color_mode = UiPreferences::default().color_mode;
    }
    let theme = preferences.ui.theme.trim();
    if theme.is_empty() || theme.chars().count() > 512 {
        preferences.ui.theme = UiPreferences::default().theme;
    } else {
        preferences.ui.theme = theme.to_owned();
    }
    if !SIDEBAR_POSITIONS.contains(&preferences.schema.sidebar_position.as_str()) {
        preferences.schema.sidebar_position = SchemaPreferences::default().sidebar_position;
    }
    if !CONNECTION_MODES.contains(&preferences.connection.mode.as_str()) {
        preferences.connection.mode = ConnectionPreferences::default().mode;
    }
    if !SSL_MODES.contains(&preferences.connection.sslmode.as_str()) {
        preferences.connection.sslmode = ConnectionPreferences::default().sslmode;
    }
    if preferences.connection.port == 0 {
        preferences.connection.port = ConnectionPreferences::default().port;
    }
    preferences
}

/// Directorio de configuración propio: `<config_dir>/trompa-studio`.
///
/// No se usa `app_config_dir()` porque deriva del identificador
/// (`com.nuxtri.app`); el directorio de configuración elegido es
/// `trompa-studio`.
pub(crate) fn config_directory(app: &tauri::AppHandle) -> Result<PathBuf, DbError> {
    let directory = app.path().config_dir().map_err(|error| {
        DbError::new(format!(
            "No se pudo resolver el directorio de configuración: {error}"
        ))
    })?;
    Ok(directory.join(CONFIG_DIRECTORY))
}

/// Ruta del archivo de preferencias: `config_dir()/trompa-studio/preferences.toml`.
pub fn preferences_path(app: &tauri::AppHandle) -> Result<PathBuf, DbError> {
    Ok(config_directory(app)?.join(PREFERENCES_FILE))
}

/// Lee las preferencias, creando el archivo con los valores por defecto si
/// no existe.
///
/// Nunca bloquea el arranque: un archivo ilegible o inválido se reporta por
/// el log y se devuelven los defaults sin pisar el contenido del usuario.
pub fn read_preferences_at(path: &Path) -> Result<Preferences, DbError> {
    if !path.exists() {
        return match write_preferences_at(path, Preferences::default()) {
            Ok(preferences) => Ok(preferences),
            Err(error) => {
                log::warn!(
                    "No se pudo crear el archivo de preferencias en {}: {error:?}",
                    path.display()
                );
                Ok(Preferences::default())
            }
        };
    }

    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) => {
            log::warn!(
                "No se pudo leer el archivo de preferencias {}: {error}",
                path.display()
            );
            return Ok(Preferences::default());
        }
    };

    match toml::from_str::<Preferences>(&text) {
        Ok(preferences) => Ok(normalize(preferences)),
        Err(error) => {
            log::warn!(
                "El archivo de preferencias {} no es TOML válido: {error}",
                path.display()
            );
            Ok(Preferences::default())
        }
    }
}

/// Escribe las preferencias normalizadas de forma atómica.
///
/// Se serializa a `<path>.tmp` y se renombra sobre el destino, para que un
/// fallo a mitad de escritura no deje un TOML truncado.
pub fn write_preferences_at(path: &Path, preferences: Preferences) -> Result<Preferences, DbError> {
    let preferences = normalize(preferences);

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            DbError::new(format!(
                "No se pudieron guardar las preferencias: {error}"
            ))
        })?;
    }

    let text = toml::to_string_pretty(&preferences)
        .map_err(|error| DbError::new(format!("No se pudieron guardar las preferencias: {error}")))?;
    let temporary = path.with_extension("toml.tmp");

    fs::write(&temporary, text)
        .and_then(|()| fs::rename(&temporary, path))
        .map_err(|error| {
            let _ = fs::remove_file(&temporary);
            DbError::new(format!("No se pudieron guardar las preferencias: {error}"))
        })?;

    Ok(preferences)
}

pub fn read_preferences_with(app: &tauri::AppHandle) -> Result<Preferences, DbError> {
    read_preferences_at(&preferences_path(app)?)
}

pub fn write_preferences_with(
    app: &tauri::AppHandle,
    preferences: Preferences,
) -> Result<Preferences, DbError> {
    write_preferences_at(&preferences_path(app)?, preferences)
}

#[tauri::command]
pub async fn read_preferences(app: tauri::AppHandle) -> Result<Preferences, DbError> {
    read_preferences_with(&app)
}

#[tauri::command]
pub async fn write_preferences(
    app: tauri::AppHandle,
    preferences: Preferences,
) -> Result<Preferences, DbError> {
    write_preferences_with(&app, preferences)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Directorio temporal exclusivo de cada test: se borra al soltarlo.
    struct TempDirectory(PathBuf);

    impl TempDirectory {
        fn new() -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);

            // Los tests corren en paralelo dentro del mismo proceso: sin un
            // nombre propio, uno borraría el directorio de otro.
            let id = NEXT.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir()
                .join(format!("trompa-prefs-{}-{id}", std::process::id()));

            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).expect("crear el directorio temporal");
            Self(path)
        }

        fn join(&self, name: &str) -> PathBuf {
            self.0.join(name)
        }
    }

    impl Drop for TempDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn non_default() -> Preferences {
        Preferences {
            ui: UiPreferences {
                color_mode: "dark".to_owned(),
                theme: "papel".to_owned(),
            },
            schema: SchemaPreferences {
                sidebar_position: "left".to_owned(),
            },
            connection: ConnectionPreferences {
                mode: "url".to_owned(),
                host: "db.local".to_owned(),
                port: 6543,
                database: "trompa".to_owned(),
                user: "spicy".to_owned(),
                sslmode: "require".to_owned(),
                sslrootcert: "/tmp/ca.pem".to_owned(),
                url: "postgresql://spicy@db.local:6543/trompa".to_owned(),
            },
        }
    }

    #[test]
    fn crea_el_archivo_con_defaults() {
        let directory = TempDirectory::new();
        let path = directory.join("preferences.toml");

        let preferences = read_preferences_at(&path).expect("leer preferencias");

        assert_eq!(preferences, Preferences::default());
        let text = fs::read_to_string(&path).expect("el archivo debe existir");
        assert!(text.contains("colorMode = \"system\""), "{text}");
        assert!(text.contains("sidebarPosition = \"right\""), "{text}");
        assert!(text.contains("port = 5432"), "{text}");
    }

    #[test]
    fn roundtrip() {
        let directory = TempDirectory::new();
        let path = directory.join("preferences.toml");

        let written = write_preferences_at(&path, non_default()).expect("escribir preferencias");
        let read = read_preferences_at(&path).expect("leer preferencias");

        assert_eq!(written, non_default());
        assert_eq!(read, non_default());
    }

    #[test]
    fn toml_invalido_devuelve_defaults_y_conserva_el_archivo() {
        let directory = TempDirectory::new();
        let path = directory.join("preferences.toml");
        let corrupto = "esto no es toml {{";
        fs::write(&path, corrupto).expect("escribir el archivo corrupto");

        let preferences = read_preferences_at(&path).expect("leer preferencias");

        assert_eq!(preferences, Preferences::default());
        assert_eq!(fs::read_to_string(&path).expect("leer el archivo"), corrupto);
    }

    #[test]
    fn normaliza_valores_desconocidos() {
        let directory = TempDirectory::new();
        let path = directory.join("preferences.toml");
        fs::write(
            &path,
            "[ui]\ncolorMode = \"neon\"\ntheme = \"   \"\n\n[schema]\nsidebarPosition = \"top\"\n\n[connection]\nport = 0\nsslmode = \"quizas\"\n",
        )
        .expect("escribir el archivo");

        let preferences = read_preferences_at(&path).expect("leer preferencias");

        assert_eq!(preferences.ui.color_mode, "system");
        assert_eq!(preferences.ui.theme, "trompa");
        assert_eq!(preferences.schema.sidebar_position, "right");
        assert_eq!(preferences.connection.port, 5432);
        assert_eq!(preferences.connection.sslmode, "prefer");
    }

    #[test]
    fn claves_desconocidas_se_ignoran() {
        let directory = TempDirectory::new();
        let path = directory.join("preferences.toml");
        fs::write(&path, "[otra]\nvalor = 1\n").expect("escribir el archivo");

        let preferences = read_preferences_at(&path).expect("leer preferencias");

        assert_eq!(preferences, Preferences::default());
    }

    #[test]
    fn write_normaliza_y_serializa() {
        let directory = TempDirectory::new();
        let path = directory.join("preferences.toml");
        let mut preferences = Preferences::default();
        preferences.ui.color_mode = "neon".to_owned();

        let written = write_preferences_at(&path, preferences).expect("escribir preferencias");

        assert_eq!(written.ui.color_mode, "system");
        let text = fs::read_to_string(&path).expect("leer el archivo");
        assert!(text.contains("colorMode = \"system\""), "{text}");
    }
}
