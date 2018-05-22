#[macro_use]
extern crate arrayref;
extern crate cadence;
extern crate chrono;
extern crate crypto;
extern crate env_logger;
extern crate failure;
extern crate hex;
extern crate multimap;
extern crate multipart;
#[macro_use]
extern crate mysql;
extern crate pipe;
extern crate rdedup_lib as rdedup;
extern crate rdedup_lib;
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
use failures::*;
use multipart::server::{Multipart, MultipartField, ReadEntry, ReadEntryResult};
use rdedup::Repo as RdedupRepo;
use results::*;
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
pub mod failures;
pub mod encryptor;
pub mod structs;
pub mod results;

struct DigestDataStream {
    inner: Arc<Mutex<DigestDataStreamInner>>,
    handle_upload_chunk: Box<Fn(u64) -> () + Send + Sync + 'static>
}

impl DigestDataStream {
    pub fn new(inner: Arc<Mutex<DigestDataStreamInner>>, handle_upload_chunk: Box<Fn(u64) -> () + Send + Sync + 'static>) -> DigestDataStream {
        DigestDataStream {
            inner,
            handle_upload_chunk
        }
    }
}

impl Read for DigestDataStream {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, std::io::Error> {
        let mut inner = self.inner.lock().unwrap();

        inner.read(buf)
            .map(|s| {
                inner.size_inc(s);
                inner.hash_update(&buf[..s]);
                (self.handle_upload_chunk)(s as u64);
                s
            })
    }
}

struct DigestDataStreamInner {
    file_entry: MultipartField<Multipart<DataStream>>,
    hasher: Sha256,
    size: u64
}

impl DigestDataStreamInner {
    pub fn new(file_entry: MultipartField<Multipart<DataStream>>) -> DigestDataStreamInner {
        DigestDataStreamInner {
            file_entry,
            hasher: Sha256::default(),
            size: 0
        }
    }

    pub fn size_inc(&mut self, s: usize) -> () {
        self.size += s as u64;
    }

    pub fn hash_update(&mut self, bytes: &[u8]) -> () {
        self.hasher.input(bytes);
    }
}

impl Read for DigestDataStreamInner {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.file_entry.data.read(buf)
    }
}

fn process_multipart_upload(logger: &Logger, statsd_client: StatsdClient, repo: &Repo, boundary: &str, data: Data, storage_name: &str, device_id: String) -> Result<UploadedData, Error> {
    let multipart = Multipart::with_body(data.open(), boundary);

    // read file:

    let file_entry: MultipartField<Multipart<DataStream>> = match multipart.read_entry() {
        ReadEntryResult::Entry(entry) => entry,
        ReadEntryResult::End(_) => return Err(Error::from(CustomError::new("'file' part is missing"))),
        ReadEntryResult::Error(_, err) => return Err(Error::from(err))
    };

    debug!(logger, "Handling file upload");

    let data_inner = Arc::new(Mutex::new(DigestDataStreamInner::new(file_entry)));

    let statsd_client_cp = statsd_client.clone();
    let device_id_cp = device_id.clone();

    let stream = DigestDataStream::new(data_inner.clone(),
                                       Box::from(move |copied_bytes| {
                                           #[allow(unused_must_use)] {
                                               statsd_client_cp.count("upload.total.bytes", copied_bytes as i64);
                                               statsd_client_cp.count(&format!("upload.devices.{}.bytes", &device_id_cp), copied_bytes as i64);
                                           }
                                       }));

    let encrypt_handle = repo.repo.unlock_encrypt(&*repo.pass)?;
    repo.repo.write(storage_name, stream, &encrypt_handle)?;

    let data = {
        Arc::try_unwrap(data_inner).map_err(|_| Error::from(CustomError::new("Could not unlock the file_entry after reading")))?.into_inner()?
    };

    let hash_calculated: String = hex::encode(data.hasher.result());

    // read file hash:

    let file_entry: MultipartField<Multipart<DataStream>> = data.file_entry;

    let mut file_entry = match file_entry.next_entry() {
        ReadEntryResult::Entry(entry) => entry,
        ReadEntryResult::End(_) => return Err(Error::from(CustomError::new("'file-hash' part is missing"))),
        ReadEntryResult::Error(_, err) => return Err(Error::from(err))
    };

    let mut hash_declared: Vec<u8> = Vec::new();
    file_entry.data.read_to_end(&mut hash_declared)?;
    let hash_declared: String = String::from_utf8(hash_declared)?;

    debug!(logger, "Declared hash '{}', calculated '{}'", &hash_declared, &hash_calculated);

    // check hash and return

    if hash_calculated == hash_declared {
        Ok(UploadedData::Success(data.size, hash_calculated))
    } else {
        warn!(logger, "Declared hash '{}' doesn't match calculated '{}'", &hash_declared, &hash_calculated);
        #[allow(unused_must_use)] {
            statsd_client.count("upload.total.failed", 1);
            statsd_client.count(format!("upload.devices.{}.failed", &device_id).as_ref(), 1);
        }
        Ok(UploadedData::MismatchSha256)
    }
}

