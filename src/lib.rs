extern crate failure;
extern crate tempfile;
extern crate env_logger;
#[macro_use]
extern crate log;
extern crate time;

use std::fs::File;
use std::path::Path;
use failure::Error;
use std::process::Command;
use tempfile::{NamedTempFile, NamedTempFileOptions};
use std::time::{SystemTime, UNIX_EPOCH};
use std::io;
use std::io::Write;


pub fn save(repo_dir: String, pc_id: String, orig_file_name: String, temp_file_name: &str) -> Result<(), Error> {
    let current_time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Could not get current time");

    let time_stamp = (current_time.as_secs() * 1000 + (current_time.subsec_nanos() as u64) / 1000).to_string();

    debug!("Current time: {}", time_stamp);

    let file_name_final = (pc_id + "_" + &orig_file_name + "_" + &time_stamp)
        .replace("|", "-")
        .replace("/", "|");

    debug!("Final name: {}", file_name_final);

    let file = File::open(Path::new(&temp_file_name)).expect("Could not open file");

    let output = Command::new("rdedup")
        .stdin(file)
        .arg("--dir")
        .arg(repo_dir)
        .arg("store")
        .arg(file_name_final)
        .output()
        .expect("Could not execute 'rdedup'!");

    let exit_code = output.status.code().expect("Could not get exit code of ext program");
    debug!("Exit code: {}", exit_code);

    if exit_code == 0 {
        let output = String::from_utf8(output.stdout).expect("Could not convert output to UTF-8");
        let split: Vec<&str> = output.trim().split("\n").collect();

        debug!("Output: {:?}", split);

        Ok(())
    } else {
        let output = String::from_utf8(output.stderr).expect("Could not convert error output to UTF-8");

        warn!("Exit code {}, stderr: {:?}", exit_code, output);

        // TODO Err
        Ok(())
    }
}

pub fn load(repo_dir: String, pc_id: String, orig_file_name: String, time_stamp: u64) -> io::Result<NamedTempFile> {
    let temp_file = NamedTempFileOptions::new()
        .prefix("rbackup")
        .create()
        .expect("Could not create temp file");

    let output_file_name = String::from(temp_file.path().to_str().expect("Could not extract path from temp file"));

    let file_name_final = (pc_id + "_" + &orig_file_name + "_" + time_stamp.to_string().as_ref())
        .replace("|", "-")
        .replace("/", "|");

    println!("Requested name: {}", file_name_final);
    println!("TMP file: {}", output_file_name);

    Command::new("rdedup")
        .env("RDEDUP_PASSPHRASE", "jenda")
        .arg("--dir")
        .arg(repo_dir)
        .arg("load")
        .arg(file_name_final)
        .output()
        .and_then(|output| {
            File::create(temp_file.path())
                .and_then(|mut file| {
                    file
                        .write(&output.stdout)
                        .map(|r| {
                            println!("Output: {} B", r);
                            println!("Status: {:?}, stderr: {}", output.status, String::from_utf8(output.stderr).expect("Could not convert error output to UTF-8"));

                            temp_file
                        })
                })
        })
}

pub fn list (repo_dir: String, pc_id: String) -> io::Result<String> {
    Command::new("rdedup")
        .arg("--dir")
        .arg(repo_dir)
        .arg("ls")
        .output()
        .map(|output|{
            let out = String::from_utf8(output.stdout).expect("Could not convert stdout to string");
            let lines: Vec<&str> = out.trim().split("\n").collect();

//            let data: Vec<Vec<&str>> = lines.mapped(|l|{
//                l.split("_").collect()
//            });

//            data.map(|f| {
//                f.get
//            })

            String::from(lines.join(";"))

        })
}