#![feature(plugin, custom_derive)]
#![plugin(rocket_codegen)]

extern crate cadence;
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

use cadence::prelude::*;
use cadence::StatsdClient;
use failure::Error;
use rbackup::dao::Dao;
use rbackup::encryptor::Encryptor;
use rbackup::structs::*;
use rdedup::Repo as RdedupRepo;
use rocket::Data;
use rocket::http::{ContentType, Status};
use rocket::Outcome;
use rocket::request::{self, FromRequest, Request};
use rocket::response::{Response, status};
use rocket::response::status::Custom;
use rocket::State;
use slog::{Drain, Level, Logger};
use slog_async::Async;
use slog_term::{CompactFormat, TermDecorator};
use std::io::{Error as IoError, ErrorKind};
use std::path::Path;

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
    #[allow(unused_must_use)] {
        config.statsd_client.count("requests.login", 1);
        config.statsd_client.count("requests.total", 1);
    }

    match rbackup::login(&config.repo, &config.dao, &config.encryptor, &metadata.device_id, &metadata.repo_pass) {
        Ok(pass) => {
            #[allow(unused_must_use)] {
                config.statsd_client.count("login.ok", 1);
            }
            status::Custom(Status::Ok, pass)
        },
        Err(_) => {
            #[allow(unused_must_use)] {
                config.statsd_client.count("login.failed", 1);
            }
            status::Custom(Status::Unauthorized, "Cannot authenticate".to_string())
        },
    }
}

#[get("/list")]
fn list(config: State<AppConfig>, headers: Headers) -> status::Custom<String> {
    #[allow(unused_must_use)] { config.statsd_client.count("requests.list", 1); }

    with_authentication(&config.logger, &config.statsd_client, &config.dao, &config.encryptor, &headers.session_pass, |device| {
        match rbackup::list(&config.dao, &device.id) {
            Ok(list) => status_ok(list),
            Err(e) => status_internal_server_error(e)
        }
    })
}

#[get("/download?<metadata>")]
fn download(config: State<AppConfig>, headers: Headers, metadata: DownloadMetadata) -> Result<Response, Error> {
    #[allow(unused_must_use)] {
        config.statsd_client.count("requests.total", 1);
        config.statsd_client.count("requests.download", 1);
    }

    match rbackup::authenticate(&config.dao, &config.encryptor, &headers.session_pass) {
        Ok(Some(device)) => {
            #[allow(unused_must_use)] { config.statsd_client.count("authentication.ok", 1); }
            debug!(config.logger, "Opening repo");

            let repo = Repo::new(config.repo.clone(), device.repo_pass);

            rbackup::load(config.logger.clone(), &repo, &config.dao, metadata.file_version_id)
                .and_then(|o| {
                    match o {
                        Some((hash, read)) => {
                            rocket::response::Response::build()
                                .raw_header("RBackup-File-Hash", hash)
                                .streamed_body(read)
                                .ok()
                        },
                        None => {
                            rocket::response::Response::build()
                                .status(Status::NotFound)
                                .ok()
                        }
                    }
                })
        },
        Ok(None) => {
            #[allow(unused_must_use)] { config.statsd_client.count("authentication.not_found", 1); }
            debug!(config.logger, "Unauthenticated request! SessionId: {}", &headers.session_pass);

            rocket::response::Response::build()
                .status(Status::Unauthorized)
                .sized_body(std::io::Cursor::new("Cannot find session"))
                .ok()
        },
        Err(e) => {
            #[allow(unused_must_use)] { config.statsd_client.count("authentication.failure", 1); }
            warn!(config.logger, "Error while authenticating: {}", e);

            rocket::response::Response::build()
                .status(Status::InternalServerError)
                .sized_body(std::io::Cursor::new(format!("{:?}", e)))
                .ok()
        }
    }
}

#[post("/upload?<metadata>", data = "<data>")]
fn upload(config: State<AppConfig>, headers: Headers, metadata: UploadMetadata, data: Data, cont_type: &ContentType) -> Custom<String> {
    #[allow(unused_must_use)] { config.statsd_client.count("requests.upload", 1); }

    if !cont_type.is_form_data() {
        return Custom(
            Status::BadRequest,
            "Content-Type not multipart/form-data".into()
        );
    }

    let (_, boundary) = cont_type.params().find(|&(k, _)| k == "boundary").unwrap();

    with_authentication(&config.logger, &config.statsd_client, &config.dao, &config.encryptor, &headers.session_pass, |device| {
        let uploaded_file_metadata = UploadedFile {
            name: String::from(metadata.file_name.clone()),
            device_id: String::from(device.id)
        };

        let repo = Repo::new(config.repo.clone(), device.repo_pass);

        let result = rbackup::save(&config.logger, config.statsd_client.clone(), &repo, &config.dao, uploaded_file_metadata, boundary, data)
            .and_then(|f| { serde_json::to_string(&f).map_err(Error::from) });

        // TODO return bad request for request errors
        match result {
            Ok(file) => status_ok(file),
            Err(e) => status_internal_server_error(e)
        }
    })
}