pub fn register(logger: &Logger, dao: &Dao, username: &str, pass: &str) -> Result<RegisterResult, Error> {
    dao.register(username, pass)
        .and_then(|r| match r {
            RegisterResult::Created(account_id) => {
                info!(logger, "Registered new account with ID {}", account_id);
                RdedupRepo::init(&std::path::Path::new(&format!("/data/deduprepo/{}", account_id)), &*Box::new(move || { Ok(String::from(pass)) }), rdedup::settings::Repo::new(), logger.clone())
                    .map(|_| RegisterResult::Created(account_id))
                    .map_err(Error::from)
            },
            RegisterResult::Exists => Ok(RegisterResult::Exists)
        })
}

pub fn login(dao: &Dao, enc: &Encryptor, device_id: &str, username: &str, pass: &str) -> Result<results::LoginResult, Error> {
    dao.login(enc, device_id, username, pass)
        .map_err(Error::from)
}

pub fn authenticate(dao: &Dao, enc: &Encryptor, session_pass: &str) -> Result<Option<DeviceIdentity>, Error> {
    dao.authenticate(enc, session_pass)
        .map_err(Error::from)
}

pub fn save(logger: &Logger, statsd_client: StatsdClient, repo: &Repo, dao: &Dao, uploaded_file: UploadedFile, boundary: &str, data: Data) -> Result<UploadResult, Error> {
    let current_time = SystemTime::now()
        .duration_since(UNIX_EPOCH)?;

    let stopwatch = Stopwatch::start_new();

    let time_stamp = NaiveDateTime::from_timestamp(current_time.as_secs() as i64, current_time.subsec_nanos());
    let storage_name = to_storage_name(&uploaded_file.device_id, &uploaded_file.name, time_stamp);

    debug!(logger, "Current time {}, final name {}", time_stamp, storage_name);

    process_multipart_upload(logger, statsd_client.clone(), repo, boundary, data, &storage_name, uploaded_file.device_id.clone())
        .and_then(|uploaded| match uploaded {
            UploadedData::Success(size, hash) => {
                let duration = stopwatch.elapsed_ms() as u64;
                debug!(logger, "Uploaded file with size {} B, name '{}', declared hash {} in time {}", size, &uploaded_file.name, hex::encode(&hash), duration);

                #[allow(unused_must_use)] {
                    statsd_client.time("upload.total.length", duration);
                    statsd_client.time(format!("upload.devices.{}.length", uploaded_file.device_id).as_ref(), duration);
                }

                let old_file = dao.find_file(&uploaded_file.device_id, &uploaded_file.name)?;

                // TODO check whether there is not already last version with the same hash
                let new_version = FileVersion {
                    version: 0, // cannot know now, will be filled in after DB insertion
                    size,
                    hash,
                    created: time_stamp,
                    storage_name
                };

                dao.save_new_version(&uploaded_file, old_file, new_version)
                    .map(UploadResult::Success)
                    .map_err(Error::from)
            },
            UploadedData::MismatchSha256 => Ok(UploadResult::MismatchSha256)
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

pub fn list_files(dao: &Dao, device_id: &str) -> Result<ListFileResult, Error> {
    dao.list_files(device_id)
        .map(|r| match r {
            Some(list) => ListFileResult::Success(list),
            None => ListFileResult::DeviceNotFound
        }).map_err(Error::from)
}

pub fn list_devices(dao: &Dao, account_id: &str) -> Result<String, Error> {
    let res = dao.get_devices(account_id)?;

    serde_json::to_string(&res)
        .map_err(Error::from)
}

pub fn remove_file_version(repo: &Repo, dao: &Dao, version_id: u32) -> Result<RemoveFileVersionResult, Error> {
    dao.remove_file_version(version_id)
        .map_err(Error::from)
        .map(|opt| opt.map(|sn| repo.repo.rm(&sn).map_err(Error::from)))
        .and_then(|r| match r {
            Some(Ok(_)) => Ok(RemoveFileVersionResult::Success),
            None => Ok(RemoveFileVersionResult::FileNotFound),
            Some(Err(e)) => Err(e)
        })
}

pub fn remove_file(logger: &Logger, repo: &Repo, dao: &Dao, device_id: &str, file_name: &str) -> Result<RemoveFileResult, Error> {
    dao.remove_file(logger, device_id, file_name)
        .map(|opt| match opt {
            Some(storage_names) => {
                let (_, failures): (Vec<_>, Vec<_>) = storage_names
                    .into_iter()
                    .map(|storage_name| {
                        repo.repo.rm(&storage_name)
                    }).partition(Result::is_ok);

                let failures: Vec<_> = failures.into_iter().map(Result::unwrap_err).collect();

                if failures.is_empty() {
                    RemoveFileResult::Success
                } else {
                    RemoveFileResult::PartialFailure(failures)
                }
            },
            None => RemoveFileResult::FileNotFound
        })
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
