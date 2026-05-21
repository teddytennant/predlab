//! Club student roster, persisted in `~/.predlab/students.db`.
//!
//! Schema is identical to the previous Python admin TUI so existing data is
//! read back unchanged:
//!
//! ```sql
//! CREATE TABLE students (
//!     username     TEXT PRIMARY KEY,
//!     display_name TEXT,
//!     poly_key     TEXT,
//!     kalshi_key   TEXT,
//!     created_at   TEXT
//! )
//! ```

use std::path::{Path, PathBuf};

use anyhow::Result;
use rusqlite::{params, Connection};

/// One club member with their paper keys for both simulators.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Student {
    pub username: String,
    pub display_name: String,
    pub poly_key: String,
    pub kalshi_key: String,
    /// Access role granted on both sims: "member", "admin", or "owner".
    pub role: String,
    pub created_at: String,
}

/// Location of the shared roster DB (`$HOME/.predlab/students.db`).
pub fn default_db_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".predlab").join("students.db")
}

/// Open the roster at `path`, creating the parent directory and table if needed.
pub fn open(path: &Path) -> Result<Connection> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(path)?;
    init_schema(&conn)?;
    Ok(conn)
}

/// Open an ephemeral in-memory roster (used by tests).
pub fn open_in_memory() -> Result<Connection> {
    let conn = Connection::open_in_memory()?;
    init_schema(&conn)?;
    Ok(conn)
}

fn init_schema(conn: &Connection) -> Result<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS students (
            username     TEXT PRIMARY KEY,
            display_name TEXT,
            poly_key     TEXT,
            kalshi_key   TEXT,
            role         TEXT NOT NULL DEFAULT 'member',
            created_at   TEXT
        )",
        [],
    )?;
    // Migrate rosters created before roles existed. Errors (duplicate column on
    // a fresh DB that already has it) are expected and ignored.
    let _ = conn.execute(
        "ALTER TABLE students ADD COLUMN role TEXT NOT NULL DEFAULT 'member'",
        [],
    );
    Ok(())
}

/// Insert or update a student (keyed by username), matching the Python TUI's
/// `INSERT OR REPLACE` behaviour.
pub fn save_student(conn: &Connection, s: &Student) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO students
            (username, display_name, poly_key, kalshi_key, role, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            s.username,
            s.display_name,
            s.poly_key,
            s.kalshi_key,
            s.role,
            s.created_at
        ],
    )?;
    Ok(())
}

/// All students, newest first (then alphabetical) for stable display.
pub fn list_students(conn: &Connection) -> Result<Vec<Student>> {
    let mut stmt = conn.prepare(
        "SELECT username, display_name, poly_key, kalshi_key, role, created_at
         FROM students
         ORDER BY created_at DESC, username ASC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(Student {
            username: row.get(0)?,
            display_name: row.get::<_, Option<String>>(1)?.unwrap_or_default(),
            poly_key: row.get::<_, Option<String>>(2)?.unwrap_or_default(),
            kalshi_key: row.get::<_, Option<String>>(3)?.unwrap_or_default(),
            role: row
                .get::<_, Option<String>>(4)?
                .unwrap_or_else(|| "member".to_string()),
            created_at: row.get::<_, Option<String>>(5)?.unwrap_or_default(),
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn student(name: &str) -> Student {
        Student {
            username: name.to_string(),
            display_name: format!("{name} display"),
            poly_key: format!("pm_paper_{name}"),
            kalshi_key: format!("ks_live_{name}"),
            role: "member".to_string(),
            created_at: "2026-05-20T10:00:00".to_string(),
        }
    }

    #[test]
    fn save_persists_role() {
        let conn = open_in_memory().unwrap();
        let mut vp = student("vp");
        vp.role = "admin".to_string();
        save_student(&conn, &vp).unwrap();
        assert_eq!(list_students(&conn).unwrap()[0].role, "admin");
    }

    #[test]
    fn empty_roster_lists_nothing() {
        let conn = open_in_memory().unwrap();
        assert!(list_students(&conn).unwrap().is_empty());
    }

    #[test]
    fn save_and_list_roundtrip() {
        let conn = open_in_memory().unwrap();
        let alice = student("alice");
        save_student(&conn, &alice).unwrap();
        let all = list_students(&conn).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0], alice);
    }

    #[test]
    fn save_is_upsert_by_username() {
        let conn = open_in_memory().unwrap();
        let mut bob = student("bob");
        save_student(&conn, &bob).unwrap();
        bob.poly_key = "pm_paper_bob_rotated".to_string();
        save_student(&conn, &bob).unwrap();

        let all = list_students(&conn).unwrap();
        assert_eq!(all.len(), 1, "username is the primary key, no duplicate row");
        assert_eq!(all[0].poly_key, "pm_paper_bob_rotated");
    }

    #[test]
    fn open_creates_file_and_schema() {
        let dir = std::env::temp_dir().join(format!("predlab-reg-{}", std::process::id()));
        let path = dir.join("students.db");
        let _ = std::fs::remove_dir_all(&dir);
        {
            let conn = open(&path).unwrap();
            save_student(&conn, &student("carol")).unwrap();
        }
        // Reopen from disk: data persists and schema is reused.
        let conn = open(&path).unwrap();
        assert_eq!(list_students(&conn).unwrap()[0].username, "carol");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
