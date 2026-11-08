//! S3-compatible virtual filesystem (path-style access, AWS SigV4).

use std::time::Duration;

use quick_xml::events::Event;
use quick_xml::Reader;
use ring::digest::{digest, SHA256};
use ring::hmac;

use crate::timeutil::{amz_datetime, parse_http_date, parse_iso8601, unix_now};
use crate::util::{encode_path, encode_segment};
use crate::{FileEntry, OpenMode, Vfs, VfsFile, VfsFuture};

const MAX_OBJECT_BYTES: u64 = 64 * 1024 * 1024;
const EMPTY_SHA256: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

#[derive(Clone)]
pub struct S3Vfs {
    endpoint: String,
    host: String,
    region: String,
    bucket: String,
    access_key: String,
    secret_key: String,
    prefix: String,
    client: reqwest::Client,
}

impl S3Vfs {
    pub fn new(
        endpoint: &str,
        region: &str,
        bucket: &str,
        access_key: &str,
        secret_key: &str,
        prefix: &str,
    ) -> Result<Self, String> {
        let endpoint = endpoint.trim().trim_end_matches('/').to_string();
        let parsed =
            reqwest::Url::parse(&endpoint).map_err(|err| format!("invalid S3 endpoint: {err}"))?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err("S3 endpoint must use http or https".to_string());
        }
        let host = match (parsed.host_str(), parsed.port()) {
            (Some(host), Some(port)) => format!("{host}:{port}"),
            (Some(host), None) => host.to_string(),
            (None, _) => return Err("S3 endpoint is missing a host".to_string()),
        };
        let bucket = bucket.trim().trim_matches('/').to_string();
        if bucket.is_empty()
            || !bucket
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '.' | '_'))
        {
            return Err("invalid S3 bucket name".to_string());
        }
        if access_key.trim().is_empty() || secret_key.is_empty() {
            return Err("S3 access key and secret key are required".to_string());
        }
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(60))
            .connect_timeout(Duration::from_secs(10))
            .user_agent(concat!("Ferriterm/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|err| err.to_string())?;
        Ok(Self {
            endpoint,
            host,
            region: if region.trim().is_empty() {
                "us-east-1".to_string()
            } else {
                region.trim().to_string()
            },
            bucket,
            access_key: access_key.trim().to_string(),
            secret_key: secret_key.to_string(),
            prefix: prefix.trim().trim_matches('/').to_string(),
            client,
        })
    }

    fn key_for(&self, path: &str, directory: bool) -> String {
        let rel = path.trim_matches('/');
        let mut key = if self.prefix.is_empty() {
            rel.to_string()
        } else if rel.is_empty() {
            self.prefix.clone()
        } else {
            format!("{}/{}", self.prefix, rel)
        };
        if directory && !key.is_empty() && !key.ends_with('/') {
            key.push('/');
        }
        key
    }

    fn path_for(&self, key: &str) -> String {
        let rel = if self.prefix.is_empty() {
            key
        } else {
            key.strip_prefix(&self.prefix)
                .unwrap_or(key)
                .trim_start_matches('/')
        };
        let rel = rel.trim_end_matches('/');
        if rel.is_empty() {
            "/".to_string()
        } else {
            format!("/{rel}")
        }
    }

    fn uri_for(&self, key: &str) -> String {
        format!(
            "{}/{}/{}",
            self.endpoint,
            encode_segment(&self.bucket),
            encode_path(key)
        )
    }

    async fn request(
        &self,
        method: &str,
        key: &str,
        query: &[(&str, &str)],
        body: Vec<u8>,
        extra: &[(&str, &str)],
    ) -> Result<reqwest::Response, String> {
        let uri = format!("/{}/{}", self.bucket, encode_path(key));
        let mut pairs: Vec<(String, String)> = query
            .iter()
            .map(|(name, value)| (encode_segment(name), encode_segment(value)))
            .collect();
        pairs.sort();
        let canonical_query = pairs
            .iter()
            .map(|(name, value)| format!("{name}={value}"))
            .collect::<Vec<_>>()
            .join("&");
        let url = if canonical_query.is_empty() {
            self.uri_for(key)
        } else {
            format!("{}?{canonical_query}", self.uri_for(key))
        };

        let payload_hash = if body.is_empty() {
            EMPTY_SHA256.to_string()
        } else {
            sha256_hex(&body)
        };
        let (amzdate, date) = amz_datetime(unix_now());
        let mut headers: Vec<(String, String)> = vec![
            ("host".to_string(), self.host.clone()),
            ("x-amz-content-sha256".to_string(), payload_hash.clone()),
            ("x-amz-date".to_string(), amzdate.clone()),
        ];
        for (name, value) in extra {
            headers.push((name.to_string(), value.to_string()));
        }
        headers.sort_by(|a, b| a.0.cmp(&b.0));
        let canonical_headers = headers
            .iter()
            .map(|(name, value)| format!("{name}:{}\n", value.trim()))
            .collect::<String>();
        let signed_headers = headers
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>()
            .join(";");

        let signature = sign_v4(
            &self.secret_key,
            &self.region,
            &amzdate,
            &date,
            method,
            &uri,
            &canonical_query,
            &canonical_headers,
            &signed_headers,
            &payload_hash,
        );
        let authorization = format!(
            "AWS4-HMAC-SHA256 Credential={}/{date}/{}/s3/aws4_request, SignedHeaders={signed_headers}, Signature={signature}",
            self.access_key, self.region
        );

        let mut builder = self
            .client
            .request(
                reqwest::Method::from_bytes(method.as_bytes()).map_err(|err| err.to_string())?,
                url,
            )
            .header("x-amz-content-sha256", payload_hash)
            .header("x-amz-date", amzdate)
            .header("Authorization", authorization);
        for (name, value) in extra {
            builder = builder.header(*name, *value);
        }
        builder = builder.body(body);
        builder.send().await.map_err(|err| err.to_string())
    }

    async fn list_page(&self, key_prefix: &str, token: &str) -> Result<S3List, String> {
        let mut query = vec![
            ("list-type", "2"),
            ("delimiter", "/"),
            ("prefix", key_prefix),
            ("max-keys", "1000"),
        ];
        if !token.is_empty() {
            query.push(("continuation-token", token));
        }
        let response = self.request("GET", "", &query, Vec::new(), &[]).await?;
        let response = check_status(response).await?;
        let body = response.text().await.map_err(|err| err.to_string())?;
        parse_list(&body)
    }

    async fn list_all(&self, key_prefix: &str) -> Result<Vec<String>, String> {
        let mut keys = Vec::new();
        let mut token = String::new();
        for _ in 0..20 {
            let page = self.list_page_recursive(key_prefix, &token).await?;
            keys.extend(page.files.into_iter().map(|(key, _, _)| key));
            keys.extend(page.dirs);
            if page.next_token.is_empty() {
                break;
            }
            token = page.next_token;
        }
        Ok(keys)
    }

    async fn list_page_recursive(&self, key_prefix: &str, token: &str) -> Result<S3List, String> {
        let mut query = vec![
            ("list-type", "2"),
            ("prefix", key_prefix),
            ("max-keys", "1000"),
        ];
        if !token.is_empty() {
            query.push(("continuation-token", token));
        }
        let response = self.request("GET", "", &query, Vec::new(), &[]).await?;
        let response = check_status(response).await?;
        let body = response.text().await.map_err(|err| err.to_string())?;
        parse_list(&body)
    }

    async fn put_object(&self, key: &str, body: Vec<u8>) -> Result<(), String> {
        let response = self.request("PUT", key, &[], body, &[]).await?;
        check_status(response).await?;
        Ok(())
    }

    async fn delete_object(&self, key: &str) -> Result<(), String> {
        let response = self.request("DELETE", key, &[], Vec::new(), &[]).await?;
        if response.status().as_u16() == 404 {
            return Ok(());
        }
        check_status(response).await?;
        Ok(())
    }

    async fn copy_object(&self, from: &str, to: &str) -> Result<(), String> {
        let source = format!("/{}/{}", self.bucket, encode_path(from));
        let response = self
            .request(
                "PUT",
                to,
                &[],
                Vec::new(),
                &[("x-amz-copy-source", &source)],
            )
            .await?;
        check_status(response).await?;
        Ok(())
    }
}

