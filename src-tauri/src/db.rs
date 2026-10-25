//! Conexión a PostgreSQL y ejecución de consultas para el frontend.
//!
//! Se usa el protocolo simple de `tokio-postgres`: todos los valores llegan
//! como texto y se admiten varias sentencias en una misma llamada.

use std::str::FromStr;
use std::sync::RwLock;
use std::time::Instant;

use deadpool_postgres::{Manager, Pool};
use rustls::{ClientConfig, RootCertStore};
use serde::{Deserialize, Serialize};
use tauri::State;
use tokio_postgres::config::SslMode;
use tokio_postgres::{Config as PgConfig, SimpleQueryMessage};
use tokio_postgres_rustls::MakeRustlsConnect;

/// Estado compartido: como mucho hay un pool activo.
#[derive(Default)]
pub struct DbState {
    pool: RwLock<Option<Pool>>,
}

impl DbState {
    fn pool(&self) -> Result<Pool, DbError> {
        self.pool
            .read()
            .map_err(|_| DbError::new("El estado de la conexión no está disponible"))?
            .clone()
            .ok_or_else(|| DbError::new("No hay ninguna conexión activa"))
    }

    fn replace(&self, pool: Option<Pool>) -> Result<(), DbError> {
        *self
            .pool
            .write()
            .map_err(|_| DbError::new("El estado de la conexión no está disponible"))? = pool;
        Ok(())
    }
}

/// Error tal cual lo consume el frontend.
#[derive(Debug, Serialize)]
pub struct DbError {
    message: String,
    code: Option<String>,
}

impl DbError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            code: None,
        }
    }
}

