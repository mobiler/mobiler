//! Axum + SQLx JSON API for the notes demo.
//!
//! Dev uses SQLite (a local file); production can move to PostgreSQL by swapping the
//! SQLx `sqlite` feature for `postgres`, the connection URL, and the table DDL. We use
//! SQLx's *runtime* query API (`sqlx::query` / `query_as`) rather than the compile-time
//! macros, so this builds with no `DATABASE_URL` and no `sqlx prepare` step.

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::get,
};
use domain::{NewNote, Note};
use sqlx::SqlitePool;
use sqlx::sqlite::{
    SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous,
};
use std::net::SocketAddr;
use std::str::FromStr;
use std::time::Duration;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Structured logs; RUST_LOG overrides (default: info).
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,tower_http=info".into()),
        )
        .init();

    let pool = connect().await?;
    migrate(&pool).await?;

    // Bind 127.0.0.1 by default (sit behind a reverse proxy in prod); BIND_ADDR overrides
    // — set it to 0.0.0.0:3000 in a container.
    let addr: SocketAddr = std::env::var("BIND_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:3000".to_string())
        .parse()?;
    tracing::info!("notes server listening on http://{addr}");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, router(pool.clone()))
        .with_graceful_shutdown(shutdown(pool))
        .await?;
    Ok(())
}

/// Open the SQLite pool with the pragmas a server needs in production:
/// - **WAL** journal so readers never block the writer (and vice versa) — essential for a
///   web server, and required by Litestream for continuous backup.
/// - **busy_timeout** so a brief write lock *waits* instead of erroring with the classic
///   "database is locked" (SQLite serializes writes; this is the #1 web-server gotcha).
/// - **synchronous = NORMAL** — durable under WAL and much faster than FULL.
/// - **foreign keys** enforced (off by default in SQLite).
///
/// Writes serialize at the DB level regardless of pool size, so a small pool is plenty;
/// WAL lets the read connections run concurrently. `DATABASE_URL` overrides the path
/// (e.g. `sqlite:/var/lib/notes/notes.db`).
async fn connect() -> anyhow::Result<SqlitePool> {
    let url = std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite:notes.db".to_string());
    connect_with_url(&url).await
}

async fn connect_with_url(url: &str) -> anyhow::Result<SqlitePool> {
    let options = SqliteConnectOptions::from_str(url)?
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .busy_timeout(Duration::from_secs(5))
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?;
    Ok(pool)
}

/// Wait for Ctrl-C / SIGTERM, then close the pool — which checkpoints the WAL back into the
/// main DB file, so the process exits with a clean database (and a `docker stop` is graceful).
async fn shutdown(pool: SqlitePool) {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut sig) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            sig.recv().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    tracing::info!("shutting down: closing the pool (checkpoints WAL)…");
    pool.close().await;
}

/// The API. CORS is permissive so a web client on another origin can call it.
fn router(pool: SqlitePool) -> Router {
    Router::new()
        .route("/notes", get(list_notes).post(create_note))
        .route("/notes/{id}", get(get_note).delete(delete_note))
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .layer(tower_http::cors::CorsLayer::permissive())
        .with_state(pool)
}

/// Create the schema if needed. A real app would use `sqlx migrate` + a migrations dir;
/// kept inline here so the demo runs with zero setup. (Postgres: `BIGSERIAL PRIMARY KEY`.)
async fn migrate(pool: &SqlitePool) -> anyhow::Result<()> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS notes (
            id    INTEGER PRIMARY KEY AUTOINCREMENT,
            title TEXT NOT NULL,
            body  TEXT NOT NULL
        )",
    )
    .execute(pool)
    .await?;
    Ok(())
}

// ---------- data access (runtime SQLx queries) ----------

async fn db_list(pool: &SqlitePool) -> sqlx::Result<Vec<Note>> {
    let rows = sqlx::query_as::<_, (i64, String, String)>("SELECT id, title, body FROM notes ORDER BY id")
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(|(id, title, body)| Note { id, title, body }).collect())
}

