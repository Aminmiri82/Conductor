mod connection;
mod migrations;

use std::{
    ffi::c_int,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::Instant,
};

use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use thiserror::Error;

const LEARNED_ROWID_MODEL_ERROR: i32 = 16;
const LEARNED_ROWID_TARGETS: &[(&str, &str)] = &[
    ("requests", "id"),
    ("collection_nodes", "id"),
    ("request_history", "id"),
];

#[derive(Clone)]
pub struct Database {
    write_connection: Arc<Mutex<Connection>>,
    read_connections: Arc<Vec<Mutex<Connection>>>,
    next_read_connection: Arc<AtomicUsize>,
    path: PathBuf,
    raw_import_dir: PathBuf,
}

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("failed to create app data directory at {path}: {source}")]
    CreateDirectory {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to open sqlite database at {path}: {source}")]
    OpenDatabase {
        path: PathBuf,
        source: rusqlite::Error,
    },
    #[error("failed to access sqlite connection")]
    ConnectionPoisoned,
    #[error("sqlite operation failed: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("json operation failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("file operation failed at {path}: {source}")]
    FileOperation {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("cannot move a folder into itself or one of its descendants")]
    InvalidTreeMove,
    #[error("{0}")]
    InvalidInput(String),
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseStatus {
    pub path: String,
    pub schema_version: i64,
    pub collection_count: i64,
    pub request_count: i64,
    pub learned_rowid_enabled: bool,
    pub learned_rowid_table_moveto_calls: u64,
    pub learned_rowid_attempted: u64,
    pub learned_rowid_fallback: u64,
    pub learned_rowid_exact_first_probe: u64,
    pub learned_rowid_comparisons: u64,
    pub learned_rowid_model_count: u64,
    pub learned_rowid_model_segments: u64,
    pub learned_rowid_model_bytes: u64,
    pub learned_rowid_model_predictions: u64,
    pub learned_rowid_predicted_slot_average: f64,
    pub learned_rowid_prediction_error_average: f64,
    pub learned_rowid_prediction_error_max: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LearnedRowidStatsSnapshot {
    pub table_moveto_calls: u64,
    pub attempted: u64,
    pub fallback: u64,
    pub exact_first_probe: u64,
    pub comparisons: u64,
    pub model_count: u64,
    pub model_segments: u64,
    pub model_bytes: u64,
    pub model_predictions: u64,
    pub predicted_slot_average: f64,
    pub prediction_error_average: f64,
    pub prediction_error_max: u64,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct LearnedRowidModelSegment {
    first_key: i64,
    slope: f64,
    intercept: f64,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct StoredLearnedRowidModel {
    segments: Vec<LearnedRowidModelSegment>,
    row_count: i64,
    max_error: i32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LearnedRowidBenchmarkRun {
    pub enabled: bool,
    pub lookup_count: u64,
    pub elapsed_micros: u64,
    pub comparisons_per_lookup: f64,
    pub stats: LearnedRowidStatsSnapshot,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LearnedRowidBenchmarkScenario {
    pub name: String,
    pub row_count: i64,
    pub disabled: LearnedRowidBenchmarkRun,
    pub enabled: LearnedRowidBenchmarkRun,
    pub comparison_delta: i64,
    pub comparison_delta_percent: f64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LearnedRowidBenchmark {
    pub scenarios: Vec<LearnedRowidBenchmarkScenario>,
    pub status: DatabaseStatus,
}

impl Database {
    pub fn open(app_data_dir: impl AsRef<Path>) -> Result<Self, StorageError> {
        let app_data_dir = app_data_dir.as_ref();
        let raw_import_dir = app_data_dir.join("raw-imports");
        let (write_connection, read_connections, path) = connection::open(app_data_dir)?;

        let database = Self {
            write_connection: Arc::new(Mutex::new(write_connection)),
            read_connections: Arc::new(read_connections.into_iter().map(Mutex::new).collect()),
            next_read_connection: Arc::new(AtomicUsize::new(0)),
            path,
            raw_import_dir,
        };
        database.load_persisted_learned_rowid_models()?;
        Ok(database)
    }

    pub fn status(&self) -> Result<DatabaseStatus, StorageError> {
        let connection = self.read_connection()?;
        let schema_version =
            connection.query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))?;
        let collection_count = count_rows(&connection, "collections")?;
        let request_count = count_rows(&connection, "requests")?;
        let learned_rowid = learned_rowid_stats();

        Ok(DatabaseStatus {
            path: self.path.display().to_string(),
            schema_version,
            collection_count,
            request_count,
            learned_rowid_enabled: learned_rowid_enabled(),
            learned_rowid_table_moveto_calls: learned_rowid.table_moveto_calls,
            learned_rowid_attempted: learned_rowid.attempted,
            learned_rowid_fallback: learned_rowid.fallback,
            learned_rowid_exact_first_probe: learned_rowid.exact_first_probe,
            learned_rowid_comparisons: learned_rowid.comparisons,
            learned_rowid_model_count: learned_rowid.model_count,
            learned_rowid_model_segments: learned_rowid.model_segments,
            learned_rowid_model_bytes: learned_rowid.model_bytes,
            learned_rowid_model_predictions: learned_rowid.model_predictions,
            learned_rowid_predicted_slot_average: learned_rowid.predicted_slot_average,
            learned_rowid_prediction_error_average: learned_rowid.prediction_error_average,
            learned_rowid_prediction_error_max: learned_rowid.prediction_error_max,
        })
    }

    pub fn benchmark_learned_rowid(&self) -> Result<LearnedRowidBenchmark, StorageError> {
        const ROW_COUNT: i64 = 5_000;
        const LOOKUP_COUNT: u64 = 4_096;

        let mut connection = self.write_connection()?;
        let was_enabled = learned_rowid_enabled();
        let scenarios = [
            ("dense", dense_probe_keys(ROW_COUNT)),
            ("sparse quadratic", sparse_quadratic_probe_keys(ROW_COUNT)),
        ]
        .into_iter()
        .map(|(name, keys)| {
            run_learned_rowid_benchmark_scenario(&mut connection, name, &keys, LOOKUP_COUNT)
        })
        .collect::<Result<Vec<_>, _>>()?;
        set_learned_rowid_enabled(was_enabled);
        load_persisted_learned_rowid_models_from_connection(&connection)?;

        Ok(LearnedRowidBenchmark {
            scenarios,
            status: self.status()?,
        })
    }

    pub fn rebuild_learned_rowid_models(&self) -> Result<DatabaseStatus, StorageError> {
        let connection = self.write_connection()?;
        sqlite_clear_learned_rowid_models();
        for (table_name, column_name) in LEARNED_ROWID_TARGETS {
            build_store_and_register_learned_rowid_model(&connection, table_name, column_name)?;
        }
        self.status()
    }

    pub fn with_connection<T>(
        &self,
        operation: impl FnOnce(&mut Connection) -> Result<T, StorageError>,
    ) -> Result<T, StorageError> {
        let mut connection = self.write_connection()?;
        operation(&mut connection)
    }

    pub fn with_read_connection<T>(
        &self,
        operation: impl FnOnce(&Connection) -> Result<T, StorageError>,
    ) -> Result<T, StorageError> {
        let connection = self.read_connection()?;
        operation(&connection)
    }

    pub fn raw_import_dir(&self) -> &Path {
        &self.raw_import_dir
    }

    fn write_connection(&self) -> Result<std::sync::MutexGuard<'_, Connection>, StorageError> {
        self.write_connection
            .lock()
            .map_err(|_| StorageError::ConnectionPoisoned)
    }

    fn read_connection(&self) -> Result<std::sync::MutexGuard<'_, Connection>, StorageError> {
        let index =
            self.next_read_connection.fetch_add(1, Ordering::Relaxed) % self.read_connections.len();
        self.read_connections[index]
            .lock()
            .map_err(|_| StorageError::ConnectionPoisoned)
    }

    fn load_persisted_learned_rowid_models(&self) -> Result<(), StorageError> {
        let connection = self.write_connection()?;
        load_persisted_learned_rowid_models_from_connection(&connection)
    }
}

fn count_rows(connection: &Connection, table_name: &str) -> Result<i64, rusqlite::Error> {
    connection.query_row(
        &format!("SELECT COUNT(*) FROM {table_name}"),
        params![],
        |row| row.get(0),
    )
}

fn build_store_and_register_learned_rowid_model(
    connection: &Connection,
    table_name: &str,
    column_name: &str,
) -> Result<(), StorageError> {
    let root_page = table_root_page(connection, "main", table_name)?.ok_or_else(|| {
        StorageError::InvalidInput(format!("missing table for learned model: {table_name}"))
    })?;
    let keys = load_sorted_rowid_keys(connection, table_name, column_name)?;
    if keys.is_empty() {
        connection.execute(
            "DELETE FROM learned_index_models WHERE table_name = ?1 AND column_name = ?2",
            params![table_name, column_name],
        )?;
        return Ok(());
    }

    let model = StoredLearnedRowidModel {
        segments: build_pgm_segments(&keys, LEARNED_ROWID_MODEL_ERROR),
        row_count: keys.len() as i64,
        max_error: LEARNED_ROWID_MODEL_ERROR,
    };
    let model_json = serde_json::to_string(&model)?;
    let now = Utc::now().to_rfc3339();
    connection.execute(
        "INSERT INTO learned_index_models (
            table_name, column_name, root_page, model_json, row_count, max_error, built_at
         )
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(table_name, column_name) DO UPDATE SET
            root_page = excluded.root_page,
            model_json = excluded.model_json,
            row_count = excluded.row_count,
            max_error = excluded.max_error,
            built_at = excluded.built_at",
        params![
            table_name,
            column_name,
            root_page,
            model_json,
            model.row_count,
            model.max_error,
            now
        ],
    )?;
    register_learned_rowid_model(root_page, &model.segments, model.row_count, model.max_error)
}

fn load_persisted_learned_rowid_models_from_connection(
    connection: &Connection,
) -> Result<(), StorageError> {
    sqlite_clear_learned_rowid_models();
    let mut statement = connection.prepare(
        "SELECT table_name, column_name, root_page, model_json, row_count, max_error
         FROM learned_index_models
         ORDER BY table_name, column_name",
    )?;
    let models = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, u32>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, i64>(4)?,
            row.get::<_, i32>(5)?,
        ))
    })?;

    for model in models {
        let (table_name, _column_name, root_page, model_json, row_count, max_error) = model?;
        if table_root_page(connection, "main", &table_name)? != Some(root_page) {
            continue;
        }
        let stored: StoredLearnedRowidModel = serde_json::from_str(&model_json)?;
        if stored.row_count != row_count || stored.max_error != max_error {
            continue;
        }
        register_learned_rowid_model(
            root_page,
            &stored.segments,
            stored.row_count,
            stored.max_error,
        )?;
    }
    Ok(())
}

