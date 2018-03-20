#![feature(plugin, custom_derive)]
#![plugin(rocket_codegen)]

extern crate config;
extern crate failure;
extern crate mysql;
extern crate pipe;
extern crate rbackup;
extern crate rdedup_lib as rdedup;
extern crate rocket;
extern crate serde;
extern crate serde_json;
#[macro_use]
extern crate slog;
extern crate slog_async;
extern crate slog_term;

use rdedup::Repo as RdedupRepo;
use rocket::Data;
use rocket::http::Status;
use rocket::Outcome;
use rocket::request::{self, FromRequest, Request};
use rocket::response::status;
use rocket::response::Stream;
use rocket::State;
use std::io::{Error as IoError, ErrorKind};
use std::path::Path;
use failure::Error;
use rbackup::dao::Dao;
use rbackup::encryptor::Encryptor;
use rbackup::structs::*;

#[derive(FromForm)]
struct UploadMetadata {
    file_name: String,
}

#[derive(FromForm)]
struct DownloadMetadata {
    file_version_id: u32,
}

#[derive(FromForm)]
struct RemoveFileMetadata {
    file_name: String,
}

#[derive(FromForm)]
struct RemoveFileVersionMetadata {
    file_version_id: u32,
}

#[derive(FromForm)]
struct LoginMetadata {
    device_id: String,
    repo_pass: String
}

struct Headers {
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
fn login(config: State<AppConfig>, metadata: LoginMetadata) -> status::Custom<String> {
    match rbackup::login(&config.repo, &config.dao, &config.encryptor, &metadata.device_id, &metadata.repo_pass) {
        Ok(pass) => status::Custom(Status::Ok, pass),
        Err(_) => status::Custom(Status::Unauthorized, "Cannot authenticate".to_string()),
    }
}

#[get("/list")]
fn list(config: State<AppConfig>, headers: Headers) -> status::Custom<String> {
    with_authentication(&config.dao, &config.encryptor, &headers.session_pass, |device| {
        match rbackup::list(&config.dao, &device.id) {
            Ok(list) => status_ok(list),
            Err(e) => status_internal_server_error(e)
        }
    })
}

#[get("/download?<metadata>")]
fn download(config: State<AppConfig>, headers: Headers, metadata: DownloadMetadata) -> Result<Stream<pipe::PipeReader>, Error> {
    let device = rbackup::authenticate(&config.dao, &config.encryptor, &headers.session_pass)?.expect("Could not find session");

    let repo = Repo::new(config.repo.clone(), device.repo_pass);

    rbackup::load(&repo, &config.dao, metadata.file_version_id)
        .map(Stream::from)
}

#[post("/upload?<metadata>", format = "application/octet-stream", data = "<data>")]
fn upload(config: State<AppConfig>, headers: Headers, metadata: UploadMetadata, data: Data) -> status::Custom<String> {
    with_authentication(&config.dao, &config.encryptor, &headers.session_pass, |device| {
        let uploaded_file_metadata = UploadedFile {
            name: String::from(metadata.file_name.clone()),
            device_id: String::from(device.id)
        };

        let repo = Repo::new(config.repo.clone(), device.repo_pass);

        let result = rbackup::save(&repo, &config.dao, uploaded_file_metadata, data)
            .and_then(|f| { serde_json::to_string(&f).map_err(Error::from) });

        match result {
            Ok(file) => status_ok(file),
            Err(e) => status_internal_server_error(e)
        }
    })
}

#[get("/remove/file/version?<metadata>")]
fn remove_file_version(config: State<AppConfig>, headers: Headers, metadata: RemoveFileVersionMetadata)-> status::Custom<String> {
    with_authentication(&config.dao, &config.encryptor, &headers.session_pass, |device| {
        let repo = Repo::new(config.repo.clone(), device.repo_pass);

        match rbackup::remove_file_version(&repo, &config.dao, metadata.file_version_id) {
            Ok((status, body)) => status::Custom(Status::raw(status), body),
            Err(e) => status_internal_server_error(e)
        }
    })
}

#[get("/remove/file?<metadata>")]
fn remove_file(config: State<AppConfig>, headers: Headers, metadata: RemoveFileMetadata)-> status::Custom<String> {
    with_authentication(&config.dao, &config.encryptor, &headers.session_pass, |device| {
        let repo = Repo::new(config.repo.clone(), device.repo_pass);

        match rbackup::remove_file(&repo, &config.dao, &metadata.file_name) {
            Ok((status, body)) => status::Custom(Status::raw(status), body),
            Err(e) => status_internal_server_error(e)
        }
    })
}

fn with_authentication<F2: FnOnce(DeviceIdentity) -> status::Custom<String>>(dao: &Dao, enc: &Encryptor, session_pass: &str, f2: F2) -> status::Custom<String> {
    match rbackup::authenticate(dao, enc, session_pass) {
        Ok(Some(identity)) => f2(identity.clone()),
        Ok(None) => status::Custom(Status::Unauthorized, "Cannot find session".to_string()),
        Err(e) => status::Custom(Status::InternalServerError, format!("{}", e))
    }
}

fn status_internal_server_error(e: Error) -> status::Custom<String> {
    status::Custom(Status::InternalServerError, format!("{}", e))
}

fn status_ok(s: String) -> status::Custom<String> {
    status::Custom(Status::Ok, s)
}

struct AppConfig {
    repo: RdedupRepo,
    dao: Dao,
    encryptor: Encryptor,
    logger: slog::Logger,
}

fn start_server() -> () {
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

    let secret = config.clone().get_str("secret").expect("There is no secret provided");

    // configure server:

//    let tls_config = config.get_table("tls").unwrap();

    let rocket_config = rocket::Config::build(rocket::config::Environment::Development)
        .address(config.get_str("address").expect("There is no bind address provided"))
        .port(config.get_int("port").expect("There is no bind port provided") as u16)
        .workers(config.get_int("workers").expect("There is no workers count provided") as u16)
//        .tls(tls_config.get("certs").expect("There is no TLS cert path provided").to_string(),
//             tls_config.get("key").expect("There is no TLS key path provided").to_string())
        .unwrap();

    rocket::custom(rocket_config, true)
        .mount("/", routes![upload])
        .mount("/", routes![download])
        .mount("/", routes![list])
        .mount("/", routes![remove_file])
        .mount("/", routes![remove_file_version])
        .mount("/", routes![login])
        .manage(AppConfig {
            repo,
            dao,
            encryptor: Encryptor::new(secret.clone()),
            logger,
        })
        .launch();
}

fn main() {

//    let decorator = slog_term::PlainDecorator::new(std::io::stdout());
//    let drain = slog_term::CompactFormat::new(decorator).build().fuse();
//    let drain = slog_async::Async::new(drain).build().fuse();

    // TODO commands

    start_server()
}
