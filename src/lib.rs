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

use multimap::MultiMap;
use failure::Error;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
use std::process::Stdio;
use std::process::ChildStdout;
use std::ops::Deref;
use std::str;
use rocket::data::Data;

use rdedup::{Repo as RdedupRepo, DecryptHandle, EncryptHandle};

pub struct Repo {
    pub repo: RdedupRepo,
    pub decrypt: DecryptHandle,
    pub encrypt: EncryptHandle
}

#[derive(Debug, Fail)]
#[fail(display = "Could not get exit code of ext program")]
struct MissingExitCode;
//
//pub fn save(repo: Repo, pc_id: &str, orig_file_name: &str, data: Data) -> Result<(), Error> {
//    let current_time = SystemTime::now()
//        .duration_since(UNIX_EPOCH)?;
//
//    let time_stamp = current_time.as_secs() * 1000 + u64::from(current_time.subsec_nanos()) / 1000;
//
//    debug!("Current time: {}", time_stamp);
//
//    let file_name_final = to_final_name(pc_id, orig_file_name, time_stamp);
//
//    debug!("Final name: {}", file_name_final);
//
//    let mut child = Command::new("rdedup")
//        .stdin(Stdio::piped())
//        .arg("--dir")
//        .arg(repo_dir)
//        .arg("store")
//        .arg(file_name_final)
//        .spawn()?;
//
//
//    let stdin = child.stdin.take();
//
//    data.stream_to(&mut stdin.unwrap())?;
//
//    let output = child.wait_with_output()?;
//
//    let exit_code = output.status.code().ok_or(MissingExitCode)?;
//    debug!("Exit code: {}", exit_code);
//
//    if exit_code == 0 {
//        let split = str::from_utf8(&output.stdout)?
//            .trim()
//            .lines();
//
//        debug!("Output: {:?}", split.collect::<Vec<_>>());
//
//        Ok(())
//    } else {
//        let output = String::from_utf8(output.stderr)?;
//
//        warn!("Exit code {}, stderr: {:?}", exit_code, output);
//
//        // TODO Err
//        Ok(())
//    }
//}
//
//pub fn load(repo: &Repo, pc_id: &str, orig_file_name: &str, time_stamp: u64) -> Result<ChildStdout, Error> {
//    let file_name_final = to_final_name(pc_id, orig_file_name, time_stamp);
//
//    debug!("Requested name: {}", file_name_final);
//
//    Command::new("rdedup")
//        .env("RDEDUP_PASSPHRASE", "jenda")
//        .arg("--dir")
//        .arg(repo_dir)
//        .arg("load")
//        .arg(file_name_final)
//        .stdout(Stdio::piped())
//        .spawn()
//        .map_err(Error::from)
//        .map(|stdout| {
//            stdout.stdout.unwrap()
//        })
//}

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
