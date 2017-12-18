#[macro_use]
extern crate failure_derive;
extern crate failure;
extern crate env_logger;
#[macro_use]
extern crate log;
extern crate time;
extern crate serde_json;
extern crate multimap;
extern crate rocket;
extern crate rdedup_lib as rdedup;
extern crate pipe;

use multimap::MultiMap;
use failure::Error;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
use std::process::Stdio;
use std::process::ChildStdout;
use std::ops::Deref;
use std::str;
use rocket::data::Data;
use rocket::response:: Stream;
use rdedup::{Repo as RdedupRepo, DecryptHandle, EncryptHandle};

use std::io::{Write, Read, BufReader};

pub struct Repo {
    pub repo: RdedupRepo,
    pub decrypt: DecryptHandle,
    pub encrypt: EncryptHandle
}

pub fn save(repo: &Repo, pc_id: &str, orig_file_name: &str, data: Data) -> Result<(), Error> {
    let current_time = SystemTime::now()
        .duration_since(UNIX_EPOCH)?;

    let time_stamp = current_time.as_secs() * 1000 + u64::from(current_time.subsec_nanos()) / 1000;

    debug!("Current time: {}", time_stamp);

    let file_name_final = to_final_name(pc_id, orig_file_name, time_stamp);

    debug!("Final name: {}", file_name_final);

    repo.repo
        .write(file_name_final.deref(), data.open(), &repo.encrypt)
        .map(|stats| ())
        .map_err(Error::from)
}

pub fn load(repo: &Repo, pc_id: &str, orig_file_name: &str, time_stamp: u64) -> Result<pipe::PipeReader, Error> {
    let file_name_final = Box::new(to_final_name(pc_id, orig_file_name, time_stamp));

    debug!("Requested name: {}", file_name_final);

    use std::thread::spawn;

    let (mut reader, mut writer) = pipe::pipe();

    let message = "Hello, world!";


    let r = Box::from(repo.repo.clone());

    let r2 = Box::new(
        repo.decrypt
    );

    let mut r3 = Box::from(writer);

    spawn(move|| {

        r
            .read(file_name_final.deref(), &mut r3 , r2.deref());

            ()
    });



    Ok(reader)
}

pub fn list(repo: &Repo, pc_id: &str) -> Result<String, Error> {
    repo.repo.list_names()
        .map_err(Error::from)
        .and_then(|output| {
            let data = output.into_iter().filter_map(|l: String| {
                let parts = l.split('#').collect::<Vec<&str>>();

                match (parts.get(0), parts.get(1), parts.get(2).map(|num| num.parse::<u64>())) {
                    (prefix, _, _) if prefix != Some(&pc_id.deref()) => None,
                    (_, _, None) |
                    (_, None, Some(Ok(_))) => None,
                    (_, _, Some(Err(e))) => {
                        error!("Wrong num: {}", e);
                        None
                    }
                    (_, Some(name), Some(Ok(num))) => {
                        let original_name = name.replace("|", "/").replace("!*!", "#");
                        Some((original_name, num))
                    }
                }
            }).collect::<MultiMap<String, _>>();

            Ok(serde_json::to_string(&data)?)
        })
}

fn to_final_name(pc_id: &str, orig_file_name: &str, time_stamp: u64) -> String {
    let file_name_final = orig_file_name
        .replace("|", "-")
        .replace("#", "!*!")
        .replace("/", "|");

    format!("{}#{}#{}", pc_id, file_name_final, time_stamp)
}