fn table_root_page(
    connection: &Connection,
    schema_name: &str,
    table_name: &str,
) -> Result<Option<u32>, StorageError> {
    let schema = match schema_name {
        "main" => "main",
        "temp" => "temp",
        _ => {
            return Err(StorageError::InvalidInput(
                "invalid schema name".to_string(),
            ))
        }
    };
    let sql =
        format!("SELECT rootpage FROM {schema}.sqlite_schema WHERE type = 'table' AND name = ?1");
    connection
        .query_row(&sql, params![table_name], |row| row.get::<_, u32>(0))
        .optional()
        .map_err(Into::into)
}

fn load_sorted_rowid_keys(
    connection: &Connection,
    table_name: &str,
    column_name: &str,
) -> Result<Vec<i64>, StorageError> {
    let table = quoted_learned_rowid_table_name(table_name)?;
    if column_name != "id" {
        return Err(StorageError::InvalidInput(format!(
            "unsupported learned rowid column: {column_name}"
        )));
    }

    let mut statement = connection.prepare(&format!("SELECT id FROM {table} ORDER BY id"))?;
    let keys = statement
        .query_map([], |row| row.get::<_, i64>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(keys)
}

fn quoted_learned_rowid_table_name(table_name: &str) -> Result<&'static str, StorageError> {
    match table_name {
        "requests" => Ok("\"requests\""),
        "collection_nodes" => Ok("\"collection_nodes\""),
        "request_history" => Ok("\"request_history\""),
        "learned_rowid_probe" => Ok("\"learned_rowid_probe\""),
        _ => Err(StorageError::InvalidInput(format!(
            "unsupported learned rowid table: {table_name}"
        ))),
    }
}

