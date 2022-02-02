use std::fmt::Debug;
use std::ops::Deref;

use markup5ever_rcdom::Handle;
use markup5ever_rcdom::NodeData;

use crate::util::Error::Custom;

pub trait HandleExtensions {
    fn check_attribute(self, attribute_name: &str, attribute_value: &str) -> Result<Handle, Error>;

    fn get_node_by_tag_name(&self, tag_name: &str) -> Option<Handle>;
    fn get_nodes_by_tag_name(&self, tag_name: &str) -> Vec<Handle>;

    fn get_attribute_value(&self, attribute_name: &str) -> Option<String>;

    fn get_content(&self) -> Option<String>;
    fn get_text_nodes(&self) -> Vec<String>;
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
                    return Some(attr.value.to_string());
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

    fn get_text_nodes(&self) -> Vec<String> {
        self.children.borrow().iter()
            .filter_map(|node| match &node.data {
                NodeData::Text { contents } => Some(contents.borrow().to_string()),
                _ => None,
            }).collect()
    }
}

pub type Year = i32;
pub type Month = u32;
pub type Day = u32;

pub fn get_month_from_german(text: &str) -> Result<Month, Error> {
    match text.to_lowercase().as_str() {
        "januar"    => Ok(1),
        "februar"   => Ok(2),
        "märz"      => Ok(3),
        "april"     => Ok(4),
        "mai"       => Ok(5),
        "juni"      => Ok(6),
        "juli"      => Ok(7),
        "august"    => Ok(8),
        "september" => Ok(9),
        "oktober"   => Ok(10),
        "november"  => Ok(11),
        "dezember"  => Ok(12),
        _ => {
            Err(format!("Failed to resolve month \"{}\"", text).into())
        }
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
