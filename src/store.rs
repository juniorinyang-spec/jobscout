//! Persistent record of postings already reported, so each run only bundles
//! what is genuinely new.

use crate::json::Json;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

pub struct Seen {
    path: PathBuf,
    /// job key -> ISO timestamp of first sighting
    map: HashMap<String, String>,
}

impl Seen {
    pub fn load(path: &Path) -> Result<Seen, String> {
        let mut map = HashMap::new();
        if path.exists() {
            let text = fs::read_to_string(path).map_err(|e| format!("read {}: {}", path.display(), e))?;
            let v = Json::parse(&text).map_err(|e| format!("{}: {}", path.display(), e))?;
            if let Some(obj) = v.get("seen").and_then(|s| s.as_obj()) {
                for (k, when) in obj {
                    map.insert(k.clone(), when.to_text());
                }
            }
        }
        Ok(Seen { path: path.to_path_buf(), map })
    }

    pub fn contains(&self, key: &str) -> bool {
        self.map.contains_key(key)
    }

    pub fn mark(&mut self, key: &str, when: &str) {
        self.map.entry(key.to_string()).or_insert_with(|| when.to_string());
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn clear(&mut self) {
        self.map.clear();
    }

    pub fn save(&self) -> Result<(), String> {
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir).map_err(|e| format!("create {}: {}", dir.display(), e))?;
        }
        let mut entries: Vec<(&String, &String)> = self.map.iter().collect();
        entries.sort();
        let obj = Json::Obj(entries.into_iter().map(|(k, v)| (k.clone(), Json::Str(v.clone()))).collect());
        let mut root = Json::obj();
        root.set("seen", obj);
        let tmp = self.path.with_extension("json.tmp");
        fs::write(&tmp, root.to_pretty()).map_err(|e| format!("write {}: {}", tmp.display(), e))?;
        fs::rename(&tmp, &self.path).map_err(|e| format!("rename {}: {}", self.path.display(), e))
    }
}
