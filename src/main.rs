#![feature(plugin, custom_derive)]
#![plugin(rocket_codegen)]

extern crate args;
extern crate cadence;
extern crate config;
extern crate failure;
extern crate getopts;
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
extern crate slog_stream;
extern crate slog_term;
extern crate stopwatch;

use args::{Args, ArgsError};
use cadence::prelude::*;
use cadence::StatsdClient;
use failure::Error;
use getopts::Occur;
use rbackup::dao::Dao;
use rbackup::encryptor::Encryptor;
use rbackup::results::*;
use rbackup::structs::*;
use rocket::Data;
use rocket::http::{ContentType, Status};
use rocket::Outcome;
use rocket::request::{self, FromRequest, Request};
use rocket::response::{Response, status};
use rocket::State;
use slog::{Drain, Level, Logger};
use slog_async::Async;
use slog_term::{CompactFormat, TermDecorator};
use std::process::exit;

type HandlerResult<T> = Result<T, status::Custom<String>>;

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
struct ListFilesMetadata {
    device_id: Option<String>,
}

#[derive(FromForm)]
struct RemoveFileVersionMetadata {
    file_version_id: u32,
}

#[derive(FromForm)]
struct LoginMetadata {
    device_id: String,
    username: String,
    password: String
}

#[derive(FromForm)]
struct RegisterMetadata {
    username: String,
    password: String
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

#[get("/account/register?<metadata>")]
fn register(config: State<HandlerConfig>, metadata: RegisterMetadata) -> HandlerResult<RegisterResult> {
    with_metrics(&config.statsd_client, "register", || {
        rbackup::register(&config.logger, &config.dao, &metadata.username, &metadata.password)
            .map_err(status_internal_server_error)
    })
}

#[get("/account/login?<metadata>")]
fn login(config: State<HandlerConfig>, metadata: LoginMetadata) -> HandlerResult<LoginResult> {
    with_metrics(&config.statsd_client, "login", || {
        rbackup::login(&config.dao, &config.encryptor, &metadata.device_id, &metadata.username, &metadata.password)
            .map_err(status_internal_server_error)
    })
}

#[get("/list/files?<metadata>")]
fn list_files(config: State<HandlerConfig>, headers: Headers, metadata: ListFilesMetadata) -> HandlerResult<ListFileResult> {
    with_authentication(&config.logger, "list_files", &config.statsd_client, &config.dao, &config.encryptor, &headers.session_pass, |device| {
        rbackup::list_files(&config.dao, &metadata.device_id.unwrap_or(device.id))
    })
}

#[get("/list/devices")]
fn list_devices(config: State<HandlerConfig>, headers: Headers) -> HandlerResult<String> {
    with_authentication(&config.logger, "list_devices", &config.statsd_client, &config.dao, &config.encryptor, &headers.session_pass, |device| {
        rbackup::list_devices(&config.dao, &device.account_id)
    })
}

#[get("/download?<metadata>")]
fn download(config: State<HandlerConfig>, headers: Headers, metadata: DownloadMetadata) -> HandlerResult<Response> {
    with_authentication(&config.logger, "download", &config.statsd_client, &config.dao, &config.encryptor, &headers.session_pass, |device| {
        debug!(config.logger, "Opening repo");

        Repo::new(&config.repo_root, &device.account_id, device.repo_pass, config.logger.clone())
            .and_then(|repo| rbackup::load(config.logger.clone(), &repo, &config.dao, metadata.file_version_id))
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
    })
}

#[post("/upload?<metadata>", data = "<data>")]
fn upload(config: State<HandlerConfig>, headers: Headers, metadata: UploadMetadata, data: Data, cont_type: &ContentType) -> HandlerResult<UploadResult> {
    with_authentication(&config.logger, "upload", &config.statsd_client, &config.dao, &config.encryptor, &headers.session_pass, |device| {
        let uploaded_file_metadata = UploadedFile {
            name: String::from(metadata.file_name.clone()),
            device_id: String::from(device.id)
        };

        if !cont_type.is_form_data() {
            return Ok(UploadResult::InvalidRequest("Content-Type not multipart/form-data".to_string()));
        }

        let (_, boundary) = cont_type.params().find(|&(k, _)| k == "boundary").unwrap();

        Repo::new(&config.repo_root, &device.account_id, device.repo_pass, config.logger.clone())
            .and_then(|repo| {
                rbackup::save(&config.logger, config.statsd_client.clone(), &repo, &config.dao, uploaded_file_metadata, boundary, data)
            })
    })
}

#[get("/remove/fileVersion?<metadata>")]
fn remove_file_version(config: State<HandlerConfig>, headers: Headers, metadata: RemoveFileVersionMetadata) -> HandlerResult<RemoveFileVersionResult> {
    with_authentication(&config.logger, "remove_file_version", &config.statsd_client, &config.dao, &config.encryptor, &headers.session_pass, |device| {
        Repo::new(&config.repo_root, &device.account_id, device.repo_pass, config.logger.clone())
            .and_then(|repo| {
                rbackup::remove_file_version(&repo, &config.dao, metadata.file_version_id)
            })
    })
}

#[get("/remove/file?<metadata>")]
fn remove_file(config: State<HandlerConfig>, headers: Headers, metadata: RemoveFileMetadata) -> HandlerResult<RemoveFileResult> {
    with_authentication(&config.logger, "remove_file", &config.statsd_client, &config.dao, &config.encryptor, &headers.session_pass, |device| {
        Repo::new(&config.repo_root, &device.account_id, device.repo_pass.clone(), config.logger.clone())
            .and_then(|repo| {
                rbackup::remove_file(&config.logger, &repo, &config.dao, &device.id, &metadata.file_name)
            })
    })
}

fn with_authentication<'a, R: rocket::response::Responder<'a>, F2: FnOnce(DeviceIdentity) -> Result<R, Error>>(logger: &Logger, name: &str, statsd_client: &StatsdClient, dao: &Dao, enc: &Encryptor, session_id: &str, f2: F2) -> HandlerResult<R> {
    debug!(logger, "Authenticating request");

    with_metrics(statsd_client, name, || {
        match rbackup::authenticate(dao, enc, session_id) {
            Ok(Some(identity)) => {
                #[allow(unused_must_use)] { statsd_client.count("authentication.ok", 1); }
                f2(identity.clone()).map_err(status_internal_server_error)
            },
            Ok(None) => {
                #[allow(unused_must_use)] { statsd_client.count("authentication.not_found", 1); }
                debug!(logger, "Unauthenticated request! SessionId: {}", session_id);
                Err(status::Custom(Status::Unauthorized, "Cannot find session".to_string()))
            },
            Err(e) => {
                #[allow(unused_must_use)] { statsd_client.count("authentication.failure", 1); }
                warn!(logger, "Error while authenticating: {}", e);
                Err(status::Custom(Status::InternalServerError, format!("{}", e)))
            }
        }
    })
}

fn with_metrics<O, E, F: FnOnce() -> Result<O, E>>(statsd_client: &StatsdClient, name: &str, r: F) -> Result<O, E> {
    #[allow(unused_must_use)] {
        statsd_client.count("requests.total", 1);
        statsd_client.count(format!("requests.{}.total", name).as_ref(), 1);
    }

