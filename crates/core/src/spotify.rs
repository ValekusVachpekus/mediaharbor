use crate::model::MediaCandidate;
use anyhow::{bail, Context, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rand::RngCore;
use reqwest::{Client, Url};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::sync::Mutex;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

const REDIRECT: &str = "http://127.0.0.1:43827/callback";
const TOKEN_KEY: &str = "spotify-refresh-token";

pub fn client() -> Result<Client> {
    Ok(Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() > 5 {
                return attempt.stop();
            }
            if attempt.url().scheme() == "https"
                && matches!(
                    attempt.url().host_str(),
                    Some("open.spotify.com" | "spotify.link")
                )
            {
                attempt.follow()
            } else {
                attempt.stop()
            }
        }))
        .build()?)
}
pub async fn spotify_url(value: &str) -> Result<Url> {
    let mut url = Url::parse(value)?;
    anyhow::ensure!(
        url.scheme() == "https"
            && url.port().is_none()
            && url.username().is_empty()
            && url.password().is_none(),
        "Некорректная ссылка Spotify"
    );
    if url.host_str() == Some("spotify.link") {
        url = client()?
            .get(url)
            .send()
            .await?
            .error_for_status()?
            .url()
            .clone();
    }
    anyhow::ensure!(
        url.host_str() == Some("open.spotify.com"),
        "Нужна ссылка open.spotify.com"
    );
    // Locale paths such as /intl-ru/track/... are normalized before use.
    let path: Vec<_> = url.path_segments().into_iter().flatten().collect();
    let path = if path.first().is_some_and(|p| p.starts_with("intl-")) {
        &path[1..]
    } else {
        &path[..]
    };
    anyhow::ensure!(
        path.len() == 2
            && ["track", "playlist", "album"].contains(&path[0])
            && path[1].len() == 22
            && path[1].chars().all(|c| c.is_ascii_alphanumeric()),
        "Нужна ссылка Spotify на трек, альбом или плейлист"
    );
    Ok(Url::parse(&format!(
        "https://open.spotify.com/{}/{}",
        path[0], path[1]
    ))?)
}

pub async fn title(value: &str) -> Result<MediaCandidate> {
    let url = spotify_url(value).await?;
    anyhow::ensure!(
        url.path().starts_with("/track/"),
        "Состав коллекции: откройте её в браузере или подключите API Spotify"
    );
    let value: Value = client()?
        .get("https://open.spotify.com/oembed")
        .query(&[("url", url.as_str())])
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(MediaCandidate {
        id: url.path().rsplit('/').next().unwrap_or_default().into(),
        title: value["title"]
            .as_str()
            .context("Spotify не вернул название")?
            .into(),
        url: url.into(),
        artist: None,
        duration: None,
        thumbnail: value["thumbnail_url"].as_str().map(Into::into),
    })
}