fn build_pgm_segments(keys: &[i64], max_error: i32) -> Vec<LearnedRowidModelSegment> {
    if keys.is_empty() {
        return Vec::new();
    }
    if keys.len() == 1 {
        return vec![LearnedRowidModelSegment {
            first_key: keys[0],
            slope: 0.0,
            intercept: 0.0,
        }];
    }

    let error = f64::from(max_error.max(0));
    let mut segments = Vec::new();
    let mut start = 0_usize;

    while start < keys.len() {
        let start_key = keys[start];
        let start_pos = start as f64;
        let mut lower_slope = f64::NEG_INFINITY;
        let mut upper_slope = f64::INFINITY;
        let mut end = start;
        let mut index = start + 1;

        while index < keys.len() {
            let key_delta = (keys[index] - start_key) as f64;
            if key_delta <= 0.0 {
                break;
            }
            let pos = index as f64;
            let min_slope = (pos - error - start_pos) / key_delta;
            let max_slope = (pos + error - start_pos) / key_delta;
            let next_lower = lower_slope.max(min_slope);
            let next_upper = upper_slope.min(max_slope);
            if next_lower > next_upper {
                break;
            }
            lower_slope = next_lower;
            upper_slope = next_upper;
            end = index;
            index += 1;
        }

        if end == start {
            end = (start + 1).min(keys.len() - 1);
        }

        let end_key = keys[end];
        let slope = if end_key == start_key {
            0.0
        } else {
            (end as f64 - start_pos) / (end_key - start_key) as f64
        };
        segments.push(LearnedRowidModelSegment {
            first_key: start_key,
            slope,
            intercept: start_pos - slope * start_key as f64,
        });

        if end >= keys.len() - 1 {
            break;
        }
        start = end;
    }

    segments
}

