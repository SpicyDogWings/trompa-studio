//! Temas visuales: esquema, validación y catálogo.
//!
//! Un tema es un archivo `<id>.toml` en `<config_dir>/trompa-studio/themes/`
//! o uno de los integrados compilados en el binario. Aquí solo se parsea,
//! valida y resuelve la referencia guardada en `[ui] theme`; el frontend
//! recibe el catálogo por IPC y lo traduce a variables CSS.
//!
//! Todas las tablas y claves son opcionales salvo `name`: lo ausente deja el
//! valor por defecto de Nuxt UI, sin herencia entre temas. Una clave
//! desconocida es un error, no un silencio.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::db::DbError;
use crate::preferences;

/// Temas compilados en el binario: `(id, contenido TOML)`.
const BUILTIN_THEMES: [(&str, &str); 2] = [
    ("trompa", include_str!("../themes/trompa.toml")),
    ("papel", include_str!("../themes/papel.toml")),
];

/// Subdirectorio del directorio de configuración con los temas del usuario.
const THEMES_DIRECTORY: &str = "themes";

/// Nombres aceptados por `useAppConfig().ui.colors` (paleta de Tailwind).
const PALETTE_COLORS: [&str; 22] = [
    "slate", "gray", "zinc", "neutral", "stone", "red", "orange", "amber", "yellow", "lime",
    "green", "emerald", "teal", "cyan", "sky", "blue", "indigo", "violet", "purple", "fuchsia",
    "pink", "rose",
];

const MAX_NAME: usize = 48;
const MAX_DESCRIPTION: usize = 160;

/// Contenido de un archivo de tema. Tablas anidadas por modo y por región.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct ThemeFile {
    pub name: String,
    pub description: String,
    pub palette: ThemePalette,
    pub shape: ThemeShape,
    pub light: ThemeMode,
    pub dark: ThemeMode,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct ThemePalette {
    pub primary: Option<String>,
    pub secondary: Option<String>,
    pub success: Option<String>,
    pub info: Option<String>,
    pub warning: Option<String>,
    pub error: Option<String>,
    pub neutral: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct ThemeShape {
    pub radius: Option<String>,
    pub container: Option<String>,
    pub header_height: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct ThemeMode {
    pub background: ThemeBackground,
    pub text: ThemeText,
    pub border: ThemeBorder,
    pub chart: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct ThemeBackground {
    pub default: Option<String>,
    pub muted: Option<String>,
    pub elevated: Option<String>,
    pub accented: Option<String>,
    pub inverted: Option<String>,
    pub sidebar: ThemePanel,
    pub header: ThemeSurface,
    pub content: ThemeSurface,
}

/// Región con borde propio: el panel del esquema.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct ThemePanel {
    pub default: Option<String>,
    pub border: Option<String>,
}

/// Región de una sola superficie: cabecera y área de trabajo.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct ThemeSurface {
    pub default: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct ThemeText {
    pub default: Option<String>,
    pub dimmed: Option<String>,
    pub muted: Option<String>,
    pub toned: Option<String>,
    pub highlighted: Option<String>,
    pub inverted: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct ThemeBorder {
    pub default: Option<String>,
    pub muted: Option<String>,
    pub accented: Option<String>,
    pub inverted: Option<String>,
}

/// Tema listo para el frontend: contenido validado más los metadatos de la
/// carga (id, origen y ruta del archivo).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeDefinition {
    pub id: String,
    pub name: String,
    pub description: String,
    pub builtin: bool,
    pub palette: ThemePalette,
    pub shape: ThemeShape,
    pub light: ThemeMode,
    pub dark: ThemeMode,
    pub path: Option<String>,
}

/// Archivo que no se pudo cargar. El catálogo sigue siendo usable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThemeErrorInfo {
    pub file: String,
    pub message: String,
}

/// Resultado de `list_themes`: temas disponibles y archivos descartados.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeCatalog {
    pub themes: Vec<ThemeDefinition>,
    pub errors: Vec<ThemeErrorInfo>,
    pub directory: String,
}

