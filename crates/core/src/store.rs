use crate::model::*;
use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use std::{path::PathBuf, sync::Mutex};

pub struct Store {
    conn: Mutex<Connection>,
    pub dir: PathBuf,
}
pub fn data_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("MEDIAHARBOR_DATA_DIR") {
        return PathBuf::from(dir);
    }
    #[cfg(windows)]
    let root = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    #[cfg(not(windows))]
    let root = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/share")
        });
    root.join("mediaharbor")
}
impl Store {
    pub fn open(dir: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&dir)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))?;
        }
        let conn = Connection::open(dir.join("library.sqlite"))?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        conn.execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE IF NOT EXISTS jobs (id TEXT PRIMARY KEY, data TEXT NOT NULL); CREATE TABLE IF NOT EXISTS settings (id INTEGER PRIMARY KEY CHECK(id=1), data TEXT NOT NULL); CREATE TABLE IF NOT EXISTS receipts(id TEXT PRIMARY KEY, request TEXT NOT NULL, response TEXT NOT NULL); PRAGMA user_version=1;")?;
        let store = Self {
            conn: Mutex::new(conn),
            dir,
        };
        for mut job in store.jobs()? {
            if ["queued", "running"].contains(&job.status.as_str()) {
                job.status = "interrupted".into();
                job.stage = "Прервано при завершении программы".into();
                job.updated_at = now();
                store.save(&job)?;
            }
        }
        Ok(store)
    }
    pub fn save(&self, job: &Job) -> Result<()> {
        self.conn.lock().unwrap().execute(
            "INSERT OR REPLACE INTO jobs(id,data) VALUES(?1,?2)",
            params![job.id, serde_json::to_string(job)?],
        )?;
        Ok(())
    }
    pub fn receipt(&self, request: &Request) -> Result<Option<serde_json::Value>> {
        let conn = self.conn.lock().unwrap();
        let result = conn.query_row(
            "SELECT request,response FROM receipts WHERE id=?1",
            [&request.request_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        );
        match result {
            Ok((original, response)) => {
                anyhow::ensure!(
                    original == serde_json::to_string(request)?,
                    "requestId уже использован для другой команды"
                );
                Ok(Some(serde_json::from_str(&response)?))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
    pub fn commit_request(
        &self,
        request: &Request,
        response: &serde_json::Value,
        jobs: &[Job],
    ) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        for job in jobs {
            tx.execute(
                "INSERT OR REPLACE INTO jobs(id,data) VALUES(?1,?2)",
                params![job.id, serde_json::to_string(job)?],
            )?;
        }
        tx.execute(
            "INSERT OR REPLACE INTO receipts(id,request,response) VALUES(?1,?2,?3)",
            params![
                request.request_id,
                serde_json::to_string(request)?,
                serde_json::to_string(response)?
            ],
        )?;
        tx.commit()?;
        Ok(())
    }
    pub fn jobs(&self) -> Result<Vec<Job>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT data FROM jobs ORDER BY rowid DESC")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        rows.map(|r| Ok(serde_json::from_str(&r?)?)).collect()
    }
    pub fn job(&self, id: &str) -> Result<Job> {
        let text: String = self
            .conn
            .lock()
            .unwrap()
            .query_row("SELECT data FROM jobs WHERE id=?1", [id], |r| r.get(0))
            .context("Задача не найдена")?;
        Ok(serde_json::from_str(&text)?)
    }
    pub fn settings(&self) -> Result<Settings> {
        let conn = self.conn.lock().unwrap();
        let result = conn.query_row("SELECT data FROM settings WHERE id=1", [], |r| {
            r.get::<_, String>(0)
        });
        match result {
            Ok(s) => Ok(serde_json::from_str(&s)?),
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
                    .map(PathBuf::from)
                    .unwrap_or_else(|| self.dir.clone());
                Ok(Settings {
                    output_dir: home.join("Downloads/MediaHarbor").to_string_lossy().into(),
                    theme: "dark".into(),
                    spotify_client_id: String::new(),
                    tools_dir: String::new(),
                })
            }
            Err(e) => Err(e.into()),
        }
    }
    pub fn set_settings(&self, s: &Settings) -> Result<()> {
        anyhow::ensure!(
            std::path::Path::new(&s.output_dir).is_absolute(),
            "Нужен абсолютный путь папки"
        );
        anyhow::ensure!(
            ["dark", "light", "system"].contains(&s.theme.as_str()),
            "Неизвестная тема"
        );
        anyhow::ensure!(
            s.spotify_client_id.is_empty()
                || (s.spotify_client_id.len() == 32
                    && s.spotify_client_id.chars().all(|c| c.is_ascii_hexdigit())),
            "Некорректный Spotify Client ID"
        );
        anyhow::ensure!(
            s.tools_dir.is_empty() || std::path::Path::new(&s.tools_dir).is_absolute(),
            "Нужен абсолютный путь инструментов"
        );
        self.conn.lock().unwrap().execute(
            "INSERT OR REPLACE INTO settings(id,data) VALUES(1,?1)",
            [serde_json::to_string(s)?],
        )?;
        Ok(())
    }
}
