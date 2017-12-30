#[macro_use]
extern crate failure_derive;
extern crate failure;
extern crate env_logger;
#[macro_use]
extern crate log;
extern crate time;
extern crate chrono;
#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate serde_json;
extern crate multimap;
extern crate rocket;
extern crate rdedup_lib as rdedup;
extern crate pipe;
#[macro_use]
extern crate mysql;

pub mod dao;

use multimap::MultiMap;
use failure::Error;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
use chrono::prelude::*;
use std::process::Stdio;
use std::process::ChildStdout;
use std::ops::Deref;
use std::str;
use rocket::data::Data;
use rocket::response::Stream;
use rdedup::{Repo as RdedupRepo, DecryptHandle, EncryptHandle};

use std::io::{Write, Read, BufReader};

use dao::Dao;

pub struct Repo {
    pub repo: RdedupRepo,
    pub decrypt: Box<Fn() -> std::io::Result<String> + Send + Sync>,
    pub encrypt: Box<Fn() -> std::io::Result<String> + Send + Sync>,
}

pub fn save(repo: &Repo, pc_id: &str, orig_file_name: &str, data: Data) -> Result<(), Error> {
    let current_time = SystemTime::now()
        .duration_since(UNIX_EPOCH)?;

    let time_stamp = current_time.as_secs() * 1000 + u64::from(current_time.subsec_nanos()) / 1000;

    debug!("Current time: {}", time_stamp);

    let file_name_final = to_final_name(pc_id, orig_file_name, time_stamp);

    debug!("Final name: {}", file_name_final);

    let encrypt_handle = repo.repo.unlock_encrypt(&*repo.encrypt)?;

    repo.repo
        .write(file_name_final.deref(), data.open(), &encrypt_handle)
        .map(|stats| ())
        .map_err(Error::from)
}

pub fn load(repo: &Repo, pc_id: &str, orig_file_name: &str, time_stamp: u64) -> Result<pipe::PipeReader, Error> {
    let file_name_final = Box::new(to_final_name(pc_id, orig_file_name, time_stamp));

    debug!("Requested name: {}", file_name_final);

    use std::thread::spawn;

    let (mut reader, mut writer) = pipe::pipe();
    let mut writer = Box::from(writer);

    let boxed_repo = Box::from(repo.repo.clone());
    let decrypt_handle = repo.repo.unlock_decrypt(&*repo.decrypt)?;

    spawn(move || {
        boxed_repo.read(&file_name_final, &mut writer, &decrypt_handle);
        // TODO handle error
        ()
    });

    Ok(reader)
}

pub fn list(dao: &Dao, device_id: &str) -> Result<String, Error> {
    let res = dao.list_files(device_id)?;
    Ok(serde_json::to_string(&res)?)
}

fn to_final_name(pc_id: &str, orig_file_name: &str, time_stamp: u64) -> String {
    let file_name_final = orig_file_name
        .replace("|", "-")
        .replace("#", "!*!")
        .replace("/", "|");

    format!("{}#{}#{}", pc_id, file_name_final, time_stamp)
}