/// `"Mi Tema Bonito"` → `"mi-tema-bonito"`.
///
/// Minúsculas ASCII con los diacríticos españoles plegados; cualquier otra
/// racha que no sea `[a-z0-9]` se convierte en un único `-`. El frontend
/// replica la regla en `app/utils/theme.ts` para resolver `[ui] theme` sin
/// otro viaje por IPC, así que aquí solo se cubre con tests.
#[cfg(test)]
pub fn theme_slug(reference: &str) -> String {
    let mut slug = String::with_capacity(reference.len());
    let mut pending_separator = false;

    for character in reference.trim().chars() {
        let folded = match character {
            'á' | 'à' | 'ä' | 'â' | 'ã' | 'Á' | 'À' | 'Ä' | 'Â' | 'Ã' => 'a',
            'é' | 'è' | 'ë' | 'ê' | 'É' | 'È' | 'Ë' | 'Ê' => 'e',
            'í' | 'ì' | 'ï' | 'î' | 'Í' | 'Ì' | 'Ï' | 'Î' => 'i',
            'ó' | 'ò' | 'ö' | 'ô' | 'õ' | 'Ó' | 'Ò' | 'Ö' | 'Ô' | 'Õ' => 'o',
            'ú' | 'ù' | 'ü' | 'û' | 'Ú' | 'Ù' | 'Ü' | 'Û' => 'u',
            'ñ' | 'Ñ' => 'n',
            'ç' | 'Ç' => 'c',
            other => other,
        };
        let lowered = folded.to_ascii_lowercase();

        if lowered.is_ascii_alphanumeric() {
            if pending_separator && !slug.is_empty() {
                slug.push('-');
            }
            pending_separator = false;
            slug.push(lowered);
        } else {
            pending_separator = true;
        }
    }

    slug
}

/// Un color es un hex (`#rgb`, `#rrggbb`, `#rrggbbaa`) o una función CSS con
/// cuerpo restringido; el literal `transparent` también vale.
///
/// Cerrar el conjunto de valores impide inyectar CSS arbitrario desde un
/// archivo de tema.
fn is_valid_color(value: &str) -> bool {
    if value == "transparent" {
        return true;
    }

    if let Some(hex) = value.strip_prefix('#') {
        return matches!(hex.len(), 3 | 6 | 8) && hex.chars().all(|c| c.is_ascii_hexdigit());
    }

    ["oklch(", "rgba(", "rgb(", "hsla(", "hsl("].iter().any(|prefix| {
        value.strip_prefix(prefix).and_then(|rest| rest.strip_suffix(')')).is_some_and(|body| {
            !body.is_empty()
                && body
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | ',' | '%' | '/' | ' ' | '-'))
        })
    })
}

/// Longitud CSS: `^[0-9]+(\.[0-9]+)?(px|rem|em)$`.
fn is_valid_length(value: &str) -> bool {
    ["px", "rem", "em"].iter().any(|unit| {
        let Some(number) = value.strip_suffix(unit) else {
            return false;
        };
        let mut parts = number.split('.');

        let whole = parts.next().unwrap_or_default();
        let fraction = parts.next();

        if parts.next().is_some() || whole.is_empty() || !whole.chars().all(|c| c.is_ascii_digit()) {
            return false;
        }

        fraction.is_none_or(|fraction| {
            !fraction.is_empty() && fraction.chars().all(|c| c.is_ascii_digit())
        })
    })
}

fn check_color(field: &str, value: &Option<String>) -> Result<(), String> {
    match value {
        Some(value) if !is_valid_color(value) => {
            Err(format!("{field}: \"{value}\" no es un color válido"))
        }
        _ => Ok(()),
    }
}

fn check_length(field: &str, value: &Option<String>) -> Result<(), String> {
    match value {
        Some(value) if !is_valid_length(value) => {
            Err(format!("{field}: \"{value}\" no es una longitud válida"))
        }
        _ => Ok(()),
    }
}

