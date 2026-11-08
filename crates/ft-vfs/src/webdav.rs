//! WebDAV-backed virtual filesystem (PROPFIND/MKCOL/MOVE/GET/PUT).

use std::time::Duration;

use quick_xml::events::Event;
use quick_xml::Reader;

use crate::timeutil::parse_http_date;
use crate::util::{decode, encode_path};
use crate::{FileEntry, OpenMode, Vfs, VfsFile, VfsFuture};

const PROPFIND_BODY: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<d:propfind xmlns:d="DAV:">
  <d:prop>
    <d:displayname/>
    <d:getcontentlength/>
    <d:getlastmodified/>
    <d:resourcetype/>
  </d:prop>
</d:propfind>"#;

const MAX_OBJECT_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Clone)]
pub struct WebdavVfs {
    base: String,
    base_path: String,
    username: String,
    password: String,
    client: reqwest::Client,
}

impl WebdavVfs {
    pub fn new(base: &str, username: &str, password: &str) -> Result<Self, String> {
        let base = base.trim().trim_end_matches('/').to_string();
        let parsed =
            reqwest::Url::parse(&base).map_err(|err| format!("invalid WebDAV URL: {err}"))?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err("WebDAV URL must use http or https".to_string());
        }
        let base_path = parsed.path().trim_end_matches('/').to_string();
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .user_agent(concat!("Ferriterm/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|err| err.to_string())?;
        Ok(Self {
            base,
            base_path,
            username: username.to_string(),
            password: password.to_string(),
            client,
        })
    }

    fn url_for(&self, path: &str) -> String {
        let rel = path.trim_matches('/');
        if rel.is_empty() {
            self.base.clone()
        } else {
            format!("{}/{}", self.base, encode_path(rel))
        }
    }

    fn auth(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        if self.username.is_empty() {
            request
        } else {
            request.basic_auth(&self.username, Some(&self.password))
        }
    }

    fn vfs_path(&self, href: &str) -> String {
        let decoded = decode(href);
        let path = if decoded.contains("://") {
            reqwest::Url::parse(&decoded)
                .map(|url| url.path().to_string())
                .unwrap_or(decoded)
        } else {
            decoded
        };
        let base_path = self.base_path.as_str();
        let rel = if path == base_path {
            ""
        } else {
            path.strip_prefix(base_path).unwrap_or(path.as_str())
        };
        let rel = rel.trim_end_matches('/');
        if rel.is_empty() {
            "/".to_string()
        } else if rel.starts_with('/') {
            rel.to_string()
        } else {
            format!("/{rel}")
        }
    }

    fn entry_from(&self, dav: &DavEntry) -> FileEntry {
        let path = self.vfs_path(&dav.href);
        let name = path
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap_or_default()
            .to_string();
        FileEntry {
            name,
            path,
            size: dav.size,
            mode: if dav.is_dir { 0o755 } else { 0o644 },
            uid: 0,
            gid: 0,
            mtime: dav.mtime,
            is_dir: dav.is_dir,
            is_symlink: false,
        }
    }

    async fn propfind(&self, url: &str, depth: &str) -> Result<Vec<DavEntry>, String> {
        let request = self
            .auth(self.client.request(
                reqwest::Method::from_bytes(b"PROPFIND").map_err(|err| err.to_string())?,
                url,
            ))
            .header("Depth", depth)
            .header("Content-Type", "application/xml; charset=utf-8")
            .body(PROPFIND_BODY);
        let response = request.send().await.map_err(|err| err.to_string())?;
        let response = check_status(response).await?;
        let body = response.text().await.map_err(|err| err.to_string())?;
        parse_propfind(&body)
    }
}

struct DavEntry {
    href: String,
    is_dir: bool,
    size: u64,
    mtime: u64,
}

fn parse_propfind(xml: &str) -> Result<Vec<DavEntry>, String> {
    let mut reader = Reader::from_str(xml);
    let mut entries = Vec::new();
    let mut current: Option<DavEntry> = None;
    let mut is_dir = false;
    let mut capture: Option<&'static str> = None;
    let mut text = String::new();
    loop {
        match reader.read_event() {
            Ok(Event::Start(event)) => match event.local_name().as_ref() {
                "response" => {
                    current = Some(DavEntry {
                        href: String::new(),
                        is_dir: false,
                        size: 0,
                        mtime: 0,
                    });
                    is_dir = false;
                }
                "collection" => is_dir = true,
                "href" => capture = Some("href"),
                "getcontentlength" => capture = Some("size"),
                "getlastmodified" => capture = Some("mtime"),
                _ => {}
            },
            Ok(Event::Empty(event)) => {
                if event.local_name().as_ref() == "collection" {
                    is_dir = true;
                }
            }
            Ok(Event::Text(event)) => {
                if capture.is_some() {
                    let value = quick_xml::escape::unescape(event.as_ref())
                        .map_err(|err| err.to_string())?;
                    text.push_str(&value);
                }
            }
            Ok(Event::End(event)) => match event.local_name().as_ref() {
                "href" | "getcontentlength" | "getlastmodified" => {
                    if let (Some(entry), Some(field)) = (current.as_mut(), capture.take()) {
                        let value = text.trim();
                        match field {
                            "href" => entry.href = value.to_string(),
                            "size" => entry.size = value.parse().unwrap_or(0),
                            "mtime" => entry.mtime = parse_http_date(value),
                            _ => {}
                        }
                    }
                    text.clear();
                }
                "response" => {
                    if let Some(mut entry) = current.take() {
                        entry.is_dir = is_dir;
                        if !entry.href.is_empty() {
                            entries.push(entry);
                        }
                    }
                    is_dir = false;
                }
                _ => {}
            },
            Ok(Event::Eof) => break,
            Err(err) => return Err(format!("invalid WebDAV response: {err}")),
            _ => {}
        }
    }
    Ok(entries)
}

async fn check_status(response: reqwest::Response) -> Result<reqwest::Response, String> {
    if response.status().is_success() {
        return Ok(response);
    }
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    let body: String = body.chars().take(200).collect();
    Err(format!("WebDAV returned {status}: {body}"))
}

struct WebdavFile {
    client: reqwest::Client,
    url: String,
    username: String,
    password: String,
    data: Vec<u8>,
    writable: bool,
}

impl WebdavFile {
    fn auth(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        if self.username.is_empty() {
            request
        } else {
            request.basic_auth(&self.username, Some(&self.password))
        }
    }
}

impl VfsFile for WebdavFile {
    fn read<'a>(&'a mut self, offset: u64, len: u32) -> VfsFuture<'a, Vec<u8>> {
        Box::pin(async move {
            let start = usize::try_from(offset).map_err(|_| "offset out of range".to_string())?;
            if start >= self.data.len() {
                return Ok(Vec::new());
            }
            let end = (start + len as usize).min(self.data.len());
            Ok(self.data[start..end].to_vec())
        })
    }

    fn write<'a>(&'a mut self, offset: u64, data: &'a [u8]) -> VfsFuture<'a, ()> {
        Box::pin(async move {
            if !self.writable {
                return Err("file is opened read-only".to_string());
            }
            let start = usize::try_from(offset).map_err(|_| "offset out of range".to_string())?;
            let end = start + data.len();
            if end > MAX_OBJECT_BYTES as usize {
                return Err("WebDAV file is too large".to_string());
            }
            if self.data.len() < end {
                self.data.resize(end, 0);
            }
            self.data[start..end].copy_from_slice(data);
            Ok(())
        })
    }

    fn close<'a>(&'a mut self) -> VfsFuture<'a, ()> {
        Box::pin(async move {
            if self.writable {
                let response = self
                    .auth(self.client.put(&self.url))
                    .header("Content-Type", "application/octet-stream")
                    .body(self.data.clone())
                    .send()
                    .await
                    .map_err(|err| err.to_string())?;
                check_status(response).await?;
            }
            Ok(())
        })
    }
}

