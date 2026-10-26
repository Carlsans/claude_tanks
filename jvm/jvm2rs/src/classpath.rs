//! Finds class files by internal name in directories, jars and JDK `.jmod`
//! files, in class-path order (the first definition of a name wins).

use crate::classfile::{ClassFile, Error, Result};
use std::collections::HashMap;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

enum Source {
    Dir(PathBuf),
    /// A zip archive in memory (a jar, or a jmod without its header), and
    /// the prefix of class entries inside it (`classes/` in jmods).
    Zip(zip::ZipArchive<Cursor<Vec<u8>>>, &'static str),
}

pub struct ClassPath {
    sources: Vec<Source>,
    /// Which sources are the Java library (JDK jmods), by position.
    library: Vec<bool>,
    /// Name to the source that defines it, for archives.
    index: HashMap<String, usize>,
}

impl ClassPath {
    pub fn new() -> ClassPath {
        ClassPath {
            sources: Vec::new(),
            library: Vec::new(),
            index: HashMap::new(),
        }
    }

    /// Adds a directory, jar or jmod.
    pub fn add(&mut self, path: &Path) -> Result<()> {
        let e = |m: String| Error(format!("{}: {m}", path.display()));
        if path.is_dir() {
            self.sources.push(Source::Dir(path.to_path_buf()));
            self.library.push(false);
            return Ok(());
        }
        let mut bytes = std::fs::read(path).map_err(|x| e(x.to_string()))?;
        let prefix = if bytes.starts_with(b"JM\x01\x00") {
            bytes.drain(..4);
            "classes/"
        } else {
            ""
        };
        let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|x| e(x.to_string()))?;
        let k = self.sources.len();
        for i in 0..zip.len() {
            let name = zip
                .by_index(i)
                .map_err(|x| e(x.to_string()))?
                .name()
                .to_string();
            if let Some(n) = name
                .strip_prefix(prefix)
                .and_then(|n| n.strip_suffix(".class"))
            {
                self.index.entry(n.to_string()).or_insert(k);
            }
        }
        self.sources.push(Source::Zip(zip, prefix));
        self.library.push(!prefix.is_empty());
        Ok(())
    }

    /// Every class name in the archives (not directories).
    pub fn archive_classes(&self) -> impl Iterator<Item = &String> {
        self.index.keys()
    }

    /// The classes an archive on the path defines (by its position).
    pub fn classes_of(&self, source: usize) -> Vec<String> {
        let mut v: Vec<String> = self
            .index
            .iter()
            .filter(|(_, s)| **s == source)
            .map(|(n, _)| n.clone())
            .collect();
        v.sort();
        v
    }

    pub fn len(&self) -> usize {
        self.sources.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sources.is_empty()
    }

    /// Whether a class comes from the Java library (a jmod).
    pub fn is_library(&self, name: &str) -> bool {
        let file = format!("{name}.class");
        self.sources
            .iter()
            .enumerate()
            .position(|(k, s)| match s {
                Source::Dir(d) => d.join(&file).is_file(),
                Source::Zip(..) => self.index.get(name) == Some(&k),
            })
            .is_some_and(|k| self.library[k])
    }

    pub fn load(&mut self, name: &str) -> Result<Option<ClassFile>> {
        let file = format!("{name}.class");
        let hit = self.sources.iter().enumerate().position(|(k, s)| match s {
            Source::Dir(d) => d.join(&file).is_file(),
            Source::Zip(..) => self.index.get(name) == Some(&k),
        });
        let Some(k) = hit else {
            return Ok(None);
        };
        let bytes = match &mut self.sources[k] {
            Source::Dir(d) => {
                std::fs::read(d.join(&file)).map_err(|x| Error(format!("{name}: {x}")))?
            }
            Source::Zip(z, prefix) => {
                let mut f = z
                    .by_name(&format!("{prefix}{file}"))
                    .map_err(|x| Error(format!("{name}: {x}")))?;
                let mut b = Vec::new();
                f.read_to_end(&mut b)
                    .map_err(|x| Error(format!("{name}: {x}")))?;
                b
            }
        };
        ClassFile::parse(&bytes)
            .map(Some)
            .map_err(|x| Error(format!("{name}: {x}")))
    }
}

impl Default for ClassPath {
    fn default() -> Self {
        ClassPath::new()
    }
}
