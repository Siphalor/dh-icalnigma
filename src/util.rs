use std::ops::Deref;

use markup5ever_rcdom::Handle;
use markup5ever_rcdom::NodeData;
use crate::util::Error::Custom;
use std::fmt::Debug;
use std::io;

pub fn write_ical_field<W, K, V>(output: &mut W, key: K, value: V)
    where W: io::Write, K: Into<String>, V: Into<String> {
    let key = key.into();
    let value = value.into().replace(",", "\\,");
    let line = format!("{}:{}", &key, &value);
    let mut line_rest: &str = line.as_str();

    let mut first = true;
    let mut current_line_length = 0;
    while !line_rest.is_empty() {
        for line_char in line_rest.chars() {
            current_line_length += line_char.len_utf8();
            if current_line_length > 72 {
                current_line_length -= line_char.len_utf8();
                break;
            }
        }

        let parts = line_rest.split_at(current_line_length);
        line_rest = parts.1;
        current_line_length = 0;

        if first {
            write!(output, "{}\r\n", parts.0).ok();
            first = false;
        } else {
            write!(output, " {}\r\n", parts.0).ok();
        }
    }
}

pub trait HandleExtensions {
    fn check_attribute(self, attribute_name: &str, attribute_value: &str) -> Result<Handle, Error>;

    fn get_node_by_tag_name(&self, tag_name: &str) -> Option<Handle>;
    fn get_nodes_by_tag_name(&self, tag_name: &str) -> Vec<Handle>;

    fn get_attribute_value(&self, attribute_name: &str) -> Option<String>;

    fn get_content(&self) -> Option<String>;
}

impl HandleExtensions for Handle {
    fn check_attribute(self, attribute_name: &str, attribute_value: &str) -> Result<Self, Error> {
        let val = self.get_attribute_value(attribute_name).ok_or(format!("No such attribute: {}", attribute_name))?;
        if &val == attribute_value {
            Ok(self)
        } else {
            Err(format!("Unexpected value {} for attribute {}, expected {}", val, attribute_name, attribute_value).into())
        }
    }

    fn get_node_by_tag_name(&self, tag_name: &str) -> Option<Handle> {
        let children = self.children.borrow();
        children.iter().filter(|handle| {
            if let NodeData::Element { name, .. } = &handle.data {
                return tag_name == &name.local;
            }
            false
        }).next().map(|handle| handle.clone())
    }

    fn get_nodes_by_tag_name(&self, tag_name: &str) -> Vec<Handle> {
        let children = self.children.borrow();
        children.iter().filter(|handle| {
            if let NodeData::Element { name, .. } = &handle.data {
                return tag_name == &name.local;
            }
            false
        }).map(|val| val.clone()).collect()
    }

    fn get_attribute_value(&self, attribute_name: &str) -> Option<String> {
        if let NodeData::Element { attrs, .. } = &self.data {
            for attr in attrs.borrow().deref() {
                if &attr.name.local == attribute_name {
                    return Some(attr.value.to_string())
                }
            }
        }
        None
    }

    fn get_content(&self) -> Option<String> {
        for child in &*self.children.borrow() {
            if let NodeData::Text { contents } = &child.data {
                return Some(contents.borrow().to_string());
            }
        };
        None
    }
}

#[derive(Debug)]
pub enum Error {
    Custom(String)
}

impl From<String> for Error {
    fn from(text: String) -> Self {
        Custom(text)
    }
}

impl From<&str> for Error {
    fn from(text: &str) -> Self {
        text.to_string().into()
    }
}

#[derive(Hash, Debug)]
pub struct EventHash<'a> {
    pub creation_time: i64,
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub creator: Option<&'a String>,
    pub event_id: Option<&'a String>,
}