fn register_learned_rowid_probe_model(
    connection: &Connection,
    keys: &[i64],
) -> Result<(), StorageError> {
    let root_page =
        table_root_page(connection, "temp", "learned_rowid_probe")?.ok_or_else(|| {
            StorageError::InvalidInput("missing learned rowid benchmark table".to_string())
        })?;
    let segments = build_pgm_segments(keys, LEARNED_ROWID_MODEL_ERROR);
    register_learned_rowid_model(
        root_page,
        &segments,
        keys.len() as i64,
        LEARNED_ROWID_MODEL_ERROR,
    )
}

fn register_learned_rowid_model(
    root_page: u32,
    segments: &[LearnedRowidModelSegment],
    row_count: i64,
    max_error: i32,
) -> Result<(), StorageError> {
    if segments.is_empty() {
        return Ok(());
    }
    let result = unsafe {
        sqlite3_learned_rowid_register_model(
            root_page,
            segments.as_ptr(),
            segments.len().try_into().unwrap_or(i32::MAX),
            row_count,
            max_error,
        )
    };
    if result == 0 {
        Ok(())
    } else {
        Err(StorageError::InvalidInput(format!(
            "sqlite learned rowid model registration failed with code {result}"
        )))
    }
}

fn run_learned_rowid_benchmark_scenario(
    connection: &mut Connection,
    name: &str,
    keys: &[i64],
    lookup_count: u64,
) -> Result<LearnedRowidBenchmarkScenario, StorageError> {
    rebuild_learned_rowid_probe_table(connection, keys)?;
    let disabled = run_learned_rowid_probe(connection, false, keys, lookup_count)?;
    let enabled = run_learned_rowid_probe(connection, true, keys, lookup_count)?;
    let comparison_delta = enabled.stats.comparisons as i64 - disabled.stats.comparisons as i64;
    let comparison_delta_percent = if disabled.stats.comparisons == 0 {
        0.0
    } else {
        (comparison_delta as f64 / disabled.stats.comparisons as f64) * 100.0
    };

    Ok(LearnedRowidBenchmarkScenario {
        name: name.to_string(),
        row_count: keys.len() as i64,
        disabled,
        enabled,
        comparison_delta,
        comparison_delta_percent,
    })
}

