//! Conexión a PostgreSQL y ejecución de consultas para el frontend.
//!
//! Se usa el protocolo simple de `tokio-postgres`: todos los valores llegan
//! como texto y se admiten varias sentencias en una misma llamada.

use std::collections::{BTreeMap, HashMap};
use std::io::BufReader;
use std::path::Path;
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Instant;

use deadpool_postgres::{Manager, Object, Pool, PoolError, TimeoutType};
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
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            code: None,
        }
    }

    /// Traduce un fallo del pool conservando el error real del backend.
    ///
    /// El `Display` de `PoolError` es genérico ("db error"), pero el error
    /// de PostgreSQL viaja en `PoolError::Backend`, así que hay que
    /// desenvolverlo en vez de formatear el envoltorio.
    fn from_pool_error(context: &str, error: PoolError) -> Self {
        let mut mapped = match error {
            PoolError::Backend(error) => Self::from(error),
            PoolError::Timeout(TimeoutType::Wait) => {
                Self::new("se agotó el tiempo de espera para obtener una conexión del pool")
            }
            PoolError::Timeout(TimeoutType::Create) => {
                Self::new("se agotó el tiempo de espera al crear una conexión")
            }
            PoolError::Timeout(TimeoutType::Recycle) => {
                Self::new("se agotó el tiempo de espera al reciclar una conexión")
            }
            PoolError::Closed => Self::new("el pool de conexiones está cerrado"),
            PoolError::NoRuntimeSpecified => {
                Self::new("el pool de conexiones no tiene un runtime asociado")
            }
            PoolError::PostCreateHook(error) => {
                Self::new(format!("el hook post_create del pool falló: {error}"))
            }
        };

        mapped.message = format!("{context}: {}", mapped.message);
        mapped
    }
}

/// Compone un mensaje con la cadena completa de causas de un error.
///
/// `tokio_postgres::Error` solo describe la clase del fallo ("db error",
/// "error connecting to server"); el detalle real (el `DbError` de
/// PostgreSQL o el error de E/S) vive en `source()`.
fn describe_error_chain(error: &(dyn std::error::Error + 'static)) -> String {
    let mut message = error.to_string();
    let mut source = error.source();

    while let Some(cause) = source {
        let detail = cause.to_string();
        if !detail.is_empty() && !message.contains(&detail) {
            message.push_str(": ");
            message.push_str(&detail);
        }
        source = cause.source();
    }

    message
}

impl From<tokio_postgres::Error> for DbError {
    fn from(error: tokio_postgres::Error) -> Self {
        // Un error del servidor ya trae el mensaje y el SQLSTATE reales.
        if let Some(db_error) = error.as_db_error() {
            return Self {
                message: db_error.message().to_string(),
                code: Some(db_error.code().code().to_string()),
            };
        }

        Self {
            message: describe_error_chain(&error),
            code: None,
        }
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

/// Una página de registros de una tabla o vista, con el total de filas.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TablePage {
    columns: Vec<String>,
    rows: Vec<Vec<Option<String>>>,
    total: i64,
    page: u32,
    page_size: u32,
}

/// Identificador SQL citado: `"esquema"."tabla"` con las comillas internas duplicadas.
fn table_ident(schema: &str, table: &str) -> String {
    format!(
        "\"{}\".\"{}\"",
        schema.replace('"', "\"\""),
        table.replace('"', "\"\"")
    )
}

/// Objeto del catálogo: su OID y su nombre dentro del esquema.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaObject {
    oid: i64,
    name: String,
}

/// Función del catálogo: el OID identifica la sobrecarga concreta.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaFunction {
    oid: i64,
    name: String,
    signature: String,
}

/// Todo lo que el sidebar lista de un esquema.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaInfo {
    name: String,
    tables: Vec<SchemaObject>,
    views: Vec<SchemaObject>,
    functions: Vec<SchemaFunction>,
    enums: Vec<SchemaObject>,
}

impl SchemaInfo {
    fn empty(name: String) -> Self {
        Self {
            name,
            tables: Vec::new(),
            views: Vec::new(),
            functions: Vec::new(),
            enums: Vec::new(),
        }
    }
}

