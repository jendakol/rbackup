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
use cadence::StatsdClient;
use failure::Error;
use getopts::Occur;
use rbackup::dao::Dao;
use rbackup::encryptor::Encryptor;
use slog::{Drain, Level, Logger};
use slog_async::Async;
use slog_term::{CompactFormat, TermDecorator};
use std::process::exit;

mod server;
use server::*;


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
