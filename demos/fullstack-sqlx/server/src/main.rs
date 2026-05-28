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
use sqlx::sqlite::SqlitePoolOptions;
use std::net::SocketAddr;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // SQLite file for dev; `?mode=rwc` creates it if missing.
    let url =
        std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite:notes.db?mode=rwc".to_string());
    let pool = SqlitePool::connect(&url).await?;
    migrate(&pool).await?;

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    println!("notes server listening on http://{addr}");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, router(pool)).await?;
    Ok(())
}

/// The API. CORS is permissive so a web client on another origin can call it.
fn router(pool: SqlitePool) -> Router {
    Router::new()
        .route("/notes", get(list_notes).post(create_note))
        .route("/notes/{id}", get(get_note).delete(delete_note))
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
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

fn internal(_e: sqlx::Error) -> StatusCode {
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
}
