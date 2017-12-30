#![feature(plugin, custom_derive)]
#![plugin(rocket_codegen)]

extern crate failure;
extern crate rbackup;
extern crate config;
extern crate rocket;
#[macro_use]
extern crate slog;
extern crate slog_async;
extern crate slog_term;
extern crate pipe;
extern crate serde;
extern crate serde_json;
extern crate rdedup_lib as rdedup;
extern crate mysql;

use std::sync::Arc;

use failure::Error;
use rocket::Data;
use rocket::response::Stream;
use rocket::State;
use std::io::{Error as IoError, ErrorKind};

use std::path::Path;
use rdedup::{Repo as RdedupRepo};

use rbackup::dao::Dao;

#[derive(FromForm)]
struct UploadMetadata {
    orig_file_name: String,
    pc_id: String,
}

#[derive(FromForm)]
struct DownloadMetadata {
    orig_file_name: String,
    time_stamp: u64,
    pc_id: String,
}

#[derive(FromForm)]
struct ListMetadata {
    device_id: String
}

#[get("/list?<metadata>")]
fn list(config: State<AppConfig>, metadata: ListMetadata) -> Result<String, Error> {
    rbackup::list(&config.dao, &metadata.device_id)
}

#[get("/download?<metadata>")]
fn download(config: State<AppConfig>, metadata: DownloadMetadata) -> Result<Stream<pipe::PipeReader>, Error> {
    rbackup::load(&config.repo, &metadata.pc_id, &metadata.orig_file_name, metadata.time_stamp)
        .map(Stream::from)
}

#[post("/upload?<metadata>", format = "application/octet-stream", data = "<data>")]
fn upload(config: State<AppConfig>, data: Data, metadata: UploadMetadata) -> &'static str {
    match rbackup::save(&config.repo, &metadata.pc_id, &metadata.orig_file_name, data) {
        Ok(()) => {
            "ok"
        }
        Err(e) => {
            warn!(config.logger, "{}", e);
            "FAIL"
        }
    }
}


struct AppConfig {
    repo: rbackup::Repo,
    dao: Dao,
    logger: slog::Logger,
}


fn main() {

//    let decorator = slog_term::PlainDecorator::new(std::io::stdout());
//    let drain = slog_term::CompactFormat::new(decorator).build().fuse();
//    let drain = slog_async::Async::new(drain).build().fuse();

    let logger = slog::Logger::root(slog::Discard, o!());

    let mut config = config::Config::default();
    config.merge(config::File::with_name("Settings")).unwrap();

    // open Repo

    let repo = config.get_str("repo_dir")
        .map_err(|e| IoError::new(ErrorKind::NotFound, e))
        .and_then(|repo_dir| {
            RdedupRepo::open(&Path::new(&repo_dir), logger.clone())
        }).expect("Could not open repo");

    let config = Arc::new(config);
    let config_dec = Arc::clone(&config);
    let dec = Box::new(move || { config_dec.get_str("repo_pass").map_err(|e| IoError::new(ErrorKind::NotFound, e)) });

    let config_enc = Arc::clone(&config);
    let enc = Box::new(move || { config_enc.get_str("repo_pass").map_err(|e| IoError::new(ErrorKind::NotFound, e)) });

    // create DAO

    let dao = Dao::new(&format!("mysql://{}:{}@{}:{}",
                                config.get_str("db_user").unwrap(),
                                config.get_str("db_pass").unwrap(),
                                config.get_str("db_host").unwrap(),
                                config.get_str("db_port").unwrap()),
                       &config.get_str("db_name").unwrap(),
    );

    println!("{:?}", dao.get_devices());
    println!("{:?}", dao.list_files("placka"));


    let config = AppConfig {
        repo: rbackup::Repo {
            repo,
            decrypt: dec,
            encrypt: enc,
        },
        dao,
        logger,
    };

    rocket::ignite()
        .mount("/", routes![upload])
        .mount("/", routes![download])
        .mount("/", routes![list])
        .manage(config)
        .launch();
}
