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
mod failures;
pub mod structs;

use failure::Error;
use std::time::{SystemTime, UNIX_EPOCH};
use chrono::prelude::*;
use std::ops::Deref;
use std::str;
use rocket::data::Data;
use rocket::data::DataStream;
use std::io::{Write, Read};
use sha2::{Sha256, Digest};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use dao::Dao;
use structs::*;
use std::ops::Add;

use std::cell::RefCell;

struct DigestDataStream {
    data_stream: DataStream,
    hasher: Arc<Mutex<Sha256>>,
    size: Arc<Mutex<u64>>
}

impl DigestDataStream {
    pub fn new(stream: DataStream, hasher: Arc<Mutex<Sha256>>, size: Arc<Mutex<u64>>) -> DigestDataStream {
        DigestDataStream {
            data_stream: stream,
            hasher,
            size
        }
    }
}

impl Read for DigestDataStream {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, std::io::Error> {
        self.data_stream
            .read(buf)
            .map(|s| {
                let mut h = self.hasher.lock().unwrap();
                h.input(&buf[0..s]);
                *self.size.lock().unwrap() += s as u64;

                s
            })
    }
}

pub fn save(repo: &Repo, dao: &Dao, uploaded_file: UploadedFile, data: Data) -> Result<(), Error> {
    let current_time = SystemTime::now()
        .duration_since(UNIX_EPOCH)?;

    let time_stamp = NaiveDateTime::from_timestamp(current_time.as_secs() as i64, current_time.subsec_nanos());

    debug!("Current time: {}", time_stamp);

    let file_name_final = to_final_name(&uploaded_file.device_id, &uploaded_file.name, time_stamp);

    debug!("Final name: {}", file_name_final);

    let encrypt_handle = repo.repo.unlock_encrypt(&*repo.encrypt)?;

    let hasher = Arc::new(Mutex::new(Sha256::default()));
    let size = Arc::new(Mutex::new(0u64));
    let stream = DigestDataStream::new(data.open(), hasher.clone(), size.clone());

    repo.repo
        .write(&file_name_final, stream, &encrypt_handle)
        .map_err(Error::from)
        .and_then(|stats| {
            let res_size: u64 = Arc::try_unwrap(size).unwrap().into_inner().unwrap();

            let res_hash = Arc::try_unwrap(hasher).unwrap().into_inner().unwrap().result();
            let hex_string = to_hex_string(&res_hash);

            if hex_string == uploaded_file.sha256.to_ascii_uppercase() && res_size == uploaded_file.size {
                Ok(())
            } else {
                // TODO delete the file
                Err(Error::from(failures::CustomError::new("Hash or size of uploaded file is different than specified")))
            }
        }).and_then(|_| {
        let old_file = dao.find_file(&uploaded_file.device_id, &uploaded_file.name)?;

        // TODO check whether there is not already last version with the same hash
        let new_version = dao::FileVersion {
            size: uploaded_file.size,
            hash: uploaded_file.sha256.clone(),
            created: time_stamp,
            storage_name: file_name_final
        };

        dao.save_new_version(&uploaded_file, old_file, new_version)?;

        Ok(())
    })
}

pub fn load(repo: &Repo, pc_id: &str, orig_file_name: &str, time_stamp: u64) -> Result<pipe::PipeReader, Error> {
    let file_name_final = Box::new(to_final_name(pc_id, orig_file_name, NaiveDateTime::from_timestamp(time_stamp as i64, 0)));

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

fn to_final_name(pc_id: &str, orig_file_name: &str, time_stamp: NaiveDateTime) -> String {
    let mut hasher = Sha256::default();

    hasher.input(pc_id.as_bytes());
    hasher.input(orig_file_name.as_bytes());
    hasher.input(&transform_u32_to_bytes(time_stamp.second()));
    hasher.input(&transform_u32_to_bytes(time_stamp.nanosecond()));

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

fn transform_u32_to_bytes(x: u32) -> [u8; 4] {
    let b1: u8 = ((x >> 24) & 0xff) as u8;
    let b2: u8 = ((x >> 16) & 0xff) as u8;
    let b3: u8 = ((x >> 8) & 0xff) as u8;
    let b4: u8 = (x & 0xff) as u8;

    return [b1, b2, b3, b4]
}
