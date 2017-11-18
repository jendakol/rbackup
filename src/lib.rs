extern crate failure;
extern crate futures;
extern crate futures_cpupool;
extern crate env_logger;
#[macro_use]
extern crate log;
//#[macro_use]
//extern crate lazy_static;
extern crate time;

use std::fs::File;
//use std::io::prelude::*;
use std::path::Path;
use failure::Error;
//use futures::{future, Future};
//use futures_cpupool::CpuPool;
use std::process::Command;

use std::time::{SystemTime, UNIX_EPOCH};

//lazy_static! {
//    static ref POOL: CpuPool = CpuPool::new_num_cpus();
//}

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