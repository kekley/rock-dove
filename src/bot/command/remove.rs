use std::num::ParseIntError;

use compact_str::CompactString;

pub enum RemoveMode {
    FromUser,
    At,
    Until,
    Past,
}

#[derive(Debug, Clone)]
pub enum RemoveArgument {
    From(CompactString),
    At(u32),
    Until(u32),
    Past(u32),
}

#[derive(Debug, thiserror::Error)]
pub enum RemoveArgParseError {
    #[error(
        "You need to specify a remove mode: from (user), at (position), until (position), or past (position)"
    )]
    NoModeSpecified,
    #[error(
        "Valid remove arguments: from (user), at (position), until (position), or past (position)"
    )]
    InvalidModeSpecified,

    #[error("The positional argument should be a number")]
    InvalidArg(#[from] ParseIntError),
}

impl RemoveArgument {
    pub(crate) fn parse(str: &str) -> Result<Self, RemoveArgParseError> {
        let Some((kind, arg)) = str.split_once(" ") else {
            return Err(RemoveArgParseError::NoModeSpecified);
        };
        let mut copied = CompactString::from(kind);
        copied.make_ascii_lowercase();
        match copied.as_str() {
            "from" => Ok(RemoveArgument::From(CompactString::from(arg))),
            "at" => {
                let arg = arg.trim().parse::<u32>()?;
                Ok(RemoveArgument::At(arg))
            }
            "until" => {
                let arg = arg.trim().parse::<u32>()?;
                Ok(RemoveArgument::Until(arg))
            }
            "past" => {
                let arg = arg.trim().parse::<u32>()?;
                Ok(RemoveArgument::Past(arg))
            }
            _ => Err(RemoveArgParseError::InvalidModeSpecified),
        }
    }
}
