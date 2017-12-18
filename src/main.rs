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
extern crate rdedup_lib as rdedup;

use failure::Error;
use rocket::Data;
use rocket::response::Stream;
use rocket::State;
use std::process::ChildStdout;
use std::io::{Error as IoError, ErrorKind, Read};

use std::path::Path;
use rdedup::{Repo as RdedupRepo, DecryptHandle, EncryptHandle};

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
    rbackup::list(&config.repo, &metadata.pc_id)
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
    logger: slog::Logger
}


fn main() {
//    let decorator = slog_term::PlainDecorator::new(std::io::stdout());
//    let drain = slog_term::CompactFormat::new(decorator).build().fuse();
//    let drain = slog_async::Async::new(drain).build().fuse();

    let logger = slog::Logger::root(slog::Discard, o!());

    let mut config = config::Config::default();
    let config = config.merge(config::File::with_name("Settings")).unwrap();

    let repo = config.get_str("repo_dir")
        .map_err(|e| IoError::new(ErrorKind::NotFound, e))
        .and_then(|repo_dir| {
            RdedupRepo::open(&Path::new(&repo_dir), logger.clone())
        }).expect("Could not open repo");

    let dec = repo.unlock_decrypt(&|| { config.get_str("repo_pass").map_err(|e| IoError::new(ErrorKind::NotFound, e)) })
        .expect("Could not init repo decryption");

    let enc = repo.unlock_encrypt(&|| { config.get_str("repo_pass").map_err(|e| IoError::new(ErrorKind::NotFound, e)) })
        .expect("Could not init repo encryption");

    let config = AppConfig {
        repo: rbackup::Repo {
            repo,
            decrypt: dec,
            encrypt: enc
        },
        logger
    };

    rocket::ignite()
        .mount("/", routes![upload])
        .mount("/", routes![download])
        .mount("/", routes![list])
        .manage(config)
        .launch();
}
