#![feature(plugin, custom_derive)]
#![plugin(rocket_codegen)]

extern crate rbackup;
extern crate config;
extern crate rocket;
extern crate tempfile;
#[macro_use]
extern crate log;

//use std::collections::HashMap;
//use std::process;
//use std::env;
use rocket::Data;
use rocket::response::Stream;
use std::io;
use std::fs::File;
use tempfile::NamedTempFile;

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
fn list(metadata: ListMetadata) -> io::Result<String> {
    rbackup::list(String::from("/data/deduprepo/"), metadata.pc_id)
}

#[get("/download?<metadata>")]
fn download(metadata: DownloadMetadata) -> io::Result<Stream<File>> {
    rbackup::load(String::from("/data/deduprepo/"), metadata.pc_id, metadata.orig_file_name, metadata.time_stamp)
        .and_then(|path| {
            println!("Temp file: {:?}", path);

            Result::Ok(
                Stream::from(
                    File::from(path)
                )
            )
        })
}

#[post("/upload?<metadata>", format = "application/octet-stream", data = "<data>")]
fn upload(data: Data, metadata: UploadMetadata) -> &'static str {
    // TODO stream the data!

    let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");

    io::copy(&mut data.open(), &mut temp_file).expect("Could not copy received data to temp file");

    let temp_file_name = temp_file.path().to_str().expect("Could not extract filename from temp file");

    match rbackup::save(String::from("/data/deduprepo/"), metadata.pc_id, metadata.orig_file_name, temp_file_name) {
        Ok(()) => {
            "ok"
        }
        Err(e) => {
            warn!("{}", e);
            "FAIL"
        }
    }
}

fn main() {
    //    let args: Vec<String> = env::args().collect();
    //
    //    if args.len() != 2 {
    //        eprintln!("No filename was provided!");
    //        process::exit(1);
    //    }
    //
    //    let file_name = &args[1];
    //
    //    let mut settings = config::Config::default();
    //    settings
    //        // Add in `./Settings.toml`
    //        //        .merge(config::File::with_name("Settings")).unwrap()
    //        // Add in settings from the environment (with a prefix of APP)
    //        // Eg.. `APP_DEBUG=1 ./target/app` would set the `debug` key
    //        .merge(config::Environment::with_prefix("APP")).unwrap();
    //
    //    //    println!("{:?}",
    //    //             settings.deserialize::<HashMap<String, String>>().unwrap());
    //    //
    //    //    process::exit(0);
    //
    //    let repo_dir = String::from("/data/deduprepo/");

    rocket::ignite()
        .mount("/", routes![upload])
        .mount("/", routes![download])
        .mount("/", routes![list])
        .launch();
}