fn validate_palette(palette: &ThemePalette) -> Result<(), String> {
    let entries = [
        ("palette.primary", &palette.primary),
        ("palette.secondary", &palette.secondary),
        ("palette.success", &palette.success),
        ("palette.info", &palette.info),
        ("palette.warning", &palette.warning),
        ("palette.error", &palette.error),
        ("palette.neutral", &palette.neutral),
    ];

    for (field, value) in entries {
        if let Some(value) = value {
            if !PALETTE_COLORS.contains(&value.as_str()) {
                return Err(format!("{field}: \"{value}\" no es una paleta de Tailwind"));
            }
        }
    }

    Ok(())
}

fn validate_mode(mode: &str, value: &ThemeMode) -> Result<(), String> {
    let background = &value.background;

    for (field, color) in [
        ("default", &background.default),
        ("muted", &background.muted),
        ("elevated", &background.elevated),
        ("accented", &background.accented),
        ("inverted", &background.inverted),
    ] {
        check_color(&format!("{mode}.background.{field}"), color)?;
    }
    check_color(&format!("{mode}.background.sidebar.default"), &background.sidebar.default)?;
    check_color(&format!("{mode}.background.sidebar.border"), &background.sidebar.border)?;
    check_color(&format!("{mode}.background.header.default"), &background.header.default)?;
    check_color(&format!("{mode}.background.content.default"), &background.content.default)?;

    for (field, color) in [
        ("default", &value.text.default),
        ("dimmed", &value.text.dimmed),
        ("muted", &value.text.muted),
        ("toned", &value.text.toned),
        ("highlighted", &value.text.highlighted),
        ("inverted", &value.text.inverted),
    ] {
        check_color(&format!("{mode}.text.{field}"), color)?;
    }

    for (field, color) in [
        ("default", &value.border.default),
        ("muted", &value.border.muted),
        ("accented", &value.border.accented),
        ("inverted", &value.border.inverted),
    ] {
        check_color(&format!("{mode}.border.{field}"), color)?;
    }

    for (key, color) in &value.chart {
        if !matches!(key.as_str(), "1" | "2" | "3" | "4" | "5" | "6") {
            return Err(format!("{mode}.chart: la clave \"{key}\" no está entre 1 y 6"));
        }
        if !is_valid_color(color) {
            return Err(format!("{mode}.chart.{key}: \"{color}\" no es un color válido"));
        }
    }

    Ok(())
}

fn validate(theme: &ThemeFile) -> Result<(), String> {
    let name = theme.name.trim();

    if name.is_empty() {
        return Err("name no puede estar vacío".to_owned());
    }
    if name.chars().count() > MAX_NAME {
        return Err(format!("name no puede superar {MAX_NAME} caracteres"));
    }
    if theme.description.trim().chars().count() > MAX_DESCRIPTION {
        return Err(format!(
            "description no puede superar {MAX_DESCRIPTION} caracteres"
        ));
    }

    validate_palette(&theme.palette)?;
    check_length("shape.radius", &theme.shape.radius)?;
    check_length("shape.container", &theme.shape.container)?;
    check_length("shape.headerHeight", &theme.shape.header_height)?;
    validate_mode("light", &theme.light)?;
    validate_mode("dark", &theme.dark)
}

fn parse_theme(id: &str, source: &str, path: Option<&Path>) -> Result<ThemeDefinition, String> {
    let file = toml::from_str::<ThemeFile>(source).map_err(|error| error.to_string())?;
    validate(&file)?;

    Ok(ThemeDefinition {
        id: id.to_owned(),
        name: file.name.trim().to_owned(),
        description: file.description.trim().to_owned(),
        builtin: path.is_none(),
        palette: file.palette,
        shape: file.shape,
        light: file.light,
        dark: file.dark,
        path: path.map(|path| path.display().to_string()),
    })
}

