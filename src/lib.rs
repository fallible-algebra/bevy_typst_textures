use std::{collections::HashMap, path::PathBuf};

pub mod asset_loading;
pub mod file_resolver;
pub mod template;

pub struct TemplateServer {
    templates: HashMap<PathBuf, ()>,
}
