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
extern crate url;
extern crate rdedup_lib as rdedup;

use failure::Error;
use rocket::Data;
use rocket::response::Stream;
use rocket::State;
use std::process::ChildStdout;

use std::path::Path;
use rdedup::Repo;

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

//
//#[get("/download?<metadata>")]
//fn download(config: State<AppConfig>, metadata: DownloadMetadata) -> Result<Stream<ChildStdout>, Error> {
//    rbackup::load(&config.repo_dir, &metadata.pc_id, &metadata.orig_file_name, metadata.time_stamp)
//        .map(|stdout| {
//            Stream::from(
//                stdout
//            )
//        })
//}
//
//#[post("/upload?<metadata>", format = "application/octet-stream", data = "<data>")]
//fn upload(config: State<AppConfig>, data: Data, metadata: UploadMetadata) -> &'static str {
//    match rbackup::save(&config.repo_dir, &metadata.pc_id, &metadata.orig_file_name, data) {
//        Ok(()) => {
//            "ok"
//        }
//        Err(e) => {
//            warn!(config.logger, "{}", e);
//            "FAIL"
//        }
//    }
//}


struct AppConfig {
    repo: Repo,
    logger: slog::Logger
}


fn main() {
//    let decorator = slog_term::PlainDecorator::new(std::io::stdout());
//    let drain = slog_term::CompactFormat::new(decorator).build().fuse();
//    let drain = slog_async::Async::new(drain).build().fuse();

    let logger = slog::Logger::root(slog::Discard, o!());

    let mut config = config::Config::default();
    let config = config.merge(config::File::with_name("Settings")).unwrap();

    let repo_dir = &config.get_str("repo").expect("Could not extract repo path from config");

    let config = AppConfig {
        repo: Repo::open(&Path::new(repo_dir), logger.clone()).expect("Could not open repo"),
        logger
    };

    rocket::ignite()
//        .mount("/", routes![upload])
//        .mount("/", routes![download])
        .mount("/", routes![list])
        .manage(config)
        .launch();
}
