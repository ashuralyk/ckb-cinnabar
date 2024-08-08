use ckb_std::error::SysError;

#[macro_export]
macro_rules! custom_error {
    ($err:expr) => {
        Error::Custom($err as i8)
    };
}

pub const CUSTOM_ERROR_START: i8 = 20;

pub enum Error {
    // Errors under 10 are reserved for system errors
    IndexOutOfBound,
    ItemMissing,
    LengthNotEnough,
    Encoding,
    UnknownSystemError,

    // Errors under 20 are reserved for framework errors
    NotFoundRootVerifier,
    NotFoundBranchVerifier,

    // Custom errors are supposed to be greator than 20
    Custom(i8),
}

impl From<SysError> for Error {
    fn from(value: SysError) -> Self {
        match value {
            SysError::IndexOutOfBound => Self::IndexOutOfBound,
            SysError::ItemMissing => Self::ItemMissing,
            SysError::LengthNotEnough(_) => Self::LengthNotEnough,
            SysError::Encoding => Self::Encoding,
            _ => Self::UnknownSystemError,
        }
    }
}

impl From<Error> for i8 {
    fn from(value: Error) -> i8 {
        match value {
            Error::IndexOutOfBound => 1,
            Error::ItemMissing => 2,
            Error::LengthNotEnough => 3,
            Error::Encoding => 4,
            Error::UnknownSystemError => 5,
            Error::NotFoundRootVerifier => 10,
            Error::NotFoundBranchVerifier => 11,
            Error::Custom(value) => value,
        }
    }
}

pub type Result<T> = core::result::Result<T, Error>;
