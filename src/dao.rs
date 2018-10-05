extern crate chrono;
extern crate multimap;
extern crate mysql;
extern crate serde;
extern crate serde_json;
extern crate stopwatch;
extern crate time;

use cadence::prelude::*;
use cadence::StatsdClient;
use dao::stopwatch::Stopwatch;
use encryptor::Encryptor;
use failure::Error;
use failures::CustomError;
use hex;
use responses::*;
use sha2::*;
use slog::Logger;
use std::time::{SystemTime, UNIX_EPOCH};
use structs::*;
use uuid::Uuid;

pub struct Dao {
    pool: mysql::Pool,
    db_name: String,
    statsd_client: Option<StatsdClient>
}

impl Dao {
    pub fn new(connection_query: &str, db_name: &str, statsd_client: Option<StatsdClient>) -> Result<Dao, Error> {
        mysql::Pool::new(connection_query)
            .map(|pool| {
                Dao {
                    pool,
                    db_name: String::from(db_name),
                    statsd_client
                }
            }).map_err(Error::from)
    }

    fn report_timer(&self, name: &str, stopwatch: Stopwatch) -> () {
        #[allow(unused_must_use)] {
            match self.statsd_client {
                Some(ref cl) => {
                    cl.time(format!("dao.{}", name).as_ref(), stopwatch.elapsed_ms() as u64);
                },
                None => () // ok
            }
        }
    }

    pub fn exec(&self, query: &str) -> mysql::error::Result<()> {
        let query = query.replace("DBNAME", &self.db_name);
        let string = query.clone();

        string.split(";").map(String::from).fold(Ok(()), |acc, q| {
            acc.and_then(|_| {
                let trimmed = q.trim();

                if !trimmed.is_empty() {
                    self.pool.prep_exec(trimmed, ()).map(|_| ())
                } else {
                    Ok(())
                }
            })
        })
    }

    fn get_or_insert_file(&self, logger: &Logger, uploaded_file: &UploadedFile) -> mysql::error::Result<File> {
        let stopwatch = Stopwatch::start_new();

        debug!(logger, "Trying to create record for file {:?}", &uploaded_file);

        let file_identity_hash = Dao::create_file_identity_hash(uploaded_file);

        let qr = self.pool.prep_exec(
            format!("insert ignore into `{}`.files (device_id, original_name, identity_hash) values (:device_id, :original_name, :identity_hash)", self.db_name),
            params! {"device_id" => &uploaded_file.device_id,
                                   "original_name" => &uploaded_file.original_name,
                                   "identity_hash" => &file_identity_hash
                                   })?;

        self.report_timer("insert_file", stopwatch);

        if qr.affected_rows() > 0 {
            debug!(logger, "File {:?} was inserted into DB", &uploaded_file)
        }

        let file = self.find_file(logger, &uploaded_file.device_id, &uploaded_file.original_name)?.expect("Just inserted file was not found in DB");

        debug!(logger, "File {:?} has ID {} in DB", &uploaded_file, file.id);

        // TODO what if now the flow fails - orphaned record in DB! is it a problem?

        Ok(file)
    }

    pub fn save_file_version(&self, logger: &Logger, uploaded_file: &UploadedFile, new_file_version: FileVersion) -> mysql::error::Result<File> {
        let file = self.get_or_insert_file(logger, uploaded_file)?;

        let stopwatch = Stopwatch::start_new();

        let r = self.pool.prep_exec(
            format!("insert into `{}`.files_versions (file_id, created, size, hash, storage_name) values (:file_id, :created, :size, :hash, :storage_name)", self.db_name),
            params! {"file_id" => file.id,
                                   "created" => &new_file_version.created,
                                   "size" => &new_file_version.size,
                                   "hash" => &new_file_version.hash,
                                   "storage_name" => &new_file_version.storage_name
                                   })?;

        self.report_timer("insert_file_version", stopwatch);

        let new_id = r.last_insert_id();

        let mut new_file_version = new_file_version.clone();
        new_file_version.version = new_id;

        let mut versions = file.versions;
        versions.push(new_file_version);

        Ok(File {
            id: file.id,
            device_id: file.device_id,
            original_name: file.original_name,
            versions
        })
    }

    pub fn find_file(&self, logger: &Logger, device_id: &str, orig_file_name: &str) -> mysql::error::Result<Option<File>> {
        let stopwatch = Stopwatch::start_new();

        let result = self.pool.prep_exec(
            format!("select files.id, device_id, original_name, files_versions.id, size, hash, created, storage_name from `{}`.files join `{}`.files_versions on `{}`.files_versions.file_id = `{}`.files.id where device_id=:device_id and original_name=:original_name",
                    self.db_name, self.db_name, self.db_name, self.db_name),
            params! { "device_id" => device_id, "original_name" => orig_file_name}
        )?;

        self.report_timer("find_file", stopwatch);

        // TODO optimize
        let file_with_versions = result.map(|x| x.unwrap()).map(|row| {
            let (id, device_id, original_name, versionid, size, hash, created, storage_name) = mysql::from_row(row);

            (
                (id, device_id, original_name),
                FileVersion {
                    version: versionid,
                    size,
                    hash,
                    created,
                    storage_name
                }
            )
        }).collect::<multimap::MultiMap<(u64, String, String), FileVersion>>()
            .into_iter()
            .next()
            .map(|((id, device_id, original_name), versions)| {
                File {
                    id,
                    device_id,
                    original_name,
                    versions
                }
            });


        match file_with_versions {
            Some(f) => Ok(Some(f)),
            None => {
                // try fallback and find file even without any versions yet

                debug!(logger, "Trying fallback - find file with no versions so far ('{}', '{}')", device_id, orig_file_name);

                let stopwatch = Stopwatch::start_new();

                self.pool.prep_exec(
                    format!("select files.id, device_id, original_name from `{}`.files where device_id=:device_id and original_name=:original_name",
                            self.db_name),
                    params! { "device_id" => device_id, "original_name" => orig_file_name}
                ).map(|result| {
                    self.report_timer("find_file", stopwatch);

                    result.map(|x| x.unwrap()).map(|row| {
                        let (id, device_id, original_name) = mysql::from_row(row);

                        File {
                            id,
                            device_id,
                            original_name,
                            versions: Vec::new()
                        }
                    }).into_iter().next()
                })
            }
        }
    }

    pub fn get_hash_size_and_storage_name(&self, version_id: u32) -> mysql::error::Result<Option<(String, u64, String)>> {
        let stopwatch = Stopwatch::start_new();

        self.pool.prep_exec(format!("select hash, size, storage_name from `{}`.files_versions where id=:version_id", self.db_name),
                            params! {"version_id" => version_id})
            .map(|result| {
                self.report_timer("get_storage_name", stopwatch);

                result.map(|r| r.unwrap())
                    .map(|row| {
                        mysql::from_row(row)
                    })
                    .into_iter().next()
            })
    }

    pub fn get_storage_names(&self, device_id: &str, file_id: u32) -> mysql::error::Result<Vec<String>> {
        let stopwatch = Stopwatch::start_new();

        self.pool.prep_exec(format!("select storage_name from `{}`.files_versions join `{}`.files on `{}`.files_versions.file_id=`{}`.files.id where `{}`.files.id=:file_id and `{}`.files.device_id=:device_id", self.db_name, self.db_name, self.db_name, self.db_name, self.db_name, self.db_name),
                            params! {"file_id" => file_id, "device_id" => device_id})
            .map(|result| {
                self.report_timer("get_storage_names", stopwatch);

                result.map(|r| r.unwrap())
                    .map(|row| {
                        mysql::from_row(row)
                    })
                    .collect::<Vec<String>>()
            })
    }

    pub fn list_files(&self, account_id: &str, device_id: &str) -> mysql::error::Result<Option<Vec<File>>> {
        let stopwatch = Stopwatch::start_new();

        self.pool.prep_exec(
            format!("select files.id, device_id, original_name, files_versions.id, size, hash, created, storage_name from `{}`.files join `{}`.files_versions on `{}`.files_versions.file_id = `{}`.files.id where device_id=:device_id",
                    self.db_name, self.db_name, self.db_name, self.db_name), params! { "device_id" => device_id}
        ).and_then(|result| {
            self.report_timer("list_files", stopwatch);

            let files: Vec<File> = result.map(|x| x.unwrap()).map(|row| {
                let (id, device_id, original_name, versionid, size, hash, created, storage_name) = mysql::from_row(row);

                (
                    (id, device_id, original_name),
                    FileVersion {
                        version: versionid,
                        size,
                        hash,
                        created,
                        storage_name
                    }
                )
            }).collect::<multimap::MultiMap<(u64, String, String), FileVersion>>()
                .into_iter().map(|((id, device_id, original_name), versions)| {
                File {
                    id,
                    device_id,
                    original_name,
                    versions
                }
            }).collect();

            if files.len() >= 1 {
                Ok(Some(files))
            } else {
                // if no files were found, check if the device itself "is known"
                self.is_known_device(account_id, device_id).map(|exists| {
                    if exists { Some(Vec::new()) } else { None }
                })
            }
        })
    }

    pub fn remove_file_version(&self, logger: &Logger, version_id: u32) -> mysql::error::Result<Option<String>> {
        debug!(logger, "Deleting file version with ID '`{}`'", version_id);

        self.get_hash_size_and_storage_name(version_id)
            .and_then(|st| {
                let stopwatch = Stopwatch::start_new();

                self.pool.prep_exec(format!("delete from `{}`.files_versions where id=:version_id limit 1", self.db_name),
                                    params! {"version_id" => version_id})
                    .map(|result| {
                        self.report_timer("remove_file_version", stopwatch);

                        if result.affected_rows() > 0 {
                            st.map(|o| o.2)
                        } else { None }
                    })
            })
    }

    pub fn remove_file(&self, logger: &Logger, device_id: &str, file_id: u32) -> Result<Option<Vec<String>>, Error> {
        debug!(logger, "Deleting file versions for file with ID '`{}`' from device `{}`", file_id, device_id);

        self.get_storage_names(device_id, file_id)
            .map_err(Error::from)
            .and_then(|st| {
                let stopwatch = Stopwatch::start_new();

                if st.len() >= 1 {
                    self.pool.prep_exec(format!("delete `{}`.files_versions from `{}`.files_versions join `{}`.files on `{}`.files_versions.file_id=`{}`.files.id where `{}`.files.id=:file_id and `{}`.files.device_id=:device_id", self.db_name, self.db_name, self.db_name, self.db_name, self.db_name, self.db_name, self.db_name),
                                        params! {"file_id" => file_id, "device_id" => device_id})
                        .map_err(Error::from)
                        .and_then(|result| {
                            self.report_timer("remove_file", stopwatch);

                            let deleted = result.affected_rows();

                            debug!(logger, "Deleted file versions: `{}`", deleted);

                            if deleted == st.len() as u64 {
                                Ok(Some(st))
                            } else { Err(Error::from(CustomError::new("Could not delete all"))) }
                        })
                } else {
                    Ok(None)
                }
            }).and_then(|list| match list {
            Some(versions) => {
                // versions were deleted, now delete the file itself
                self.pool.prep_exec(format!("delete from `{}`.files where `{}`.files.id=:file_id and `{}`.files.device_id=:device_id", self.db_name, self.db_name, self.db_name),
                                    params! {"file_id" => file_id, "device_id" => device_id})
                    .map_err(Error::from)
                    .map(|_| Some(versions))
            },
            None => Ok(None)
        }).map_err(Error::from)
    }

    pub fn get_devices(&self, account_id: &str) -> mysql::error::Result<Vec<String>> {
        let stopwatch = Stopwatch::start_new();

        self.pool.prep_exec(format!("SELECT distinct device_id from `{}`.sessions where account_id=:account_id", self.db_name), params! {"account_id" => account_id})
            .map(|result| {
                self.report_timer("get_devices", stopwatch);

                result.map(|x| x.unwrap()).map(|row| {
                    let device_id: String = mysql::from_row(row);
                    device_id
                }).collect()
            })
    }

    pub fn is_known_device(&self, account_id: &str, device_id: &str) -> mysql::error::Result<bool> {
        let stopwatch = Stopwatch::start_new();

        self.pool.prep_exec(format!("SELECT device_id from `{}`.sessions where account_id=:account_id and device_id=:device_id", self.db_name), params! {"account_id" => account_id, "device_id" => device_id})
            .map(|result| {
                self.report_timer("device_exists", stopwatch);

                result.map(|x| x.unwrap()).map(|_| {
                    true
                }).into_iter().next().unwrap_or_else(|| false)
            })
    }

    pub fn authenticate(&self, enc: &Encryptor, session_pass: &str) -> mysql::error::Result<Option<DeviceIdentity>> {
        let hashed_pass: String = {
            let mut hasher = Sha256::new();
            hasher.input(session_pass.as_bytes());
            hex::encode(&hasher.result())
        };

        let stopwatch = Stopwatch::start_new();

        self.pool.prep_exec(format!("SELECT device_id, account_id, pass from `{}`.sessions where id=:id", self.db_name), params!("id" => hashed_pass.clone()))
            .map(|result| {
                self.report_timer("find_session", stopwatch);

                result.map(|x| x.unwrap()).map(|row| {
                    let (device_id, account_id, pass) = mysql::from_row(row);

                    let pass: String = pass;
                    let pass = hex::decode(pass).expect("Could not convert hex to bytes");

                    let real_pass = enc.decrypt(&pass, session_pass.as_bytes()).expect("Could not decrypt repo pass");

                    DeviceIdentity {
                        id: device_id,
                        account_id,
                        repo_pass: String::from_utf8(real_pass).expect("Could not convert repo pass to UTF-8")
                    }
                }).into_iter().next()
            }).and_then(|ident_opt| {
            // if authentication was successful, update last_used field
            match ident_opt {
                Some(identity) => {
                    self.pool.prep_exec(format!("update `{}`.sessions set last_used = CURRENT_TIMESTAMP where id=:id", self.db_name), params!("id" => hashed_pass))
                        .map(|_| {
                            Some(identity)
                        })
                },
                None => Ok(None)
            }
        })
    }

    pub fn login(&self, logger: &Logger, enc: &Encryptor, device_id: &str, username: &str, pass: &str) -> Result<LoginResult, Error> {
        let hashed_pass: String = {
            let mut hasher = Sha256::new();
            hasher.input(pass.as_bytes());
            hex::encode(&hasher.result())
        };

        let stopwatch = Stopwatch::start_new();

        let find_account_result: Option<String> = {
            self.pool.prep_exec(format!("select id from `{}`.accounts where username=:username and password=:pass limit 1", self.db_name), params!("username" => username, "pass" => &hashed_pass))
                .map(|r| r.map(|x| x.unwrap())
                    .map(|row| {
                        let s: String = mysql::from_row(row);
                        s
                    }).into_iter().next())?
        };

        match find_account_result {
            Some(account_id) => {
                let find_session_result: Option<String> = {
                    self.pool.prep_exec(format!("select id from `{}`.sessions where device_id=:device_id and account_id=:account_id limit 1", self.db_name), params!("device_id" => device_id, "account_id"=>&account_id))
                        .map(|r| r.map(|x| x.unwrap())
                            .map(|row| {
                                let s: String = mysql::from_row(row);
                                s
                            }).into_iter().next())?
                };

                let new_session_id = Uuid::new_v4().hyphenated().to_string();

                let hashed_session_id: String = {
                    let mut hasher = Sha256::new();
                    hasher.input(new_session_id.as_bytes());
                    hex::encode(&hasher.result())
                };

                let encrypted_pass: String = hex::encode(&enc.encrypt(pass.as_bytes(), new_session_id.as_bytes()).ok().unwrap());

                let stopwatch = Stopwatch::start_new();

                self.pool.prep_exec(format!("insert into `{}`.sessions (id, account_id, device_id, pass) values(:id, :account_id, :device_id, :pass )", self.db_name), params!("id" => hashed_session_id, "account_id" => &account_id, "device_id" => device_id, "pass" => encrypted_pass ))
                    .map(|_| {
                        self.report_timer("login", stopwatch);

                        match find_session_result {
                            Some(_) => {
                                debug!(logger, "Renewed session: {}", &new_session_id);
                                LoginResult::RenewedSession(new_session_id)
                            }, // TODO this is bullshit
                            None => {
                                debug!(logger, "New session: {}", &new_session_id);
                                LoginResult::NewSession(new_session_id)
                            }
                        }
                    })
                    .map_err(Error::from)
            },
            None => {
                self.report_timer("loginNotFound", stopwatch);
                Ok(LoginResult::AccountNotFound)
            }
        }
    }

    pub fn register(&self, username: &str, pass: &str) -> Result<RegisterResult, Error> {
        // TODO check format of username

        let stopwatch = Stopwatch::start_new();

        let hashed_pass: String = {
            let mut hasher = Sha256::new();
            hasher.input(pass.as_bytes());
            hex::encode(&hasher.result())
        };

        let find_result = self.pool.prep_exec(format!("select id from `{}`.accounts where username=:username and password=:pass limit 1", self.db_name), params!("username" => username, "pass" => &hashed_pass))?;

        if find_result.count() == 0 {
            let account_id = Dao::create_account_id(username, &hashed_pass);

            let insert_result = self.pool.prep_exec(format!("insert into `{}`.accounts (id, username, password) values (:id, :username, :pass)", self.db_name), params!("id" => &account_id ,"username" => username, "pass" => hashed_pass))?;

            self.report_timer("register", stopwatch);

            if insert_result.affected_rows() == 1 {
                Ok(RegisterResult::Created(account_id))
            } else {
                Err(Error::from(CustomError::new("")))
            }
        } else {
            self.report_timer("registerExists", stopwatch);
            Ok(RegisterResult::Exists)
        }
    }

    fn create_account_id(username: &str, pass: &str) -> String {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();

        let mut hasher = Sha256::new();
        hasher.input(format!("{:?}", now).as_bytes());
        hasher.input(username.as_bytes());
        hasher.input(pass.as_bytes());
        hex::encode(&hasher.result())
    }

    fn create_file_identity_hash(uploaded_file: &UploadedFile) -> String {
        let mut hasher = Sha256::new();
        hasher.input(uploaded_file.device_id.as_bytes());
        hasher.input(uploaded_file.original_name.as_bytes());
        hex::encode(&hasher.result())
    }
}
