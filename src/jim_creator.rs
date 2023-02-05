use jubako as jbk;

use jbk::creator::schema;
use mime_guess::Mime;
use std::cell::RefCell;
use std::os::unix::ffi::OsStringExt;
use std::path::PathBuf;
use std::rc::Rc;
/*
pub enum JimEntry {
    Content {
        idx: jbk::Vow<jbk::EntryIdx>,
        path: Value,
        mimetype: Value,
        content: Value,
    },
    Redirection {
        idx: jbk::Vow<jbk::EntryIdx>,
        path: Value,
        redirect: Value,
    },
}

impl jbk::creator::EntryTrait for JimEntry {
    fn variant_id(&self) -> Option<jbk::VariantIdx> {
        Some(match self {
            Self::Content {
                idx: _,
                path: _,
                mimetype: _,
                content: _,
            } => 0.into(),
            Self::Redirection {
                idx: _,
                path: _,
                redirect: _,
            } => 1.into(),
        })
    }
    fn value(&self, id: jbk::PropertyIdx) -> &Value {
        match self {
            Self::Content {
                idx: _,
                path,
                mimetype,
                content,
            } => match id.into_u8() {
                0 => &path,
                1 => &mimetype,
                2 => &content,
                _ => unreachable!(),
            },
            Self::Redirection {
                idx: _,
                path,
                redirect,
            } => match id.into_u8() {
                0 => &path,
                1 => &redirect,
                _ => unreachable!(),
            },
        }
    }
    fn value_count(&self) -> jbk::PropertyCount {
        match self {
            Self::Content {
                idx: _,
                path: _,
                mimetype: _,
                content: _,
            } => 3.into(),
            Self::Redirection {
                idx: _,
                path: _,
                redirect: _,
            } => 2.into(),
        }
    }
    fn set_idx(&mut self, new_idx: jbk::EntryIdx) {
        match self {
            Self::Content {
                idx,
                path: _,
                mimetype: _,
                content: _,
            } => idx.fulfil(new_idx),
            Self::Redirection {
                idx,
                path: _,
                redirect: _,
            } => idx.fulfil(new_idx),
        }
    }
    fn get_idx(&self) -> jbk::Bound<jbk::EntryIdx> {
        match self {
            Self::Content {
                idx,
                path: _,
                mimetype: _,
                content: _,
            } => idx.bind(),
            Self::Redirection {
                idx,
                path: _,
                redirect: _,
            } => idx.bind(),
        }
    }
}

impl JimEntry {
    pub fn new_redirect(path: Value, redirect: Value) -> Self {
        Self::Redirection {
            idx: Default::default(),
            path,
            redirect,
        }
    }
    pub fn new_content(path: Value, mimetype: Value, content: Value) -> Self {
        Self::Content {
            idx: Default::default(),
            path,
            mimetype,
            content,
        }
    }
}*/

pub struct JimCreator {
    directory_pack: Rc<RefCell<Option<jbk::creator::DirectoryPackCreator>>>,
    entry_store: Box<jbk::creator::EntryStore<jbk::creator::BasicEntry>>,
    entry_count: jbk::EntryCount,
    main_entry_path: PathBuf,
    main_entry_id: jbk::EntryIdx,
}

impl JimCreator {
    pub fn new(
        directory_pack: Rc<RefCell<Option<jbk::creator::DirectoryPackCreator>>>,
        main_entry: PathBuf,
    ) -> Self {
        let path_store = directory_pack
            .borrow_mut()
            .as_mut()
            .unwrap()
            .create_value_store(jbk::creator::ValueStoreKind::Plain);
        let mime_store = directory_pack
            .borrow_mut()
            .as_mut()
            .unwrap()
            .create_value_store(jbk::creator::ValueStoreKind::Indexed);

        let schema = schema::Schema::new(
            // Common part
            schema::CommonProperties::new(vec![
                schema::Property::VLArray(1, Rc::clone(&path_store)), // the path
            ]),
            vec![
                // Content
                schema::VariantProperties::new(vec![
                    schema::Property::VLArray(0, Rc::clone(&mime_store)), // the mimetype
                    schema::Property::ContentAddress,
                ]),
                // Redirect
                schema::VariantProperties::new(vec![
                    schema::Property::VLArray(0, Rc::clone(&path_store)), // Id of the linked entry
                ]),
            ],
        );

        let entry_store = Box::new(jbk::creator::EntryStore::new(schema));

        Self {
            directory_pack,
            entry_store,
            entry_count: 0.into(),
            main_entry_path: main_entry,
            main_entry_id: Default::default(),
        }
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
            "jim_entries",
            jubako::ContentAddress::new(0.into(), 0.into()),
            jbk::PropertyIdx::from(0),
            entry_store_id,
            self.entry_count,
            jubako::EntryIdx::from(0),
        );
        directory_pack.as_mut().unwrap().create_index(
            "jim_main",
            jubako::ContentAddress::new(0.into(), 0.into()),
            jbk::PropertyIdx::from(0),
            entry_store_id,
            jubako::EntryCount::from(1),
            self.main_entry_id,
        );
        Ok(())
    }

    pub fn add_content(
        &mut self,
        entry_path: PathBuf,
        mimetype: Mime,
        content_address: jbk::ContentAddress,
    ) {
        if entry_path == self.main_entry_path {
            self.main_entry_id = self.entry_count.into_u32().into();
        }

        let mut value_entry_path = entry_path.into_os_string().into_vec();
        value_entry_path.truncate(255);
        let value_entry_path = jbk::Value::Array(value_entry_path);

        self.add_entry(jbk::creator::BasicEntry::new_from_schema(
            self.schema(),
            Some(0.into()),
            vec![
                value_entry_path,
                jbk::Value::Array(mimetype.to_string().into()),
                jbk::Value::Content(content_address),
            ],
        ));
    }

    pub fn add_redirect(&mut self, entry_path: PathBuf, mut redirect: Vec<u8>) {
        let mut value_entry_path = entry_path.into_os_string().into_vec();
        value_entry_path.truncate(255);
        let value_entry_path = jbk::Value::Array(value_entry_path);

        redirect.truncate(255);
        let value_redirect = jbk::Value::Array(redirect);

        self.add_entry(jbk::creator::BasicEntry::new_from_schema(
            self.schema(),
            Some(1.into()),
            vec![value_entry_path, value_redirect],
        ));
    }

    fn add_entry(&mut self, entry: jbk::creator::BasicEntry) {
        self.entry_store.add_entry(entry);
        self.entry_count += 1;
    }
}