#[get("/remove/file/version?<metadata>")]
fn remove_file_version(config: State<AppConfig>, headers: Headers, metadata: RemoveFileVersionMetadata) -> status::Custom<String> {
    #[allow(unused_must_use)] { config.statsd_client.count("requests.remove_file_version", 1); }

    with_authentication(&config.logger, &config.statsd_client, &config.dao, &config.encryptor, &headers.session_pass, |device| {
        let repo = Repo::new(config.repo.clone(), device.repo_pass);

        match rbackup::remove_file_version(&repo, &config.dao, metadata.file_version_id) {
            Ok((status, body)) => status::Custom(Status::raw(status), body),
            Err(e) => status_internal_server_error(e)
        }
    })
}

#[get("/remove/file?<metadata>")]
fn remove_file(config: State<AppConfig>, headers: Headers, metadata: RemoveFileMetadata) -> status::Custom<String> {
    #[allow(unused_must_use)] { config.statsd_client.count("requests.remove_file", 1); }

    with_authentication(&config.logger, &config.statsd_client, &config.dao, &config.encryptor, &headers.session_pass, |device| {
        let repo = Repo::new(config.repo.clone(), device.repo_pass);

        match rbackup::remove_file(&repo, &config.dao, &metadata.file_name) {
            Ok((status, body)) => status::Custom(Status::raw(status), body),
            Err(e) => status_internal_server_error(e)
        }
    })
}

fn with_authentication<F2: FnOnce(DeviceIdentity) -> status::Custom<String>>(logger: &Logger, statsd_client: &StatsdClient, dao: &Dao, enc: &Encryptor, session_pass: &str, f2: F2) -> status::Custom<String> {
    debug!(logger, "Authenticating request");

    #[allow(unused_must_use)] { statsd_client.count("requests.total", 1); }

    match rbackup::authenticate(dao, enc, session_pass) {
        Ok(Some(identity)) => {
            #[allow(unused_must_use)] { statsd_client.count("authentication.ok", 1); }
            f2(identity.clone())
        },
        Ok(None) => {
            #[allow(unused_must_use)] { statsd_client.count("authentication.not_found", 1); }
            debug!(logger, "Unauthenticated request! SessionId: {}", session_pass);
            status::Custom(Status::Unauthorized, "Cannot find session".to_string())
        },
        Err(e) => {
            #[allow(unused_must_use)] { statsd_client.count("authentication.failure", 1); }
            warn!(logger, "Error while authenticating: {}", e);
            status::Custom(Status::InternalServerError, format!("{}", e))
        }
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
    statsd_client: StatsdClient
}

fn start_server(logger: Logger, config: config::Config, statsd_client: StatsdClient) -> () {

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
                       statsd_client.clone()
    );

    let secret = config.clone().get_str("secret").expect("There is no secret provided");

    // configure server:

    info!(logger, "Configuring server");

//    let tls_config = config.get_table("tls").unwrap();

    let rocket_config = rocket::Config::build(rocket::config::Environment::Development)
        .address(config.get_str("address").expect("There is no bind address provided"))
        .port(config.get_int("port").expect("There is no bind port provided") as u16)
        .workers(config.get_int("workers").expect("There is no workers count provided") as u16)
//        .tls(tls_config.get("certs").expect("There is no TLS cert path provided").to_string(),
//             tls_config.get("key").expect("There is no TLS key path provided").to_string())
        .log_level(rocket::logger::LoggingLevel::Critical)
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
            statsd_client
        })
        .launch();
}

fn create_statsd_client(logger: Logger, host: &str, port: u16, prefix: &str) -> Result<StatsdClient, Error> {
    use std::net::{UdpSocket, ToSocketAddrs};
    use cadence::{QueuingMetricSink};

    let socket = UdpSocket::bind("0.0.0.0:0")?;
    socket.set_nonblocking(true)?;

    let host_and_port = format!("{}:{}", host, port).to_socket_addrs()?.next().unwrap();

    info!(logger, "Creating StatsD client reporting to {} with prefix '{}'", host_and_port, prefix);

    let udp_sink = cadence::UdpMetricSink::from(host_and_port, socket)?;
    let queuing_sink = QueuingMetricSink::from(udp_sink);

    Ok(
        StatsdClient::builder(prefix, queuing_sink)
            .with_error_handler(move |err| {
                error!(logger.clone(), "Error while sending stats: {}", err);
            })
            .build()
    )
}

fn main() {
    // This bit configures a logger
    // The nice colored stderr logger
    let decorator = TermDecorator::new().stderr().build();
    let term = CompactFormat::new(decorator)
        .use_local_timestamp()
        .build()
        .filter_level(Level::Info);
    // Run it in a separate thread, both for performance and because the terminal one isn't Sync
    let async = Async::new(term.ignore_res())
        // Especially in test builds, we have quite large bursts of messages, so have more space to
        // store them.
        .chan_size(2048)
        .build();
    let logger = Logger::root(async.ignore_res(),
                              o!("app" => format!("{}/{}",
                                                  env!("CARGO_PKG_NAME"),
                                                  env!("CARGO_PKG_VERSION"))));

    let mut config = config::Config::default();
    config.merge(config::File::with_name("Settings")).unwrap();

    let statsd_client = create_statsd_client(
        logger.clone(),
        config.get_str("statsd_host").expect("").as_ref(),
        config.get_int("statsd_port").expect("") as u16,
        config.get_str("statsd_prefix").expect("").as_ref(),
    ).unwrap();

    // TODO commands

    start_server(logger, config, statsd_client)
}
