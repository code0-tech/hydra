use std::str::FromStr;

pub enum Registry {
    GitHub,
    GitLab,
    Custom { url: String },
}

impl Registry {
    pub fn url(&self) -> &str {
        match self {
            Registry::GitHub => "ghcr.io/code0-tech/reticulum/ci-builds",
            Registry::GitLab => "registry.gitlab.com/code0-tech/packages",
            Registry::Custom { url } => url,
        }
    }
}
impl FromStr for Registry {
    type Err = ();

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input.to_lowercase().as_str() {
            "github" => Ok(Registry::GitHub),
            "gitlab" => Ok(Registry::GitLab),
            _ => Err(()),
        }
    }
}