#[derive(Clone)]
pub struct Access {
    token: String,
    expires: std::time::Instant,
    client_id: String,
}
pub struct Spotify {
    access: Mutex<Option<Access>>,
    login: std::sync::Arc<tokio::sync::Mutex<()>>,
}
impl Default for Spotify {
    fn default() -> Self {
        Self {
            access: Mutex::new(None),
            login: std::sync::Arc::new(tokio::sync::Mutex::new(())),
        }
    }
}
#[derive(Deserialize)]
struct Tokens {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: u64,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Login {
    pub url: String,
    pub redirect_uri: &'static str,
}

impl Spotify {
    fn entry(client_id: &str) -> Result<keyring::Entry> {
        Ok(keyring::Entry::new(
            "io.mediaharbor.app",
            &format!("{TOKEN_KEY}:{client_id}"),
        )?)
    }
    pub fn connected(&self, client_id: &str) -> bool {
        !client_id.is_empty()
            && Self::entry(client_id)
                .and_then(|e| e.get_password().map_err(Into::into))
                .is_ok()
    }
    pub fn logout(&self, client_id: &str) -> Result<()> {
        match Self::entry(client_id)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => {}
            Err(e) => return Err(e.into()),
        }
        *self.access.lock().unwrap() = None;
        Ok(())
    }
    fn remember(&self, client_id: &str, tokens: Tokens) -> Result<()> {
        if let Some(refresh) = tokens.refresh_token {
            Self::entry(client_id)?
                .set_password(&refresh)
                .context("Не удалось сохранить токен в системное хранилище секретов")?;
        }
        *self.access.lock().unwrap() = Some(Access {
            token: tokens.access_token,
            expires: std::time::Instant::now()
                + std::time::Duration::from_secs(tokens.expires_in.saturating_sub(60)),
            client_id: client_id.into(),
        });
        Ok(())
    }
    pub async fn begin(self: &std::sync::Arc<Self>, client_id: &str) -> Result<Login> {
        anyhow::ensure!(
            client_id.len() == 32 && client_id.chars().all(|c| c.is_ascii_hexdigit()),
            "Укажите собственный Spotify Client ID в настройках"
        );
        let guard = self
            .login
            .clone()
            .try_lock_owned()
            .context("Вход уже запущен; завершите его или дождитесь истечения 3 минут")?;
        let listener = TcpListener::bind("127.0.0.1:43827")
            .await
            .context("Порт 43827 занят")?;
        let mut bytes = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut bytes);
        let verifier = URL_SAFE_NO_PAD.encode(bytes);
        rand::thread_rng().fill_bytes(&mut bytes);
        let state = URL_SAFE_NO_PAD.encode(bytes);
        let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
        let mut url = Url::parse("https://accounts.spotify.com/authorize")?;
        url.query_pairs_mut().extend_pairs([
            ("client_id", client_id),
            ("response_type", "code"),
            ("redirect_uri", REDIRECT),
            ("code_challenge_method", "S256"),
            ("code_challenge", &challenge),
            ("state", &state),
            ("scope", "playlist-read-private playlist-read-collaborative"),
        ]);
        let this = self.clone();
        let client_id = client_id.to_owned();
        tokio::spawn(async move {
            let _guard = guard;
            let result = tokio::time::timeout(std::time::Duration::from_secs(180), async {
                loop {
                    let (mut socket, _) = listener.accept().await?;
                    let mut buffer = vec![0u8; 8192];
                    let count = tokio::time::timeout(std::time::Duration::from_secs(5), socket.read(&mut buffer)).await??;
                    let line = std::str::from_utf8(&buffer[..count])?.lines().next().unwrap_or("");
                    let target = line.strip_prefix("GET ").and_then(|s| s.split_whitespace().next()).unwrap_or("");
                    let callback = Url::parse(&format!("http://127.0.0.1:43827{target}"))?;
                    let values: std::collections::HashMap<_, _> = callback.query_pairs().into_owned().collect();
                    if callback.path() != "/callback" || values.get("state") != Some(&state) {
                        socket.write_all(b"HTTP/1.1 400 Bad Request\r\nConnection: close\r\nContent-Length: 0\r\n\r\n").await?; continue;
                    }
                    let result: Result<()> = async {
                        let code = values.get("code").context("Вход отклонён пользователем")?;
                        let response = Client::new().post("https://accounts.spotify.com/api/token").timeout(std::time::Duration::from_secs(30)).form(&[("grant_type", "authorization_code"), ("code", code.as_str()), ("redirect_uri", REDIRECT), ("client_id", &client_id), ("code_verifier", &verifier)]).send().await?;
                        anyhow::ensure!(response.status().is_success(), "Spotify отклонил вход: проверьте Client ID, redirect URI и доступ аккаунта");
                        this.remember(&client_id, response.json().await?)
                    }.await;
                    let message = if result.is_ok() { "Connected. You can close this tab." } else { "Connection failed. Check Spotify settings and system credential storage, then retry." };
                    let response = format!("HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}", message.len(), message);
                    socket.write_all(response.as_bytes()).await?;
                    return result;
                }
            }).await;
            if !matches!(result, Ok(Ok(()))) {
                eprintln!("Spotify authorization did not complete; no credentials logged");
            }
        });
        Ok(Login {
            url: url.into(),
            redirect_uri: REDIRECT,
        })
    }
    async fn token(&self, client_id: &str) -> Result<String> {
        if let Some(a) = self.access.lock().unwrap().as_ref() {
            if a.client_id == client_id && a.expires > std::time::Instant::now() {
                return Ok(a.token.clone());
            }
        }
        let refresh = Self::entry(client_id)?
            .get_password()
            .context("Подключите Spotify в настройках")?;
        let response = Client::new()
            .post("https://accounts.spotify.com/api/token")
            .timeout(std::time::Duration::from_secs(30))
            .form(&[
                ("grant_type", "refresh_token"),
                ("refresh_token", refresh.as_str()),
                ("client_id", client_id),
            ])
            .send()
            .await?;
        anyhow::ensure!(
            response.status().is_success(),
            "Spotify отклонил обновление токена. Подключите аккаунт повторно"
        );
        let tokens: Tokens = response.json().await?;
        let token = tokens.access_token.clone();
        self.remember(client_id, tokens)?;
        Ok(token)
    }
    async fn get(&self, client_id: &str, url: &str) -> Result<Value> {
        let url = Url::parse(url)?;
        anyhow::ensure!(
            url.scheme() == "https"
                && url.host_str() == Some("api.spotify.com")
                && url.port().is_none(),
            "Недопустимый адрес Spotify API"
        );
        let token = self.token(client_id).await?;
        let response = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::none())
            .build()?
            .get(url)
            .bearer_auth(token)
            .send()
            .await?;
        let status = response.status();
        if status.as_u16() == 429 {
            let retry = response
                .headers()
                .get("Retry-After")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("60")
                .to_string();
            bail!("Spotify ограничил запросы. Повторите через {retry} с; при исчерпании квоты ожидание может быть дольше");
        }
        anyhow::ensure!(status.is_success(), "Spotify API: HTTP {}. Проверьте авторизацию и права на коллекцию; можно импортировать её со страницы", status.as_u16());
        Ok(response.json().await?)
    }
    pub async fn collection(&self, source: &str, client_id: &str, offset: usize) -> Result<Value> {
        let url = spotify_url(source).await?;
        let parts: Vec<_> = url.path_segments().unwrap().collect();
        let kind = parts[0];
        let id = parts[1];
        if kind == "track" {
            let data = self
                .get(
                    client_id,
                    &format!("https://api.spotify.com/v1/tracks/{id}"),
                )
                .await?;
            return Ok(
                serde_json::json!({"items":[track(&data).context("Трек недоступен")?],"total":1,"nextOffset":null}),
            );
        }
        let endpoint = if kind == "playlist" {
            format!("https://api.spotify.com/v1/playlists/{id}/items?limit=50&offset={offset}")
        } else {
            format!("https://api.spotify.com/v1/albums/{id}/tracks?limit=50&offset={offset}")
        };
        let data = self.get(client_id, &endpoint).await?;
        let entries = data["items"].as_array().context(
            "Состав коллекции недоступен через API. Используйте импорт со страницы или файла",
        )?;
        let items: Vec<_> = entries
            .iter()
            .filter_map(|v| {
                if kind == "playlist" {
                    track(
                        v.get("item")
                            .or_else(|| v.get("track"))
                            .unwrap_or(&Value::Null),
                    )
                } else {
                    track(v)
                }
            })
            .collect();
        Ok(
            serde_json::json!({"items":items,"total":data["total"],"nextOffset":if data["next"].is_string() {Some(offset+entries.len())} else {None}}),
        )
    }
}
fn track(v: &Value) -> Option<MediaCandidate> {
    if v["is_local"] == true || v["type"] != "track" {
        return None;
    }
    Some(MediaCandidate {
        id: v["id"].as_str()?.into(),
        title: v["name"].as_str()?.into(),
        artist: Some(
            v["artists"]
                .as_array()?
                .iter()
                .filter_map(|a| a["name"].as_str())
                .collect::<Vec<_>>()
                .join(", "),
        ),
        url: v["external_urls"]["spotify"].as_str()?.into(),
        duration: v["duration_ms"].as_f64().map(|v| v / 1000.0),
        thumbnail: v["album"]["images"][0]["url"].as_str().map(Into::into),
    })
}
