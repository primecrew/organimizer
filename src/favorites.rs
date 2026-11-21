//! Favorite folders (3.0): the ☆ button next to the folder path, the
//! Favorites menu and Tools > Manage Favorites. Stored as `[Favorites]` in
//! the settings file (`favN = Name | Path`).

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Favorite {
    pub name: String,
    pub path: String,
}

/// Comparison key: case-insensitive, `/` = `\`, no trailing separator.
pub fn path_key(p: &str) -> String {
    let s = p.trim().replace('/', "\\");
    let t = s.trim_end_matches('\\');
    // keep "C:\" distinguishable from "C:" relative paths
    let t = if t.len() == 2 && t.ends_with(':') { s.as_str() } else { t };
    t.to_lowercase()
}

/// Default display name: the last folder name, or the path for a root.
pub fn default_name(path: &str) -> String {
    let s = path.trim().replace('/', "\\");
    let last = s.trim_end_matches('\\').rsplit('\\').next().unwrap_or_default();
    let name = if last.is_empty() || last.ends_with(':') { s.trim().to_string() } else { last.to_string() };
    clean_name(&name)
}

/// Names are single-line and may not contain `|` (the INI separator).
pub fn clean_name(name: &str) -> String {
    name.replace(['\r', '\n', '|'], " ").trim().chars().take(80).collect()
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Favorites {
    pub items: Vec<Favorite>,
}

impl Favorites {
    pub fn index_of(&self, path: &str) -> Option<usize> {
        let k = path_key(path);
        if k.is_empty() {
            return None;
        }
        self.items.iter().position(|f| path_key(&f.path) == k)
    }

    pub fn contains(&self, path: &str) -> bool {
        self.index_of(path).is_some()
    }

    /// Add (if new). Returns false for an empty path or a duplicate.
    pub fn add(&mut self, name: &str, path: &str) -> bool {
        let path = path.trim();
        if path.is_empty() || self.contains(path) {
            return false;
        }
        let name = clean_name(name);
        let name = if name.is_empty() { default_name(path) } else { name };
        self.items.push(Favorite { name, path: path.to_string() });
        true
    }

    pub fn remove_path(&mut self, path: &str) -> bool {
        match self.index_of(path) {
            Some(i) => {
                self.items.remove(i);
                true
            }
            None => false,
        }
    }

    /// The star button: add when missing, remove when present. Returns
    /// true if the folder is a favorite afterwards.
    pub fn toggle(&mut self, path: &str) -> bool {
        if self.remove_path(path) { false } else { self.add("", path) }
    }

    /// Edit entry `i`. Fails on an empty path or one that another entry has.
    pub fn edit(&mut self, i: usize, name: &str, path: &str) -> Result<(), String> {
        let path = path.trim();
        if path.is_empty() {
            return Err("Enter a folder path".into());
        }
        if let Some(j) = self.index_of(path)
            && j != i
        {
            return Err(format!("“{}” is already a favorite", self.items[j].name));
        }
        let Some(f) = self.items.get_mut(i) else { return Err("No such favorite".into()) };
        let name = clean_name(name);
        f.name = if name.is_empty() { default_name(path) } else { name };
        f.path = path.to_string();
        Ok(())
    }

    /// Move entry `i` by `delta` (-1 up, +1 down). Returns the new index.
    pub fn move_by(&mut self, i: usize, delta: isize) -> Option<usize> {
        let j = i.checked_add_signed(delta)?;
        if i >= self.items.len() || j >= self.items.len() {
            return None;
        }
        self.items.swap(i, j);
        Some(j)
    }

    pub fn to_ini_lines(&self) -> String {
        self.items
            .iter()
            .enumerate()
            .map(|(i, f)| format!("fav{} = {} | {}\n", i + 1, clean_name(&f.name), f.path.replace(['\r', '\n'], " ")))
            .collect()
    }

    /// Parse `favN = Name | Path` values (in N order; bad lines skipped).
    pub fn from_ini_map(map: Option<&std::collections::BTreeMap<String, String>>) -> Favorites {
        let mut list: Vec<(u32, &String)> = map
            .into_iter()
            .flatten()
            .filter_map(|(k, v)| k.strip_prefix("fav").and_then(|n| n.parse::<u32>().ok()).map(|n| (n, v)))
            .collect();
        list.sort_by_key(|(n, _)| *n);
        let mut f = Favorites::default();
        for (_, v) in list {
            let (name, path) = match v.split_once('|') {
                Some((n, p)) => (n.trim(), p.trim()),
                None => ("", v.trim()),
            };
            f.add(name, path);
        }
        f
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn star_toggle_add_edit_remove_reorder() {
        let mut f = Favorites::default();
        assert!(f.toggle("D:\\Media\\Movies"));
        assert_eq!(f.items[0].name, "Movies");
        assert!(f.contains("d:/media/movies/"), "case / slash / trailing separator insensitive");
        assert!(!f.add("again", "D:\\MEDIA\\Movies\\"));
        assert!(!f.toggle("d:\\media\\movies"), "second click removes");
        assert!(f.items.is_empty());
        assert!(f.add("Photos", "C:\\Users\\x\\Pictures"));
        assert!(f.add("", "E:\\"));
        assert_eq!(f.items[1].name, "E:\\");
        assert!(f.add("TV | Shows", "D:\\TV"));
        assert_eq!(f.items[2].name, "TV   Shows");
        assert!(!f.add("x", "  "));
        assert!(f.edit(0, "Pics", "C:\\Pics").is_ok());
        assert_eq!(f.items[0], Favorite { name: "Pics".into(), path: "C:\\Pics".into() });
        assert!(f.edit(0, "dup", "d:\\tv").is_err());
        assert!(f.edit(0, "", "").is_err());
        assert_eq!(f.move_by(2, -1), Some(1));
        assert_eq!(f.items[1].path, "D:\\TV");
        assert_eq!(f.move_by(0, -1), None);
        assert_eq!(f.move_by(2, 1), None);
        assert!(f.remove_path("C:\\PICS\\"));
        assert_eq!(f.items.len(), 2);
    }

    #[test]
    fn ini_roundtrip() {
        let mut f = Favorites::default();
        f.add("Movies", "D:\\Media\\Movies");
        f.add("Root", "E:\\");
        let text = f.to_ini_lines();
        let mut map = std::collections::BTreeMap::new();
        for line in text.lines() {
            let (k, v) = line.split_once('=').unwrap();
            map.insert(k.trim().to_string(), v.trim().to_string());
        }
        map.insert("fav10".into(), "D:\\NoName".into());
        map.insert("favx".into(), "ignored".into());
        let back = Favorites::from_ini_map(Some(&map));
        assert_eq!(back.items.len(), 3);
        assert_eq!(back.items[..2], f.items[..]);
        assert_eq!(back.items[2].name, "NoName");
    }
}
