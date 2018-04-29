#[macro_use]
extern crate arrayref;
extern crate cadence;
extern crate chrono;
extern crate crypto;
extern crate env_logger;
extern crate failure;
extern crate hex;
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
#[macro_use]
extern crate slog;
extern crate stopwatch;
extern crate time;
extern crate uuid;

use cadence::prelude::*;
use cadence::StatsdClient;
use chrono::prelude::*;
use dao::Dao;
use encryptor::Encryptor;
use failure::Error;
use rocket::data::Data;
use rocket::data::DataStream;
use sha2::{Digest, Sha256};
use slog::Logger;
use std::io::Read;
use std::str;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use stopwatch::Stopwatch;
use structs::*;

pub mod dao;
mod failures;
pub mod encryptor;
pub mod structs;

struct DigestDataStream {
    data_stream: DataStream,
    hasher: Arc<Mutex<Sha256>>,
    size: Arc<Mutex<u64>>,
    buff: Arc<Mutex<Vec<u8>>>,
    hash_from_stream: Arc<Mutex<Vec<u8>>>,
    handle_upload_chunk: Box<Fn(u64) -> () + Send + Sync + 'static>
}

impl DigestDataStream {
    pub fn new(stream: DataStream, hasher: Arc<Mutex<Sha256>>, size: Arc<Mutex<u64>>, hash_from_stream: Arc<Mutex<Vec<u8>>>, handle_upload_chunk: Box<Fn(u64) -> () + Send + Sync + 'static>) -> DigestDataStream {
        let buff = Arc::new(Mutex::new(Vec::new()));

        DigestDataStream {
            data_stream: stream,
            hasher,
            size,
            buff,
            hash_from_stream,
            handle_upload_chunk
        }
    }
}

