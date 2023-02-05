use crate::arx_creator::ArxCreator;
use crate::jim_creator::JimCreator;
use jubako as jbk;
use mime_guess::mime;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::ffi::OsString;
use std::fs;
use std::os::unix::ffi::OsStringExt;
use std::path::{Path, PathBuf};
use std::rc::Rc;

const VENDOR_ID: u32 = 0x6a_69_6d_00;

#[derive(Debug)]
enum EntryKind {
    Dir,
    File,
    Link,
    Other,
}

#[derive(Debug)]
pub struct Entry {
    kind: EntryKind,
    path: PathBuf,
    parent: jbk::Bound<jbk::EntryIdx>,
}

impl Entry {
    pub fn new(path: PathBuf, parent: jbk::Bound<jbk::EntryIdx>) -> jbk::Result<Self> {
        let attr = fs::symlink_metadata(&path)?;
        Ok(if attr.is_dir() {
            Self {
                kind: EntryKind::Dir,
                path,
                parent,
            }
        } else if attr.is_file() {
            Self {
                kind: EntryKind::File,
                path,
                parent,
            }
        } else if attr.is_symlink() {
            Self {
                kind: EntryKind::Link,
                path,
                parent,
            }
        } else {
            Self {
                kind: EntryKind::Other,
                path,
                parent,
            }
        })
    }

    pub fn new_from_fs(dir_entry: fs::DirEntry, parent: jbk::Bound<jbk::EntryIdx>) -> Self {
        let path = dir_entry.path();
        if let Ok(file_type) = dir_entry.file_type() {
            if file_type.is_dir() {
                Self {
                    kind: EntryKind::Dir,
                    path,
                    parent,
                }
            } else if file_type.is_file() {
                Self {
                    kind: EntryKind::File,
                    path,
                    parent,
                }
            } else if file_type.is_symlink() {
                Self {
                    kind: EntryKind::Link,
                    path,
                    parent,
                }
            } else {
                Self {
                    kind: EntryKind::Other,
                    path,
                    parent,
                }
            }
        } else {
            Self {
                kind: EntryKind::Other,
                path,
                parent,
            }
        }
    }
}

pub struct Creator {
    content_pack: jbk::creator::ContentPackCreator,
    directory_pack: Rc<RefCell<Option<jbk::creator::DirectoryPackCreator>>>,
    entry_count: jbk::EntryCount,
    queue: VecDeque<Entry>,
    jim_creator: JimCreator,
    arx_creator: ArxCreator,
}

impl Creator {
    pub fn new<P: AsRef<Path>>(outfile: P, main_entry: PathBuf) -> jbk::Result<Self> {
        let outfile = outfile.as_ref();
        let mut outfilename: OsString = outfile.file_name().unwrap().to_os_string();
        outfilename.push(".rvpc");
        let mut content_pack_path = PathBuf::new();
        content_pack_path.push(outfile);
        content_pack_path.set_file_name(outfilename);
        let content_pack = jbk::creator::ContentPackCreator::new(
            content_pack_path,
            jbk::PackId::from(1),
            VENDOR_ID,
            jbk::FreeData40::clone_from_slice(&[0x00; 40]),
            jbk::CompressionType::Zstd,
        )?;

        outfilename = outfile.file_name().unwrap().to_os_string();
        outfilename.push(".rvpd");
        let mut directory_pack_path = PathBuf::new();
        directory_pack_path.push(outfile);
        directory_pack_path.set_file_name(outfilename);
        let directory_pack = Rc::new(RefCell::new(Some(jbk::creator::DirectoryPackCreator::new(
            directory_pack_path,
            jbk::PackId::from(0),
            VENDOR_ID,
            jbk::FreeData31::clone_from_slice(&[0x00; 31]),
        ))));

        let jim_creator = JimCreator::new(Rc::clone(&directory_pack), main_entry);
        let arx_creator = ArxCreator::new(Rc::clone(&directory_pack));

        Ok(Self {
            content_pack,
            directory_pack,
            entry_count: 0.into(),
            queue: VecDeque::<Entry>::new(),
            jim_creator,
            arx_creator,
        })
    }

    fn finalize(self, outfile: PathBuf) -> jbk::Result<()> {
        self.jim_creator.finalize()?;
        self.arx_creator.finalize()?;
        let directory_pack = self.directory_pack.take().unwrap();
        let directory_pack_info = directory_pack.finalize()?;
        let content_pack_info = self.content_pack.finalize()?;
        let mut manifest_creator = jbk::creator::ManifestPackCreator::new(
            outfile,
            VENDOR_ID,
            jbk::FreeData63::clone_from_slice(&[0x00; 63]),
        );

        manifest_creator.add_pack(directory_pack_info);
        manifest_creator.add_pack(content_pack_info);
        manifest_creator.finalize()?;
        Ok(())
    }

    pub fn push_back(&mut self, entry: Entry) {
        if let EntryKind::Other = entry.kind {
            // do not add other to the queue
        } else {
            self.queue.push_back(entry);
        }
    }

    fn next_id(&self) -> jbk::EntryCount {
        // Return the id that will be pushed back.
        // The id is the entry_count (entries already added) + the size of the queue (entries to add)
        self.entry_count + self.queue.len() as u32
    }

    pub fn run(mut self, outfile: PathBuf) -> jbk::Result<()> {
        self.arx_creator
            .set_root_count((self.queue.len() as u32).into());
        while !self.queue.is_empty() {
            let entry = self.queue.pop_front().unwrap();
            self.handle(entry)?;
            if self.entry_count.into_u32() % 1000 == 0 {
                println!("{}", self.entry_count);
            }
        }
        self.finalize(outfile)
    }

    fn handle(&mut self, entry: Entry) -> jbk::Result<()> {
        let entry_path = entry.path.clone();
        match entry.kind {
            EntryKind::Dir => {
                let nb_entries = jbk::Vow::new(0_u64);
                let first_entry = self.next_id() + 1; // The current directory is not in the queue but not yet added we need to count it now.

                let idx = self.arx_creator.add_directory(
                    entry_path,
                    entry.parent,
                    first_entry,
                    nb_entries.bind(),
                );
                let mut entry_count = 0;
                for sub_entry in fs::read_dir(&entry.path)? {
                    self.push_back(Entry::new_from_fs(sub_entry?, idx.clone()));
                    entry_count += 1;
                }
                nb_entries.fulfil(entry_count);
                self.entry_count += 1;
            }
            EntryKind::File => {
                let file = jbk::Reader::from(jbk::creator::FileSource::open(&entry.path)?);

                let mime_type = match mime_guess::from_path(entry.path).first() {
                    Some(m) => m,
                    None => {
                        let mut buf = [0u8; 100];
                        let size = std::cmp::min(100, file.size().into_usize());
                        file.create_stream_to(jbk::End::new_size(size))
                            .read_exact(&mut buf[..size])?;
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
                let content_id = self.content_pack.add_content(file)?;
                let content_address = jbk::ContentAddress::new(jbk::PackId::from(1), content_id);

                self.jim_creator
                    .add_content(entry_path.clone(), mime_type, content_address);
                self.arx_creator
                    .add_content(entry_path, entry.parent, content_address);
                self.entry_count += 1;
            }
            EntryKind::Link => {
                let target = fs::read_link(&entry.path)?.into_os_string().into_vec();
                self.jim_creator
                    .add_redirect(entry_path.clone(), target.clone());
                self.arx_creator
                    .add_redirect(entry_path, entry.parent, target);
                self.entry_count += 1;
            }
            EntryKind::Other => unreachable!(),
        };
        Ok(())
    }
}
