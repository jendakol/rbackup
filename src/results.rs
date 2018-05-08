extern crate failure;
extern crate rocket;
extern crate serde_json;

use rocket::http::Status;
use rocket::request::Request;
use rocket::response::{Responder, Response};
use rocket::response::status::Custom as CustomStatus;
use std::io::{Cursor, Error as IoError};
use structs::*;

pub enum RegisterResult {
    Created(String),
    Exists
}

pub enum LoginResult {
    NewSession(String),
    ExistingSession(String),
    AccountNotFound
}

pub enum UploadedData {
    Success(u64, String),
    MismatchSha256
}

pub enum UploadResult {
    Success(File),
    InvalidRequest(String),
    MismatchSha256
}

pub enum ListFileResult {
    Success(Vec<File>),
    DeviceNotFound
}

pub enum RemoveFileResult {
    Success,
    PartialFailure(Vec<IoError>),
    FileNotFound
}

pub enum RemoveFileVersionResult {
    Success,
    FileNotFound
}

impl<'r> Responder<'r> for RegisterResult {
    fn respond_to(self, _: &Request) -> Result<Response<'r>, Status> {
        match self {
            RegisterResult::Created(account_id) =>
                Response::build()
                    .status(Status::Created)
                    .sized_body(Cursor::new(account_id))
                    .ok(),
            RegisterResult::Exists =>
                Response::build()
                    .status(Status::Conflict)
                    .ok(),
        }
    }
}

impl<'r> Responder<'r> for LoginResult {
    fn respond_to(self, _: &Request) -> Result<Response<'r>, Status> {
        match self {
            LoginResult::NewSession(session_id) =>
                Response::build()
                    .status(Status::Created)
                    .sized_body(Cursor::new(session_id))
                    .ok(),
            LoginResult::ExistingSession(session_id) =>
                Response::build()
                    .status(Status::Ok)
                    .sized_body(Cursor::new(session_id))
                    .ok(),
            LoginResult::AccountNotFound =>
                Response::build()
                    .status(Status::Unauthorized)
                    .ok()
        }
    }
}

impl<'r> Responder<'r> for UploadResult {
    fn respond_to(self, req: &Request) -> Result<Response<'r>, Status> {
        match self {
            UploadResult::Success(file) =>
                serde_json::to_string(&file)
                    .map_err(failure::Error::from)
                    .map_err(status_internal_server_error)
                    .respond_to(req),
            UploadResult::MismatchSha256 =>
                Response::build()
                    .status(Status::BadRequest)
                    .sized_body(Cursor::new("Mismatch SHA 256"))
                    .ok(),
            UploadResult::InvalidRequest(desc) =>
                Response::build()
                    .status(Status::BadRequest)
                    .sized_body(Cursor::new(desc))
                    .ok()
        }
    }
}

impl<'r> Responder<'r> for RemoveFileResult {
    fn respond_to(self, _: &Request) -> Result<Response<'r>, Status> {
        match self {
            RemoveFileResult::Success =>
                Response::build()
                    .status(Status::Ok)
                    .ok(),
            RemoveFileResult::PartialFailure(failures) =>
                Response::build()
                    .status(Status::InternalServerError)
                    .sized_body(Cursor::new(format!("{:?}", failures)))
                    .ok(),
            RemoveFileResult::FileNotFound =>
                Response::build()
                    .status(Status::NotFound)
                    .ok()
        }
    }
}

impl<'r> Responder<'r> for RemoveFileVersionResult {
    fn respond_to(self, _: &Request) -> Result<Response<'r>, Status> {
        match self {
            RemoveFileVersionResult::Success =>
                Response::build()
                    .status(Status::Ok)
                    .ok(),
            RemoveFileVersionResult::FileNotFound =>
                Response::build()
                    .status(Status::NotFound)
                    .ok()
        }
    }
}

impl<'r> Responder<'r> for ListFileResult {
    fn respond_to(self, req: &Request) -> Result<Response<'r>, Status> {
        match self {
            ListFileResult::Success(files) =>
                serde_json::to_string(&files)
                    .map_err(failure::Error::from)
                    .map_err(status_internal_server_error)
                    .respond_to(req),
            ListFileResult::DeviceNotFound =>
                Response::build()
                    .status(Status::NotFound)
                    .sized_body(Cursor::new("Device not found"))
                    .ok()
        }
    }
}

fn status_internal_server_error(e: failure::Error) -> CustomStatus<String> {
    CustomStatus(Status::InternalServerError, format!("{}", e))
}