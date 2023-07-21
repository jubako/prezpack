use jubako as jbk;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

pub type Void = jbk::Result<()>;

const VENDOR_ID: u32 = 0x6a_69_6d_00;

pub enum ConcatMode {
    OneFile,
    TwoFiles,
    NoConcat,
}

pub struct EntryStoreCreator {
    waj_creator: waj::create::EntryStoreCreator,
    arx_creator: arx::create::EntryStoreCreator,
}

impl EntryStoreCreator {
    pub fn new(main_entry: PathBuf) -> Self {
        Self {
            waj_creator: waj::create::EntryStoreCreator::new(main_entry),
            arx_creator: arx::create::EntryStoreCreator::new(),
        }
    }

    pub fn finalize(self, directory_pack: &mut jbk::creator::DirectoryPackCreator) -> Void {
        self.waj_creator.finalize(directory_pack)?;
        self.arx_creator.finalize(directory_pack);
        Ok(())
    }

    pub fn add_entry<E>(&mut self, entry: &E) -> Void
    where
        E: waj::create::EntryTrait + arx::create::EntryTrait,
    {
        self.waj_creator.add_entry(entry)?;
        self.arx_creator.add_entry(entry)
    }
}

pub struct ContentAdder {
    content_pack: jbk::creator::CachedContentPackCreator,
}

impl ContentAdder {
    fn new(content_pack: jbk::creator::CachedContentPackCreator) -> Self {
        Self { content_pack }
    }

    fn into_inner(self) -> jbk::creator::CachedContentPackCreator {
        self.content_pack
    }
}

impl super::fs_adder::Adder for ContentAdder {
    fn add(&mut self, reader: jbk::Reader) -> jbk::Result<jbk::ContentAddress> {
        let content_id = self.content_pack.add_content(reader)?;
        Ok(jbk::ContentAddress::new(1.into(), content_id))
    }
}

pub struct Creator {
    adder: ContentAdder,
    directory_pack: jbk::creator::DirectoryPackCreator,
    entry_store_creator: EntryStoreCreator,
    strip_prefix: PathBuf,
    concat_mode: ConcatMode,
    tmp_path_content_pack: tempfile::TempPath,
    tmp_path_directory_pack: tempfile::TempPath,
}

impl Creator {
    pub fn new<P: AsRef<Path>>(
        outfile: P,
        strip_prefix: PathBuf,
        main_entry: PathBuf,
        concat_mode: ConcatMode,
        progress: Arc<dyn jbk::creator::Progress>,
        cache_progress: Rc<dyn jbk::creator::CacheProgress>,
    ) -> jbk::Result<Self> {
        let outfile = outfile.as_ref();
        let out_dir = outfile.parent().unwrap();

        let (tmp_content_pack, tmp_path_content_pack) =
            tempfile::NamedTempFile::new_in(out_dir)?.into_parts();
        let content_pack = jbk::creator::ContentPackCreator::new_from_file_with_progress(
            tmp_content_pack,
            jbk::PackId::from(1),
            VENDOR_ID,
            jbk::FreeData40::clone_from_slice(&[0x00; 40]),
            jbk::CompressionType::Zstd,
            progress,
        )?;

        let (_, tmp_path_directory_pack) = tempfile::NamedTempFile::new_in(out_dir)?.into_parts();
        let directory_pack = jbk::creator::DirectoryPackCreator::new(
            &tmp_path_directory_pack,
            jbk::PackId::from(0),
            VENDOR_ID,
            jbk::FreeData31::clone_from_slice(&[0x00; 31]),
        );

        let entry_store_creator = EntryStoreCreator::new(main_entry);

        Ok(Self {
            adder: ContentAdder::new(jbk::creator::CachedContentPackCreator::new(
                content_pack,
                cache_progress,
            )),
            directory_pack,
            entry_store_creator,
            strip_prefix,
            concat_mode,
            tmp_path_content_pack,
            tmp_path_directory_pack,
        })
    }

    pub fn finalize(mut self, outfile: &Path) -> jbk::Result<()> {
        self.entry_store_creator
            .finalize(&mut self.directory_pack)?;

        let directory_pack_info = match self.concat_mode {
            ConcatMode::NoConcat => {
                let mut outfilename = outfile.file_name().unwrap().to_os_string();
                outfilename.push(".jbkd");
                let mut directory_pack_path = PathBuf::new();
                directory_pack_path.push(outfile);
                directory_pack_path.set_file_name(outfilename);
                let directory_pack_info = self
                    .directory_pack
                    .finalize(Some(directory_pack_path.clone()))?;
                if let Err(e) = self.tmp_path_directory_pack.persist(&directory_pack_path) {
                    return Err(e.error.into());
                };
                directory_pack_info
            }
            _ => self.directory_pack.finalize(None)?,
        };

        let content_pack_info = match self.concat_mode {
            ConcatMode::OneFile => self.adder.into_inner().into_inner().finalize(None)?,
            _ => {
                let mut outfilename = outfile.file_name().unwrap().to_os_string();
                outfilename.push(".jbkc");
                let mut content_pack_path = PathBuf::new();
                content_pack_path.push(outfile);
                content_pack_path.set_file_name(outfilename);
                let content_pack_info = self
                    .adder
                    .into_inner()
                    .into_inner()
                    .finalize(Some(content_pack_path.clone()))?;
                if let Err(e) = self.tmp_path_content_pack.persist(&content_pack_path) {
                    return Err(e.error.into());
                }
                content_pack_info
            }
        };

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

    pub fn add_from_path(&mut self, path: &Path, recurse: bool) -> Void {
        let mut fs_adder =
            super::fs_adder::FsAdder::new(&mut self.entry_store_creator, &self.strip_prefix);
        fs_adder.add_from_path(path, recurse, &mut self.adder)
    }
}