impl Read for DigestDataStream {
    fn read(&mut self, target_buff: &mut [u8]) -> Result<usize, std::io::Error> {
        let target_buff_length = target_buff.len();

        let mut buff = self.buff.lock().unwrap();

        let capacity = target_buff_length + 32 - buff.len();
        let mut tmp_buff: Vec<u8> = Vec::with_capacity(capacity);
        tmp_buff.resize(capacity, 0);

        self.data_stream
            .read(&mut tmp_buff)
            .map(|copied| {
                let mut hasher = self.hasher.lock().unwrap();

                let buff_len = buff.len();

                (self.handle_upload_chunk)(copied as u64);

                if copied == 0 {
                    if buff_len == 0 { 0 } else {
                        let curr_buff = buff.clone();

                        let hash = &curr_buff[(buff_len - 32)..];
                        let to_return = &curr_buff[0..(buff_len - 32)];

                        *self.hash_from_stream.lock().unwrap() = hash.to_vec(); // we have final hash
                        *buff = Vec::new();

                        target_buff[0..to_return.len()].clone_from_slice(to_return);
                        hasher.input(to_return);
                        *self.size.lock().unwrap() += to_return.len() as u64;
                        to_return.len()
                    }
                } else {
                    let mut current = buff.clone();
                    current.append(&mut tmp_buff.as_mut_slice()[0..copied].to_vec());

                    *buff = current[(current.len() - 32)..].to_vec();

                    // val toReturn = current.dropRight(hashSize).take(targetBufferLength) - no nned for the min
                    let to_return = &current[0..(current.len() - 32)][0..std::cmp::min(target_buff_length, current.len() - 32)];

                    target_buff[0..to_return.len()].clone_from_slice(to_return);

                    hasher.input(to_return);
                    *self.size.lock().unwrap() += to_return.len() as u64;

                    to_return.len()
                }
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

pub fn save(logger: &Logger, statsd_client: StatsdClient, repo: &Repo, dao: &Dao, uploaded_file: UploadedFile, data: Data) -> Result<dao::File, Error> {
    let statsd_client = Arc::new(statsd_client);

    let current_time = SystemTime::now()
        .duration_since(UNIX_EPOCH)?;

    let stopwatch = Stopwatch::start_new();

    let time_stamp = NaiveDateTime::from_timestamp(current_time.as_secs() as i64, current_time.subsec_nanos());

    let device_id = Arc::new(uploaded_file.device_id.clone());

    let statsd_client2 = statsd_client.clone(); // TODO this is hack!!!
    let device_id2 = Arc::new(uploaded_file.device_id.clone());


    let file_name_final = to_storage_name(&device_id, &uploaded_file.name, time_stamp);

    debug!(logger, "Current time {}, final name {}", time_stamp, file_name_final);

    let encrypt_handle = repo.repo.unlock_encrypt(&*repo.pass)?;

    let hasher = Arc::new(Mutex::new(Sha256::default()));
    let hash_from_stream = Arc::new(Mutex::new(Vec::new()));
    let size = Arc::new(Mutex::new(0u64));
    let stream = DigestDataStream::new(data.open(),
                                       hasher.clone(),
                                       size.clone(),
                                       hash_from_stream.clone(),
                                       Box::from(move |copied_bytes| {
                                           #[allow(unused_must_use)] {
                                               let client = statsd_client2.clone();
                                               client.count("upload.total.bytes", copied_bytes as i64);
                                               client.count(format!("upload.devices.{}.bytes", &device_id2.clone()).as_ref(), copied_bytes as i64);
                                           }
                                       }));

    repo.repo
        .write(&file_name_final, stream, &encrypt_handle)
        .map_err(Error::from)
        .and_then(|_| {
            let duration = stopwatch.elapsed_ms() as u64;
            let statsd_client = statsd_client.clone();
            #[allow(unused_must_use)] {
                statsd_client.time("upload.total.length", duration);
                statsd_client.time(format!("upload.devices.{}.length", &device_id.clone()).as_ref(), duration);
            }

            let transferred_bytes: u64 = Arc::try_unwrap(size).unwrap().into_inner().unwrap();
            let hash_calculated = Arc::try_unwrap(hasher).unwrap().into_inner().unwrap().result();
            let hash_declared = Arc::try_unwrap(hash_from_stream).unwrap().into_inner().unwrap();

            if hash_declared.is_empty() {
                warn!(logger, "Upload from device '{}' wasn't finished, transferred {} B of data in {}", device_id, transferred_bytes, duration);
                #[allow(unused_must_use)] {
                    statsd_client.count("upload.total.failed", 1);
                    statsd_client.count(format!("upload.devices.{}.failed", &device_id.clone()).as_ref(), 1);
                }
                Err(Error::from(failures::CustomError::new("Upload wasn't finished properly")))
            } else {
                debug!(logger, "Uploaded file from device '{}' with size {} B, name '{}', declared hash {}", device_id, transferred_bytes, &uploaded_file.name, hex::encode(&hash_declared));

                if hash_calculated.to_vec() == hash_declared {
                    Ok((hex::encode(&hash_calculated), transferred_bytes))
                } else {
                    warn!(logger, "Declared hash '{}' don't match calculated '{}'", hex::encode(&hash_declared), hex::encode(&hash_calculated));
                    Err(Error::from(failures::CustomError::new("Declared and real sha256 don't match")))
                }
            }
        }).and_then(|(hash, size)| {
        let old_file = dao.find_file(&device_id, &uploaded_file.name)?;

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

pub fn load(logger: Logger, repo: &Repo, dao: &Dao, version_id: u32) -> Result<Option<(String, Box<Read>)>, Error> {
    dao.get_hash_and_storage_name(version_id)
        .map(|n| {
            n.map(|(hash, storage_name)| {
                use std::thread::spawn;

                let (reader, writer) = pipe::pipe();
                let mut writer = Box::from(writer);

                let boxed_repo = Box::from(repo.repo.clone());
                let decrypt_handle = repo.repo.unlock_decrypt(&*repo.pass).unwrap();

                spawn(move || {
                    match boxed_repo.read(&storage_name, &mut writer, &decrypt_handle) {
                        Ok(_) => (), // ok
                        Err(err) => warn!(logger, "Error while reading the file: {}", err)
                    }
                    ()
                });

                (hash, Box::from(reader) as Box<Read>)
            })
        }).map_err(Error::from)
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
