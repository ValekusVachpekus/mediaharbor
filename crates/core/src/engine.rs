use crate::{media, model::*, spotify::Spotify, store::Store};
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

pub struct Engine {
    pub store: Arc<Store>,
    pub spotify: Arc<Spotify>,
    downloads: Arc<Semaphore>,
    encodes: Arc<Semaphore>,
    active: Mutex<HashMap<String, CancellationToken>>,
    collections: Mutex<HashMap<String, Vec<MediaCandidate>>>,
    pub shutdown: CancellationToken,
    analysis: Semaphore,
    mutations: tokio::sync::Mutex<()>,
}
impl Engine {
    pub fn new(store: Store) -> Arc<Self> {
        Arc::new(Self {
            store: Arc::new(store),
            spotify: Arc::new(Spotify::default()),
            downloads: Arc::new(Semaphore::new(2)),
            encodes: Arc::new(Semaphore::new(1)),
            active: Mutex::new(HashMap::new()),
            collections: Mutex::new(HashMap::new()),
            shutdown: CancellationToken::new(),
            analysis: Semaphore::new(2),
            mutations: tokio::sync::Mutex::new(()),
        })
    }
    pub async fn response(self: &Arc<Self>, req: Request) -> Value {
        let mutation =
            ["jobs.create", "jobs.cancel", "jobs.retry", "settings.set"].contains(&req.op.as_str());
        let _guard = if mutation {
            Some(self.mutations.lock().await)
        } else {
            None
        };
        let result = if req.version != VERSION {
            Err(anyhow::anyhow!("Несовместимая версия протокола"))
        } else if req.request_id.is_empty() || req.request_id.len() > 128 {
            Err(anyhow::anyhow!("Некорректный requestId"))
        } else if mutation {
            match self.store.receipt(&req) {
                Ok(Some(response)) => return response,
                Err(error) => Err(error),
                Ok(None) => {
                    if req.op == "jobs.create" {
                        self.create_jobs(&req)
                    } else {
                        self.handle(&req.op, req.payload.clone()).await
                    }
                }
            }
        } else {
            self.handle(&req.op, req.payload.clone()).await
        };
        match result {
            Ok(data) => {
                let response =
                    json!({"version":VERSION,"requestId":req.request_id,"ok":true,"data":data});
                if mutation {
                    if let Err(e) = self.store.commit_request(&req, &response, &[]) {
                        return json!({"version":VERSION,"requestId":req.request_id,"ok":false,"error":{"code":"STORE_ERROR","message":e.to_string()}});
                    }
                }
                response
            }
            Err(e) => {
                json!({"version":VERSION,"requestId":req.request_id,"ok":false,"error":{"code":if req.version != VERSION {"VERSION_MISMATCH"} else {"REQUEST_FAILED"},"message":format!("{e:#}")}})
            }
        }
    }
    async fn handle(self: &Arc<Self>, op: &str, p: Value) -> Result<Value> {
        match op {
            "ping" => Ok(json!({"version":VERSION,"name":"MediaHarbor"})),
            "settings.get" => Ok(serde_json::to_value(self.store.settings()?)?),
            "settings.set" => {
                self.store.set_settings(&serde_json::from_value(p)?)?;
                Ok(json!(true))
            }
            "tools.check" => Ok(media::tools(&self.store.settings()?.tools_dir).await),
            "tools.updates" => crate::updates::check().await,
            "jobs.list" => {
                let offset = p["offset"].as_u64().unwrap_or(0) as usize;
                let jobs = self.store.jobs()?;
                let total = jobs.len();
                Ok(
                    json!({"items":jobs.into_iter().skip(offset).take(50).collect::<Vec<_>>(),"total":total}),
                )
            }
            "jobs.cancel" => {
                let id = p["id"].as_str().context("Нет ID задачи")?;
                if let Some(token) = self.active.lock().unwrap().get(id) {
                    token.cancel();
                }
                let mut job = self.store.job(id)?;
                anyhow::ensure!(
                    ["queued", "running"].contains(&job.status.as_str()),
                    "Задача уже завершена"
                );
                job.status = "cancelled".into();
                job.stage = "Отменено".into();
                job.updated_at = now();
                self.store.save(&job)?;
                Ok(json!(job))
            }
            "jobs.retry" => {
                let id = p["id"].as_str().context("Нет ID задачи")?;
                let mut job = self.store.job(id)?;
                anyhow::ensure!(
                    ["failed", "cancelled", "interrupted"].contains(&job.status.as_str()),
                    "Повтор недоступен"
                );
                anyhow::ensure!(
                    !self.active.lock().unwrap().contains_key(id),
                    "Дождитесь остановки предыдущего процесса"
                );
                job.status = "queued".into();
                job.progress = 0.0;
                job.error = None;
                job.warning = None;
                job.stage = "В очереди".into();
                job.updated_at = now();
                self.store.save(&job)?;
                self.start(id.into());
                Ok(json!(job))
            }
            "media.search" => {
                let _permit = self.analysis.acquire().await?;
                let query = p["query"].as_str().context("Нет запроса")?;
                Ok(json!(
                    media::search(query, &self.store.settings()?.tools_dir).await?
                ))
            }
            "media.analyze" => {
                let _permit = self.analysis.acquire().await?;
                let source = p["source"].as_str().context("Нет источника")?;
                let items = media::inspect(source, &self.store.settings()?.tools_dir).await?;
                let id = uuid::Uuid::new_v4().to_string();
                let total = items.len();
                let page: Vec<_> = items.iter().take(50).cloned().collect();
                {
                    let mut cache = self.collections.lock().unwrap();
                    if cache.len() >= 8 {
                        cache.clear();
                    }
                    cache.insert(id.clone(), items);
                }
                Ok(
                    json!({"collectionId":id,"items":page,"total":total,"nextOffset":if total>50 {Some(50)} else {None}}),
                )
            }
            "collection.page" => {
                let id = p["collectionId"].as_str().context("Нет ID коллекции")?;
                let offset = p["offset"].as_u64().unwrap_or(0) as usize;
                let cache = self.collections.lock().unwrap();
                let items = cache
                    .get(id)
                    .context("Коллекция устарела. Откройте ссылку повторно")?;
                Ok(
                    json!({"items":items.iter().skip(offset).take(50).collect::<Vec<_>>(),"total":items.len(),"nextOffset":if offset+50<items.len() {Some(offset+50)} else {None}}),
                )
            }
            "media.probe" => {
                let path = PathBuf::from(p["path"].as_str().context("Нет пути")?);
                anyhow::ensure!(path.is_absolute() && path.is_file(), "Файл не найден");
                media::probe(&path, &self.store.settings()?.tools_dir).await
            }
            "spotify.title" => Ok(json!(
                crate::spotify::title(p["source"].as_str().context("Нет ссылки")?).await?
            )),
            "spotify.status" => Ok(
                json!({"connected":self.spotify.connected(&self.store.settings()?.spotify_client_id)}),
            ),
            "spotify.login" => Ok(serde_json::to_value(
                self.spotify
                    .begin(&self.store.settings()?.spotify_client_id)
                    .await?,
            )?),
            "spotify.logout" => {
                self.spotify
                    .logout(&self.store.settings()?.spotify_client_id)?;
                Ok(json!(true))
            }
            "spotify.collection" => {
                self.spotify
                    .collection(
                        p["source"].as_str().context("Нет ссылки")?,
                        &self.store.settings()?.spotify_client_id,
                        p["offset"].as_u64().unwrap_or(0) as usize,
                    )
                    .await
            }
            "worker.shutdown" => {
                self.shutdown.cancel();
                Ok(json!(true))
            }
            _ => bail!("Неизвестная команда: {op}"),
        }
    }
    fn create_jobs(self: &Arc<Self>, req: &Request) -> Result<Value> {
        let inputs: Vec<JobInput> = serde_json::from_value(req.payload["items"].clone())?;
        anyhow::ensure!(
            !inputs.is_empty() && inputs.len() <= 50,
            "За запрос можно добавить 1–50 задач"
        );
        for input in &inputs {
            input.validate()?;
            if input.kind == "download" {
                anyhow::ensure!(
                    !youtube_url(&input.source)?.contains("/playlist"),
                    "Сначала выберите элементы плейлиста"
                );
            }
        }
        let jobs: Vec<_> = inputs
            .into_iter()
            .map(|input| Job {
                id: uuid::Uuid::new_v4().to_string(),
                input,
                status: "queued".into(),
                stage: "В очереди".into(),
                progress: 0.0,
                created_at: now(),
                updated_at: now(),
                output: None,
                original: None,
                error: None,
                warning: None,
            })
            .collect();
        let data = json!(jobs);
        let response = json!({"version":VERSION,"requestId":req.request_id,"ok":true,"data":data});
        anyhow::ensure!(
            serde_json::to_vec(&response)?.len() < MAX_FRAME - 1024,
            "Пакет слишком большой; уменьшите число задач"
        );
        self.store.commit_request(req, &response, &jobs)?;
        for job in &jobs {
            self.start(job.id.clone());
        }
        Ok(data)
    }
    fn start(self: &Arc<Self>, id: String) {
        let cancel = CancellationToken::new();
        self.active
            .lock()
            .unwrap()
            .insert(id.clone(), cancel.clone());
        let this = self.clone();
        tokio::spawn(async move {
            let result = this.execute(&id, &cancel).await;
            if let Ok(mut job) = this.store.job(&id) {
                match result {
                    Ok((output, warning)) => {
                        job.status = "completed".into();
                        job.stage = "Готово".into();
                        job.progress = 100.0;
                        job.output = Some(output.to_string_lossy().into());
                        job.warning = warning;
                    }
                    Err(_) if cancel.is_cancelled() || this.shutdown.is_cancelled() => {
                        job.status = if this.shutdown.is_cancelled() {
                            "interrupted"
                        } else {
                            "cancelled"
                        }
                        .into();
                        job.stage = "Остановлено".into();
                    }
                    Err(e) => {
                        job.status = "failed".into();
                        job.stage = "Ошибка".into();
                        job.error = Some(format!("{e:#}"));
                    }
                }
                job.updated_at = now();
                let _ = this.store.save(&job);
            }
            this.active.lock().unwrap().remove(&id);
        });
    }
    async fn execute(
        &self,
        id: &str,
        cancel: &CancellationToken,
    ) -> Result<(PathBuf, Option<String>)> {
        let mut job = self.store.job(id)?;
        let semaphore = if job.input.kind == "download" {
            &self.downloads
        } else {
            &self.encodes
        };
        let permit = tokio::select! { _ = cancel.cancelled() => bail!("Отменено"), permit = semaphore.acquire() => permit? };
        if cancel.is_cancelled() {
            bail!("Отменено");
        }
        job.status = "running".into();
        job.updated_at = now();
        self.store.save(&job)?;
        let source = if job.input.kind == "download" {
            media::download(self.store.clone(), &job, cancel).await?
        } else {
            PathBuf::from(&job.input.source)
        };
        job = self.store.job(id)?;
        job.original = Some(source.to_string_lossy().into());
        self.store.save(&job)?;
        if job.input.kind == "download" {
            drop(permit);
            let options = job.input.download.as_ref().unwrap();
            if cancel.is_cancelled() {
                bail!("Отменено");
            }
            if options.format == "original" && job.input.compression.is_none() {
                return Ok((source, None));
            }
            media::progress(&self.store, id, "Ожидание обработки", 0.0);
            let _encode = tokio::select! { _ = cancel.cancelled() => bail!("Отменено"), permit = self.encodes.acquire() => permit? };
            let profile = job.input.compression.clone().unwrap_or(CompressionOptions {
                format: options.format.clone(),
                preset: "balanced".into(),
                height: Some(options.height),
                audio_bitrate: options.audio_bitrate,
                crf: None,
                target_mb: None,
            });
            media::encode(self.store.clone(), &job, &source, &profile, cancel).await
        } else {
            media::encode(
                self.store.clone(),
                &job,
                &source,
                job.input.compression.as_ref().unwrap(),
                cancel,
            )
            .await
        }
    }
    pub async fn stop(&self) {
        self.shutdown.cancel();
        for token in self.active.lock().unwrap().values() {
            token.cancel();
        }
        for _ in 0..100 {
            if self.active.lock().unwrap().is_empty() {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    }
}
