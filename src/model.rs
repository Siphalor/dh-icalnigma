use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use chrono::{Datelike, DateTime, Utc};

pub type Months = BTreeMap<String, Vec<Event>>;

#[derive(Deserialize, Serialize)]
pub struct Event {
    pub creation: Option<DateTime<Utc>>,
    pub creator: Option<String>,
    pub begin: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub name: String,
    pub lecturers: Vec<Lecturer>,
    pub locations: Vec<String>,
    pub courses: Vec<String>,
    /// Additional event data
    pub data: EventData,
}

#[derive(Deserialize, Serialize)]
pub enum EventData {
    Lecture {
        /// The event number in Rapla - not unique on its own!
        number: Option<String>,
        /// The language as loaded from Rapla
        language: Option<String>,
        /// The event kind as loaded from Rapla
        kind: Option<String>,
        /// The categories as loaded from Rapla
        categories: Vec<String>,
        /// The total number of hours for this lecture module as loaded from Rapla
        total_hours: Option<u32>,
    },
    Exam,
    Other,
}

#[derive(Deserialize, Serialize, Clone)]
pub struct Lecturer {
    pub name: String,
}

impl Event {
    pub fn hash(&self) -> u64 {
        #[derive(Hash, Debug)]
        struct EventHash<'a> {
            creation_time: i64,
            name: &'a String,
            year: i32,
            month: u32,
            day: u32,
            creator: Option<&'a String>,
        }

        let mut hasher = DefaultHasher::new();

        EventHash {
            creation_time: self.creation.map_or(0, |time| time.timestamp()),
            name: &self.name,
            year: self.begin.year(),
            month: self.begin.month(),
            day: self.begin.day(),
            creator: self.creator.as_ref(),
        }.hash(&mut hasher);

        return hasher.finish();
    }

    pub fn title(&self) -> String {
        if let EventData::Lecture{kind, ..} = &self.data {
            if let Some(kind) = kind {
                return format!("{} - {}", self.name, kind);
            }
        }
        return self.name.clone();
    }
}