/// Catálogo completo de la base conectada.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaCatalog {
    database: String,
    schemas: Vec<SchemaInfo>,
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
        .map_err(|error| DbError::from_pool_error("No se pudo conectar a PostgreSQL", error))?;

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

/// Resultado de una respuesta simple: columnas de la última descripción,
/// filas como texto y filas afectadas de la última sentencia que las reporte.
fn text_result(
    messages: Vec<SimpleQueryMessage>,
) -> (Vec<String>, Vec<Vec<Option<String>>>, Option<u64>) {
    let mut columns = Vec::new();
    let mut rows = Vec::new();
    let mut rows_affected = None;

    for message in messages {
        match message {
            // Cada sentencia abre su propia descripción: no mezclamos resultados.
            SimpleQueryMessage::RowDescription(description) => {
                columns = description
                    .iter()
                    .map(|column| column.name().to_string())
                    .collect();
                rows.clear();
            }
            SimpleQueryMessage::Row(row) => {
                if columns.is_empty() {
                    columns = row
                        .columns()
                        .iter()
                        .map(|column| column.name().to_string())
                        .collect();
                }

                rows.push(
                    (0..row.len())
                        .map(|index| row.get(index).map(str::to_owned))
                        .collect(),
                );
            }
            SimpleQueryMessage::CommandComplete(count) => {
                rows_affected = Some(count);
            }
            _ => {}
        }
    }

    (columns, rows, rows_affected)
}

/// Lógica de consulta independiente de Tauri.
pub async fn run_query_with(state: &DbState, sql: &str) -> Result<QueryResult, DbError> {
    let pool = state.pool()?;
    let client = pool
        .get()
        .await
        .map_err(|error| DbError::from_pool_error("No se pudo obtener una conexión", error))?;

    let started = Instant::now();
    let (columns, rows, rows_affected) = text_result(client.simple_query(sql).await?);

    Ok(QueryResult {
        columns,
        rows,
        rows_affected,
        // La recolección de mensajes también cuenta como tiempo de consulta.
        elapsed_ms: started.elapsed().as_millis() as u64,
    })
}

/// Lógica de página de registros independiente de Tauri.
pub async fn fetch_table_page_with(
    state: &DbState,
    schema: &str,
    table: &str,
    page: u32,
    page_size: u32,
) -> Result<TablePage, DbError> {
    let page_size = page_size.clamp(1, 1000);
    let ident = table_ident(schema, table);

    let pool = state.pool()?;
    let client = pool
        .get()
        .await
        .map_err(|error| DbError::from_pool_error("No se pudo obtener una conexión", error))?;

    // El total se pide siempre: el frontend necesita el número de páginas.
    let (_, rows, _) = text_result(
        client
            .simple_query(&format!("SELECT count(*) FROM {ident}"))
            .await?,
    );
    let total = rows
        .first()
        .and_then(|row| row.first())
        .and_then(|cell| cell.as_deref())
        .and_then(|text| text.trim().parse::<i64>().ok())
        .unwrap_or(0);

    // El recorte se hace aquí para que el frontend nunca pida una página vacía.
    let pages = (total + page_size as i64 - 1) / page_size as i64;
    let last = pages.max(1) as u32;
    let page = page.clamp(1, last);
    let offset = (page - 1) as i64 * page_size as i64;

    // Sin `ORDER BY`: se respeta el orden que devuelva Postgres.
    let (columns, rows, _) = text_result(
        client
            .simple_query(&format!(
                "SELECT * FROM {ident} LIMIT {page_size} OFFSET {offset}"
            ))
            .await?,
    );

    Ok(TablePage {
        columns,
        rows,
        total,
        page,
        page_size,
    })
}

#[tauri::command]
pub async fn run_query(sql: String, state: State<'_, DbState>) -> Result<QueryResult, DbError> {
    run_query_with(&state, &sql).await
}

#[tauri::command]
pub async fn fetch_table_page(
    schema: String,
    table: String,
    page: u32,
    page_size: u32,
    state: State<'_, DbState>,
) -> Result<TablePage, DbError> {
    fetch_table_page_with(&state, &schema, &table, page, page_size).await
}