    let stopwatch = stopwatch::Stopwatch::start_new();

    r().map(|res| {
        #[allow(unused_must_use)] { statsd_client.time(format!("requests.{}.successes", name).as_ref(), stopwatch.elapsed_ms() as u64); }
        res
    }).map_err(|err| {
        #[allow(unused_must_use)] { statsd_client.time(format!("requests.{}.failures", name).as_ref(), stopwatch.elapsed_ms() as u64); }
        err
    })
}

fn status_internal_server_error(e: Error) -> status::Custom<String> {
    status::Custom(Status::InternalServerError, format!("{}", e))
}

struct HandlerConfig {
    repo_root: String,
    dao: Dao,
    encryptor: Encryptor,
    logger: slog::Logger,
    statsd_client: StatsdClient
}

struct AppConfig {
    settings_file: String,
}

fn get_app_config() -> Result<Option<AppConfig>, ArgsError> {
    let mut args = Args::new("RBackup", "Deduplicating secure backup server");
    args.flag("h", "help", "Print the usage menu");
    args.option("c",
                "config",
                "Path to file (TOML format) with settings",
                "PATH",
                Occur::Optional,
                None);

    args.parse_from_cli()?;

    let help = args.value_of("help")?;
    if help {
        println!("{}", args.full_usage());
        return Ok(None);
    }

    Ok(
        Some(AppConfig {
            settings_file: args.value_of("config")?
        })
    )
}

