use jubako as jbk;

use super::create::{EntryStoreCreator, Void};
use jubako::creator::InputReader;
use mime_guess::mime;
use std::borrow::Cow;
use std::fs;
use std::io::Read;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

pub enum FsEntryKind {
    Dir,
    File(jbk::ContentAddress, mime::Mime, jbk::Size),
    Link,
    Other,
}

pub trait Adder {
    fn add<R: jbk::creator::InputReader>(&mut self, reader: R) -> jbk::Result<jbk::ContentAddress>;
}

pub struct FsEntry {
    pub kind: FsEntryKind,
    pub path: PathBuf,
    pub name: PathBuf,
    uid: u64,
    gid: u64,
    mode: u64,
    mtime: u64,
}

impl FsEntry {
    pub fn new_from_walk_entry<A: Adder>(
        dir_entry: walkdir::DirEntry,
        name: PathBuf,
        adder: &mut A,
    ) -> jbk::Result<Box<Self>> {
        let fs_path = dir_entry.path().to_path_buf();
        let attr = dir_entry.metadata().unwrap();
        let kind = if attr.is_dir() {
            FsEntryKind::Dir
        } else if attr.is_file() {
            let mut reader = jbk::creator::InputFile::open(&fs_path)?;
            let size = reader.size();
            let mime_type = match mime_guess::from_path(&fs_path).first() {
                Some(m) => m,
                None => {
                    let mut buf = [0u8; 100];
                    let size = std::cmp::min(100, size.into_usize());
                    reader.read_exact(&mut buf[..size])?;
                    (|| {
                        for window in buf[..size].windows(4) {
                            if window == b"html" {
                                return mime::TEXT_HTML;
                            }
                        }
                        mime::APPLICATION_OCTET_STREAM
                    })()
                }
            };
            let content_address = adder.add(reader)?;
            FsEntryKind::File(content_address, mime_type, size)
        } else if attr.is_symlink() {
            FsEntryKind::Link
        } else {
            FsEntryKind::Other
        };
        Ok(Box::new(Self {
            kind,
            path: fs_path,
            name,
            uid: attr.uid() as u64,
            gid: attr.gid() as u64,
            mode: attr.mode() as u64,
            mtime: attr.mtime() as u64,
        }))
    }
}

impl waj::create::EntryTrait for FsEntry {
    fn kind(&self) -> jbk::Result<Option<waj::create::EntryKind>> {
        Ok(match self.kind {
            FsEntryKind::File(content_address, ref mime, _size) => Some(
                waj::create::EntryKind::Content(content_address, mime.clone()),
            ),
            FsEntryKind::Link => Some(waj::create::EntryKind::Redirect(
                fs::read_link(&self.path)?.to_str().unwrap().to_string(),
            )),
            _ => None,
        })
    }
    fn name(&self) -> Cow<str> {
        Cow::Owned(
            self.name
                .to_str()
                .unwrap_or_else(|| panic!("{:?} must be a utf8", self.name))
                .to_owned(),
        )
    }
}

impl arx::create::EntryTrait for FsEntry {
    fn kind(&self) -> jbk::Result<Option<arx::create::EntryKind>> {
        Ok(match self.kind {
            FsEntryKind::Dir => Some(arx::create::EntryKind::Dir),
            FsEntryKind::File(content_address, ref _mime, size) => {
                Some(arx::create::EntryKind::File(size, content_address))
            }

            FsEntryKind::Link => Some(arx::create::EntryKind::Link(
                arx::Path::from_path(&fs::read_link(&self.path)?)
                    .unwrap_or_else(|_| panic!("Must be utf8"))
                    .to_owned(),
            )),
            FsEntryKind::Other => None,
        })
    }

    fn path(&self) -> &arx::Path {
        #![allow(clippy::misnamed_getters)]
        // The "path" in a arx is the name here
        arx::Path::from_path(&self.name).unwrap()
    }

    fn uid(&self) -> u64 {
        self.uid
    }
    fn gid(&self) -> u64 {
        self.gid
    }
    fn mode(&self) -> u64 {
        self.mode
    }
    fn mtime(&self) -> u64 {
        self.mtime
    }
}

pub struct FsAdder<'a> {
    creator: &'a mut EntryStoreCreator,
    strip_prefix: &'a Path,
}

impl<'a> FsAdder<'a> {
    pub fn new(creator: &'a mut EntryStoreCreator, strip_prefix: &'a Path) -> Self {
        Self {
            creator,
            strip_prefix,
        }
    }

    pub fn add_from_path<P, A>(&mut self, path: P, recurse: bool, adder: &mut A) -> Void
    where
        P: AsRef<std::path::Path>,
        A: Adder,
    {
        self.add_from_path_with_filter(path, recurse, |_e| true, adder)
    }

    pub fn add_from_path_with_filter<P, F, A>(
        &mut self,
        path: P,
        recurse: bool,
        filter: F,
        adder: &mut A,
    ) -> Void
    where
        P: AsRef<std::path::Path>,
        F: FnMut(&walkdir::DirEntry) -> bool,
        A: Adder,
    {
        let mut walker = walkdir::WalkDir::new(path);
        if !recurse {
            walker = walker.max_depth(0);
        }
        let walker = walker.into_iter();
        for entry in walker.filter_entry(filter) {
            let entry = entry.unwrap();
            let wpack_path = entry
                .path()
                .strip_prefix(self.strip_prefix)
                .unwrap()
                .to_path_buf();
            if wpack_path.as_os_str().is_empty() {
                continue;
            }
            let entry = FsEntry::new_from_walk_entry(entry, wpack_path, adder)?;
            self.creator.add_entry(entry.as_ref())?;
        }
        Ok(())
    }
}