/// Catálogo de esquema independiente de Tauri.
///
/// Se consultan `pg_class`, `pg_proc` y `pg_type` por separado en vez de unir
/// las tres: una unión multiplicaría filas y obligaría a deduplicar en Rust.
/// Cada consulta ya viene ordenada, así que los `Vec` de cada categoría salen
/// alfabéticos; el `BTreeMap` ordena los esquemas por nombre.
pub async fn list_schema_with(state: &DbState) -> Result<SchemaCatalog, DbError> {
    let pool = state.pool()?;
    let client = pool
        .get()
        .await
        .map_err(|error| DbError::from_pool_error("No se pudo obtener una conexión", error))?;

    let database: String = client
        .query_one("SELECT current_database()::text", &[])
        .await?
        .get(0);

    let mut schemas: BTreeMap<String, SchemaInfo> = BTreeMap::new();

    let relations = client
        .query(
            "SELECT n.nspname, c.relname, c.oid::bigint, c.relkind::text
             FROM pg_class c
             JOIN pg_namespace n ON n.oid = c.relnamespace
             WHERE c.relkind IN ('r','p','v','m')
               AND n.nspname <> 'information_schema'
               AND n.nspname !~ '^pg_'
             ORDER BY n.nspname, c.relname",
            &[],
        )
        .await?;

    for row in relations {
        let schema: String = row.get(0);
        let object = SchemaObject {
            oid: row.get(2),
            name: row.get(1),
        };
        let entry = schemas
            .entry(schema.clone())
            .or_insert_with(|| SchemaInfo::empty(schema));

        match row.get::<_, String>(3).as_str() {
            "r" | "p" => entry.tables.push(object),
            _ => entry.views.push(object),
        }
    }

    let functions = client
        .query(
            "SELECT n.nspname, p.proname, p.oid::bigint,
                    pg_get_function_identity_arguments(p.oid)
             FROM pg_proc p
             JOIN pg_namespace n ON n.oid = p.pronamespace
             WHERE p.prokind = 'f'
               AND n.nspname <> 'information_schema'
               AND n.nspname !~ '^pg_'
             ORDER BY n.nspname, p.proname, pg_get_function_identity_arguments(p.oid)",
            &[],
        )
        .await?;

    for row in functions {
        let schema: String = row.get(0);
        let function = SchemaFunction {
            oid: row.get(2),
            name: row.get(1),
            signature: row.get(3),
        };
        schemas
            .entry(schema.clone())
            .or_insert_with(|| SchemaInfo::empty(schema))
            .functions
            .push(function);
    }

    let enums = client
        .query(
            "SELECT n.nspname, t.typname, t.oid::bigint
             FROM pg_type t
             JOIN pg_namespace n ON n.oid = t.typnamespace
             WHERE t.typtype = 'e'
               AND n.nspname <> 'information_schema'
               AND n.nspname !~ '^pg_'
             ORDER BY n.nspname, t.typname",
            &[],
        )
        .await?;

    for row in enums {
        let schema: String = row.get(0);
        let object = SchemaObject {
            oid: row.get(2),
            name: row.get(1),
        };
        schemas
            .entry(schema.clone())
            .or_insert_with(|| SchemaInfo::empty(schema))
            .enums
            .push(object);
    }

    let mut schemas: Vec<SchemaInfo> = schemas.into_values().collect();
    schemas.sort_by(|a, b| {
        (a.name != "public")
            .cmp(&(b.name != "public"))
            .then_with(|| a.name.cmp(&b.name))
    });

    Ok(SchemaCatalog { database, schemas })
}

