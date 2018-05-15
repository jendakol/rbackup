#![feature(plugin, custom_derive)]
#![plugin(rocket_codegen)]

extern crate cadence;
extern crate clap;
extern crate config;
extern crate either;
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
extern crate slog_stream;
extern crate slog_term;
extern crate stopwatch;

use cadence::StatsdClient;
use clap::{App, Arg, SubCommand};
use either::{Either, Left, Right};
use failure::Error;
use rbackup::dao::Dao;
use rbackup::encryptor::Encryptor;
use server::*;
use slog::{Drain, Level, Logger};
use slog_async::Async;
use slog_term::{CompactFormat, TermDecorator};
use std::process::exit;

mod server;

#[derive(Debug)]
struct StatsdConfig {
    host: String,
    port: u16,
    prefix: String
}

#[derive(Debug)]
struct GeneralConfig {
    data_dir: String,
    secret: String,
}

#[derive(Debug)]
struct DatabaseConfig {
    user: String,
    pass: String,
    host: String,
    port: u16,
    name: String,
}

#[derive(Debug)]
struct TlsConfig {
    key: String,
    certs: String,
}

#[derive(Debug)]
struct ServerConfig {
    address: String,
    port: u16,
    workers: u16,
    tls_config: Option<TlsConfig>
}

#[derive(Debug)]
struct AppConfig {
    general: GeneralConfig,
    statsd: StatsdConfig,
    server: ServerConfig,
    database: DatabaseConfig
}

#[derive(Debug)]
enum AppCommand {
    DbInit(String)
}

fn exec_command(logger: &Logger, app_command: AppCommand) -> i32 {
    use AppCommand::*;

    info!(logger, "Executing command: {:?}", app_command);

    match app_command {
        DbInit(config_file) => {
            unimplemented!() // TODO
        }
    }
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

fn get_app_config(logger: &Logger) -> Result<Either<AppConfig, AppCommand>, Error> {
    let matches = App::new("RBackup")
        .version("0.0.5") // TODO parametrize version
        .about("Deduplicating secure backup server")
        .arg(Arg::with_name("config")
            .short("c")
            .long("config")
            .value_name("FILE")
            .help("Sets a custom config file (TOML format)")
            .takes_value(true))
        .subcommand(SubCommand::with_name("dbinit")
            .about("Initializes DB using provided (or default) config file"))
        .get_matches();

    let config_file = matches.value_of("config").unwrap_or("config.toml").to_string();

    info!(logger, "Using config file: {}", config_file);

    if let Some(_) = matches.subcommand_matches("dbinit") {
        return Ok(Right(AppCommand::DbInit(config_file)))
    };

    // load config file:

    let config = load_config(&config_file)?;

    // TODO check permissions to data_dir

    Ok(Left(
        AppConfig {
            general: GeneralConfig {
                data_dir: config.get_str("general.data_dir")?,
                secret: config.get_str("general.secret")?
            },
            statsd: StatsdConfig {
                host: config.get_str("statsd.host")?,
                port: config.get_int("statsd.port")? as u16,
                prefix: config.get_str("statsd.prefix")?
            },
            server: ServerConfig {
                address: config.get_str("server.address")?,
                port: config.get_int("server.port")? as u16,
                workers: config.get_int("server.workers")? as u16,
                tls_config: if config.get_bool("server.tls.enabled").unwrap_or(true) {
                    let tls_config = config.get_table("server.tls").unwrap();

                    Some(
                        TlsConfig {
                            certs: tls_config.get("certs").expect("There is no TLS cert path provided").to_string(),
                            key: tls_config.get("key").expect("There is no TLS cert path provided").to_string(),
                        }
                    )
                } else {
                    None
                }
            },
            database: DatabaseConfig {
                user: config.get_str("database.user")?,
                pass: config.get_str("database.pass")?,
                host: config.get_str("database.host")?,
                port: config.get_int("database.port")? as u16,
                name: config.get_str("database.name")?,

            }
        }
    ))
}

fn start_server(logger: Logger, config: AppConfig, dao: Dao, statsd_client: StatsdClient) -> () {
    info!(logger, "Configuring server");

    let config_builder = rocket::Config::build(rocket::config::Environment::Development)
        .address(config.server.address)
        .port(config.server.port)
        .workers(config.server.workers);

    let config_builder = match config.server.tls_config {
        Some(tc) => config_builder.tls(tc.certs, tc.key),
        None => config_builder
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
            repo_root: config.general.data_dir,
            dao,
            encryptor: Encryptor::new(config.general.secret),
            logger,
            statsd_client
        })
        .launch();
}

fn init_dao(statsd_client: StatsdClient, config: &DatabaseConfig) -> Result<Dao, Error> {
    Dao::new(&format!("mysql://{}:{}@{}:{}",
                      config.user,
                      config.pass,
                      config.host,
                      config.port),
             &config.name,
             statsd_client
    )
}

fn create_statsd_client(logger: Logger, config: &StatsdConfig) -> Result<StatsdClient, Error> {
    use std::net::{UdpSocket, ToSocketAddrs};
    use cadence::{QueuingMetricSink};

    let socket = UdpSocket::bind("0.0.0.0:0")?;
    socket.set_nonblocking(true)?;

    let host_and_port = format!("{}:{}", config.host, config.port).to_socket_addrs()?.next().unwrap();

    info!(logger, "Creating StatsD client reporting to {} with prefix '{}'", host_and_port, config.prefix);

    let udp_sink = cadence::UdpMetricSink::from(host_and_port, socket)?;
    let queuing_sink = QueuingMetricSink::from(udp_sink);

    Ok(
        StatsdClient::builder(&config.prefix, queuing_sink)
            .with_error_handler(move |err| {
                error!(logger.clone(), "Error while sending stats: {}", err);
            })
            .build()
    )
}

fn main() {
    let logger = init_logger();

    let app_config = match get_app_config(&logger) {
        Ok(Left(app_config)) => app_config,
        Ok(Right(command)) => exit(exec_command(&logger, command)),
        Err(e) => {
            println!("Could not load app configuration: {:?}", e);
            exit(1);
        }
    };

    debug!(logger, "Running app with config: {:?}", app_config);

    let statsd_client = create_statsd_client(logger.clone(), &app_config.statsd)
        .unwrap_or_else(|e| {
            println!("Could not initialize connection to StatsD: {}", e);
            exit(1);
        });

    let dao = init_dao(statsd_client.clone(), &app_config.database)
        .unwrap_or_else(|e| {
            println!("Could not initialize connection to DB: {}", e);
            exit(1);
        });

    start_server(logger, app_config, dao, statsd_client)
}
