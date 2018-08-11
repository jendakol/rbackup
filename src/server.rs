use cadence::prelude::*;
use cadence::StatsdClient;
use failure::Error;
use rbackup;
use rbackup::dao::Dao;
use rbackup::encryptor::Encryptor;
use rbackup::results::*;
use rbackup::structs::*;
use rocket;
use rocket::Data;
use rocket::http::{ContentType, Status};
use rocket::Outcome;
use rocket::request::{self, FromRequest, Request};
use rocket::response::{Response, status};
use rocket::State;
use slog;
use slog::Logger;
use stopwatch;

type HandlerResult<T> = Result<T, status::Custom<String>>;

#[derive(FromForm)]
struct UploadMetadata {
    file_path: String,
}

#[derive(FromForm)]
struct DownloadMetadata {
    file_version_id: u32,
}

#[derive(FromForm)]
struct RemoveFileMetadata {
    file_id: u32,
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
            return Outcome::Failure((Status::Unauthorized, ()));
        }

        let session_pass = String::from(values[0]);

        return Outcome::Success(Headers {
            session_pass
        });
    }
}

#[get("/status")]
fn status() -> status::Custom<String> {
    status::Custom(Status::Ok, String::from("{\"status\": \"RBackup running\"}"))
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
fn list_devices(config: State<HandlerConfig>, headers: Headers) -> HandlerResult<ListDevicesResult> {
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
                    Some((hash, size, read)) => {
                        rocket::response::Response::build()
                            .raw_header("RBackup-File-Hash", hash)
                            .raw_header("Content-Length", format!("{}", size))
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
            path: String::from(metadata.file_path.clone()),
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

#[delete("/remove/fileVersion?<metadata>")]
fn remove_file_version(config: State<HandlerConfig>, headers: Headers, metadata: RemoveFileVersionMetadata) -> HandlerResult<RemoveFileVersionResult> {
    with_authentication(&config.logger, "remove_file_version", &config.statsd_client, &config.dao, &config.encryptor, &headers.session_pass, |device| {
        Repo::new(&config.repo_root, &device.account_id, device.repo_pass, config.logger.clone())
            .and_then(|repo| {
                rbackup::remove_file_version(&config.logger, &repo, &config.dao, metadata.file_version_id)
            })
    })
}

#[delete("/remove/file?<metadata>")]
fn remove_file(config: State<HandlerConfig>, headers: Headers, metadata: RemoveFileMetadata) -> HandlerResult<RemoveFileResult> {
    with_authentication(&config.logger, "remove_file", &config.statsd_client, &config.dao, &config.encryptor, &headers.session_pass, |device| {
        Repo::new(&config.repo_root, &device.account_id, device.repo_pass.clone(), config.logger.clone())
            .and_then(|repo| {
                rbackup::remove_file(&config.logger, &repo, &config.dao, &device.id, metadata.file_id)
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

pub struct HandlerConfig {
    pub repo_root: String,
    pub dao: Dao,
    pub encryptor: Encryptor,
    pub logger: slog::Logger,
    pub statsd_client: StatsdClient
}