#[tauri::command]
pub async fn list_schema(state: State<'_, DbState>) -> Result<SchemaCatalog, DbError> {
    list_schema_with(&state).await
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
        .map_err(|error| DbError::from_pool_error("No se pudo obtener una conexión", error))?;

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
    use deadpool_postgres::HookError;
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

    /// Error de `tokio-postgres` sin `DbError` asociado, para probar el mapeo
    /// sin depender de un servidor real.
    fn error_de_configuracion() -> tokio_postgres::Error {
        PgConfig::from_str("postgresql://localhost:no-es-un-puerto/db")
            .expect_err("la URL debía ser inválida")
    }

    #[test]
    fn error_sin_db_compone_la_cadena_de_causas() {
        let error = DbError::from(error_de_configuracion());

        assert_eq!(
            error.message,
            "invalid connection string: invalid value for option `port`"
        );
        assert_eq!(error.code, None);
    }

    #[test]
    fn describe_error_chain_agrega_causas_y_evita_duplicados() {
        #[derive(Debug)]
        struct Envoltorio {
            mensaje: &'static str,
            causa: std::io::Error,
        }

        impl std::fmt::Display for Envoltorio {
            fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str(self.mensaje)
            }
        }

        impl std::error::Error for Envoltorio {
            fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
                Some(&self.causa)
            }
        }

        let simple = Envoltorio {
            mensaje: "fallo de red",
            causa: std::io::Error::other("conexión rechazada"),
        };
        assert_eq!(
            describe_error_chain(&simple),
            "fallo de red: conexión rechazada"
        );

        // Si la causa ya está en el mensaje, no se repite.
        let repetido = Envoltorio {
            mensaje: "fallo de red: conexión rechazada",
            causa: std::io::Error::other("conexión rechazada"),
        };
        assert_eq!(
            describe_error_chain(&repetido),
            "fallo de red: conexión rechazada"
        );
    }

    #[test]
    fn pool_error_backend_expone_el_error_subyacente() {
        let error = DbError::from_pool_error(
            "No se pudo conectar a PostgreSQL",
            PoolError::Backend(error_de_configuracion()),
        );

        assert_eq!(
            error.message,
            "No se pudo conectar a PostgreSQL: invalid connection string: \
             invalid value for option `port`"
        );
        assert_eq!(error.code, None);
        assert!(!error.message.contains("db error"));
    }

    #[test]
    fn pool_error_timeout_y_estado_son_legibles() {
        let casos = [
            (
                PoolError::Timeout(TimeoutType::Wait),
                "No se pudo obtener una conexión: se agotó el tiempo de espera \
                 para obtener una conexión del pool",
            ),
            (
                PoolError::Timeout(TimeoutType::Create),
                "No se pudo obtener una conexión: se agotó el tiempo de espera \
                 al crear una conexión",
            ),
            (
                PoolError::Timeout(TimeoutType::Recycle),
                "No se pudo obtener una conexión: se agotó el tiempo de espera \
                 al reciclar una conexión",
            ),
            (
                PoolError::Closed,
                "No se pudo obtener una conexión: el pool de conexiones está cerrado",
            ),
            (
                PoolError::NoRuntimeSpecified,
                "No se pudo obtener una conexión: el pool de conexiones no tiene un \
                 runtime asociado",
            ),
            (
                PoolError::PostCreateHook(HookError::message("hook roto")),
                "No se pudo obtener una conexión: el hook post_create del pool falló: \
                 hook roto",
            ),
        ];

        for (error, esperado) in casos {
            let mapped = DbError::from_pool_error("No se pudo obtener una conexión", error);
            assert_eq!(mapped.message, esperado);
            assert_eq!(mapped.code, None);
        }
    }

    // ------------------------------------------------------------------
    // Tests de integración: requieren TROMPA_TEST_DATABASE_URL.
    //
    // Si la variable no está definida los tests fallan de forma explícita:
    // un `ok` silencioso haría creer que se probó la conexión real.
    //
    //   TROMPA_TEST_DATABASE_URL=postgresql://postgres@127.0.0.1:5432/postgres?sslmode=disable \
    //     cargo test --lib --manifest-path src-tauri/Cargo.toml
    // ------------------------------------------------------------------

    /// Devuelve la URL de la base de datos de pruebas.
    ///
    /// Falla con un mensaje claro si la variable no está configurada, para
    /// que los tests de integración nunca se omitan sin avisar.
    fn test_database_url() -> String {
        let url = std::env::var("TROMPA_TEST_DATABASE_URL").unwrap_or_default();
        assert!(
            !url.trim().is_empty(),
            "TROMPA_TEST_DATABASE_URL no está definida: los tests de integración \
             necesitan un PostgreSQL real (por ejemplo \
             postgresql://postgres@127.0.0.1:5432/postgres?sslmode=disable)"
        );
        url.trim().to_string()
    }

    /// Separa la URL de pruebas en los campos que manda la UI.
    fn fields_from_url(url: &str) -> (String, u16, String, String, String) {
        let config =
            PgConfig::from_str(url).expect("TROMPA_TEST_DATABASE_URL debe ser una URL válida");

        let host = match config.get_hosts().first() {
            Some(Host::Tcp(host)) => host.clone(),
            _ => panic!("TROMPA_TEST_DATABASE_URL debe apuntar a un host TCP"),
        };
        let port = config.get_ports().first().copied().unwrap_or(5432);
        let database = config.get_dbname().unwrap_or("postgres").to_string();
        let user = config.get_user().unwrap_or("postgres").to_string();
        let password = config
            .get_password()
            .map(|password| String::from_utf8_lossy(password).into_owned())
            .unwrap_or_default();

        (host, port, database, user, password)
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
        let url = test_database_url();

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

    /// Regresión del camino exacto de la UI: `Fields` con los valores por
    /// defecto del formulario y `sslmode: prefer` contra el PostgreSQL de
    /// devbox. Antes este camino no estaba cubierto y un fallo real se
    /// mostraba como el genérico "db error".
    #[tokio::test]
    async fn integracion_connect_with_fields_prefer_localhost_como_la_ui() {
        let url = test_database_url();
        let (_, port, database, user, password) = fields_from_url(&url);

        let state = DbState::default();
        let info = connect_with(
            ConnectRequest::Fields {
                host: "localhost".to_string(),
                port,
                database: database.clone(),
                user: user.clone(),
                password,
                sslmode: SslModeParam::Prefer,
                sslrootcert: None,
            },
            &state,
        )
        .await
        .expect("el camino Fields + sslmode=prefer de la UI debía conectar");

        assert_eq!(info.database, database);
        assert_eq!(info.user, user);
        assert!(state.pool().is_ok(), "el pool debe quedar disponible");

        let result = run_query_with(&state, "SELECT current_database()::text")
            .await
            .expect("la consulta debía funcionar tras conectar");
        assert_eq!(
            result.rows,
            vec![vec![Some(database)]],
            "la conexión debía quedar en la base elegida"
        );
    }

    /// El mismo camino de la UI, pero con el host que indique la URL de
    /// pruebas, para que el test sirva también fuera de devbox.
    #[tokio::test]
    async fn integracion_connect_with_fields_prefer() {
        let url = test_database_url();
        let (host, port, database, user, password) = fields_from_url(&url);

        let state = DbState::default();
        let info = connect_with(
            ConnectRequest::Fields {
                host,
                port,
                database: database.clone(),
                user,
                password,
                sslmode: SslModeParam::Prefer,
                sslrootcert: None,
            },
            &state,
        )
        .await
        .expect("el camino Fields + sslmode=prefer debía conectar");

        assert_eq!(info.database, database);
        assert!(state.pool().is_ok());
    }

    /// Si la base de datos indicada no existe, el frontend debe recibir el
    /// mensaje y el SQLSTATE reales de PostgreSQL, no "db error".
    #[tokio::test]
    async fn integracion_connect_with_base_inexistente_expone_mensaje_y_sqlstate() {
        let url = test_database_url();
        let (host, port, _, user, password) = fields_from_url(&url);
        let database = format!("trompa_inexistente_{}", std::process::id());

        let error = connect_with(
            ConnectRequest::Fields {
                host,
                port,
                database: database.clone(),
                user,
                password,
                sslmode: SslModeParam::Prefer,
                sslrootcert: None,
            },
            &DbState::default(),
        )
        .await
        .expect_err("la base de datos no existe");

        assert_eq!(error.code.as_deref(), Some("3D000"));
        assert!(
            error.message.contains(&database),
            "el mensaje debía nombrar la base: {}",
            error.message
        );
        assert!(
            !error.message.contains("db error"),
            "el mensaje genérico de deadpool no debe filtrarse: {}",
            error.message
        );
    }

    /// Un usuario inexistente también debe reportarse con su SQLSTATE.
    #[tokio::test]
    async fn integracion_connect_with_usuario_inexistente_expone_sqlstate() {
        let url = test_database_url();
        let (host, port, database, _, password) = fields_from_url(&url);
        let user = format!("trompa_usuario_{}", std::process::id());

        let error = connect_with(
            ConnectRequest::Fields {
                host,
                port,
                database,
                user: user.clone(),
                password,
                sslmode: SslModeParam::Prefer,
                sslrootcert: None,
            },
            &DbState::default(),
        )
        .await
        .expect_err("el usuario no existe");

        assert_eq!(error.code.as_deref(), Some("28000"));
        assert!(
            error.message.contains(&user),
            "el mensaje debía nombrar el usuario: {}",
            error.message
        );
    }

    #[tokio::test]
    async fn integracion_run_query_with_devuelve_columnas_y_nulos() {
        let url = test_database_url();
        let state = connected_state(&url).await;

        let result = run_query_with(&state, "SELECT 1::text AS n, NULL::text AS vacio")
            .await
            .expect("la consulta debía funcionar");

        assert_eq!(result.columns, vec!["n", "vacio"]);
        assert_eq!(result.rows, vec![vec![Some("1".to_owned()), None]]);
    }

    #[tokio::test]
    async fn integracion_run_query_with_cuenta_filas() {
        let url = test_database_url();
        let state = connected_state(&url).await;

        let result = run_query_with(&state, "SELECT generate_series(1,3)")
            .await
            .expect("la consulta debía funcionar");

        assert_eq!(result.rows.len(), 3);
        assert_eq!(result.rows_affected, Some(3));
    }

    #[tokio::test]
    async fn integracion_error_sql_expone_codigo_y_mensaje() {
        let url = test_database_url();
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
        let url = test_database_url();
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
        let url = test_database_url();
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
        let url = test_database_url();
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
        let url = test_database_url();
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
        let url = test_database_url();
        let state = connected_state(&url).await;
        let (tx, mut rx) = event_channel();

        let started = Instant::now();
        // `generate_series` envía el `RowDescription` en cuanto empieza a
        // ejecutarse, así que hay columnas antes de cancelar. Con
        // `pg_sleep` el servidor no manda descripción hasta terminar.
        let query_id = start_query_with(
            &state,
            "SELECT generate_series(1, 1000000000)",
            move |event| {
                let _ = tx.send(event);
            },
        )
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

    // ------------------------------------------------------------------
    // Catálogo de esquema.
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn list_schema_sin_pool_devuelve_error() {
        let state = DbState::default();

        let error = list_schema_with(&state)
            .await
            .expect_err("sin conexión el catálogo debía fallar");

        assert_eq!(error.message, "No hay ninguna conexión activa");
        assert_eq!(error.code, None);
    }

    #[test]
    fn serializa_catalogo_en_camel_case() {
        let catalog = SchemaCatalog {
            database: "trompa".to_string(),
            schemas: vec![SchemaInfo {
                name: "public".to_string(),
                tables: vec![SchemaObject {
                    oid: 16384,
                    name: "clientes".to_string(),
                }],
                views: vec![SchemaObject {
                    oid: 16400,
                    name: "vista_clientes".to_string(),
                }],
                functions: vec![SchemaFunction {
                    oid: 16500,
                    name: "fn_total".to_string(),
                    signature: "integer".to_string(),
                }],
                enums: vec![SchemaObject {
                    oid: 16600,
                    name: "estado".to_string(),
                }],
            }],
        };

        let value = serde_json::to_value(&catalog).expect("el catálogo debía serializarse");

        assert_eq!(
            value,
            json!({
                "database": "trompa",
                "schemas": [
                    {
                        "name": "public",
                        "tables": [{ "oid": 16384, "name": "clientes" }],
                        "views": [{ "oid": 16400, "name": "vista_clientes" }],
                        "functions": [
                            { "oid": 16500, "name": "fn_total", "signature": "integer" }
                        ],
                        "enums": [{ "oid": 16600, "name": "estado" }]
                    }
                ]
            })
        );
    }

    /// Crea un cliente propio para las sentencias de fixture.
    async fn fixture_client(state: &DbState) -> Object {
        state
            .pool()
            .expect("debía haber un pool activo")
            .get()
            .await
            .expect("debía obtener una conexión para el fixture")
    }

    #[tokio::test]
    async fn integracion_list_schema_incluye_tablas_vistas_funciones_y_enums() {
        let url = test_database_url();
        let state = connected_state(&url).await;

        let schema = format!("trompa_catalogo_{}", std::process::id());
        assert!(
            schema
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
            "el nombre del esquema de pruebas solo debe contener [a-z0-9_]"
        );

        let client = fixture_client(&state).await;
        client
            .batch_execute(&format!(
                "DROP SCHEMA IF EXISTS {schema} CASCADE;
                 CREATE SCHEMA {schema};
                 CREATE TABLE {schema}.tabla_prueba (id integer);
                 CREATE VIEW {schema}.vista_prueba AS SELECT 1::integer AS n;
                 CREATE TYPE {schema}.estado_prueba AS ENUM ('activo','inactivo');
                 CREATE FUNCTION {schema}.fn_prueba(integer) RETURNS integer
                   LANGUAGE sql IMMUTABLE AS 'SELECT $1';"
            ))
            .await
            .expect("debía crear los objetos del esquema de pruebas");
        drop(client);

        let catalog = list_schema_with(&state)
            .await
            .expect("list_schema_with debía funcionar");

        // El catálogo ya está en memoria: limpiar antes de los asserts deja la
        // base intacta aunque alguno falle.
        let client = fixture_client(&state).await;
        client
            .batch_execute(&format!("DROP SCHEMA IF EXISTS {schema} CASCADE"))
            .await
            .expect("debía limpiar el esquema de pruebas");
        drop(client);

        assert!(!catalog.database.is_empty(), "database no debe estar vacío");

        for item in &catalog.schemas {
            assert_ne!(item.name, "information_schema");
            assert!(
                !item.name.starts_with("pg_"),
                "no debe aparecer el esquema del sistema {}",
                item.name
            );
        }

        let first = catalog
            .schemas
            .first()
            .expect("el catálogo debía traer al menos un esquema");
        assert_eq!(first.name, "public", "public debe ir primero");
        assert!(
            first.tables.iter().any(|table| table.name == "clientes"),
            "public debía contener la tabla sembrada clientes"
        );

        let found = catalog
            .schemas
            .iter()
            .find(|item| item.name == schema)
            .expect("el esquema de pruebas debía aparecer en el catálogo");

        assert!(
            found
                .tables
                .iter()
                .any(|table| table.name == "tabla_prueba"),
            "debía listarse tabla_prueba como tabla"
        );
        assert!(
            found.views.iter().any(|view| view.name == "vista_prueba"),
            "debía listarse vista_prueba como vista"
        );
        assert!(
            found.enums.iter().any(|item| item.name == "estado_prueba"),
            "debía listarse estado_prueba como enum"
        );

        let function = found
            .functions
            .iter()
            .find(|function| function.name == "fn_prueba")
            .expect("debía listarse fn_prueba como función");
        assert_eq!(function.signature, "integer");
    }

    // ------------------------------------------------------------------
    // Registros paginados.
    // ------------------------------------------------------------------

    #[test]
    fn cita_identificadores() {
        assert_eq!(
            table_ident("pub\"lic", "tab la"),
            "\"pub\"\"lic\".\"tab la\""
        );
    }

    #[tokio::test]
    async fn integracion_fetch_table_page_pagina_y_total() {
        let url = test_database_url();
        let state = connected_state(&url).await;
        let tabla = format!("trompa_test_paginado_{}", std::process::id());

        let client = fixture_client(&state).await;
        client
            .batch_execute(&format!(
                "DROP TABLE IF EXISTS public.{tabla};
                 CREATE TABLE public.{tabla} (id integer PRIMARY KEY, nombre text);
                 INSERT INTO public.{tabla} (id, nombre)
                   SELECT serie.g, 'fila ' || serie.g
                   FROM generate_series(1, 250) AS serie(g);"
            ))
            .await
            .expect("debía crear la tabla de pruebas");
        drop(client);

        let primera = fetch_table_page_with(&state, "public", &tabla, 1, 100)
            .await
            .expect("la primera página debía cargarse");
        assert_eq!(primera.total, 250);
        assert_eq!(primera.page, 1);
        assert_eq!(primera.page_size, 100);
        assert_eq!(primera.rows.len(), 100);
        assert!(
            primera.columns.contains(&"id".to_string()),
            "las columnas debían incluir id: {:?}",
            primera.columns
        );
        assert!(
            primera.columns.contains(&"nombre".to_string()),
            "las columnas debían incluir nombre: {:?}",
            primera.columns
        );

        let tercera = fetch_table_page_with(&state, "public", &tabla, 3, 100)
            .await
            .expect("la tercera página debía cargarse");
        assert_eq!(tercera.page, 3);
        assert_eq!(tercera.rows.len(), 50);

        // Una página fuera de rango se recorta a la última.
        let recortada = fetch_table_page_with(&state, "public", &tabla, 9, 100)
            .await
            .expect("la página fuera de rango debía recortarse");
        assert_eq!(recortada.page, 3);
        assert_eq!(recortada.rows.len(), 50);

        let una_fila = fetch_table_page_with(&state, "public", &tabla, 1, 0)
            .await
            .expect("page_size cero debía limitarse");
        assert_eq!(una_fila.page_size, 1);
        assert_eq!(una_fila.rows.len(), 1);

        let client = fixture_client(&state).await;
        client
            .batch_execute(&format!("DROP TABLE IF EXISTS public.{tabla}"))
            .await
            .expect("debía limpiar la tabla de pruebas");
    }

    #[tokio::test]
    async fn integracion_fetch_table_page_lee_vistas() {
        let url = test_database_url();
        let state = connected_state(&url).await;
        let tabla = format!("trompa_test_paginado_vista_{}", std::process::id());
        let vista = format!("trompa_test_vista_{}", std::process::id());

        let client = fixture_client(&state).await;
        client
            .batch_execute(&format!(
                "DROP VIEW IF EXISTS public.{vista};
                 DROP TABLE IF EXISTS public.{tabla};
                 CREATE TABLE public.{tabla} (id integer PRIMARY KEY, nombre text);
                 INSERT INTO public.{tabla} (id, nombre)
                   SELECT serie.g, 'fila ' || serie.g
                   FROM generate_series(1, 250) AS serie(g);
                 CREATE VIEW public.{vista} AS SELECT id, nombre FROM public.{tabla};"
            ))
            .await
            .expect("debía crear la vista de pruebas");
        drop(client);

        let pagina = fetch_table_page_with(&state, "public", &vista, 1, 100)
            .await
            .expect("la vista debía paginarse");
        assert_eq!(pagina.total, 250);
        assert_eq!(pagina.rows.len(), 100);
        assert_eq!(pagina.columns, vec!["id".to_string(), "nombre".to_string()]);

        let client = fixture_client(&state).await;
        client
            .batch_execute(&format!(
                "DROP VIEW IF EXISTS public.{vista};
                 DROP TABLE IF EXISTS public.{tabla}"
            ))
            .await
            .expect("debía limpiar la vista y la tabla");
    }

    #[tokio::test]
    async fn integracion_fetch_table_page_tabla_inexistente_devuelve_error() {
        let url = test_database_url();
        let state = connected_state(&url).await;
        let tabla = format!("trompa_test_inexistente_{}", std::process::id());

        let error = fetch_table_page_with(&state, "public", &tabla, 1, 100)
            .await
            .expect_err("la tabla no existe");

        assert_eq!(error.code, Some("42P01".to_string()));
    }
}
