use jubako as jbk;

use jbk::creator::schema;
use jbk::creator::EntryTrait;
use std::cell::RefCell;
use std::os::unix::ffi::OsStringExt;
use std::path::PathBuf;
use std::rc::Rc;

pub struct ArxCreator {
    directory_pack: Rc<RefCell<Option<jbk::creator::DirectoryPackCreator>>>,
    entry_store: Box<jbk::creator::EntryStore<jbk::creator::BasicEntry>>,
    entry_count: jbk::EntryCount,
    root_count: jbk::EntryCount,
}

impl ArxCreator {
    pub fn new(directory_pack: Rc<RefCell<Option<jbk::creator::DirectoryPackCreator>>>) -> Self {
        let path_store = directory_pack
            .borrow_mut()
            .as_mut()
            .unwrap()
            .create_value_store(jbk::creator::ValueStoreKind::Plain);

        let schema = schema::Schema::new(
            // Common part
            schema::CommonProperties::new(vec![
                schema::Property::VLArray(1, Rc::clone(&path_store)), // the path
                schema::Property::new_int(),                          // index of the parent entry
            ]),
            vec![
                // File
                schema::VariantProperties::new(vec![schema::Property::ContentAddress]),
                // Directory
                schema::VariantProperties::new(vec![
                    schema::Property::new_int(), // index of the first entry
                    schema::Property::new_int(), // nb entries in the directory
                ]),
                // Link
                schema::VariantProperties::new(vec![
                    schema::Property::VLArray(1, Rc::clone(&path_store)), // Id of the linked entry
                ]),
            ],
        );

        let entry_store = Box::new(jbk::creator::EntryStore::new(schema));

        Self {
            directory_pack,
            entry_store,
            entry_count: 0.into(),
            root_count: 0.into(),
        }
    }

    pub fn set_root_count(&mut self, root_count: jbk::EntryCount) {
        self.root_count = root_count;
    }

    pub fn schema(&self) -> &schema::Schema {
        &self.entry_store.schema
    }

    pub fn finalize(self) -> jbk::Result<()> {
        let mut directory_pack = self.directory_pack.borrow_mut();
        let entry_store_id = directory_pack
            .as_mut()
            .unwrap()
            .add_entry_store(self.entry_store);
        directory_pack.as_mut().unwrap().create_index(
            "arx_entries",
            jubako::ContentAddress::new(0.into(), 0.into()),
            jbk::PropertyIdx::from(0),
            entry_store_id,
            self.entry_count,
            jubako::EntryIdx::from(0),
        );
        directory_pack.as_mut().unwrap().create_index(
            "arx_root",
            jubako::ContentAddress::new(0.into(), 0.into()),
            jbk::PropertyIdx::from(0),
            entry_store_id,
            self.root_count,
            jubako::EntryIdx::from(0),
        );
        Ok(())
    }

    pub fn add_directory(
        &mut self,
        entry_path: PathBuf,
        parent: jbk::Bound<jbk::EntryIdx>,
        first_entry: jbk::EntryCount,
        nb_entries: jbk::Bound<u64>,
    ) -> jbk::Bound<jbk::EntryIdx> {
        let entry_path =
            jbk::Value::Array(entry_path.file_name().unwrap().to_os_string().into_vec());
        let entry = jbk::creator::BasicEntry::new_from_schema(
            self.schema(),
            Some(1.into()),
            vec![
                entry_path,
                jbk::Value::Unsigned(parent.into()),
                jbk::Value::Unsigned(first_entry.into_u64().into()),
                jbk::Value::Unsigned(nb_entries.into()),
            ],
        );
        let idx = entry.get_idx();
        self.add_entry(entry);
        idx
    }

    pub fn add_content(
        &mut self,
        entry_path: PathBuf,
        parent: jbk::Bound<jbk::EntryIdx>,
        content_address: jbk::ContentAddress,
    ) {
        let entry_path =
            jbk::Value::Array(entry_path.file_name().unwrap().to_os_string().into_vec());

        self.add_entry(jbk::creator::BasicEntry::new_from_schema(
            self.schema(),
            Some(0.into()),
            vec![
                entry_path,
                jbk::Value::Unsigned(parent.into()),
                jbk::Value::Content(content_address),
            ],
        ));
    }

    pub fn add_redirect(
        &mut self,
        entry_path: PathBuf,
        parent: jbk::Bound<jbk::EntryIdx>,
        redirect: Vec<u8>,
    ) {
        let entry_path =
            jbk::Value::Array(entry_path.file_name().unwrap().to_os_string().into_vec());

        self.add_entry(jbk::creator::BasicEntry::new_from_schema(
            self.schema(),
            Some(2.into()),
            vec![
                entry_path,
                jbk::Value::Unsigned(parent.into()),
                jbk::Value::Array(redirect),
            ],
        ));
    }

    fn add_entry(&mut self, entry: jbk::creator::BasicEntry) {
        self.entry_store.add_entry(entry);
        self.entry_count += 1;
    }
}
