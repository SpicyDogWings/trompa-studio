//! Conexión a PostgreSQL y ejecución de consultas para el frontend.
//!
//! Se usa el protocolo simple de `tokio-postgres`: todos los valores llegan
//! como texto y se admiten varias sentencias en una misma llamada.

use std::collections::HashMap;
use std::io::BufReader;
use std::path::Path;
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Instant;

use deadpool_postgres::{Manager, Object, Pool};
use futures_util::{StreamExt, pin_mut};
use rustls::{ClientConfig, RootCertStore};
use serde::{Deserialize, Serialize};
use tauri::State;
use tauri::ipc::Channel;
use tokio_postgres::config::SslMode;
use tokio_postgres::{CancelToken, Config as PgConfig, SimpleQueryMessage};
use tokio_postgres_rustls::MakeRustlsConnect;

/// Filas que se acumulan antes de emitir un lote al frontend.
const ROW_BATCH_SIZE: usize = 200;

/// Consulta en curso: con qué token cancelarla y cómo hablar TLS con el servidor.
struct ActiveQuery {
    token: CancelToken,
    tls: MakeRustlsConnect,
}

/// Estado compartido: como mucho hay un pool activo.
#[derive(Default)]
pub struct DbState {
    pool: RwLock<Option<Pool>>,
    /// Conector TLS del pool activo, necesario para cancelar consultas cuando
    /// el servidor usa una CA personalizada.
    tls: RwLock<Option<MakeRustlsConnect>>,
    next_query_id: AtomicU64,
    /// `Arc` para que la tarea que consume el stream pueda desregistrarse.
    active: Arc<Mutex<HashMap<u64, ActiveQuery>>>,
}

impl DbState {
    fn pool(&self) -> Result<Pool, DbError> {
        self.pool
            .read()
            .map_err(|_| DbError::new("El estado de la conexión no está disponible"))?
            .clone()
            .ok_or_else(|| DbError::new("No hay ninguna conexión activa"))
    }

    fn replace(&self, pool: Option<Pool>, tls: Option<MakeRustlsConnect>) -> Result<(), DbError> {
        *self
            .pool
            .write()
            .map_err(|_| DbError::new("El estado de la conexión no está disponible"))? = pool;
        *self
            .tls
            .write()
            .map_err(|_| DbError::new("El estado de la conexión no está disponible"))? = tls;
        Ok(())
    }

    fn tls(&self) -> Result<MakeRustlsConnect, DbError> {
        self.tls
            .read()
            .map_err(|_| DbError::new("El estado de la conexión no está disponible"))?
            .clone()
            .ok_or_else(|| DbError::new("No hay ninguna conexión activa"))
    }

    fn next_query_id(&self) -> u64 {
        self.next_query_id.fetch_add(1, Ordering::Relaxed)
    }

    fn register_active(&self, query_id: u64, query: ActiveQuery) -> Result<(), DbError> {
        registry_insert(&self.active, query_id, query)
    }

    fn take_active(&self, query_id: u64) -> Result<Option<ActiveQuery>, DbError> {
        registry_remove(&self.active, query_id)
    }
}

/// Error legible cuando el registro de consultas no se puede bloquear.
fn registry_error() -> DbError {
    DbError::new("El registro de consultas no está disponible")
}

fn registry_insert<T>(
    registry: &Mutex<HashMap<u64, T>>,
    query_id: u64,
    value: T,
) -> Result<(), DbError> {
    registry
        .lock()
        .map_err(|_| registry_error())?
        .insert(query_id, value);
    Ok(())
}

fn registry_remove<T>(
    registry: &Mutex<HashMap<u64, T>>,
    query_id: u64,
) -> Result<Option<T>, DbError> {
    Ok(registry
        .lock()
        .map_err(|_| registry_error())?
        .remove(&query_id))
}

/// Eventos incrementales que consume el frontend mientras corre una consulta.
#[derive(Debug, Clone, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum QueryEvent {
    Started {
        query_id: u64,
    },
    Columns {
        columns: Vec<String>,
    },
    Rows {
        rows: Vec<Vec<Option<String>>>,
    },
    Done {
        rows_affected: u64,
        elapsed_ms: u64,
    },
    Error {
        message: String,
        code: Option<String>,
    },
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
        #[serde(default)]
        sslrootcert: Option<String>,
    },
    Url {
        url: String,
        #[serde(default)]
        sslrootcert: Option<String>,
    },
}

