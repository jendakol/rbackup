extern crate slog;
extern crate failure;

use rdedup::Repo as RdedupRepo;
use std;
use failure::Error;

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