/// Una referencia es una ruta si trae separador o extensión `.toml`; el resto
/// se resuelve por `id` contra el catálogo.
fn looks_like_path(reference: &str) -> bool {
    reference.contains(std::path::MAIN_SEPARATOR) || reference.ends_with(".toml")
}

fn theme_files(directory: &Path, errors: &mut Vec<ThemeErrorInfo>) -> Vec<PathBuf> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) => {
            errors.push(ThemeErrorInfo {
                file: directory.display().to_string(),
                message: format!("No se pudo leer el directorio de temas: {error}"),
            });
            return Vec::new();
        }
    };

    let mut files: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("toml"))
        })
        .collect();

    files.sort();
    files
}

fn push_theme(
    themes: &mut Vec<ThemeDefinition>,
    errors: &mut Vec<ThemeErrorInfo>,
    theme: ThemeDefinition,
    file: String,
) {
    if themes.iter().any(|existing| existing.id == theme.id) {
        errors.push(ThemeErrorInfo {
            file,
            message: format!("el id \"{}\" ya está en uso", theme.id),
        });
        return;
    }

    themes.push(theme);
}

fn load_file(
    themes: &mut Vec<ThemeDefinition>,
    errors: &mut Vec<ThemeErrorInfo>,
    path: &Path,
) -> bool {
    let file = path.display().to_string();
    let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
        errors.push(ThemeErrorInfo {
            file,
            message: "el archivo de tema no tiene nombre".to_owned(),
        });
        return false;
    };

    let source = match fs::read_to_string(path) {
        Ok(source) => source,
        Err(error) => {
            errors.push(ThemeErrorInfo {
                file,
                message: format!("No se pudo leer el archivo: {error}"),
            });
            return false;
        }
    };

    match parse_theme(stem, &source, Some(path)) {
        Ok(theme) => {
            push_theme(themes, errors, theme, file);
            true
        }
        Err(message) => {
            errors.push(ThemeErrorInfo { file, message });
            false
        }
    }
}

/// Carga integrados, archivos del directorio y el archivo apuntado por
/// `referenced` si vive fuera del directorio.
///
/// Nunca falla: lo que no se puede cargar se reporta en `errors` y el
/// catálogo conserva el resto. Los archivos del directorio se deduplican por
/// `id` en orden integrado → referenciado → nombre de archivo.
pub fn load_themes(directory: &Path, referenced: Option<&str>) -> ThemeCatalog {
    let mut themes = Vec::with_capacity(BUILTIN_THEMES.len() + 2);
    let mut errors = Vec::new();

    for (id, source) in BUILTIN_THEMES {
        match parse_theme(id, source, None) {
            Ok(theme) => themes.push(theme),
            Err(message) => errors.push(ThemeErrorInfo {
                file: format!("{id}.toml"),
                message,
            }),
        }
    }

    if let Err(error) = fs::create_dir_all(directory) {
        errors.push(ThemeErrorInfo {
            file: directory.display().to_string(),
            message: format!("No se pudo crear el directorio de temas: {error}"),
        });
        return ThemeCatalog {
            themes,
            errors,
            directory: directory.display().to_string(),
        };
    }

    let files = theme_files(directory, &mut errors);

    if let Some(reference) = referenced.map(str::trim).filter(|reference| looks_like_path(reference)) {
        let path = PathBuf::from(reference);

        if !files.iter().any(|file| file == &path) {
            load_file(&mut themes, &mut errors, &path);
        }
    }

    for file in &files {
        load_file(&mut themes, &mut errors, file);
    }

    ThemeCatalog {
        themes,
        errors,
        directory: directory.display().to_string(),
    }
}

/// Directorio de temas del usuario: `<config_dir>/trompa-studio/themes`.
pub fn themes_directory(app: &tauri::AppHandle) -> Result<PathBuf, DbError> {
    Ok(preferences::config_directory(app)?.join(THEMES_DIRECTORY))
}