impl Vfs for WebdavVfs {
    fn list<'a>(&'a self, path: &'a str, show_hidden: bool) -> VfsFuture<'a, Vec<FileEntry>> {
        Box::pin(async move {
            let url = self.url_for(path);
            let entries = self.propfind(&url, "1").await?;
            let self_path = self.vfs_path(&url);
            let mut out = Vec::new();
            for entry in entries {
                let vfs_path = self.vfs_path(&entry.href);
                if vfs_path == self_path {
                    continue;
                }
                let file = self.entry_from(&entry);
                if file.name.is_empty() {
                    continue;
                }
                if !show_hidden && file.name.starts_with('.') {
                    continue;
                }
                out.push(file);
            }
            out.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(a.name.cmp(&b.name)));
            Ok(out)
        })
    }

    fn stat<'a>(&'a self, path: &'a str) -> VfsFuture<'a, FileEntry> {
        Box::pin(async move {
            let url = self.url_for(path);
            let mut entries = self.propfind(&url, "0").await?;
            let entry = entries
                .drain(..)
                .next()
                .ok_or_else(|| "path not found".to_string())?;
            Ok(self.entry_from(&entry))
        })
    }

    fn home<'a>(&'a self) -> VfsFuture<'a, String> {
        Box::pin(async { Ok("/".to_string()) })
    }

    fn mkdir<'a>(&'a self, path: &'a str) -> VfsFuture<'a, ()> {
        Box::pin(async move {
            let response = self
                .auth(self.client.request(
                    reqwest::Method::from_bytes(b"MKCOL").map_err(|err| err.to_string())?,
                    self.url_for(path),
                ))
                .send()
                .await
                .map_err(|err| err.to_string())?;
            if response.status().as_u16() == 405 {
                return Ok(());
            }
            check_status(response).await?;
            Ok(())
        })
    }

    fn remove_file<'a>(&'a self, path: &'a str) -> VfsFuture<'a, ()> {
        Box::pin(async move {
            let response = self
                .auth(self.client.delete(self.url_for(path)))
                .send()
                .await
                .map_err(|err| err.to_string())?;
            check_status(response).await?;
            Ok(())
        })
    }

    fn remove_dir_all<'a>(&'a self, path: &'a str) -> VfsFuture<'a, ()> {
        self.remove_file(path)
    }

    fn rename<'a>(&'a self, from: &'a str, to: &'a str) -> VfsFuture<'a, ()> {
        Box::pin(async move {
            let destination = self.url_for(to);
            let response = self
                .auth(self.client.request(
                    reqwest::Method::from_bytes(b"MOVE").map_err(|err| err.to_string())?,
                    self.url_for(from),
                ))
                .header("Destination", destination)
                .header("Overwrite", "T")
                .send()
                .await
                .map_err(|err| err.to_string())?;
            check_status(response).await?;
            Ok(())
        })
    }

    fn chmod<'a>(&'a self, _path: &'a str, _mode: u32) -> VfsFuture<'a, ()> {
        Box::pin(async { Err("WebDAV does not support POSIX permissions".to_string()) })
    }

    fn open<'a>(&'a self, path: &'a str, mode: OpenMode) -> VfsFuture<'a, Box<dyn VfsFile + Send>> {
        Box::pin(async move {
            let url = self.url_for(path);
            let writable = matches!(mode, OpenMode::Write { .. });
            let mut data = Vec::new();
            if matches!(mode, OpenMode::Read) {
                let response = self
                    .auth(self.client.get(&url))
                    .send()
                    .await
                    .map_err(|err| err.to_string())?;
                let response = check_status(response).await?;
                if let Some(length) = response.content_length() {
                    if length > MAX_OBJECT_BYTES {
                        return Err("WebDAV file is too large to open".to_string());
                    }
                }
                data = response
                    .bytes()
                    .await
                    .map_err(|err| err.to_string())?
                    .to_vec();
            } else if let OpenMode::Write { truncate: false } = mode {
                if let Ok(response) = self.auth(self.client.get(&url)).send().await {
                    if response.status().is_success() {
                        data = response
                            .bytes()
                            .await
                            .map_err(|err| err.to_string())?
                            .to_vec();
                    }
                }
            }
            Ok(Box::new(WebdavFile {
                client: self.client.clone(),
                url,
                username: self.username.clone(),
                password: self.password.clone(),
                data,
                writable,
            }) as Box<dyn VfsFile + Send>)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_propfind_responses() {
        let xml = r#"<?xml version="1.0"?>
        <d:multistatus xmlns:d="DAV:">
          <d:response>
            <d:href>/dav/</d:href>
            <d:propstat><d:prop><d:resourcetype><d:collection/></d:resourcetype></d:prop></d:propstat>
          </d:response>
          <d:response>
            <d:href>/dav/report.txt</d:href>
            <d:propstat><d:prop>
              <d:getcontentlength>42</d:getcontentlength>
              <d:getlastmodified>Wed, 21 Oct 2015 07:28:00 GMT</d:getlastmodified>
              <d:resourcetype/>
            </d:prop></d:propstat>
          </d:response>
        </d:multistatus>"#;
        let entries = parse_propfind(xml).unwrap();
        assert_eq!(entries.len(), 2);
        assert!(entries[0].is_dir);
        assert_eq!(entries[1].href, "/dav/report.txt");
        assert_eq!(entries[1].size, 42);
        assert_eq!(entries[1].mtime, 1_445_412_480);
    }

    #[test]
    fn builds_paths_and_urls() {
        let vfs = WebdavVfs::new("https://example.com/dav/", "u", "p").unwrap();
        assert_eq!(vfs.url_for("/"), "https://example.com/dav");
        assert_eq!(
            vfs.url_for("/folder/a b.txt"),
            "https://example.com/dav/folder/a%20b.txt"
        );
        assert_eq!(vfs.vfs_path("/dav/folder/a%20b.txt"), "/folder/a b.txt");
        assert_eq!(vfs.vfs_path("/dav/"), "/");
    }

    #[test]
    fn rejects_invalid_urls() {
        assert!(WebdavVfs::new("ftp://example.com", "", "").is_err());
        assert!(WebdavVfs::new("not a url", "", "").is_err());
    }
}
