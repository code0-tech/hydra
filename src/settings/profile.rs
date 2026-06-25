use core::fmt;
use std::str::FromStr;

#[derive(Debug)]
pub enum Profile {
    Ide,
    Runtime,
    Ai,
}

impl fmt::Display for Profile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Profile::Ide => "ide",
            Profile::Runtime => "runtime",
            Profile::Ai => "ide_velorum",
        };

        write!(f, "{s}")
    }
}
impl FromStr for Profile {
    type Err = ();

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input.to_lowercase().as_str() {
            "ide" => Ok(Profile::Ide),
            "runtime" => Ok(Profile::Runtime),
            "ide_velorum" | "ai" => Ok(Profile::Ai),
            _ => Err(()),
        }
    }
}