fn rebuild_learned_rowid_probe_table(
    connection: &mut Connection,
    keys: &[i64],
) -> Result<(), StorageError> {
    connection.execute_batch(
        "DROP TABLE IF EXISTS temp.learned_rowid_probe;
         CREATE TEMP TABLE learned_rowid_probe(
            id INTEGER PRIMARY KEY,
            payload TEXT NOT NULL
         );",
    )?;
    let transaction = connection.transaction()?;
    {
        let mut insert = transaction
            .prepare("INSERT INTO temp.learned_rowid_probe(id, payload) VALUES (?1, ?2)")?;
        for id in keys {
            insert.execute(params![id, format!("request-{id}")])?;
        }
    }
    transaction.commit()?;
    register_learned_rowid_probe_model(connection, keys)?;
    Ok(())
}

fn run_learned_rowid_probe(
    connection: &Connection,
    enabled: bool,
    keys: &[i64],
    lookup_count: u64,
) -> Result<LearnedRowidBenchmarkRun, StorageError> {
    set_learned_rowid_enabled(enabled);
    reset_learned_rowid_stats();

    let started = Instant::now();
    {
        let mut lookup =
            connection.prepare("SELECT payload FROM temp.learned_rowid_probe WHERE id = ?1")?;
        for index in 0..lookup_count {
            let key_index = ((index as usize) * 7_919) % keys.len();
            let id = keys[key_index];
            let _: String = lookup.query_row(params![id], |row| row.get(0))?;
        }
    }
    let elapsed_micros = started.elapsed().as_micros().try_into().unwrap_or(u64::MAX);
    let stats = learned_rowid_stats();
    let comparisons_per_lookup = if lookup_count == 0 {
        0.0
    } else {
        stats.comparisons as f64 / lookup_count as f64
    };

    Ok(LearnedRowidBenchmarkRun {
        enabled,
        lookup_count,
        elapsed_micros,
        comparisons_per_lookup,
        stats,
    })
}

fn dense_probe_keys(row_count: i64) -> Vec<i64> {
    (1..=row_count).collect()
}

fn sparse_quadratic_probe_keys(row_count: i64) -> Vec<i64> {
    (1..=row_count).map(|id| id * id * 3 + id * 11).collect()
}

extern "C" {
    fn sqlite3_learned_rowid_reset();
    fn sqlite3_learned_rowid_enable(enabled: std::ffi::c_int);
    fn sqlite3_learned_rowid_enabled() -> std::ffi::c_int;
    fn sqlite3_learned_rowid_clear_models();
    fn sqlite3_learned_rowid_register_model(
        root_page: u32,
        segments: *const LearnedRowidModelSegment,
        segment_count: c_int,
        row_count: i64,
        max_error: c_int,
    ) -> c_int;
    fn sqlite3_learned_rowid_stats(
        table_moveto_calls: *mut u64,
        attempted: *mut u64,
        fallback: *mut u64,
        exact_first_probe: *mut u64,
        comparisons: *mut u64,
    );
    fn sqlite3_learned_rowid_model_stats(
        model_count: *mut u64,
        model_segments: *mut u64,
        model_bytes: *mut u64,
        model_predictions: *mut u64,
        predicted_slot_sum: *mut u64,
        prediction_error_sum: *mut u64,
        prediction_error_max: *mut u64,
    );
}

pub fn set_learned_rowid_enabled(enabled: bool) {
    unsafe {
        sqlite3_learned_rowid_enable(enabled as std::ffi::c_int);
    }
}

pub fn reset_learned_rowid_stats() {
    unsafe {
        sqlite3_learned_rowid_reset();
    }
}

fn sqlite_clear_learned_rowid_models() {
    unsafe {
        sqlite3_learned_rowid_clear_models();
    }
}