impl ConnectRequest {
    /// Ruta a la CA adicional, si la UI ha indicado una. Una cadena vacía
    /// equivale a no usar CA personalizada.
    fn sslrootcert_path(&self) -> Option<&Path> {
        let value = match self {
            Self::Fields { sslrootcert, .. } | Self::Url { sslrootcert, .. } => {
                sslrootcert.as_deref()
            }
        };

        value
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(Path::new)
    }

    fn into_config(self) -> Result<PgConfig, DbError> {
        match self {
            Self::Url { url, .. } => PgConfig::from_str(url.trim())
                .map_err(|error| DbError::new(format!("URL de conexión inválida: {error}"))),
            Self::Fields {
                host,
                port,
                database,
                user,
                password,
                sslmode,
                ..
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

fn tls_connector(extra_ca: Option<&Path>) -> Result<MakeRustlsConnect, DbError> {
    let mut roots = RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

    if let Some(path) = extra_ca {
        let data = std::fs::read(path).map_err(|error| {
            DbError::new(format!(
                "No se pudo leer el certificado CA '{}': {error}",
                path.display()
            ))
        })?;

        let mut added = 0;
        for certificate in rustls_pemfile::certs(&mut BufReader::new(data.as_slice())) {
            let certificate = certificate.map_err(|error| {
                DbError::new(format!(
                    "Certificado CA inválido en '{}': {error}",
                    path.display()
                ))
            })?;

            roots.add(certificate).map_err(|error| {
                DbError::new(format!(
                    "No se pudo usar el certificado CA '{}': {error}",
                    path.display()
                ))
            })?;
            added += 1;
        }

        if added == 0 {
            return Err(DbError::new(format!(
                "El archivo '{}' no contiene certificados PEM válidos",
                path.display()
            )));
        }
    }

    let config = ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();

    Ok(MakeRustlsConnect::new(config))
}

fn build_pool(config: PgConfig, tls: MakeRustlsConnect) -> Result<Pool, DbError> {
    Pool::builder(Manager::new(config, tls))
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
    let extra_ca = request.sslrootcert_path().map(Path::to_path_buf);
    let tls = tls_connector(extra_ca.as_deref())?;
    let pool = build_pool(request.into_config()?, tls.clone())?;

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
    state.replace(Some(pool), Some(tls))?;

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
    state.replace(None, None)
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

/// Lanza una consulta en segundo plano y emite sus resultados incrementalmente.
///
/// El callback corre en la tarea de streaming, sin depender de Tauri, de modo
/// que los tests pueden usar un canal de `tokio`.
pub async fn start_query_with<F>(state: &DbState, sql: &str, emit: F) -> Result<u64, DbError>
where
    F: Fn(QueryEvent) + Send + 'static,
{
    let pool = state.pool()?;
    let client = pool
        .get()
        .await
        .map_err(|error| DbError::new(format!("No se pudo obtener una conexión: {error}")))?;

    let token = client.cancel_token();
    let query_id = state.next_query_id();
    state.register_active(
        query_id,
        ActiveQuery {
            token,
            tls: state.tls()?,
        },
    )?;

    emit(QueryEvent::Started { query_id });

    let sql = sql.to_owned();
    let active = Arc::clone(&state.active);
    tauri::async_runtime::spawn(async move {
        stream_query(client, &sql, emit).await;
        // Cancela o no, la consulta ya ha terminado: no debe quedar registrada.
        let _ = registry_remove(&active, query_id);
    });

    Ok(query_id)
}

/// Consume el stream del protocolo simple y traduce cada mensaje a un evento.
async fn stream_query<F>(client: Object, sql: &str, emit: F)
where
    F: Fn(QueryEvent) + Send + 'static,
{
    let started = Instant::now();
    let mut columns: Vec<String> = Vec::new();
    let mut rows: Vec<Vec<Option<String>>> = Vec::new();

    let stream = match client.simple_query_raw(sql).await {
        Ok(stream) => stream,
        Err(error) => {
            let error = DbError::from(error);
            emit(QueryEvent::Error {
                message: error.message,
                code: error.code,
            });
            return;
        }
    };
    pin_mut!(stream);

    while let Some(message) = stream.next().await {
        match message {
            Ok(SimpleQueryMessage::RowDescription(description)) => {
                // Cada sentencia abre su propia descripción: emitimos lo pendiente
                // antes de reiniciar columnas y filas.
                flush_rows(&mut rows, &emit);
                columns = description
                    .iter()
                    .map(|column| column.name().to_string())
                    .collect();
                emit(QueryEvent::Columns {
                    columns: columns.clone(),
                });
            }
            Ok(SimpleQueryMessage::Row(row)) => {
                if columns.is_empty() {
                    columns = row
                        .columns()
                        .iter()
                        .map(|column| column.name().to_string())
                        .collect();
                    emit(QueryEvent::Columns {
                        columns: columns.clone(),
                    });
                }

                rows.push(
                    (0..row.len())
                        .map(|index| row.get(index).map(str::to_owned))
                        .collect(),
                );

                if rows.len() >= ROW_BATCH_SIZE {
                    flush_rows(&mut rows, &emit);
                }
            }
            Ok(SimpleQueryMessage::CommandComplete(count)) => {
                flush_rows(&mut rows, &emit);
                emit(QueryEvent::Done {
                    rows_affected: count,
                    elapsed_ms: started.elapsed().as_millis() as u64,
                });
            }
            Ok(_) => {}
            Err(error) => {
                flush_rows(&mut rows, &emit);
                let error = DbError::from(error);
                emit(QueryEvent::Error {
                    message: error.message,
                    code: error.code,
                });
                return;
            }
        }
    }
}

fn flush_rows<F>(rows: &mut Vec<Vec<Option<String>>>, emit: &F)
where
    F: Fn(QueryEvent) + Send + 'static,
{
    if rows.is_empty() {
        return;
    }

    emit(QueryEvent::Rows {
        rows: std::mem::take(rows),
    });
}

/// Cancela una consulta en curso usando su token de PostgreSQL.
pub async fn cancel_query_with(state: &DbState, query_id: u64) -> Result<(), DbError> {
    let active = state.take_active(query_id)?.ok_or_else(|| {
        DbError::new(format!(
            "No hay ninguna consulta en ejecución con id {query_id}"
        ))
    })?;

    active.token.cancel_query(active.tls).await?;
    Ok(())
}

#[tauri::command]
pub async fn start_query(
    sql: String,
    on_event: Channel<QueryEvent>,
    state: State<'_, DbState>,
) -> Result<u64, DbError> {
    start_query_with(&state, &sql, move |event| {
        // Si la vista ya no escucha, el fallo no debe romper la consulta.
        let _ = on_event.send(event);
    })
    .await
}

#[tauri::command]
pub async fn cancel_query(query_id: u64, state: State<'_, DbState>) -> Result<(), DbError> {
    cancel_query_with(&state, query_id).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::time::Duration;
    use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
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
        let tls = tls_connector(None).unwrap();
        let pool = build_pool(config, tls.clone()).unwrap();

        state.replace(Some(pool), Some(tls)).unwrap();
        assert!(state.pool().is_ok());
        assert!(state.tls().is_ok());

        state.replace(None, None).unwrap();
        assert!(state.pool().is_err());
    }

    fn fixture(name: &str) -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name)
    }

    #[test]
    fn deserializa_sslrootcert_en_campos_y_url() {
        let fields: ConnectRequest = serde_json::from_value(json!({
            "mode": "fields",
            "host": "localhost",
            "port": 5432,
            "database": "postgres",
            "user": "postgres",
            "sslrootcert": "  /etc/ssl/ca.pem  "
        }))
        .unwrap();
        assert_eq!(
            fields.sslrootcert_path(),
            Some(Path::new("/etc/ssl/ca.pem"))
        );

        let url: ConnectRequest = serde_json::from_value(json!({
            "mode": "url",
            "url": "postgresql://ana@db.local:5432/ventas",
            "sslrootcert": "/etc/ssl/ca.pem"
        }))
        .unwrap();
        assert_eq!(url.sslrootcert_path(), Some(Path::new("/etc/ssl/ca.pem")));

        let sin_ca: ConnectRequest = serde_json::from_value(json!({
            "mode": "fields",
            "host": "localhost",
            "port": 5432,
            "database": "postgres",
            "user": "postgres"
        }))
        .unwrap();
        assert_eq!(sin_ca.sslrootcert_path(), None);

        let vacia: ConnectRequest = serde_json::from_value(json!({
            "mode": "fields",
            "host": "localhost",
            "port": 5432,
            "database": "postgres",
            "user": "postgres",
            "sslrootcert": "   "
        }))
        .unwrap();
        assert_eq!(vacia.sslrootcert_path(), None);
    }

    #[test]
    fn ca_inexistente_devuelve_error_legible() {
        let path = fixture("no-existe.pem");

        let error = match tls_connector(Some(&path)) {
            Ok(_) => panic!("la CA inexistente debía fallar"),
            Err(error) => error,
        };

        assert!(
            error
                .message
                .starts_with("No se pudo leer el certificado CA"),
            "mensaje inesperado: {}",
            error.message
        );
        assert!(error.message.contains("no-existe.pem"));
    }

    #[test]
    fn ca_pem_valida_se_carga() {
        let path = fixture("ca.pem");

        if let Err(error) = tls_connector(Some(&path)) {
            panic!("el PEM válido debía cargarse: {}", error.message);
        }
    }

    #[test]
    fn ca_pem_corrupta_devuelve_error() {
        let path = fixture("ca-invalida.pem");

        let error = match tls_connector(Some(&path)) {
            Ok(_) => panic!("el PEM corrupto debía fallar"),
            Err(error) => error,
        };

        assert!(
            error.message.starts_with("Certificado CA inválido")
                || error.message.starts_with("El archivo"),
            "mensaje inesperado: {}",
            error.message
        );
        assert!(error.message.contains("ca-invalida.pem"));
    }
    #[test]
    fn serializa_eventos_con_etiqueta_type_y_camel_case() {
        assert_eq!(
            serde_json::to_value(QueryEvent::Started { query_id: 7 }).unwrap(),
            json!({ "type": "started", "queryId": 7 })
        );

        assert_eq!(
            serde_json::to_value(QueryEvent::Columns {
                columns: vec!["n".to_owned(), "vacio".to_owned()],
            })
            .unwrap(),
            json!({ "type": "columns", "columns": ["n", "vacio"] })
        );

        assert_eq!(
            serde_json::to_value(QueryEvent::Rows {
                rows: vec![vec![Some("1".to_owned()), None]],
            })
            .unwrap(),
            json!({ "type": "rows", "rows": [["1", null]] })
        );

        assert_eq!(
            serde_json::to_value(QueryEvent::Done {
                rows_affected: 3,
                elapsed_ms: 12,
            })
            .unwrap(),
            json!({ "type": "done", "rowsAffected": 3, "elapsedMs": 12 })
        );

        assert_eq!(
            serde_json::to_value(QueryEvent::Error {
                message: "cancelando la sentencia".to_owned(),
                code: Some("57014".to_owned()),
            })
            .unwrap(),
            json!({ "type": "error", "message": "cancelando la sentencia", "code": "57014" })
        );

        assert_eq!(
            serde_json::to_value(QueryEvent::Error {
                message: "sin conexión".to_owned(),
                code: None,
            })
            .unwrap(),
            json!({ "type": "error", "message": "sin conexión", "code": null })
        );
    }

    #[test]
    fn el_registro_inserta_y_elimina_consultas() {
        let registry: Mutex<HashMap<u64, &str>> = Mutex::new(HashMap::new());

        registry_insert(&registry, 1, "primera").unwrap();
        registry_insert(&registry, 2, "segunda").unwrap();
        assert_eq!(registry.lock().unwrap().len(), 2);

        assert_eq!(registry_remove(&registry, 1).unwrap(), Some("primera"));
        assert_eq!(registry_remove(&registry, 1).unwrap(), None);
        assert_eq!(registry.lock().unwrap().len(), 1);
    }

    #[test]
    fn asigna_ids_crecientes_y_empieza_sin_consultas() {
        let state = DbState::default();

        assert_eq!(state.next_query_id(), 0);
        assert_eq!(state.next_query_id(), 1);
        assert!(state.active.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn cancelar_un_id_desconocido_devuelve_error_legible() {
        let state = DbState::default();

        let error = cancel_query_with(&state, 42)
            .await
            .expect_err("no había ninguna consulta registrada");

        assert!(error.message.contains("42"), "mensaje: {}", error.message);
        assert_eq!(error.code, None);
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
                sslrootcert: None,
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
        let info = connect_with(
            ConnectRequest::Url {
                url,
                sslrootcert: None,
            },
            &state,
        )
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
                sslrootcert: None,
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

    // ------------------------------------------------------------------
    // Tests de integración del streaming y la cancelación.
    // ------------------------------------------------------------------

    fn event_channel() -> (UnboundedSender<QueryEvent>, UnboundedReceiver<QueryEvent>) {
        tokio::sync::mpsc::unbounded_channel()
    }

    /// Recoge eventos hasta el `Done` o el `Error` que cierra la consulta.
    async fn collect_until_finished(rx: &mut UnboundedReceiver<QueryEvent>) -> Vec<QueryEvent> {
        let mut events = Vec::new();

        loop {
            let event = tokio::time::timeout(Duration::from_secs(5), rx.recv())
                .await
                .expect("el stream debía terminar a tiempo")
                .expect("el canal no debía cerrarse sin evento final");

            let finished = matches!(event, QueryEvent::Done { .. } | QueryEvent::Error { .. });
            events.push(event);

            if finished {
                return events;
            }
        }
    }

    #[tokio::test]
    async fn integracion_start_query_emite_columnas_filas_y_done() {
        let Some(url) = test_database_url() else {
            return;
        };
        let state = connected_state(&url).await;
        let (tx, mut rx) = event_channel();

        let query_id = start_query_with(&state, "SELECT generate_series(1,3) AS n", move |event| {
            let _ = tx.send(event);
        })
        .await
        .expect("start_query_with debía lanzar la consulta");

        let events = collect_until_finished(&mut rx).await;

        match &events[0] {
            QueryEvent::Started { query_id: started } => assert_eq!(*started, query_id),
            other => panic!("el primer evento debía ser Started: {other:?}"),
        }

        let columns = events
            .iter()
            .find_map(|event| match event {
                QueryEvent::Columns { columns } => Some(columns.clone()),
                _ => None,
            })
            .expect("debía emitirse Columns");
        assert_eq!(columns, vec!["n".to_owned()]);

        let rows: Vec<Vec<Option<String>>> = events
            .iter()
            .filter_map(|event| match event {
                QueryEvent::Rows { rows } => Some(rows.clone()),
                _ => None,
            })
            .flatten()
            .collect();
        assert_eq!(
            rows,
            vec![
                vec![Some("1".to_owned())],
                vec![Some("2".to_owned())],
                vec![Some("3".to_owned())],
            ]
        );

        match events.last().expect("debía haber evento final") {
            QueryEvent::Done {
                rows_affected,
                elapsed_ms,
            } => {
                assert_eq!(*rows_affected, 3);
                assert!(*elapsed_ms < 5_000, "elapsed_ms fuera de rango");
            }
            other => panic!("el último evento debía ser Done: {other:?}"),
        }

        // La consulta terminada se desregistra al cerrarse su tarea.
        tokio::time::timeout(Duration::from_secs(5), async {
            while !state.active.lock().expect("registro disponible").is_empty() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("la consulta terminada debía desregistrarse");

        let error = cancel_query_with(&state, query_id)
            .await
            .expect_err("la consulta ya no está activa");
        assert!(error.message.contains(&query_id.to_string()));
    }

    #[tokio::test]
    async fn integracion_cancelar_consulta_larga_emite_error_57014() {
        let Some(url) = test_database_url() else {
            return;
        };
        let state = connected_state(&url).await;
        let (tx, mut rx) = event_channel();

        let started = Instant::now();
        let query_id = start_query_with(&state, "SELECT pg_sleep(30)", move |event| {
            let _ = tx.send(event);
        })
        .await
        .expect("start_query_with debía lanzar la consulta");

        let first = tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .expect("debía llegar Started")
            .expect("el canal no debía cerrarse");
        assert!(matches!(
            first,
            QueryEvent::Started { query_id: started } if started == query_id
        ));

        // Esperar a `Columns` garantiza que la sentencia ya está corriendo en
        // el servidor y la cancelación no llega antes de tiempo.
        loop {
            let event = tokio::time::timeout(Duration::from_secs(5), rx.recv())
                .await
                .expect("debían llegar las columnas")
                .expect("el canal no debía cerrarse");
            if matches!(event, QueryEvent::Columns { .. }) {
                break;
            }
        }

        tokio::time::timeout(Duration::from_secs(5), cancel_query_with(&state, query_id))
            .await
            .expect("cancelar debía responder a tiempo")
            .expect("cancel_query_with debía funcionar");

        let events = tokio::time::timeout(Duration::from_secs(5), collect_until_finished(&mut rx))
            .await
            .expect("la cancelación debía terminar la consulta en menos de 5 s");

        assert!(
            started.elapsed() < Duration::from_secs(5),
            "la cancelación tardó demasiado"
        );

        let code = events
            .iter()
            .find_map(|event| match event {
                QueryEvent::Error { code, .. } => Some(code.clone()),
                _ => None,
            })
            .expect("debía emitirse Error tras cancelar");
        assert_eq!(code.as_deref(), Some("57014"));
    }
}