async fn db_create(pool: &SqlitePool, n: &NewNote) -> sqlx::Result<Note> {
    let id = sqlx::query("INSERT INTO notes (title, body) VALUES (?, ?)")
        .bind(&n.title)
        .bind(&n.body)
        .execute(pool)
        .await?
        .last_insert_rowid();
    Ok(Note { id, title: n.title.clone(), body: n.body.clone() })
}

async fn db_get(pool: &SqlitePool, id: i64) -> sqlx::Result<Option<Note>> {
    let row = sqlx::query_as::<_, (i64, String, String)>("SELECT id, title, body FROM notes WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|(id, title, body)| Note { id, title, body }))
}

async fn db_delete(pool: &SqlitePool, id: i64) -> sqlx::Result<bool> {
    let affected = sqlx::query("DELETE FROM notes WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?
        .rows_affected();
    Ok(affected > 0)
}

// ---------- handlers ----------

async fn list_notes(State(pool): State<SqlitePool>) -> Result<Json<Vec<Note>>, StatusCode> {
    db_list(&pool).await.map(Json).map_err(internal)
}

async fn create_note(
    State(pool): State<SqlitePool>,
    Json(new): Json<NewNote>,
) -> Result<Json<Note>, StatusCode> {
    db_create(&pool, &new).await.map(Json).map_err(internal)
}

async fn get_note(
    State(pool): State<SqlitePool>,
    Path(id): Path<i64>,
) -> Result<Json<Note>, StatusCode> {
    match db_get(&pool, id).await {
        Ok(Some(note)) => Ok(Json(note)),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(e) => Err(internal(e)),
    }
}

async fn delete_note(State(pool): State<SqlitePool>, Path(id): Path<i64>) -> StatusCode {
    match db_delete(&pool, id).await {
        Ok(true) => StatusCode::NO_CONTENT,
        Ok(false) => StatusCode::NOT_FOUND,
        Err(e) => internal(e),
    }
}

/// Map a DB error to 500 — and log it, so a failing query is diagnosable instead of a
/// silent 500 (never leak the raw error to the client).
fn internal(e: sqlx::Error) -> StatusCode {
    tracing::error!("database error: {e}");
    StatusCode::INTERNAL_SERVER_ERROR
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A shared in-memory SQLite (max 1 connection, so migrate + queries hit the same DB).
    async fn test_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        migrate(&pool).await.unwrap();
        pool
    }

    #[tokio::test]
    async fn create_list_get_delete_round_trip() {
        let pool = test_pool().await;
        assert!(db_list(&pool).await.unwrap().is_empty());

        let made = db_create(&pool, &NewNote { title: "T".into(), body: "B".into() }).await.unwrap();
        assert_eq!(made.id, 1);
        assert_eq!(db_list(&pool).await.unwrap().len(), 1);
        assert_eq!(db_get(&pool, 1).await.unwrap().unwrap().title, "T");

        // sad paths: missing id reads as None; deleting twice reports not-found the 2nd time.
        assert!(db_get(&pool, 999).await.unwrap().is_none());
        assert!(db_delete(&pool, 1).await.unwrap());
        assert!(!db_delete(&pool, 1).await.unwrap());
        assert!(db_list(&pool).await.unwrap().is_empty());
    }

    /// The production `connect()` actually applies its pragmas — WAL (also required by
    /// Litestream) and enforced foreign keys — on a real on-disk database.
    #[tokio::test]
    async fn connect_applies_production_pragmas() {
        let dir = std::env::temp_dir().join(format!("mobiler_notes_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let url = format!("sqlite:{}", dir.join("notes.db").display());

        let pool = connect_with_url(&url).await.unwrap();
        let (journal,): (String,) =
            sqlx::query_as("PRAGMA journal_mode").fetch_one(&pool).await.unwrap();
        assert_eq!(journal.to_lowercase(), "wal", "WAL must be enabled");
        let (fk,): (i64,) =
            sqlx::query_as("PRAGMA foreign_keys").fetch_one(&pool).await.unwrap();
        assert_eq!(fk, 1, "foreign keys must be enforced");

        pool.close().await;
        let _ = std::fs::remove_dir_all(&dir);
    }
}