fn load_config(path: &str) -> Result<config::Config, Error> {
    let mut config = config::Config::default();
    let content: String = {
        use std::fs::File;
        use std::io::prelude::*;
        let mut file = File::open(path)
            .map_err(|e| Error::from(rbackup::failures::CustomError::new(&format!("Could not open file {}: {}", path, e))))?;
        let mut content = String::new();
        file.read_to_string(&mut content)
            .map_err(|e| Error::from(rbackup::failures::CustomError::new(&format!("Could not read from file {}: {}", path, e))))?;
        content
    };
    config.merge(config::File::from_str(content.as_ref(), config::FileFormat::Toml))?;

    Ok(config)
}

fn init_logger() -> Logger {
    let decorator = TermDecorator::new().stderr().build();
    let term = CompactFormat::new(decorator)
        .use_local_timestamp()
        .build()
        .filter_level(Level::Info);
    let async = Async::new(term.ignore_res())
        .chan_size(2048)
        .build();

    Logger::root(async.ignore_res(), o!())
}

fn start_server(logger: Logger, config: config::Config, statsd_client: StatsdClient) -> () {
    let repo_root = config.get_str("general.data_dir").expect("Could not access data dir");

    // create DAO

    let dao = Dao::new(&format!("mysql://{}:{}@{}:{}",
                                config.get_str("database.user").unwrap(),
                                config.get_str("database.pass").unwrap(),
                                config.get_str("database.host").unwrap(),
                                config.get_str("database.port").unwrap()),
                       &config.get_str("database.name").unwrap(),
                       statsd_client.clone()
    );

    let secret = config.clone().get_str("general.secret").expect("There is no secret provided");

    // configure server:

    info!(logger, "Configuring server");

    let config_builder = rocket::Config::build(rocket::config::Environment::Development)
        .address(config.get_str("server.address").expect("There is no bind address provided"))
        .port(config.get_int("server.port").expect("There is no bind port provided") as u16)
        .workers(config.get_int("server.workers").expect("There is no workers count provided") as u16);

    let config_builder = if config.get_bool("server.tls.enabled").unwrap_or(true) {
        let tls_config = config.get_table("server.tls").unwrap();

        config_builder
            .tls(tls_config.get("certs").expect("There is no TLS cert path provided").to_string(),
                 tls_config.get("key").expect("There is no TLS key path provided").to_string())
    } else {
        config_builder
    };

    let rocket_config = config_builder
        .log_level(rocket::logger::LoggingLevel::Critical)
        .unwrap();

    rocket::custom(rocket_config, true)
        .mount("/", routes![upload])
        .mount("/", routes![download])
        .mount("/", routes![list_files])
        .mount("/", routes![list_devices])
        .mount("/", routes![remove_file])
        .mount("/", routes![remove_file_version])
        .mount("/", routes![login])
        .mount("/", routes![register])
        .manage(HandlerConfig {
            repo_root,
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
    let args = match get_app_config() {
        Ok(Some(args)) => args,
        Ok(None) => exit(0),
        Err(e) => {
            println!("{:?}", e);
            exit(1);
        }
    };

    let logger = init_logger();

    let config = load_config(&args.settings_file).unwrap_or_else(|e| {
        println!("{}", e);
        exit(1);
    });

    let statsd_client = create_statsd_client(
        logger.clone(),
        config.get_str("statsd.host").expect("").as_ref(),
        config.get_int("statsd.port").expect("") as u16,
        config.get_str("statsd.prefix").expect("").as_ref(),
    ).unwrap_or_else(|e| {
        println!("{}", e);
        exit(1);
    });

    start_server(logger, config, statsd_client)
}
