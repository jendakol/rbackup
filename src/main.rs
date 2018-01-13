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

#[derive(FromForm)]
struct UploadMetadata {
    file_name: String,
}

#[derive(FromForm)]
struct DownloadMetadata {
    file_version_id: u32,
}

struct ClientIdentity {
    device_id: String,
    repo_pass: String
}

impl<'a, 'r> FromRequest<'a, 'r> for ClientIdentity {
    type Error = ();

    fn from_request(request: &'a Request<'r>) -> request::Outcome<ClientIdentity, ()> {
        // device id

        let values: Vec<_> = request.headers().get("X-Device-Id").collect();
        if values.len() != 1 {
            return Outcome::Failure((Status::BadRequest, ()));
        }

        let device_id = String::from(values[0]);

        // repo pass

        let values: Vec<_> = request.headers().get("X-Repo-Pass").collect();
        if values.len() != 1 {
            return Outcome::Failure((Status::BadRequest, ()));
        }

        let repo_pass = String::from(values[0]);

        return Outcome::Success(ClientIdentity {
            device_id,
            repo_pass
        });
    }
}

#[get("/list")]
fn list(config: State<AppConfig>, client_identity: ClientIdentity) -> Result<String, Error> {
    rbackup::list(&config.dao, &client_identity.device_id)
}

#[get("/download?<metadata>")]
fn download(config: State<AppConfig>, client_identity: ClientIdentity, metadata: DownloadMetadata) -> Result<Stream<pipe::PipeReader>, Error> {
    let repo = Repo::new(config.repo.clone(), client_identity.repo_pass);

    rbackup::load(&repo, &config.dao, metadata.file_version_id)
        .map(Stream::from)
}

#[post("/upload?<metadata>", format = "application/octet-stream", data = "<data>")]
fn upload(config: State<AppConfig>, client_identity: ClientIdentity, metadata: UploadMetadata, data: Data) -> String {
    let uploaded_file_metadata = UploadedFile {
        name: String::from(metadata.file_name),
        device_id: String::from(client_identity.device_id)
    };

    let repo = Repo::new(config.repo.clone(), client_identity.repo_pass);

    match rbackup::save(&repo, &config.dao, uploaded_file_metadata, data) {
        Ok(()) => {
            String::from("ok")
        }
        Err(e) => {
            warn!(config.logger, "{}", e);
            format!("{}", e)
        }
    }
}


struct AppConfig {
    repo: RdedupRepo,
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

    // create DAO

    let dao = Dao::new(&format!("mysql://{}:{}@{}:{}",
                                config.get_str("db_user").unwrap(),
                                config.get_str("db_pass").unwrap(),
                                config.get_str("db_host").unwrap(),
                                config.get_str("db_port").unwrap()),
                       &config.get_str("db_name").unwrap(),
    );

//    println!("{:?}", dao.get_devices());
//    println!("{:?}", dao.list_files("placka"));


    let config = AppConfig {
        repo,
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
