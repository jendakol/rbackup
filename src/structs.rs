use std;
use rdedup::{Repo as RdedupRepo};

pub struct Repo {
    pub repo: RdedupRepo,
    pub decrypt: Box<Fn() -> std::io::Result<String> + Send + Sync>,
    pub encrypt: Box<Fn() -> std::io::Result<String> + Send + Sync>,
}

pub struct UploadedFile {
    pub name: String,
    pub sha256: String,
    pub size: u32,
    pub device_id: String,
}