struct S3List {
    dirs: Vec<String>,
    files: Vec<(String, u64, u64)>,
    next_token: String,
}

fn parse_list(xml: &str) -> Result<S3List, String> {
    let mut reader = Reader::from_str(xml);
    let mut list = S3List {
        dirs: Vec::new(),
        files: Vec::new(),
        next_token: String::new(),
    };
    let mut capture: Option<&'static str> = None;
    let mut text = String::new();
    let mut in_contents = false;
    let mut in_common = false;
    let mut key = String::new();
    let mut size = 0u64;
    let mut mtime = 0u64;
    loop {
        match reader.read_event() {
            Ok(Event::Start(event)) => match event.local_name().as_ref() {
                "Contents" => {
                    in_contents = true;
                    key.clear();
                    size = 0;
                    mtime = 0;
                }
                "CommonPrefixes" => in_common = true,
                "Key" if in_contents => capture = Some("key"),
                "Size" => capture = Some("size"),
                "LastModified" => capture = Some("mtime"),
                "Prefix" if in_common => capture = Some("prefix"),
                "NextContinuationToken" => capture = Some("token"),
                _ => {}
            },
            Ok(Event::Text(event)) => {
                if capture.is_some() {
                    let value = quick_xml::escape::unescape(event.as_ref())
                        .map_err(|err| err.to_string())?;
                    text.push_str(&value);
                }
            }
            Ok(Event::End(event)) => {
                if let Some(field) = capture.take() {
                    let value = text.trim();
                    match field {
                        "key" => key = value.to_string(),
                        "size" => size = value.parse().unwrap_or(0),
                        "mtime" => mtime = parse_iso8601(value),
                        "prefix" => list.dirs.push(value.to_string()),
                        "token" => list.next_token = value.to_string(),
                        _ => {}
                    }
                    text.clear();
                }
                match event.local_name().as_ref() {
                    "Contents" => {
                        if !key.is_empty() {
                            list.files.push((key.clone(), size, mtime));
                        }
                        in_contents = false;
                    }
                    "CommonPrefixes" => in_common = false,
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(err) => return Err(format!("invalid S3 response: {err}")),
            _ => {}
        }
    }
    Ok(list)
}

async fn check_status(response: reqwest::Response) -> Result<reqwest::Response, String> {
    if response.status().is_success() {
        return Ok(response);
    }
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    let body: String = body.chars().take(300).collect();
    Err(format!("S3 returned {status}: {body}"))
}

fn sha256_hex(data: &[u8]) -> String {
    hex(digest(&SHA256, data).as_ref())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn hmac(key: &[u8], data: &[u8]) -> Vec<u8> {
    hmac::sign(&hmac::Key::new(hmac::HMAC_SHA256, key), data)
        .as_ref()
        .to_vec()
}

#[allow(clippy::too_many_arguments)]
fn sign_v4(
    secret: &str,
    region: &str,
    amzdate: &str,
    date: &str,
    method: &str,
    uri: &str,
    query: &str,
    canonical_headers: &str,
    signed_headers: &str,
    payload_hash: &str,
) -> String {
    let canonical_request =
        format!("{method}\n{uri}\n{query}\n{canonical_headers}\n{signed_headers}\n{payload_hash}");
    let scope = format!("{date}/{region}/s3/aws4_request");
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{amzdate}\n{scope}\n{}",
        sha256_hex(canonical_request.as_bytes())
    );
    let k_date = hmac(format!("AWS4{secret}").as_bytes(), date.as_bytes());
    let k_region = hmac(&k_date, region.as_bytes());
    let k_service = hmac(&k_region, b"s3");
    let k_signing = hmac(&k_service, b"aws4_request");
    hex(&hmac(&k_signing, string_to_sign.as_bytes()))
}

struct S3File {
    vfs: S3Vfs,
    key: String,
    data: Vec<u8>,
    writable: bool,
}

impl VfsFile for S3File {
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
                return Err("S3 object is too large".to_string());
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
                self.vfs.put_object(&self.key, self.data.clone()).await?;
            }
            Ok(())
        })
    }
}

