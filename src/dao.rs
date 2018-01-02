extern crate mysql;
extern crate chrono;
extern crate time;
extern crate multimap;
extern crate serde_json;
extern crate serde;

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

#[derive(Debug, PartialEq, Eq, Hash, Serialize)]
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

    pub fn save_new_version(&self, old_file: &File, new_file_version: FileVersion) -> mysql::error::Result<File> {
        unimplemented!()
    }

    pub fn list_files(&self, device_id: &str) -> mysql::error::Result<Vec<File>> {
        self.pool.prep_exec(format!("select files.id, device_id, original_name, size, hash, created, storage_name from {}.files join {}.files_versions on {}.files_versions.file_id = {}.files.id where device_id=:device_id", self.db_name, self.db_name, self.db_name, self.db_name), params! { "device_id" => device_id})
            .map(|result| {
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
