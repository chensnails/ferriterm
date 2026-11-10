//! Remote backup transports: GitHub Gist and WebDAV.
//!
//! Both transports move the sealed backup text produced by [`crate::seal`];
//! encryption happens locally, so a remote store only ever sees ciphertext.

use std::fmt;
use std::time::Duration;

use serde::{Deserialize, Serialize};

const GITHUB_API: &str = "https://api.github.com";
const DEFAULT_FILENAME: &str = "ferriterm-backup.ftbak";

#[derive(Debug)]
pub enum RemoteError {
    Config(String),
    Http(String),
    Status(u16, String),
    Format(String),
}

impl fmt::Display for RemoteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RemoteError::Config(msg) => write!(f, "{msg}"),
            RemoteError::Http(msg) => write!(f, "remote request failed: {msg}"),
            RemoteError::Status(status, body) => {
                write!(f, "remote returned HTTP {status}: {body}")
            }
            RemoteError::Format(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for RemoteError {}

#[derive(Debug, Clone)]
pub struct GistConfig {
    pub token: String,
    pub gist_id: String,
    pub filename: String,
    pub api_base: String,
}

impl GistConfig {
    pub fn new(token: impl Into<String>, gist_id: impl Into<String>) -> Self {
        Self {
            token: token.into(),
            gist_id: gist_id.into(),
            filename: DEFAULT_FILENAME.to_string(),
            api_base: GITHUB_API.to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct WebdavConfig {
    pub url: String,
    pub username: String,
    pub password: String,
    pub filename: String,
}

impl WebdavConfig {
    pub fn new(
        url: impl Into<String>,
        username: impl Into<String>,
        password: impl Into<String>,
    ) -> Self {
        Self {
            url: url.into(),
            username: username.into(),
            password: password.into(),
            filename: DEFAULT_FILENAME.to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteUpload {
    pub location: String,
    pub bytes: usize,
    /// Provider-specific handle for future uploads (the gist id; empty for WebDAV).
    pub id: String,
}

fn client() -> Result<reqwest::Client, RemoteError> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .connect_timeout(Duration::from_secs(10))
        .user_agent(concat!("Ferriterm/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|err| RemoteError::Http(err.to_string()))
}

fn validate_filename(filename: &str) -> Result<&str, RemoteError> {
    let filename = filename.trim();
    if filename.is_empty()
        || filename.contains('/')
        || filename.contains('\\')
        || filename.contains("..")
    {
        return Err(RemoteError::Config(
            "filename must be a plain file name".to_string(),
        ));
    }
    Ok(filename)
}

async fn status_error(response: reqwest::Response) -> RemoteError {
    let status = response.status().as_u16();
    let body = response.text().await.unwrap_or_default();
    let body: String = body.chars().take(300).collect();
    RemoteError::Status(status, body)
}

fn trim_slash(url: &str) -> &str {
    url.trim_end_matches('/')
}

pub async fn gist_test(config: &GistConfig) -> Result<String, RemoteError> {
    if config.token.trim().is_empty() {
        return Err(RemoteError::Config("gist token is required".to_string()));
    }
    let response = client()?
        .get(format!("{}/user", trim_slash(&config.api_base)))
        .bearer_auth(config.token.trim())
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|err| RemoteError::Http(err.to_string()))?;
    if !response.status().is_success() {
        return Err(status_error(response).await);
    }
    let value: serde_json::Value = response
        .json()
        .await
        .map_err(|err| RemoteError::Format(err.to_string()))?;
    let login = value
        .get("login")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    Ok(login.to_string())
}

pub async fn gist_upload(config: &GistConfig, content: &str) -> Result<RemoteUpload, RemoteError> {
    if config.token.trim().is_empty() {
        return Err(RemoteError::Config("gist token is required".to_string()));
    }
    let filename = validate_filename(&config.filename)?;
    let body = serde_json::json!({
        "description": "Ferriterm encrypted backup",
        "public": false,
        "files": { filename: { "content": content } },
    });
    let base = trim_slash(&config.api_base);
    let gist_id = config.gist_id.trim();
    let response = if gist_id.is_empty() {
        client()?
            .post(format!("{base}/gists"))
            .bearer_auth(config.token.trim())
            .header("Accept", "application/vnd.github+json")
            .json(&body)
            .send()
            .await
            .map_err(|err| RemoteError::Http(err.to_string()))?
    } else {
        client()?
            .patch(format!("{base}/gists/{gist_id}"))
            .bearer_auth(config.token.trim())
            .header("Accept", "application/vnd.github+json")
            .json(&body)
            .send()
            .await
            .map_err(|err| RemoteError::Http(err.to_string()))?
    };
    if !response.status().is_success() {
        return Err(status_error(response).await);
    }
    let value: serde_json::Value = response
        .json()
        .await
        .map_err(|err| RemoteError::Format(err.to_string()))?;
    let location = value
        .get("html_url")
        .and_then(|v| v.as_str())
        .or_else(|| value.get("id").and_then(|v| v.as_str()))
        .unwrap_or_default()
        .to_string();
    let id = value
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    if location.is_empty() {
        return Err(RemoteError::Format("gist response missing id".to_string()));
    }
    Ok(RemoteUpload {
        location,
        bytes: content.len(),
        id,
    })
}

pub async fn gist_download(config: &GistConfig) -> Result<String, RemoteError> {
    if config.token.trim().is_empty() {
        return Err(RemoteError::Config("gist token is required".to_string()));
    }
    let gist_id = config.gist_id.trim();
    if gist_id.is_empty() {
        return Err(RemoteError::Config("gist id is required".to_string()));
    }
    let filename = validate_filename(&config.filename)?;
    let response = client()?
        .get(format!("{}/gists/{gist_id}", trim_slash(&config.api_base)))
        .bearer_auth(config.token.trim())
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|err| RemoteError::Http(err.to_string()))?;
    if !response.status().is_success() {
        return Err(status_error(response).await);
    }
    let value: serde_json::Value = response
        .json()
        .await
        .map_err(|err| RemoteError::Format(err.to_string()))?;
    let files = value
        .get("files")
        .and_then(|v| v.as_object())
        .ok_or_else(|| RemoteError::Format("gist has no files".to_string()))?;
    let file = files
        .get(filename)
        .or_else(|| files.values().next())
        .ok_or_else(|| RemoteError::Format("gist file not found".to_string()))?;
    if let Some(content) = file.get("content").and_then(|v| v.as_str()) {
        if !content.is_empty() {
            return Ok(content.to_string());
        }
    }
    let raw_url = file
        .get("raw_url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| RemoteError::Format("gist file has no content".to_string()))?;
    let response = client()?
        .get(raw_url)
        .bearer_auth(config.token.trim())
        .send()
        .await
        .map_err(|err| RemoteError::Http(err.to_string()))?;
    if !response.status().is_success() {
        return Err(status_error(response).await);
    }
    response
        .text()
        .await
        .map_err(|err| RemoteError::Format(err.to_string()))
}

/// Name of the plain-JSON version index kept next to the WebDAV bundle files.
/// The index is not encrypted: it only holds timestamps, scopes and counts.
pub const WEBDAV_INDEX_FILE: &str = "ferriterm-versions.json";

fn webdav_base(config: &WebdavConfig) -> Result<&str, RemoteError> {
    let base = trim_slash(config.url.trim());
    if base.is_empty() {
        return Err(RemoteError::Config("webdav url is required".to_string()));
    }
    if !base.starts_with("http://") && !base.starts_with("https://") {
        return Err(RemoteError::Config(
            "webdav url must start with http:// or https://".to_string(),
        ));
    }
    Ok(base)
}

fn webdav_file_url(config: &WebdavConfig, filename: &str) -> Result<String, RemoteError> {
    let filename = validate_filename(filename)?;
    Ok(format!("{}/{filename}", webdav_base(config)?))
}

fn version_filename(id: &str) -> Result<String, RemoteError> {
    if id.is_empty()
        || id.len() > 96
        || id.contains("..")
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        return Err(RemoteError::Config("invalid version id".to_string()));
    }
    Ok(format!("{id}.ftbak"))
}

fn auth(builder: reqwest::RequestBuilder, config: &WebdavConfig) -> reqwest::RequestBuilder {
    if config.username.trim().is_empty() {
        builder
    } else {
        builder.basic_auth(config.username.trim(), Some(config.password.as_str()))
    }
}

pub async fn webdav_test(config: &WebdavConfig) -> Result<String, RemoteError> {
    let base = webdav_base(config)?;
    let method = reqwest::Method::from_bytes(b"PROPFIND")
        .map_err(|err| RemoteError::Config(err.to_string()))?;
    let response = auth(client()?.request(method, base), config)
        .header("Depth", "0")
        .send()
        .await
        .map_err(|err| RemoteError::Http(err.to_string()))?;
    if !response.status().is_success() {
        return Err(status_error(response).await);
    }
    Ok(format!("HTTP {}", response.status().as_u16()))
}

/// Uploads a plain file name relative to the WebDAV base URL. Used for the
/// main bundle, the timestamped version bundles and the JSON index alike.
pub async fn webdav_put(
    config: &WebdavConfig,
    filename: &str,
    content: &str,
) -> Result<RemoteUpload, RemoteError> {
    let url = webdav_file_url(config, filename)?;
    let response = auth(client()?.put(url.clone()), config)
        .header("Content-Type", "application/octet-stream")
        .body(content.to_string())
        .send()
        .await
        .map_err(|err| RemoteError::Http(err.to_string()))?;
    if !response.status().is_success() {
        return Err(status_error(response).await);
    }
    Ok(RemoteUpload {
        location: url,
        bytes: content.len(),
        id: String::new(),
    })
}

pub async fn webdav_get(
    config: &WebdavConfig,
    filename: &str,
) -> Result<String, RemoteError> {
    let url = webdav_file_url(config, filename)?;
    let response = auth(client()?.get(url), config)
        .send()
        .await
        .map_err(|err| RemoteError::Http(err.to_string()))?;
    if !response.status().is_success() {
        return Err(status_error(response).await);
    }
    response
        .text()
        .await
        .map_err(|err| RemoteError::Format(err.to_string()))
}

pub async fn webdav_delete(
    config: &WebdavConfig,
    filename: &str,
) -> Result<(), RemoteError> {
    let url = webdav_file_url(config, filename)?;
    let response = auth(client()?.delete(url), config)
        .send()
        .await
        .map_err(|err| RemoteError::Http(err.to_string()))?;
    // A file that is already gone is a successful prune.
    if !response.status().is_success() && response.status().as_u16() != 404 {
        return Err(status_error(response).await);
    }
    Ok(())
}

pub async fn webdav_upload(
    config: &WebdavConfig,
    content: &str,
) -> Result<RemoteUpload, RemoteError> {
    webdav_put(config, &config.filename, content).await
}

pub async fn webdav_download(config: &WebdavConfig) -> Result<String, RemoteError> {
    webdav_get(config, &config.filename).await
}

/// Reads the remote version index. A missing index (first ever sync) is an
/// empty index, not an error, so uploads can bootstrap it.
pub async fn webdav_read_index(
    config: &WebdavConfig,
) -> Result<crate::versions::VersionIndex, RemoteError> {
    let url = webdav_file_url(config, WEBDAV_INDEX_FILE)?;
    let response = auth(client()?.get(url), config)
        .send()
        .await
        .map_err(|err| RemoteError::Http(err.to_string()))?;
    if response.status().as_u16() == 404 {
        return Ok(crate::versions::VersionIndex::default());
    }
    if !response.status().is_success() {
        return Err(status_error(response).await);
    }
    let text = response
        .text()
        .await
        .map_err(|err| RemoteError::Format(err.to_string()))?;
    let mut index: crate::versions::VersionIndex = serde_json::from_str(&text)
        .map_err(|err| RemoteError::Format(format!("invalid version index: {err}")))?;
    index.sort_newest_first();
    Ok(index)
}

pub async fn webdav_write_index(
    config: &WebdavConfig,
    index: &crate::versions::VersionIndex,
) -> Result<(), RemoteError> {
    let body = serde_json::to_string_pretty(index)
        .map_err(|err| RemoteError::Format(err.to_string()))?;
    webdav_put(config, WEBDAV_INDEX_FILE, &body).await?;
    Ok(())
}

pub async fn webdav_put_version(
    config: &WebdavConfig,
    id: &str,
    sealed: &str,
) -> Result<(), RemoteError> {
    webdav_put(config, &version_filename(id)?, sealed).await?;
    Ok(())
}

pub async fn webdav_get_version(
    config: &WebdavConfig,
    id: &str,
) -> Result<String, RemoteError> {
    webdav_get(config, &version_filename(id)?).await
}

pub async fn webdav_delete_version(
    config: &WebdavConfig,
    id: &str,
) -> Result<(), RemoteError> {
    webdav_delete(config, &version_filename(id)?).await
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    use super::*;

    #[derive(Debug, Clone)]
    struct Request {
        method: String,
        path: String,
        body: String,
    }

    async fn read_request(socket: &mut tokio::net::TcpStream) -> Option<Request> {
        let mut buffer = Vec::new();
        let mut chunk = [0u8; 1024];
        loop {
            let read = socket.read(&mut chunk).await.ok()?;
            if read == 0 {
                return None;
            }
            buffer.extend_from_slice(&chunk[..read]);
            let Some(header_end) = buffer.windows(4).position(|w| w == b"\r\n\r\n") else {
                continue;
            };
            let headers = String::from_utf8_lossy(&buffer[..header_end]).to_string();
            let content_length = headers
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .and_then(|value| value.trim().parse::<usize>().ok())
                })
                .unwrap_or(0);
            let body_start = header_end + 4;
            if buffer.len() >= body_start + content_length {
                let mut parts = headers.lines();
                let mut request_line = parts.next()?.split_whitespace();
                let method = request_line.next()?.to_string();
                let path = request_line.next()?.to_string();
                let body =
                    String::from_utf8_lossy(&buffer[body_start..body_start + content_length])
                        .to_string();
                return Some(Request { method, path, body });
            }
        }
    }

    fn spawn_server(
        handler: impl Fn(&Request) -> (u16, String) + Send + Sync + 'static,
    ) -> (String, Arc<Mutex<Vec<Request>>>) {
        let requests = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&requests);
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let addr = listener.local_addr().unwrap();
        let handler = Arc::new(handler);
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            runtime.block_on(async move {
                let listener = TcpListener::from_std(listener).unwrap();
                loop {
                    let Ok((mut socket, _)) = listener.accept().await else {
                        continue;
                    };
                    let Some(request) = read_request(&mut socket).await else {
                        continue;
                    };
                    recorded.lock().unwrap().push(request.clone());
                    let (status, body) = handler(&request);
                    let response = format!(
                        "HTTP/1.1 {status} X\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = socket.write_all(response.as_bytes()).await;
                }
            });
        });
        (format!("http://{addr}"), requests)
    }

    #[tokio::test]
    async fn webdav_roundtrips_and_tests() {
        let store = Arc::new(Mutex::new(String::new()));
        let store_for_handler = Arc::clone(&store);
        let (base, requests) = spawn_server(move |request| match request.method.as_str() {
            "PROPFIND" => (207, String::new()),
            "PUT" => {
                *store_for_handler.lock().unwrap() = request.body.clone();
                (201, String::new())
            }
            "GET" => (200, store_for_handler.lock().unwrap().clone()),
            _ => (405, String::new()),
        });

        let config = WebdavConfig::new(format!("{base}/dav/"), "user", "secret");
        assert_eq!(webdav_test(&config).await.unwrap(), "HTTP 207");
        let upload = webdav_upload(&config, "ciphertext").await.unwrap();
        assert_eq!(upload.bytes, 10);
        assert!(upload.location.ends_with("/dav/ferriterm-backup.ftbak"));
        assert_eq!(webdav_download(&config).await.unwrap(), "ciphertext");

        let requests = requests.lock().unwrap();
        assert_eq!(requests.len(), 3);
        assert_eq!(requests[0].method, "PROPFIND");
        assert_eq!(requests[1].method, "PUT");
        assert_eq!(requests[2].method, "GET");
    }

    #[tokio::test]
    async fn webdav_reports_status_errors() {
        let (base, _) = spawn_server(|_| (500, "boom".to_string()));
        let config = WebdavConfig::new(base, "", "");
        let err = webdav_upload(&config, "data").await.unwrap_err();
        let text = err.to_string();
        assert!(text.contains("500"), "{text}");
        assert!(text.contains("boom"), "{text}");
    }

    #[tokio::test]
    async fn gist_creates_and_downloads() {
        let (base, requests) =
            spawn_server(
                |request| match (request.method.as_str(), request.path.as_str()) {
                    ("GET", "/user") => (200, r#"{"login":"octocat"}"#.to_string()),
                    ("POST", "/gists") => (
                        201,
                        r#"{"id":"g1","html_url":"https://gist.example/g1"}"#.to_string(),
                    ),
                    ("GET", "/gists/g1") => (
                        200,
                        r#"{"files":{"ferriterm-backup.ftbak":{"content":"sealed"}}}"#.to_string(),
                    ),
                    _ => (404, String::new()),
                },
            );

        let mut config = GistConfig::new("token", "");
        config.api_base = base.clone();
        assert_eq!(gist_test(&config).await.unwrap(), "octocat");
        let upload = gist_upload(&config, "sealed").await.unwrap();
        assert_eq!(upload.location, "https://gist.example/g1");
        assert_eq!(upload.id, "g1");

        config.gist_id = "g1".to_string();
        assert_eq!(gist_download(&config).await.unwrap(), "sealed");

        let requests = requests.lock().unwrap();
        let post = requests
            .iter()
            .find(|request| request.method == "POST")
            .unwrap();
        let body: serde_json::Value = serde_json::from_str(&post.body).unwrap();
        assert_eq!(body["files"]["ferriterm-backup.ftbak"]["content"], "sealed");
        assert!(requests
            .iter()
            .all(|request| !request.body.contains("token")));
    }

    #[test]
    fn rejects_unsafe_file_names() {
        let mut config = GistConfig::new("token", "g1");
        config.filename = "../escape".to_string();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        assert!(runtime.block_on(gist_download(&config)).is_err());
        let mut webdav = WebdavConfig::new("http://localhost", "", "");
        webdav.filename = "a/b".to_string();
        assert!(runtime.block_on(webdav_download(&webdav)).is_err());
    }

    #[tokio::test]
    async fn webdav_version_files_and_index() {
        use std::collections::HashMap;

        let files: Arc<Mutex<HashMap<String, String>>> = Arc::new(Mutex::new(HashMap::new()));
        let handler_files = Arc::clone(&files);
        let (base, requests) = spawn_server(move |request| {
            let name = request.path.rsplit('/').next().unwrap_or("").to_string();
            match request.method.as_str() {
                "PUT" => {
                    handler_files
                        .lock()
                        .unwrap()
                        .insert(name, request.body.clone());
                    (201, String::new())
                }
                "GET" => match handler_files.lock().unwrap().get(&name) {
                    Some(body) => (200, body.clone()),
                    None => (404, String::new()),
                },
                "DELETE" => {
                    handler_files.lock().unwrap().remove(&name);
                    (204, String::new())
                }
                _ => (405, String::new()),
            }
        });
        let config = WebdavConfig::new(format!("{base}/dav/"), "user", "secret");

        // No index yet: the first sync must be able to bootstrap it.
        assert!(webdav_read_index(&config).await.unwrap().entries.is_empty());

        let entry = crate::versions::VersionEntry {
            id: "v1".into(),
            created_at: 100,
            scope: vec!["hosts".into()],
            summary: serde_json::json!({"hosts": 2}),
            source: crate::versions::VersionSource::Webdav,
            bytes: 8,
            note: String::new(),
        };
        let mut index = crate::versions::VersionIndex {
            version: crate::versions::INDEX_VERSION,
            entries: vec![entry],
        };
        index.sort_newest_first();
        webdav_write_index(&config, &index).await.unwrap();

        webdav_put_version(&config, "v1", "SEALED-1").await.unwrap();
        assert_eq!(webdav_get_version(&config, "v1").await.unwrap(), "SEALED-1");
        let read_back = webdav_read_index(&config).await.unwrap();
        assert_eq!(read_back.entries.len(), 1);
        assert_eq!(read_back.entries[0].id, "v1");
        assert_eq!(read_back.entries[0].summary["hosts"], 2);

        webdav_delete_version(&config, "v1").await.unwrap();
        assert!(webdav_get_version(&config, "v1").await.is_err());
        let logged = requests.lock().unwrap();
        assert!(logged.iter().any(|request| request.method == "DELETE"));
        assert!(logged
            .iter()
            .any(|request| request.path.ends_with("ferriterm-versions.json")));
    }
}