impl Vfs for S3Vfs {
    fn list<'a>(&'a self, path: &'a str, show_hidden: bool) -> VfsFuture<'a, Vec<FileEntry>> {
        Box::pin(async move {
            let prefix = self.key_for(path, true);
            let mut token = String::new();
            let mut out = Vec::new();
            for _ in 0..10 {
                let page = self.list_page(&prefix, &token).await?;
                for key in &page.dirs {
                    let child = self.path_for(key);
                    if child == "/" {
                        continue;
                    }
                    out.push(self.entry(&child, true, 0, 0));
                }
                for (key, size, mtime) in &page.files {
                    if key.ends_with('/') && *size == 0 {
                        let child = self.path_for(key);
                        if child != "/" {
                            out.push(self.entry(&child, true, 0, *mtime));
                        }
                        continue;
                    }
                    if *key == prefix {
                        continue;
                    }
                    let child = self.path_for(key);
                    out.push(self.entry(&child, false, *size, *mtime));
                }
                if page.next_token.is_empty() {
                    break;
                }
                token = page.next_token;
            }
            if !show_hidden {
                out.retain(|entry| !entry.name.starts_with('.'));
            }
            out.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(a.name.cmp(&b.name)));
            out.dedup_by(|a, b| a.path == b.path);
            Ok(out)
        })
    }

    fn stat<'a>(&'a self, path: &'a str) -> VfsFuture<'a, FileEntry> {
        Box::pin(async move {
            if path.trim_matches('/').is_empty() {
                return Ok(self.entry("/", true, 0, 0));
            }
            let key = self.key_for(path, false);
            let response = self.request("HEAD", &key, &[], Vec::new(), &[]).await?;
            if response.status().is_success() {
                let size = response.content_length().unwrap_or(0);
                let mtime = response
                    .headers()
                    .get("last-modified")
                    .and_then(|value| value.to_str().ok())
                    .map(parse_http_date)
                    .unwrap_or(0);
                return Ok(self.entry(path, false, size, mtime));
            }
            let page = self.list_page(&self.key_for(path, true), "").await?;
            if !page.dirs.is_empty() || !page.files.is_empty() {
                return Ok(self.entry(path, true, 0, 0));
            }
            Err(format!("S3 object {key} not found"))
        })
    }

    fn home<'a>(&'a self) -> VfsFuture<'a, String> {
        Box::pin(async { Ok("/".to_string()) })
    }

    fn mkdir<'a>(&'a self, path: &'a str) -> VfsFuture<'a, ()> {
        Box::pin(async move {
            let key = self.key_for(path, true);
            if key.is_empty() {
                return Err("cannot create the bucket root".to_string());
            }
            self.put_object(&key, Vec::new()).await
        })
    }

    fn remove_file<'a>(&'a self, path: &'a str) -> VfsFuture<'a, ()> {
        Box::pin(async move { self.delete_object(&self.key_for(path, false)).await })
    }

    fn remove_dir_all<'a>(&'a self, path: &'a str) -> VfsFuture<'a, ()> {
        Box::pin(async move {
            let prefix = self.key_for(path, true);
            for key in self.list_all(&prefix).await? {
                self.delete_object(&key).await?;
            }
            if !prefix.is_empty() {
                self.delete_object(&prefix).await?;
            }
            Ok(())
        })
    }

    fn rename<'a>(&'a self, from: &'a str, to: &'a str) -> VfsFuture<'a, ()> {
        Box::pin(async move {
            let source = self.stat(from).await?;
            let from_key = self.key_for(from, source.is_dir);
            let to_key = self.key_for(to, source.is_dir);
            if source.is_dir {
                let prefix = from_key.clone();
                for key in self.list_all(&prefix).await? {
                    let suffix = key.strip_prefix(&prefix).unwrap_or(&key);
                    let target = format!("{to_key}/{suffix}");
                    self.copy_object(&key, &target).await?;
                    self.delete_object(&key).await?;
                }
                self.delete_object(&from_key).await?;
            } else {
                self.copy_object(&from_key, &to_key).await?;
                self.delete_object(&from_key).await?;
            }
            Ok(())
        })
    }

    fn chmod<'a>(&'a self, _path: &'a str, _mode: u32) -> VfsFuture<'a, ()> {
        Box::pin(async { Err("S3 does not support POSIX permissions".to_string()) })
    }

    fn open<'a>(&'a self, path: &'a str, mode: OpenMode) -> VfsFuture<'a, Box<dyn VfsFile + Send>> {
        Box::pin(async move {
            let key = self.key_for(path, false);
            let writable = matches!(mode, OpenMode::Write { .. });
            let mut data = Vec::new();
            let read = matches!(mode, OpenMode::Read)
                || matches!(mode, OpenMode::Write { truncate: false });
            if read {
                let response = self.request("GET", &key, &[], Vec::new(), &[]).await?;
                let response = check_status(response).await?;
                if let Some(length) = response.content_length() {
                    if length > MAX_OBJECT_BYTES {
                        return Err("S3 object is too large to open".to_string());
                    }
                }
                data = response
                    .bytes()
                    .await
                    .map_err(|err| err.to_string())?
                    .to_vec();
            }
            Ok(Box::new(S3File {
                vfs: self.clone(),
                key,
                data,
                writable,
            }) as Box<dyn VfsFile + Send>)
        })
    }
}

