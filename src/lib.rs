#[macro_use]
extern crate arrayref;
extern crate chrono;
extern crate crypto;
extern crate env_logger;
extern crate failure;
extern crate hex;
#[macro_use]
extern crate log;
extern crate multimap;
#[macro_use]
extern crate mysql;
extern crate pipe;
extern crate rdedup_lib as rdedup;
extern crate rocket;
#[macro_use]
extern crate serde_derive;
extern crate serde_json;
extern crate sha2;
extern crate time;
extern crate uuid;

use chrono::prelude::*;
use dao::Dao;
use encryptor::Encryptor;
use failure::Error;
use rocket::data::Data;
use rocket::data::DataStream;
use sha2::{Digest, Sha256};
use std::io::Read;
use std::str;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use structs::*;

pub mod dao;
mod failures;
pub mod encryptor;
pub mod structs;

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

pub fn login(repo: &rdedup::Repo, dao: &Dao, enc: &Encryptor, device_id: &str, repo_pass: &str) -> Result<String, Error> {
    // TODO check existing record

    // TODO check secret file in repo
    repo.unlock_decrypt(&*Box::new(move || { Ok(String::from(repo_pass)) }))
        .map_err(Error::from)
        .and_then(|_| {
            dao.login(enc, device_id, repo_pass)
                .map_err(Error::from)
        })
        .map(|session_id| { format!(r#"{{ "session_id": "{}" }}"#, session_id) })
}

pub fn authenticate(dao: &Dao, enc: &Encryptor, session_pass: &str) -> Result<Option<DeviceIdentity>, Error> {
    dao.authenticate(enc, session_pass)
        .map_err(Error::from)
}

pub fn save(repo: &Repo, dao: &Dao, uploaded_file: UploadedFile, data: Data) -> Result<dao::File, Error> {
    let current_time = SystemTime::now()
        .duration_since(UNIX_EPOCH)?;

    let time_stamp = NaiveDateTime::from_timestamp(current_time.as_secs() as i64, current_time.subsec_nanos());

    debug!("Current time: {}", time_stamp);

    let file_name_final = to_storage_name(&uploaded_file.device_id, &uploaded_file.name, time_stamp);

    debug!("Final name: {}", file_name_final);

    let encrypt_handle = repo.repo.unlock_encrypt(&*repo.pass)?;

    let hasher = Arc::new(Mutex::new(Sha256::default()));
    let size = Arc::new(Mutex::new(0u64));
    let stream = DigestDataStream::new(data.open(), hasher.clone(), size.clone());

    repo.repo
        .write(&file_name_final, stream, &encrypt_handle)
        .map_err(Error::from)
        .and_then(|_| {
            let res_size: u64 = Arc::try_unwrap(size).unwrap().into_inner().unwrap();

            let res_hash = Arc::try_unwrap(hasher).unwrap().into_inner().unwrap().result();

            Ok((hex::encode(&res_hash), res_size))
        }).and_then(|(hash, size)| {
        let old_file = dao.find_file(&uploaded_file.device_id, &uploaded_file.name)?;

        // TODO check whether there is not already last version with the same hash
        let new_version = dao::FileVersion {
            version: 0, // cannot know now, will be filled in after DB insertion
            size,
            hash: hash.clone(),
            created: time_stamp,
            storage_name: file_name_final
        };

        dao.save_new_version(&uploaded_file, old_file, new_version).map_err(Error::from)
    })
}

pub fn load(repo: &Repo, dao: &Dao, version_id: u32) -> Result<pipe::PipeReader, Error> {
    // TODO return 404 if not found
    let storage_name = dao.get_storage_name(version_id)?.unwrap();

    use std::thread::spawn;

    let (reader, writer) = pipe::pipe();
    let mut writer = Box::from(writer);

    let boxed_repo = Box::from(repo.repo.clone());
    let decrypt_handle = repo.repo.unlock_decrypt(&*repo.pass)?;

    spawn(move || {
        boxed_repo.read(&storage_name, &mut writer, &decrypt_handle);
        // TODO handle error
        ()
    });

    Ok(reader)
}

pub fn list(dao: &Dao, device_id: &str) -> Result<String, Error> {
    let res = dao.list_files(device_id)?;
    Ok(serde_json::to_string(&res)?)
}

pub fn remove_file_version(repo: &Repo, dao: &Dao, version_id: u32) -> Result<(u16, String), Error> {
    dao.remove_file_version(version_id)
        .map(|opt| {
            opt.map(|storage_name| {
                match repo.repo.rm(&storage_name) {
                    Ok(_) => (200 as u16, String::from("")),
                    Err(e) => (500 as u16, format!("{}", e))
                }
            }).or(Some((404 as u16, String::from("File was not found")))).unwrap()
        }).map_err(Error::from)
}

pub fn remove_file(repo: &Repo, dao: &Dao, file_name: &str) -> Result<(u16, String), Error> {
    dao.remove_file(file_name)
        .map(|opt_storage_names| {
            match opt_storage_names {
                Some(storage_names) => {
                    let (_, failures): (Vec<_>, Vec<_>) = storage_names
                        .into_iter()
                        .map(|storage_name| {
                            repo.repo.rm(&storage_name)
                        }).partition(Result::is_ok);

                    let failures: Vec<_> = failures.into_iter().map(Result::unwrap_err).collect();

                    if failures.is_empty() {
                        (200 as u16, String::from(""))
                    } else {
                        (500 as u16, format!("{:?}", failures))
                    }
                }
                None => (500 as u16, String::from("Error while deleting"))
            }
        }).map_err(Error::from)
}

fn to_storage_name(pc_id: &str, orig_file_name: &str, time_stamp: NaiveDateTime) -> String {
    let mut hasher = Sha256::default();

    hasher.input(pc_id.as_bytes());
    hasher.input(orig_file_name.as_bytes());
    hasher.input(&transform_u32_to_bytes(time_stamp.second()));
    hasher.input(&transform_u32_to_bytes(time_stamp.nanosecond()));

    hex::encode(&hasher.result())
}

fn transform_u32_to_bytes(x: u32) -> [u8; 4] {
    let b1: u8 = ((x >> 24) & 0xff) as u8;
    let b2: u8 = ((x >> 16) & 0xff) as u8;
    let b3: u8 = ((x >> 8) & 0xff) as u8;
    let b4: u8 = (x & 0xff) as u8;

    return [b1, b2, b3, b4]
}