/// Catálogo completo resuelto contra `[ui] theme` de las preferencias.
///
/// Un fallo leyendo las preferencias no impide listar los temas: se carga el
/// directorio sin referencia externa.
pub fn list_themes_with(app: &tauri::AppHandle) -> Result<ThemeCatalog, DbError> {
    let directory = themes_directory(app)?;
    let referenced = preferences::read_preferences_with(app)
        .ok()
        .map(|preferences| preferences.ui.theme);

    Ok(load_themes(&directory, referenced.as_deref()))
}

#[tauri::command]
pub async fn list_themes(app: tauri::AppHandle) -> Result<ThemeCatalog, DbError> {
    list_themes_with(&app)
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

            let id = NEXT.fetch_add(1, Ordering::Relaxed);
            let path =
                std::env::temp_dir().join(format!("trompa-themes-{}-{id}", std::process::id()));

            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).expect("crear el directorio temporal");
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }

        fn write(&self, name: &str, contents: &str) -> PathBuf {
            let path = self.0.join(name);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).expect("crear el directorio del archivo");
            }
            fs::write(&path, contents).expect("escribir el archivo de tema");
            path
        }
    }

    impl Drop for TempDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn papel() -> ThemeDefinition {
        parse_theme(
            "papel",
            BUILTIN_THEMES
                .iter()
                .find(|(id, _)| *id == "papel")
                .expect("papel integrado")
                .1,
            None,
        )
        .expect("analizar papel")
    }

    #[test]
    fn analiza_tema_anidado_completo() {
        let theme = papel();

        assert_eq!(theme.name, "Papel");
        assert_eq!(theme.shape.radius.as_deref(), Some("0.375rem"));
        assert_eq!(
            theme.light.background.sidebar.border.as_deref(),
            Some("#e0d3bf")
        );
        assert_eq!(theme.light.background.content.default.as_deref(), Some("transparent"));
        assert_eq!(theme.dark.text.highlighted.as_deref(), Some("#faf7f2"));
        assert_eq!(theme.light.chart.get("1").map(String::as_str), Some("#b45309"));
        assert_eq!(theme.dark.chart.len(), 6);
        assert_eq!(theme.palette.primary.as_deref(), Some("amber"));
        assert_eq!(theme.palette.neutral.as_deref(), Some("stone"));
    }

    #[test]
    fn rechaza_clave_desconocida() {
        let error = parse_theme(
            "x",
            "name = \"X\"\n[light.background]\npanel = \"#fff\"\n",
            None,
        )
        .expect_err("una tabla con clave desconocida debe fallar");

        assert!(error.contains("panel"), "{error}");
    }

    #[test]
    fn rechaza_color_invalido() {
        let error = parse_theme(
            "x",
            "name = \"X\"\n[light.background]\ndefault = \"azul\"\n",
            None,
        )
        .expect_err("un color no CSS debe fallar");

        assert!(error.contains("light.background.default"), "{error}");
    }

    #[test]
    fn rechaza_paleta_desconocida() {
        let error = parse_theme("x", "name = \"X\"\n[palette]\nprimary = \"azulado\"\n", None)
            .expect_err("una paleta fuera de Tailwind debe fallar");

        assert!(error.contains("palette.primary"), "{error}");
    }

    #[test]
    fn rechaza_clave_de_chart_invalida() {
        let error = parse_theme("x", "name = \"X\"\n[light.chart]\n7 = \"#fff\"\n", None)
            .expect_err("las claves de chart van de 1 a 6");

        assert!(error.contains("light.chart"), "{error}");
    }

    #[test]
    fn rechaza_shape_invalido() {
        assert!(parse_theme("x", "name = \"X\"\n[shape]\nradius = \"medio\"\n", None).is_err());
        assert!(parse_theme("x", "name = \"X\"\n[shape]\nradius = \"4rem\"\n", None).is_ok());
        assert!(parse_theme("x", "name = \"X\"\n[shape]\nradius = \"0.5em\"\n", None).is_ok());
    }

    #[test]
    fn slug_normaliza_espacios_y_mayusculas() {
        assert_eq!(theme_slug("Mi Tema Bonito"), "mi-tema-bonito");
        assert_eq!(theme_slug("Papél"), "papel");
        assert_eq!(theme_slug("  año  ñandú  "), "ano-nandu");
        assert_eq!(theme_slug("papel"), "papel");
        assert_eq!(theme_slug("tema.toml"), "tema-toml");
    }

    #[test]
    fn catalogo_tolera_archivo_roto() {
        let directory = TempDirectory::new();
        directory.write("bueno.toml", "name = \"Bueno\"\n");
        directory.write(
            "roto.toml",
            "name = \"Roto\"\n[light.background]\ndefault = \"azul\"\n",
        );

        let catalog = load_themes(directory.path(), None);

        assert_eq!(catalog.themes.len(), BUILTIN_THEMES.len() + 1);
        assert!(catalog.themes.iter().any(|theme| theme.id == "bueno"));
        assert_eq!(catalog.errors.len(), 1);
        assert!(catalog.errors[0].file.ends_with("roto.toml"), "{:?}", catalog.errors);
    }

    #[test]
    fn catalogo_deduplica_ids() {
        let directory = TempDirectory::new();
        directory.write("papel.toml", "name = \"Papel copia\"\n");
        directory.write("otro.toml", "name = \"Otro\"\n");

        let catalog = load_themes(directory.path(), None);
        let papel: Vec<&ThemeDefinition> =
            catalog.themes.iter().filter(|theme| theme.id == "papel").collect();

        assert_eq!(papel.len(), 1);
        assert!(papel[0].builtin);
        assert_eq!(catalog.themes.len(), BUILTIN_THEMES.len() + 1);
        assert_eq!(catalog.errors.len(), 1);
        assert!(catalog.errors[0].message.contains("ya está en uso"), "{:?}", catalog.errors);
    }

    #[test]
    fn catalogo_incluye_integrados() {
        let directory = TempDirectory::new();

        let catalog = load_themes(directory.path(), None);

        let ids: Vec<&str> = catalog.themes.iter().map(|theme| theme.id.as_str()).collect();
        assert_eq!(ids, ["trompa", "papel"]);
        assert!(catalog.themes.iter().all(|theme| theme.builtin));
        assert!(catalog.themes.iter().all(|theme| theme.path.is_none()));
        assert!(catalog.errors.is_empty(), "{:?}", catalog.errors);
        assert_eq!(catalog.directory, directory.path().display().to_string());
    }

    #[test]
    fn catalogo_carga_ruta_referenciada() {
        let directory = TempDirectory::new();
        let outside = TempDirectory::new();
        let path = outside.write("ajeno.toml", "name = \"Ajeno\"\n");

        let catalog = load_themes(directory.path(), Some(&path.display().to_string()));

        let theme = catalog
            .themes
            .iter()
            .find(|theme| theme.id == "ajeno")
            .expect("el tema referenciado debe cargarse");

        assert!(!theme.builtin);
        assert_eq!(theme.path.as_deref(), Some(path.display().to_string().as_str()));
        assert!(catalog.errors.is_empty(), "{:?}", catalog.errors);
    }

    #[test]
    fn catalogo_no_duplica_la_ruta_referenciada() {
        let directory = TempDirectory::new();
        let path = directory.write("mi-tema.toml", "name = \"Mi Tema\"\n");

        let catalog = load_themes(directory.path(), Some(&path.display().to_string()));
        let themes: Vec<&ThemeDefinition> =
            catalog.themes.iter().filter(|theme| theme.id == "mi-tema").collect();

        assert_eq!(themes.len(), 1);
        assert_eq!(catalog.errors.len(), 0);
    }

    #[test]
    fn catalogo_ignora_una_referencia_por_nombre() {
        let directory = TempDirectory::new();

        let catalog = load_themes(directory.path(), Some("papel"));

        assert_eq!(catalog.themes.len(), BUILTIN_THEMES.len());
        assert!(catalog.errors.is_empty(), "{:?}", catalog.errors);
    }
}