fn learned_rowid_enabled() -> bool {
    unsafe { sqlite3_learned_rowid_enabled() != 0 }
}

fn learned_rowid_stats() -> LearnedRowidStatsSnapshot {
    let mut table_moveto_calls = 0;
    let mut attempted = 0;
    let mut fallback = 0;
    let mut exact_first_probe = 0;
    let mut comparisons = 0;
    let mut model_count = 0;
    let mut model_segments = 0;
    let mut model_bytes = 0;
    let mut model_predictions = 0;
    let mut predicted_slot_sum = 0;
    let mut prediction_error_sum = 0;
    let mut prediction_error_max = 0;
    unsafe {
        sqlite3_learned_rowid_stats(
            &mut table_moveto_calls,
            &mut attempted,
            &mut fallback,
            &mut exact_first_probe,
            &mut comparisons,
        );
        sqlite3_learned_rowid_model_stats(
            &mut model_count,
            &mut model_segments,
            &mut model_bytes,
            &mut model_predictions,
            &mut predicted_slot_sum,
            &mut prediction_error_sum,
            &mut prediction_error_max,
        );
    }
    let predicted_slot_average = if model_predictions == 0 {
        0.0
    } else {
        predicted_slot_sum as f64 / model_predictions as f64
    };
    let prediction_error_average = if attempted == 0 {
        0.0
    } else {
        prediction_error_sum as f64 / attempted as f64
    };
    LearnedRowidStatsSnapshot {
        table_moveto_calls,
        attempted,
        fallback,
        exact_first_probe,
        comparisons,
        model_count,
        model_segments,
        model_bytes,
        model_predictions,
        predicted_slot_average,
        prediction_error_average,
        prediction_error_max,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use rusqlite::{params, Connection};

    use super::{
        build_pgm_segments, dense_probe_keys, learned_rowid_stats,
        load_persisted_learned_rowid_models_from_connection, rebuild_learned_rowid_probe_table,
        register_learned_rowid_model, reset_learned_rowid_stats,
        run_learned_rowid_benchmark_scenario, run_learned_rowid_probe, set_learned_rowid_enabled,
        sparse_quadratic_probe_keys, sqlite_clear_learned_rowid_models, table_root_page,
        LEARNED_ROWID_MODEL_ERROR,
    };

    static LEARNED_ROWID_TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn learned_rowid_stats_count_rowid_prediction() {
        let _guard = LEARNED_ROWID_TEST_LOCK.lock().unwrap();
        sqlite_clear_learned_rowid_models();
        let connection = Connection::open_in_memory().unwrap();
        let keys = dense_probe_keys(200);
        connection
            .execute_batch(
                "CREATE TABLE learned_probe(id INTEGER PRIMARY KEY, value TEXT NOT NULL);
                 WITH RECURSIVE rows(id) AS (
                   SELECT 1
                   UNION ALL
                   SELECT id + 1 FROM rows WHERE id < 200
                 )
                 INSERT INTO learned_probe(id, value)
                 SELECT id, 'value-' || id FROM rows;",
            )
            .unwrap();
        register_test_model(&connection, "learned_probe", &keys);

        set_learned_rowid_enabled(true);
        reset_learned_rowid_stats();
        let value: String = connection
            .query_row(
                "SELECT value FROM learned_probe WHERE id = ?",
                params![137_i64],
                |row| row.get(0),
            )
            .unwrap();
        let stats = learned_rowid_stats();

        assert_eq!(value, "value-137");
        assert!(stats.table_moveto_calls > 0);
        assert!(stats.attempted > 0);
        assert!(stats.model_predictions > 0);
        assert!(stats.comparisons > 0);
        sqlite_clear_learned_rowid_models();
    }

    #[test]
    fn create_learned_index_builds_native_rowid_model() {
        let _guard = LEARNED_ROWID_TEST_LOCK.lock().unwrap();
        sqlite_clear_learned_rowid_models();
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "CREATE TABLE learned_probe(id INTEGER PRIMARY KEY, value TEXT NOT NULL);
                 WITH RECURSIVE rows(id) AS (
                   SELECT 1
                   UNION ALL
                   SELECT id + 1 FROM rows WHERE id < 200
                 )
                 INSERT INTO learned_probe(id, value)
                 SELECT id, 'value-' || id FROM rows;",
            )
            .unwrap();

        connection
            .execute(
                "CREATE LEARNED INDEX idx_learned_probe_id ON learned_probe(id)",
                [],
            )
            .unwrap();

        let (index_name, row_count, segment_count): (String, i64, i64) = connection
            .query_row(
                "SELECT index_name, row_count, json_array_length(model_json, '$.segments')
                 FROM learned_index_models
                 WHERE table_name = 'learned_probe' AND column_name = 'id'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(index_name, "idx_learned_probe_id");
        assert_eq!(row_count, 200);
        assert!(segment_count > 0);

        sqlite_clear_learned_rowid_models();
        load_persisted_learned_rowid_models_from_connection(&connection).unwrap();

        let plan: String = connection
            .query_row(
                "EXPLAIN QUERY PLAN SELECT value FROM learned_probe WHERE id = 137",
                [],
                |row| row.get(3),
            )
            .unwrap();
        assert!(plan.contains("LEARNED INTEGER PRIMARY KEY"), "{plan}");

        set_learned_rowid_enabled(true);
        reset_learned_rowid_stats();
        let value: String = connection
            .query_row(
                "SELECT value FROM learned_probe WHERE id = ?",
                params![137_i64],
                |row| row.get(0),
            )
            .unwrap();
        let stats = learned_rowid_stats();

        assert_eq!(value, "value-137");
        assert!(stats.attempted > 0);
        assert!(stats.model_predictions > 0);
        sqlite_clear_learned_rowid_models();
    }

    #[test]
    fn create_learned_index_rejects_non_rowid_column() {
        let _guard = LEARNED_ROWID_TEST_LOCK.lock().unwrap();
        sqlite_clear_learned_rowid_models();
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "CREATE TABLE learned_probe(
                    id INTEGER PRIMARY KEY,
                    value INTEGER NOT NULL
                 );
                 INSERT INTO learned_probe(id, value) VALUES (1, 10), (2, 20);",
            )
            .unwrap();

        let err = connection
            .execute(
                "CREATE LEARNED INDEX idx_learned_probe_value ON learned_probe(value)",
                [],
            )
            .unwrap_err();

        assert!(
            err.to_string().contains("INTEGER PRIMARY KEY rowid alias"),
            "{err}"
        );
        sqlite_clear_learned_rowid_models();
    }

    #[test]
    fn learned_rowid_can_be_disabled_for_comparison() {
        let _guard = LEARNED_ROWID_TEST_LOCK.lock().unwrap();
        sqlite_clear_learned_rowid_models();
        let connection = Connection::open_in_memory().unwrap();
        let keys = dense_probe_keys(200);
        connection
            .execute_batch(
                "CREATE TABLE learned_probe(id INTEGER PRIMARY KEY, value TEXT NOT NULL);
                 WITH RECURSIVE rows(id) AS (
                   SELECT 1
                   UNION ALL
                   SELECT id + 1 FROM rows WHERE id < 200
                 )
                 INSERT INTO learned_probe(id, value)
                 SELECT id, 'value-' || id FROM rows;",
            )
            .unwrap();
        register_test_model(&connection, "learned_probe", &keys);

        set_learned_rowid_enabled(false);
        reset_learned_rowid_stats();
        let disabled_value: String = connection
            .query_row(
                "SELECT value FROM learned_probe WHERE id = ?",
                params![137_i64],
                |row| row.get(0),
            )
            .unwrap();
        let disabled = learned_rowid_stats();

        set_learned_rowid_enabled(true);
        reset_learned_rowid_stats();
        let enabled_value: String = connection
            .query_row(
                "SELECT value FROM learned_probe WHERE id = ?",
                params![137_i64],
                |row| row.get(0),
            )
            .unwrap();
        let enabled = learned_rowid_stats();

        assert_eq!(disabled_value, "value-137");
        assert_eq!(enabled_value, "value-137");
        assert_eq!(disabled.attempted, 0);
        assert!(disabled.fallback > 0);
        assert!(enabled.attempted > 0);
        set_learned_rowid_enabled(true);
        sqlite_clear_learned_rowid_models();
    }

    #[test]
    fn learned_rowid_probe_compares_enabled_and_disabled_runs() {
        let _guard = LEARNED_ROWID_TEST_LOCK.lock().unwrap();
        sqlite_clear_learned_rowid_models();
        let mut connection = Connection::open_in_memory().unwrap();
        let keys = dense_probe_keys(200);
        rebuild_learned_rowid_probe_table(&mut connection, &keys).unwrap();

        let disabled = run_learned_rowid_probe(&connection, false, &keys, 64).unwrap();
        let enabled = run_learned_rowid_probe(&connection, true, &keys, 64).unwrap();

        assert!(!disabled.enabled);
        assert!(enabled.enabled);
        assert_eq!(disabled.lookup_count, 64);
        assert_eq!(enabled.lookup_count, 64);
        assert_eq!(disabled.stats.attempted, 0);
        assert!(disabled.stats.fallback > 0);
        assert!(enabled.stats.attempted > 0);
        assert!(enabled.stats.comparisons > 0);
        assert!(enabled.stats.exact_first_probe > 0);
        assert!(enabled.stats.comparisons < disabled.stats.comparisons);
        set_learned_rowid_enabled(true);
        sqlite_clear_learned_rowid_models();
    }

    #[test]
    fn learned_rowid_benchmark_reports_dense_and_sparse_scenarios() {
        let _guard = LEARNED_ROWID_TEST_LOCK.lock().unwrap();
        sqlite_clear_learned_rowid_models();
        let mut connection = Connection::open_in_memory().unwrap();

        let dense = run_learned_rowid_benchmark_scenario(
            &mut connection,
            "dense",
            &dense_probe_keys(200),
            64,
        )
        .unwrap();
        let sparse = run_learned_rowid_benchmark_scenario(
            &mut connection,
            "sparse quadratic",
            &sparse_quadratic_probe_keys(200),
            64,
        )
        .unwrap();

        assert_eq!(dense.name, "dense");
        assert_eq!(sparse.name, "sparse quadratic");
        assert_eq!(dense.row_count, 200);
        assert_eq!(sparse.row_count, 200);
        assert!(dense.enabled.stats.comparisons < dense.disabled.stats.comparisons);
        assert!(sparse.enabled.stats.attempted > 0);
        assert!(sparse.enabled.stats.model_segments > 0);
        assert!(sparse.enabled.stats.model_bytes > 0);
        assert!(sparse.enabled.stats.comparisons > 0);
        set_learned_rowid_enabled(true);
        sqlite_clear_learned_rowid_models();
    }

    #[test]
    fn pgm_segments_cover_dense_and_sparse_keys() {
        let dense = dense_probe_keys(500);
        let sparse = sparse_quadratic_probe_keys(500);

        let dense_segments = build_pgm_segments(&dense, LEARNED_ROWID_MODEL_ERROR);
        let sparse_segments = build_pgm_segments(&sparse, LEARNED_ROWID_MODEL_ERROR);

        assert_eq!(
            dense_segments.first().map(|segment| segment.first_key),
            Some(1)
        );
        assert_eq!(
            sparse_segments.first().map(|segment| segment.first_key),
            sparse.first().copied()
        );
        assert!(!dense_segments.is_empty());
        assert!(!sparse_segments.is_empty());
        assert!(dense_segments.len() < dense.len());
        assert!(sparse_segments.len() < sparse.len());
    }

    fn register_test_model(connection: &Connection, table_name: &str, keys: &[i64]) {
        let root_page = table_root_page(connection, "main", table_name)
            .unwrap()
            .unwrap();
        let segments = build_pgm_segments(keys, LEARNED_ROWID_MODEL_ERROR);
        register_learned_rowid_model(
            root_page,
            &segments,
            keys.len() as i64,
            LEARNED_ROWID_MODEL_ERROR,
        )
        .unwrap();
    }
}
