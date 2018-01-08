extern crate mysql;
extern crate chrono;
extern crate time;
extern crate multimap;
extern crate serde_json;
extern crate serde;

use structs::*;

use dao::mysql::chrono::prelude::*;


#[derive(Debug, PartialEq, Eq)]
pub struct Device {
    pub id: String
}

#[derive(Debug, PartialEq, Eq, Hash, Serialize)]
pub struct File {
    pub id: u32,
    pub device_id: String,
    pub original_name: String,
    pub versions: Vec<FileVersion>
}

#[derive(Debug, PartialEq, Eq, Hash, Serialize, Clone)]
pub struct FileVersion {
    pub size: u32,
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
                let result = self.pool.prep_exec(
                    format!("insert into {}.files_versions (file_id, created, size, hash, storage_name) values (:file_id, :created, :size, :hash, :storage_name)", self.db_name),
                    params! {"file_id" => file.id,
                                   "created" => &new_file_version.created,
                                   "size" => &new_file_version.size,
                                   "hash" => &new_file_version.hash,
                                   "storage_name" => &new_file_version.storage_name
                                   })?;

                // TODO cloning :-(
                let mut versions = file.versions;
                versions.push(new_file_version.clone());

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

                // TODO clone :-(
                let file = File {
                    id: file_id as u32,
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
            format!("select files.id, device_id, original_name, size, hash, created, storage_name from {}.files join {}.files_versions on {}.files_versions.file_id = {}.files.id where device_id=:device_id and original_name=:original_name",
                    self.db_name, self.db_name, self.db_name, self.db_name),
            params! { "device_id" => device_id, "original_name" => orig_file_name}
        ).map(|result| {
            if result.more_results_exists() {
                // TODO optimize
                result.map(|x| x.unwrap()).map(|row| {
                    let (id, device_id, original_name, size, hash, created, storage_name) = mysql::from_row(row);

                    (
                        (id, device_id, original_name),
                        FileVersion {
                            size,
                            hash,
                            created,
                            storage_name
                        }
                    )
                }).collect::<multimap::MultiMap<(u32, String, String), FileVersion>>()
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

    pub fn list_files(&self, device_id: &str) -> mysql::error::Result<Vec<File>> {
        self.pool.prep_exec(
            format!("select files.id, device_id, original_name, size, hash, created, storage_name from {}.files join {}.files_versions on {}.files_versions.file_id = {}.files.id where device_id=:device_id",
                    self.db_name, self.db_name, self.db_name, self.db_name), params! { "device_id" => device_id}
        ).map(|result| {
            result.map(|x| x.unwrap()).map(|row| {
                let (id, device_id, original_name, size, hash, created, storage_name) = mysql::from_row(row);

                (
                    (id, device_id, original_name),
                    FileVersion {
                        size,
                        hash,
                        created,
                        storage_name
                    }
                )
            }).collect::<multimap::MultiMap<(u32, String, String), FileVersion>>()
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
}
