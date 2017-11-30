#[macro_use]
extern crate failure_derive;
extern crate failure;
extern crate tempfile;
extern crate env_logger;
#[macro_use]
extern crate log;
extern crate time;
extern crate serde_json;
extern crate multimap;

use multimap::MultiMap;
use std::fs::File;
use std::path::Path;
use failure::Error;
use std::process::Command;
use tempfile::{NamedTempFile, NamedTempFileOptions};
use std::time::{SystemTime, UNIX_EPOCH};
use std::io::Write;
use std::ops::Deref;
use std::str;

#[derive(Debug, Fail)]
#[fail(display = "Could not get exit code of ext program")]
struct MissingExitCode;

#[derive(Debug, Fail)]
#[fail(display = "Broken unicode")]
struct BrokenUnicode;

pub fn save(repo_dir: &str, pc_id: &str, orig_file_name: &str, temp_file_name: &str) -> Result<(), Error> {
    let current_time = SystemTime::now()
        .duration_since(UNIX_EPOCH)?;

    let time_stamp = current_time.as_secs() * 1000 + u64::from(current_time.subsec_nanos()) / 1000;

    debug!("Current time: {}", time_stamp);

    let file_name_final = to_final_name(pc_id, orig_file_name, time_stamp);

    debug!("Final name: {}", file_name_final);

    let file = File::open(Path::new(&temp_file_name))?;

    let output = Command::new("rdedup")
        .stdin(file)
        .arg("--dir")
        .arg(repo_dir)
        .arg("store")
        .arg(file_name_final)
        .output()?;

    let exit_code = output.status.code().ok_or(MissingExitCode)?;
    debug!("Exit code: {}", exit_code);

    if exit_code == 0 {
        let split = str::from_utf8(&output.stdout)?
            .trim()
            .lines();

        debug!("Output: {:?}", split.collect::<Vec<_>>());

        Ok(())
    } else {
        let output = String::from_utf8(output.stderr)?;

        warn!("Exit code {}, stderr: {:?}", exit_code, output);

        // TODO Err
        Ok(())
    }
}

pub fn load(repo_dir: &str, pc_id: &str, orig_file_name: &str, time_stamp: u64) -> Result<NamedTempFile, Error> {
    let temp_file = NamedTempFileOptions::new()
        .prefix("rbackup")
        .create()?;

    let output_file_name = String::from(temp_file.path().to_str().ok_or(BrokenUnicode)?);

    let file_name_final = to_final_name(pc_id, orig_file_name, time_stamp);

    debug!("Requested name: {}", file_name_final);
    debug!("TMP file: {}", output_file_name);

    Command::new("rdedup")
        .env("RDEDUP_PASSPHRASE", "jenda")
        .arg("--dir")
        .arg(repo_dir)
        .arg("load")
        .arg(file_name_final)
        .output()
        .map_err(Error::from)
        .and_then(|output| {
            File::create(temp_file.path())
                .map_err(Error::from)
                .and_then(|mut file| {
                    file
                        .write(&output.stdout)
                        .map_err(Error::from)
                        .and_then(|r| {
                            println!("Output: {} B", r);
                            println!("Status: {:?}, stderr: {}", output.status, String::from_utf8(output.stderr)?);

                            Ok(temp_file)
                        })
                })
            })
}

pub fn list(repo_dir: &str, pc_id: &str) -> Result<String, Error> {
    Command::new("rdedup")
        .arg("--dir")
        .arg(repo_dir)
        .arg("ls")
        .output()
        .map_err(Error::from)
        .and_then(|output| {
            let out = str::from_utf8(&output.stdout)?;

            let data = out.trim().lines().filter_map(|l: &str| {
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
