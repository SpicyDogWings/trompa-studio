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

#[tauri::command]
pub async fn connect(
    request: ConnectRequest,
    state: State<'_, DbState>,
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

    state.replace(Some(pool))?;

    Ok(info)
}

#[tauri::command]
pub fn disconnect(state: State<'_, DbState>) -> Result<(), DbError> {
    state.replace(None)
}

#[tauri::command]
pub async fn run_query(sql: String, state: State<'_, DbState>) -> Result<QueryResult, DbError> {
    let pool = state.pool()?;
    let client = pool
        .get()
        .await
        .map_err(|error| DbError::new(format!("No se pudo obtener una conexión: {error}")))?;

    let started = Instant::now();
    let messages = client.simple_query(&sql).await?;

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
