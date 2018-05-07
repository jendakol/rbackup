extern crate failure;
extern crate mysql;
extern crate slog;

use failure::Error;
use mysql::chrono::prelude::NaiveDateTime;
use rdedup::Repo as RdedupRepo;
use std;

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
    pub storage_name: String
}

pub struct Repo {
    pub repo: RdedupRepo,
    pub pass: Box<Fn() -> std::io::Result<String> + Send + Sync>
}

impl Repo {
    pub fn new(root: &str, name: &str, pass: String, logger: slog::Logger) -> Result<Repo, Error> {
        RdedupRepo::open(std::path::Path::new(&format!("{}/{}", root, name)), logger)
            .map(|repo| {
                Repo {
                    repo,
                    pass: Box::new(move || { Ok(pass.clone()) })
                }
            }).map_err(Error::from)
    }
}

pub struct UploadedFile {
    pub name: String,
    pub device_id: String,
}

#[derive(Debug, Clone)]
pub struct DeviceIdentity {
    pub id: String,
    pub account_id: String,
    pub repo_pass: String
}