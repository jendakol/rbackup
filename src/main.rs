#![feature(plugin, custom_derive)]
#![plugin(rocket_codegen)]

extern crate failure;
extern crate rbackup;
extern crate config;
extern crate rocket;
#[macro_use]
extern crate log;

use failure::Error;
use rocket::Data;
use rocket::response::Stream;
use rocket::State;
use std::process::ChildStdout;

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
fn list(config: State<AppConfig>, metadata: ListMetadata) -> Result<String, Error> {
    rbackup::list(&config.repo_dir, &metadata.pc_id)
}

#[get("/download?<metadata>")]
fn download(config: State<AppConfig>, metadata: DownloadMetadata) -> Result<Stream<ChildStdout>, Error> {
    rbackup::load(&config.repo_dir, &metadata.pc_id, &metadata.orig_file_name, metadata.time_stamp)
        .map(|stdout| {
            Stream::from(
                stdout
            )
        })
}

#[post("/upload?<metadata>", format = "application/octet-stream", data = "<data>")]
fn upload(config: State<AppConfig>, data: Data, metadata: UploadMetadata) -> &'static str {
    match rbackup::save(&config.repo_dir, &metadata.pc_id, &metadata.orig_file_name, data) {
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
    let mut config = config::Config::default();
    let config = config.merge(config::File::with_name("Settings")).unwrap();

    let config = AppConfig {
        repo_dir: config.get_str("repo").expect("Could not extract repo path from config")
    };

    rocket::ignite()
        .mount("/", routes![upload])
        .mount("/", routes![download])
        .mount("/", routes![list])
        .manage(config)
        .launch();
}