impl S3Vfs {
    fn entry(&self, path: &str, is_dir: bool, size: u64, mtime: u64) -> FileEntry {
        FileEntry {
            name: path
                .trim_end_matches('/')
                .rsplit('/')
                .next()
                .unwrap_or_default()
                .to_string(),
            path: path.to_string(),
            size,
            mode: if is_dir { 0o755 } else { 0o644 },
            uid: 0,
            gid: 0,
            mtime,
            is_dir,
            is_symlink: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vfs() -> S3Vfs {
        S3Vfs::new(
            "https://s3.example.com",
            "us-east-1",
            "bucket",
            "AKID",
            "SECRET",
            "team/backups",
        )
        .unwrap()
    }

    #[test]
    fn maps_paths_to_keys() {
        let vfs = vfs();
        assert_eq!(vfs.key_for("/", true), "team/backups/");
        assert_eq!(
            vfs.key_for("/logs/app.log", false),
            "team/backups/logs/app.log"
        );
        assert_eq!(vfs.path_for("team/backups/logs/app.log"), "/logs/app.log");
        assert_eq!(vfs.path_for("team/backups/"), "/");
        assert_eq!(
            vfs.uri_for("team/backups/a b.log"),
            "https://s3.example.com/bucket/team/backups/a%20b.log"
        );
    }

    #[test]
    fn matches_aws_signature_vector() {
        // AWS S3 SigV4 example: GET /test.txt with a Range header.
        let headers = "host:examplebucket.s3.amazonaws.com\nrange:bytes=0-9\nx-amz-content-sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855\nx-amz-date:20130524T000000Z\n";
        let signature = sign_v4(
            "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
            "us-east-1",
            "20130524T000000Z",
            "20130524",
            "GET",
            "/test.txt",
            "",
            headers,
            "host;range;x-amz-content-sha256;x-amz-date",
            EMPTY_SHA256,
        );
        assert_eq!(
            signature,
            "f0e8bdb87c964420e857bd35b5d6ed310bd44f0170aba48dd91039c6036bdb41"
        );
    }

    #[test]
    fn parses_list_objects_v2() {
        let xml = r#"<?xml version="1.0"?>
        <ListBucketResult xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
          <Name>bucket</Name>
          <Prefix>team/backups/</Prefix>
          <IsTruncated>true</IsTruncated>
          <NextContinuationToken>next</NextContinuationToken>
          <Contents>
            <Key>team/backups/app.log</Key>
            <LastModified>2024-01-31T12:00:00.000Z</LastModified>
            <Size>128</Size>
          </Contents>
          <CommonPrefixes><Prefix>team/backups/2023/</Prefix></CommonPrefixes>
        </ListBucketResult>"#;
        let list = parse_list(xml).unwrap();
        assert_eq!(list.files.len(), 1);
        assert_eq!(list.files[0].0, "team/backups/app.log");
        assert_eq!(list.files[0].1, 128);
        assert_eq!(list.files[0].2, 1_706_702_400);
        assert_eq!(list.dirs, vec!["team/backups/2023/"]);
        assert_eq!(list.next_token, "next");
    }

    #[test]
    fn rejects_invalid_configuration() {
        assert!(S3Vfs::new("ftp://x", "r", "b", "a", "s", "").is_err());
        assert!(S3Vfs::new("https://x", "r", "", "a", "s", "").is_err());
        assert!(S3Vfs::new("https://x", "r", "b", "", "s", "").is_err());
    }
}
