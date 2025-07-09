use std::{collections::BTreeMap, path::PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct TypstFauxPackage {}

#[derive(Debug, Serialize, Deserialize)]
pub struct BevyTypstDotToml {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub authors: Vec<String>,
    #[serde(default)]
    pub asset_requests: BTreeMap<PathBuf, Option<FileTypeHint>>,
    #[serde(default)]
    pub package_requests: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum FileTypeHint {
    Image,
    Font,
    Typst,
}