impl From<tokio_postgres::Error> for DbError {
    fn from(error: tokio_postgres::Error) -> Self {
        let code = error.code().map(|code| code.code().to_string());
        let message = error
            .as_db_error()
            .map(|db_error| db_error.message().to_string())
            .unwrap_or_else(|| error.to_string());

        Self { message, code }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SslModeParam {
    Disable,
    #[default]
    Prefer,
    Require,
}

impl From<SslModeParam> for SslMode {
    fn from(mode: SslModeParam) -> Self {
        match mode {
            SslModeParam::Disable => SslMode::Disable,
            SslModeParam::Prefer => SslMode::Prefer,
            SslModeParam::Require => SslMode::Require,
        }
    }
}

/// La UI puede mandar campos sueltos o una URL de conexión ya montada.
#[derive(Debug, Deserialize)]
#[serde(tag = "mode", rename_all = "lowercase")]
pub enum ConnectRequest {
    Fields {
        host: String,
        port: u16,
        database: String,
        user: String,
        #[serde(default)]
        password: String,
        #[serde(default)]
        sslmode: SslModeParam,
    },
    Url {
        url: String,
    },
}

impl ConnectRequest {
    fn into_config(self) -> Result<PgConfig, DbError> {
        match self {
            Self::Url { url } => PgConfig::from_str(url.trim())
                .map_err(|error| DbError::new(format!("URL de conexión inválida: {error}"))),
            Self::Fields {
                host,
                port,
                database,
                user,
                password,
                sslmode,
            } => {
                let mut config = PgConfig::new();
                config
                    .host(host.trim())
                    .port(port)
                    .dbname(database.trim())
                    .user(user.trim());

                if !password.is_empty() {
                    config.password(&password);
                }

                config.ssl_mode(sslmode.into());
                Ok(config)
            }
        }
    }
}

fn tls_connector() -> MakeRustlsConnect {
    let mut roots = RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

    let config = ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();

    MakeRustlsConnect::new(config)
}

fn build_pool(config: PgConfig) -> Result<Pool, DbError> {
    Pool::builder(Manager::new(config, tls_connector()))
        .max_size(8)
        .build()
        .map_err(|error| DbError::new(format!("No se pudo crear el pool de conexiones: {error}")))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionInfo {
    database: String,
    user: String,
    server_version: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryResult {
    columns: Vec<String>,
    rows: Vec<Vec<Option<String>>>,
    rows_affected: Option<u64>,
    elapsed_ms: u64,
}

/// Lógica de conexión independiente de Tauri, para poder probarla sin `State`.
pub async fn connect_with(
    request: ConnectRequest,
    state: &DbState,
) -> Result<ConnectionInfo, DbError> {
    let pool = build_pool(request.into_config()?)?;

    let client = pool
        .get()
        .await
        .map_err(|error| DbError::new(format!("No se pudo conectar a PostgreSQL: {error}")))?;

    let row = client
        .query_one(
            "SELECT current_database()::text, current_user::text, version()::text",
            &[],
        )
        .await?;

    let info = ConnectionInfo {
        database: row.get(0),
        user: row.get(1),
        server_version: row.get(2),
    };

    // Solo reemplazamos el pool cuando la conexión ya se ha validado.
    state.replace(Some(pool))?;

    Ok(info)
}

#[tauri::command]
pub async fn connect(
    request: ConnectRequest,
    state: State<'_, DbState>,
) -> Result<ConnectionInfo, DbError> {
    connect_with(request, &state).await
}

/// Lógica de desconexión independiente de Tauri.
pub fn disconnect_with(state: &DbState) -> Result<(), DbError> {
    state.replace(None)
}

#[tauri::command]
pub fn disconnect(state: State<'_, DbState>) -> Result<(), DbError> {
    disconnect_with(&state)
}

/// Lógica de consulta independiente de Tauri.
pub async fn run_query_with(state: &DbState, sql: &str) -> Result<QueryResult, DbError> {
    let pool = state.pool()?;
    let client = pool
        .get()
        .await
        .map_err(|error| DbError::new(format!("No se pudo obtener una conexión: {error}")))?;

    let started = Instant::now();
    let messages = client.simple_query(sql).await?;

    let mut result = QueryResult {
        columns: Vec::new(),
        rows: Vec::new(),
        rows_affected: None,
        elapsed_ms: started.elapsed().as_millis() as u64,
    };

    for message in messages {
        match message {
            // Cada sentencia abre su propia descripción: no mezclamos resultados.
            SimpleQueryMessage::RowDescription(description) => {
                result.columns = description
                    .iter()
                    .map(|column| column.name().to_string())
                    .collect();
                result.rows.clear();
            }
            SimpleQueryMessage::Row(row) => {
                if result.columns.is_empty() {
                    result.columns = row
                        .columns()
                        .iter()
                        .map(|column| column.name().to_string())
                        .collect();
                }

                result.rows.push(
                    (0..row.len())
                        .map(|index| row.get(index).map(str::to_owned))
                        .collect(),
                );
            }
            SimpleQueryMessage::CommandComplete(count) => {
                result.rows_affected = Some(count);
            }
            _ => {}
        }
    }

    Ok(result)
}

#[tauri::command]
pub async fn run_query(sql: String, state: State<'_, DbState>) -> Result<QueryResult, DbError> {
    run_query_with(&state, &sql).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tokio_postgres::config::Host;

    // ------------------------------------------------------------------
    // Tests unitarios: no necesitan una base de datos real.
    // ------------------------------------------------------------------

    #[test]
    fn deserializa_peticion_por_campos_con_defaults() {
        let request: ConnectRequest = serde_json::from_value(json!({
            "mode": "fields",
            "host": "localhost",
            "port": 5432,
            "database": "postgres",
            "user": "postgres"
        }))
        .expect("la petición debe ser válida");

        match request {
            ConnectRequest::Fields {
                password, sslmode, ..
            } => {
                assert!(password.is_empty());
                assert!(matches!(sslmode, SslModeParam::Prefer));
            }
            _ => panic!("se esperaba el modo fields"),
        }
    }

    #[test]
    fn construye_config_desde_campos_y_recorta_espacios() {
        let request: ConnectRequest = serde_json::from_value(json!({
            "mode": "fields",
            "host": "  db.local ",
            "port": 6543,
            "database": " ventas ",
            "user": " ana ",
            "password": "secreto",
            "sslmode": "require"
        }))
        .unwrap();

        let config = request.into_config().unwrap();

        assert_eq!(config.get_user(), Some("ana"));
        assert_eq!(config.get_dbname(), Some("ventas"));
        assert_eq!(config.get_ports(), &[6543]);
        assert_eq!(config.get_password(), Some("secreto".as_bytes()));
        assert_eq!(config.get_ssl_mode(), SslMode::Require);
        assert!(matches!(config.get_hosts(), [Host::Tcp(host)] if host.as_str() == "db.local"));
    }

    #[test]
    fn construye_config_desde_url() {
        let request: ConnectRequest = serde_json::from_value(json!({
            "mode": "url",
            "url": "  postgresql://ana@db.local:6543/ventas?sslmode=disable  "
        }))
        .unwrap();

        let config = request.into_config().unwrap();

        assert_eq!(config.get_user(), Some("ana"));
        assert_eq!(config.get_dbname(), Some("ventas"));
        assert_eq!(config.get_ports(), &[6543]);
        assert_eq!(config.get_ssl_mode(), SslMode::Disable);
        assert!(matches!(config.get_hosts(), [Host::Tcp(host)] if host.as_str() == "db.local"));
    }

    #[test]
    fn url_invalida_devuelve_error_legible() {
        let request: ConnectRequest = serde_json::from_value(json!({
            "mode": "url",
            "url": "postgresql://localhost:no-es-un-puerto/db"
        }))
        .unwrap();

        let error = match request.into_config() {
            Ok(_) => panic!("la URL debía ser inválida"),
            Err(error) => error,
        };

        assert!(error.message.starts_with("URL de conexión inválida:"));
    }

    #[test]
    fn sin_pool_activo_devuelve_error() {
        let state = DbState::default();
        match state.pool() {
            Ok(_) => panic!("no debía haber pool"),
            Err(error) => assert_eq!(error.message, "No hay ninguna conexión activa"),
        }
    }

    #[test]
    fn replace_actualiza_el_pool() {
        let state = DbState::default();
        let mut config = PgConfig::new();
        config.host("127.0.0.1").port(5432).user("postgres");
        let pool = build_pool(config).unwrap();

        state.replace(Some(pool)).unwrap();
        assert!(state.pool().is_ok());

        state.replace(None).unwrap();
        assert!(state.pool().is_err());
    }

    // ------------------------------------------------------------------
    // Tests de integración: requieren TROMPA_TEST_DATABASE_URL.
    //
    // Si la variable no está definida cada test se omite sin fallar, de
    // modo que `cargo test --lib` funciona en cualquier máquina.
    // ------------------------------------------------------------------

    /// Devuelve la URL de la base de datos de pruebas, o `None` si no se ha
    /// configurado. En ese caso deja constancia por stderr y el test termina.
    fn test_database_url() -> Option<String> {
        match std::env::var("TROMPA_TEST_DATABASE_URL") {
            Ok(url) if !url.trim().is_empty() => Some(url),
            _ => {
                eprintln!(
                    "TROMPA_TEST_DATABASE_URL no está definida; se omite el test de integración"
                );
                None
            }
        }
    }

    async fn connected_state(url: &str) -> DbState {
        let state = DbState::default();
        connect_with(
            ConnectRequest::Url {
                url: url.to_string(),
            },
            &state,
        )
        .await
        .expect("debía conectar con la base de datos de pruebas");
        state
    }

    #[tokio::test]
    async fn integracion_connect_with_devuelve_info_y_deja_pool() {
        let Some(url) = test_database_url() else {
            return;
        };

        let state = DbState::default();
        let info = connect_with(ConnectRequest::Url { url }, &state)
            .await
            .expect("connect_with debía funcionar");

        assert!(!info.database.is_empty(), "database no debe estar vacío");
        assert!(!info.user.is_empty(), "user no debe estar vacío");
        assert!(
            !info.server_version.is_empty(),
            "server_version no debe estar vacío"
        );
        assert!(state.pool().is_ok(), "el pool debe quedar disponible");
    }

    #[tokio::test]
    async fn integracion_run_query_with_devuelve_columnas_y_nulos() {
        let Some(url) = test_database_url() else {
            return;
        };
        let state = connected_state(&url).await;

        let result = run_query_with(&state, "SELECT 1::text AS n, NULL::text AS vacio")
            .await
            .expect("la consulta debía funcionar");

        assert_eq!(result.columns, vec!["n", "vacio"]);
        assert_eq!(result.rows, vec![vec![Some("1".to_owned()), None]]);
    }

    #[tokio::test]
    async fn integracion_run_query_with_cuenta_filas() {
        let Some(url) = test_database_url() else {
            return;
        };
        let state = connected_state(&url).await;

        let result = run_query_with(&state, "SELECT generate_series(1,3)")
            .await
            .expect("la consulta debía funcionar");

        assert_eq!(result.rows.len(), 3);
        assert_eq!(result.rows_affected, Some(3));
    }

    #[tokio::test]
    async fn integracion_error_sql_expone_codigo_y_mensaje() {
        let Some(url) = test_database_url() else {
            return;
        };
        let state = connected_state(&url).await;

        let error = run_query_with(&state, "SELECT * FROM tabla_que_no_existe_123456")
            .await
            .expect_err("la consulta debía fallar");

        assert_eq!(error.code.as_deref(), Some("42P01"));
        assert!(
            error.message.contains("tabla_que_no_existe_123456"),
            "el mensaje debía mencionar la tabla: {}",
            error.message
        );
    }

    #[tokio::test]
    async fn integracion_conexion_fallida_no_pisa_el_pool_anterior() {
        let Some(url) = test_database_url() else {
            return;
        };
        let state = connected_state(&url).await;

        let failed = connect_with(
            ConnectRequest::Url {
                url: "postgresql://postgres@127.0.0.1:1/postgres".to_string(),
            },
            &state,
        )
        .await;
        assert!(failed.is_err(), "la conexión al puerto 1 debía fallar");

        assert!(
            state.pool().is_ok(),
            "el pool anterior no debe haberse perdido"
        );
        let result = run_query_with(&state, "SELECT 7::text AS n")
            .await
            .expect("la consulta debía seguir funcionando");
        assert_eq!(result.rows, vec![vec![Some("7".to_owned())]]);
    }

    #[tokio::test]
    async fn integracion_disconnect_with_limpia_el_estado() {
        let Some(url) = test_database_url() else {
            return;
        };
        let state = connected_state(&url).await;

        disconnect_with(&state).expect("disconnect_with debía funcionar");

        match state.pool() {
            Ok(_) => panic!("no debía quedar pool tras desconectar"),
            Err(error) => assert_eq!(error.message, "No hay ninguna conexión activa"),
        }

        let error = run_query_with(&state, "SELECT 1")
            .await
            .expect_err("la consulta debía fallar sin conexión");
        assert_eq!(error.message, "No hay ninguna conexión activa");
    }

    /// Documenta la semántica actual: con varias sentencias, cada
    /// `RowDescription` reinicia columnas y filas, por lo que el resultado
    /// visible es el de la última sentencia.
    #[tokio::test]
    async fn integracion_varias_sentencias_devuelve_la_ultima() {
        let Some(url) = test_database_url() else {
            return;
        };
        let state = connected_state(&url).await;

        let result = run_query_with(&state, "SELECT 1 AS a; SELECT 2 AS b")
            .await
            .expect("la consulta debía funcionar");

        assert_eq!(result.columns, vec!["b"]);
        assert_eq!(result.rows, vec![vec![Some("2".to_owned())]]);
    }
}
