pub mod fml;
pub mod pdr;
pub mod rtype;

use std::fmt;

#[derive(Debug)]
pub enum VerificationResult {
    Valid,
    Invalid,
    Unknown,
}

impl fmt::Display for VerificationResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use VerificationResult::*;
        write!(
            f,
            "{}",
            match self {
                Valid => "valid",
                Invalid => "invalid",
                Unknown => "unknown",
            }
        )
    }
}

//fn infer_nu_validity(vc: )