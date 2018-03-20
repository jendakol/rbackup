extern crate chrono;
extern crate multimap;
extern crate mysql;
extern crate serde;
extern crate serde_json;
extern crate time;

use dao::mysql::chrono::prelude::*;
use encryptor::Encryptor;
use failure::Error;
use hex;
use sha2::*;
use structs::*;
use uuid::Uuid;

#[derive(Debug, PartialEq, Eq)]
pub struct Device {
    pub id: String
}

#[derive(Debug, PartialEq, Eq, Hash, Serialize)]
pub struct File {
    pub id: u64,
    pub device_id: String,
    pub original_name: String,
    pub versions: Vec<FileVersion>
}

#[derive(Debug, PartialEq, Eq, Hash, Serialize, Clone)]
pub struct FileVersion {
    pub version: u64,
    pub size: u64,
    pub hash: String,
    pub created: NaiveDateTime,
    pub storage_name: String
}

pub struct Dao {
    pool: mysql::Pool,
    db_name: String,
}

impl Dao {
    pub fn new(connection_query: &str, db_name: &str) -> Dao {
        Dao {
            pool: mysql::Pool::new(connection_query).unwrap(),
            db_name: String::from(db_name),
        }
    }

    pub fn save_new_version(&self, uploaded_file: &UploadedFile, old_file: Option<File>, new_file_version: FileVersion) -> mysql::error::Result<File> {
        match old_file {
            Some(file) => {
                let r = self.pool.prep_exec(
                    format!("insert into {}.files_versions (file_id, created, size, hash, storage_name) values (:file_id, :created, :size, :hash, :storage_name)", self.db_name),
                    params! {"file_id" => file.id,
                                   "created" => &new_file_version.created,
                                   "size" => &new_file_version.size,
                                   "hash" => &new_file_version.hash,
                                   "storage_name" => &new_file_version.storage_name
                                   })?;

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
            ,
            None => {
                let insert_file_result = self.pool.prep_exec(
                    format!("insert into {}.files (device_id, original_name) values (:device_id, :original_name)", self.db_name),
                    params! {"device_id" => &uploaded_file.device_id,
                                   "original_name" => &uploaded_file.name
                                   })?;

                // TODO what if now the flow fails - orphaned record in DB!

                let file_id = insert_file_result.last_insert_id();

                let file = File {
                    id: file_id,
                    device_id: uploaded_file.device_id.clone(),
                    original_name: uploaded_file.name.clone(),
                    versions: Vec::new()
                };

                // call recursively with filled-in old_file arg
                self.save_new_version(uploaded_file, Some(file), new_file_version)
            }
        }
    }

    pub fn find_file(&self, device_id: &str, orig_file_name: &str) -> mysql::error::Result<Option<File>> {
        self.pool.prep_exec(
            format!("select files.id, device_id, original_name, files_versions.id, size, hash, created, storage_name from {}.files join {}.files_versions on {}.files_versions.file_id = {}.files.id where device_id=:device_id and original_name=:original_name",
                    self.db_name, self.db_name, self.db_name, self.db_name),
            params! { "device_id" => device_id, "original_name" => orig_file_name}
        ).map(|result| {
            if result.more_results_exists() {
                // TODO optimize
                result.map(|x| x.unwrap()).map(|row| {
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
                    })
            } else {
                None
            }
        })
    }

    pub fn get_storage_name(&self, version_id: u32) -> mysql::error::Result<Option<String>> {
        self.pool.prep_exec(format!("select storage_name from {}.files_versions where id=:version_id", self.db_name),
                            params! {"version_id" => version_id})
            .map(|result| {
                result.map(|r| r.unwrap())
                    .map(|row| {
                        mysql::from_row(row)
                    })
                    .into_iter().next()
            })
    }

    pub fn get_storage_names(&self, file_name: &str) -> mysql::error::Result<Vec<String>> {
        self.pool.prep_exec(format!("select storage_name from {}.files_versions join {}.files on {}.files_versions.file_id={}.files.id where {}.files.original_name=:file_name", self.db_name, self.db_name, self.db_name, self.db_name, self.db_name),
                            params! {"file_name" => file_name})
            .map(|result| {
                result.map(|r| r.unwrap())
                    .map(|row| {
                        mysql::from_row(row)
                    })
                    .collect::<Vec<String>>()
            })
    }

    pub fn list_files(&self, device_id: &str) -> mysql::error::Result<Vec<File>> {
        self.pool.prep_exec(
            format!("select files.id, device_id, original_name, files_versions.id, size, hash, created, storage_name from {}.files join {}.files_versions on {}.files_versions.file_id = {}.files.id where device_id=:device_id",
                    self.db_name, self.db_name, self.db_name, self.db_name), params! { "device_id" => device_id}
        ).map(|result| {
            result.map(|x| x.unwrap()).map(|row| {
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
            }).collect()
        })
    }

    pub fn remove_file_version(&self, version_id: u32) -> mysql::error::Result<Option<String>> {
        self.get_storage_name(version_id)
            .and_then(|st| {
                self.pool.prep_exec(format!("delete from {}.files_versions where id=:version_id limit 1", self.db_name),
                                    params! {"version_id" => version_id})
                    .map(|result| {
                        if result.affected_rows() > 0 {
                            st
                        } else { None }
                    })
            })
    }

    pub fn remove_file(&self, file_name: &str) -> mysql::error::Result<Option<Vec<String>>> {
        self.get_storage_names(file_name)
            .and_then(|st| {
                self.pool.prep_exec(format!("delete {}.files_versions from {}.files_versions join {}.files on {}.files_versions.file_id={}.files.id where {}.files.original_name=:file_name", self.db_name, self.db_name, self.db_name, self.db_name, self.db_name, self.db_name),
                                    params! {"file_name" => file_name})
                    .map(|result| {
                        if result.affected_rows() == st.len() as u64 {
                            Some(st)
                        } else { None }
                    })
            })
    }

    pub fn get_devices(&self) -> mysql::error::Result<Vec<Device>> {
        self.pool.prep_exec(format!("SELECT id from {}.devices", self.db_name), ())
            .map(|result| {
                result.map(|x| x.unwrap()).map(|row| {
                    let device_id = mysql::from_row(row);
                    Device {
                        id: device_id
                    }
                }).collect()
            })
    }

    pub fn authenticate(&self, enc: &Encryptor, session_pass: &str) -> mysql::error::Result<Option<DeviceIdentity>> {
        let hashed_pass: String = {
            let mut hasher = Sha256::new();
            hasher.input(session_pass.as_bytes());
            hex::encode(&hasher.result())
        };

        self.pool.prep_exec(format!("SELECT device_id, pass from {}.sessions where id=:id", self.db_name), params!("id" => hashed_pass))
            .map(|result| {
                result.map(|x| x.unwrap()).map(|row| {
                    let (device_id, pass) = mysql::from_row(row);

                    let pass: String = pass;
                    let pass = hex::decode(pass).expect("Could not convert hex to bytes");

                    let real_pass = enc.decrypt(&pass, session_pass.as_bytes()).expect("Could not decrypt repo pass");

                    DeviceIdentity {
                        id: device_id,
                        repo_pass: String::from_utf8(real_pass).expect("Could not convert repo pass to UTF-8")
                    }
                }).into_iter().next()
            })
    }

    pub fn login(&self, enc: &Encryptor, device_id: &str, repo_pass: &str) -> Result<String, Error> {
        let pass = Uuid::new_v4().hyphenated().to_string();

        let hashed_pass: String = {
            let mut hasher = Sha256::new();
            hasher.input(pass.as_bytes());
            hex::encode(&hasher.result())
        };

        let repo_pass: String = hex::encode(&enc.encrypt(repo_pass.as_bytes(), pass.as_bytes()).ok().unwrap());

        self.pool.prep_exec(format!("insert into {}.sessions (id, device_id, pass) values(:id, :device_id, :pass )", self.db_name), params!("id" => hashed_pass, "device_id" => device_id, "pass" => repo_pass ))
            .map(|_| { pass })
            .map_err(Error::from)
    }
}
