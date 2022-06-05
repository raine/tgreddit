use std::path::PathBuf;

#[derive(Debug)]
pub struct Video {
    pub path: PathBuf,
    pub width: u16,
    pub height: u16,
}
