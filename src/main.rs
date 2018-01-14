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

use failure::Error;
use rocket::Data;
use rocket::response::Stream;
use rocket::State;
use rocket::Outcome;
use rocket::http::Status;
use rocket::request::{self, Request, FromRequest};

use std::io::{Error as IoError, ErrorKind};

use std::path::Path;
use rdedup::{Repo as RdedupRepo};

use rbackup::structs::*;
use rbackup::dao::Dao;
use rbackup::encryptor::Encryptor;

#[derive(FromForm)]
struct UploadMetadata {
    file_name: String,
}

#[derive(FromForm)]
struct DownloadMetadata {
    file_version_id: u32,
}

#[derive(FromForm)]
struct LoginMetadata {
    device_id: String,
    repo_pass: String
}

pub struct Headers {
    session_pass: String
}

impl<'a, 'r> FromRequest<'a, 'r> for Headers {
    type Error = ();

    fn from_request(request: &'a Request<'r>) -> request::Outcome<Headers, ()> {
        // device id

        let values: Vec<_> = request.headers().get("RBackup-Session-Pass").collect();
        if values.len() != 1 {
            return Outcome::Failure((Status::BadRequest, ()));
        }

        let session_pass = String::from(values[0]);

        return Outcome::Success(Headers {
            session_pass
        });
    }
}

#[get("/login?<metadata>")]
fn login(config: State<AppConfig>, metadata: LoginMetadata) -> Result<String, Error> {
    rbackup::login(&config.repo, &config.dao, &config.encryptor, &metadata.device_id, &metadata.repo_pass)
}

#[get("/list")]
fn list(config: State<AppConfig>, headers: Headers) -> Result<String, Error> {
    let device = rbackup::authenticate(&config.dao, &config.encryptor, &headers.session_pass)?.expect("Could not find session");

    rbackup::list(&config.dao, &device.id)
}

#[get("/download?<metadata>")]
fn download(config: State<AppConfig>, headers: Headers, metadata: DownloadMetadata) -> Result<Stream<pipe::PipeReader>, Error> {
    let device = rbackup::authenticate(&config.dao, &config.encryptor, &headers.session_pass)?.expect("Could not find session");

    let repo = Repo::new(config.repo.clone(), device.repo_pass);

    rbackup::load(&repo, &config.dao, metadata.file_version_id)
        .map(Stream::from)
}

#[post("/upload?<metadata>", format = "application/octet-stream", data = "<data>")]
fn upload(config: State<AppConfig>, headers: Headers, metadata: UploadMetadata, data: Data) -> Result<String, Error> {
    let device = rbackup::authenticate(&config.dao, &config.encryptor, &headers.session_pass)?.expect("Could not find session");

    let uploaded_file_metadata = UploadedFile {
        name: String::from(metadata.file_name),
        device_id: String::from(device.id)
    };

    let repo = Repo::new(config.repo.clone(), device.repo_pass);

    rbackup::save(&repo, &config.dao, uploaded_file_metadata, data)
        .map(|_| { String::from("ok") })
}


struct AppConfig {
    repo: RdedupRepo,
    dao: Dao,
    encryptor: Encryptor,
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

    // create DAO

    let dao = Dao::new(&format!("mysql://{}:{}@{}:{}",
                                config.get_str("db_user").unwrap(),
                                config.get_str("db_pass").unwrap(),
                                config.get_str("db_host").unwrap(),
                                config.get_str("db_port").unwrap()),
                       &config.get_str("db_name").unwrap(),
    );

    let secret_iv = config.clone().get_str("secret").expect("There is no secret provided");

    let app_config = AppConfig {
        repo,
        dao,
        encryptor: Encryptor::new(secret_iv),
        logger,
    };

    rocket::ignite()
        .mount("/", routes![upload])
        .mount("/", routes![download])
        .mount("/", routes![list])
        .mount("/", routes![login])
        .manage(app_config)
        .launch();
}
