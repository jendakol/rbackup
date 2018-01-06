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
extern crate sha2;

pub mod dao;
pub mod failures;

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
use rocket::data::DataStream;
use rocket::response::Stream;
use rdedup::{Repo as RdedupRepo, DecryptHandle, EncryptHandle};
use std::fmt;
use std::io::{Write, Read, BufReader};
use sha2::{Sha256, Digest};
use std::sync::Arc;
use std::rc::Rc;
use dao::Dao;

use std::cell::RefCell;

use std::collections::HashMap;

pub struct Repo {
    pub repo: RdedupRepo,
    pub decrypt: Box<Fn() -> std::io::Result<String> + Send + Sync>,
    pub encrypt: Box<Fn() -> std::io::Result<String> + Send + Sync>,
}

struct DigestDataStream {
    data_stream: DataStream,
    hasher: Rc<RefCell<HashMap<String, u32>>>
}

impl DigestDataStream {
    pub fn new(stream: DataStream, hasher: Rc<RefCell<HashMap<String, u32>>>) -> DigestDataStream {
        DigestDataStream {
            data_stream: stream,
            hasher: hasher
        }
    }

//    pub fn result_sha256(&self) -> String {
//        to_hex_string(&self.hasher.clone().result())
//    }
}

impl Read for DigestDataStream {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, std::io::Error> {
        self.data_stream
            .read(buf)
            .map(|s| {


                use std::collections::HashMap;
                use std::cell::RefCell;
                use std::rc::Rc;

                let shared_map: Rc<RefCell<_>> = Rc::new(RefCell::new(HashMap::new()));
                shared_map.borrow_mut().insert("africa", 92388);
                shared_map.borrow_mut().insert("kyoto", 11837);
                shared_map.borrow_mut().insert("piccadilly", 11826);
                shared_map.borrow_mut().insert("marbles", 38);

                println!("{:?}", shared_map);


//                let shared_map: Rc<RefCell<_>> = Rc::new(RefCell::new(HashMap::new()));
//                shared_map.borrow_mut().insert("africa", 92388);
//                shared_map.borrow_mut().insert("kyoto", 11837);
//                shared_map.borrow_mut().insert("piccadilly", 11826);
//                shared_map.borrow_mut().insert("marbles", 38);

//                self.hasher.borrow_mut().insert();

//                let mut h = self.hasher.input();
//                h.input(&buf[0..s]);
//                h.result();
                s
            })
    }
}
//
//impl Clone for DigestDataStream {
//    fn clone(&self) -> DigestDataStream {
//        DigestDataStream {
//            data_stream: self.data_stream.clone(),
//            hasher: self.hasher.clone()
//        }
//    }
//}

pub fn save(repo: &Repo, dao: &Dao, pc_id: &str, orig_file_name: &str, orig_file_hash: &str, data: Data) -> Result<(), Error> {
    let current_time = SystemTime::now()
        .duration_since(UNIX_EPOCH)?;

    let time_stamp = current_time.as_secs() * 1000 + u64::from(current_time.subsec_nanos()) / 1000;

    debug!("Current time: {}", time_stamp);

    let file_name_final = to_final_name(pc_id, orig_file_name, time_stamp);

    debug!("Final name: {}", file_name_final);

    let encrypt_handle = repo.repo.unlock_encrypt(&*repo.encrypt)?;

    let hasher = Sha256::default();
    let stream = DigestDataStream::new(data.open(), Rc::new(RefCell::new(HashMap::new())));

    unimplemented!()
//
//    repo.repo
//        .write(&file_name_final, stream, &encrypt_handle)
//        .map_err(Error::from)
//        .and_then(|stats| {
//            if to_hex_string(&hasher.result()) == orig_file_hash {
//                Ok(orig_file_hash)
//            } else {
//                Err(Error::from(failures::CustomError::new("Hash of uploaded file is different than specified")))
//            }
////
////            match to_hex_string(&hasher.result()) {
////                orig_file_hash => Ok(orig_file_hash),
////                _ => Err(Error::from(failures::CustomError::new("TODO")))
////            }
//        }).and_then(|hash| {
//
////
////            let new_version = dao::FileVersion {
////                size: 0,
////                hash: "",
////                created: NaiveDateTime::from(time_stamp),
////                storage_name: file_name_final
////            }
////
////            dao.save_new_version()
////
//        println!("Hash: {}", hash);
//        Ok(())
//    })
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
    let mut hasher = Sha256::default();

    hasher.input(pc_id.as_bytes());
    hasher.input(orig_file_name.as_bytes());
    hasher.input(&transform_u64_to_bytes(time_stamp));

    to_hex_string(&hasher.result())
}

fn to_hex_string(bytes: &[u8]) -> String {
    bytes.iter()
        .map(|b| format!("{:02X}", b))
        .collect::<Vec<String>>()
        .join("")
}

fn transform_u64_to_bytes(x: u64) -> [u8; 8] {
    let b1: u8 = ((x >> 56) & 0xff) as u8;
    let b2: u8 = ((x >> 48) & 0xff) as u8;
    let b3: u8 = ((x >> 40) & 0xff) as u8;
    let b4: u8 = ((x >> 32) & 0xff) as u8;
    let b5: u8 = ((x >> 24) & 0xff) as u8;
    let b6: u8 = ((x >> 16) & 0xff) as u8;
    let b7: u8 = ((x >> 8) & 0xff) as u8;
    let b8: u8 = (x & 0xff) as u8;

    return [b1, b2, b3, b4, b5, b6, b7, b8]
}
