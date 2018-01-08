extern crate failure;

use std::fmt;

#[derive(Debug)]
pub struct CustomError {
    desc: String
}

impl CustomError {
    pub fn new(desc: &str) -> CustomError {
        CustomError {
            desc: String::from(desc)
        }
    }
}

impl failure::Fail for CustomError{}

impl fmt::Display for CustomError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Failure: {}", self.desc)
    }
}