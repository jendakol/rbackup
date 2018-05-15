use failure::*;
use rbackup::dao::Dao;
use slog::Logger;
use std::fs::File;
use std::io::prelude::*;

pub fn db_init(logger: &Logger, dao: Dao) -> Result<i32, Error> {
    debug!(logger, "Executing DB init");

    let mut file = File::open("resources/db-scheme.sql")?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;

    info!(logger, "Creating the database scheme");

    dao.exec(&contents)
        .map(|_| {
            info!(logger, "Database scheme created");
            0
        })
        .map_err(Error::from)
}
