use std;
use rdedup::{Repo as RdedupRepo};

pub struct Repo {
    pub repo: RdedupRepo,
    pub pass: Box<Fn() -> std::io::Result<String> + Send + Sync>
}

impl Repo {
    pub fn new(repo: RdedupRepo, pass: String) -> Repo {
        Repo{
            repo,
            pass: Box::new(move || { Ok(pass.clone()) })
        }
    }
}

pub struct UploadedFile {
    pub name: String,
    pub device_id: String,
}

#[derive(Debug, Clone)]
pub struct DeviceIdentity {
    pub id: String,
    pub repo_pass: String
}