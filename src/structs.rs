use failure::Error;
use mysql::chrono::prelude::NaiveDateTime;
use crate::rdedup::Repo as RdedupRepo;
use std;
use slog;
use slog::Logger;
use url;

#[derive(Debug, PartialEq, Eq, Hash, Serialize)]
pub struct File {
    pub id: u64,
    pub device_id: String,
    pub original_name: String,
    pub versions: Vec<FileVersion>
}

#[derive(Debug, PartialEq, Eq, Hash, Serialize, Clone)]
pub struct FileVersion {
    pub version: u64,
    pub size: u64,
    pub hash: String,
    pub created: NaiveDateTime,
    pub mtime: NaiveDateTime,
    pub storage_name: String
}

pub struct Repo {
    pub repo: RdedupRepo,
    pub pass: Box<dyn Fn() -> std::io::Result<String> + Send + Sync>
}

impl Repo {
    pub fn new(root: &str, name: &str, pass: String, _logger: &Logger) -> Result<Repo, Error> {

        // TODO improve logging for rdedup_lib
        let logger: Logger = Logger::root(
            slog::Discard,
            o!("component" => "rdedup")
        );

        RdedupRepo::open(&url::Url::parse(&format!("file://{}/{}", root, name))?, logger)
            .map(|repo| {
                Repo {
                    repo,
                    pass: Box::new(move || { Ok(pass.clone()) })
                }
            }).map_err(Error::from)
    }
}

#[derive(Debug)]
pub struct UploadedFile {
    pub original_name: String,
    pub size: u64,
    pub mtime: NaiveDateTime,
    pub account_id: String,
    pub device_id: String,
    pub identity_hash: String,
}

#[derive(Debug, Clone)]
pub struct DeviceIdentity {
    pub id: String,
    pub account_id: String,
    pub repo_pass: String
}
