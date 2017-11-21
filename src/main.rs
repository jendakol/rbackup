#![feature(plugin, custom_derive)]
#![plugin(rocket_codegen)]

extern crate rbackup;
extern crate config;
extern crate rocket;
extern crate tempfile;
#[macro_use]
extern crate log;

use rocket::Data;
use rocket::response::Stream;
use rocket::State;
use std::io;
use std::fs::File;
use tempfile::NamedTempFile;

#[derive(FromForm)]
struct UploadMetadata {
    orig_file_name: String,
    pc_id: String
}

#[derive(FromForm)]
struct DownloadMetadata {
    orig_file_name: String,
    time_stamp: u64,
    pc_id: String
}

#[derive(FromForm)]
struct ListMetadata {
    pc_id: String
}

#[get("/list?<metadata>")]
fn list(config: State<AppConfig>, metadata: ListMetadata) -> io::Result<String> {
    rbackup::list(config.repo_dir.clone(), metadata.pc_id)
}

#[get("/download?<metadata>")]
fn download(config: State<AppConfig>, metadata: DownloadMetadata) -> io::Result<Stream<File>> {
    rbackup::load(config.repo_dir.clone(), metadata.pc_id, metadata.orig_file_name, metadata.time_stamp)
        .and_then(|path| {
            Result::Ok(
                Stream::from(
                    File::from(path)
                )
            )
        })
}

#[post("/upload?<metadata>", format = "application/octet-stream", data = "<data>")]
fn upload(config: State<AppConfig>, data: Data, metadata: UploadMetadata) -> &'static str {
    // TODO stream the data!

    let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");

    io::copy(&mut data.open(), &mut temp_file).expect("Could not copy received data to temp file");

    let temp_file_name = temp_file.path().to_str().expect("Could not extract filename from temp file");

    match rbackup::save(config.repo_dir.clone(), metadata.pc_id, metadata.orig_file_name, temp_file_name) {
        Ok(()) => {
            "ok"
        }
        Err(e) => {
            warn!("{}", e);
            "FAIL"
        }
    }
}

struct AppConfig {
    repo_dir: String
}

fn main() {
    let settings = config::Config::default()
        .merge(config::File::with_name("Settings")).unwrap();

    let config = AppConfig {
        repo_dir: settings.get_str("repo").expect("Could not extract repo path from config")
    };

    rocket::ignite()
        .mount("/", routes![upload])
        .mount("/", routes![download])
        .mount("/", routes![list])
        .manage(config)
        .launch();
}
