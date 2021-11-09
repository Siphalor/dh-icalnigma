use std::io;
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::option::Option::Some;

use chrono::{TimeZone, Utc};
use chrono_tz::Europe::Berlin;
use clap::Parser;
use encoding_rs_io::DecodeReaderBytesBuilder;
use html5ever::ParseOpts;
use html5ever::tendril::TendrilSink;
use lazy_static::lazy_static;
use markup5ever_rcdom::{Handle, RcDom};
use regex::Regex;
use crate::archive::{read_archive, write_archive};

use crate::icalendar::write_calendar;
use crate::model::{Event, EventData, Months};
use crate::util::{Error, HandleExtensions};

mod util;
mod model;
mod icalendar;
mod archive;

#[derive(Parser)]
#[clap(
    version = "0.2",
    author = "Siphalor <info@siphalor.de>",
    rename_all = "kebab",
    about = "An unofficial program that transpiles Rapla HTML sites to iCalendar files.",
)]
struct Opts {
    /// The HTML file to read in
    #[clap(required=true)]
    input: String,

    /// The output file
    #[clap(required=true)]
    output: String,

    /// Sets the archive file and enables archiving
    #[clap(short, long)]
    archive: Option<String>,
}

fn main() {
    let opts: Opts = Opts::parse();

    match File::open(opts.input) {
        Ok(mut input_file) => {

            match OpenOptions::new().read(false).write(true).truncate(true).create(true).open(opts.output) {
                Ok(mut output_file) => {
                    let res = load_events(&mut input_file);

                    if let Err(error) = res {
                        eprintln!("Failed to load events from file: {:?}", error);
                        return;
                    }

                    let mut months = res.unwrap();

                    if let Some(archive_path) = &opts.archive {
                        match read_archive(archive_path) {
                            Ok(mut archive_months) => {
                                archive_months.extend(months);
                                months = archive_months;
                            }
                            Err(error) => eprintln!("Failed to read archive: {}", error),
                        }

                        if let Err(error) = write_archive(archive_path, &months) {
                            eprintln!("Failed to write archive: {}", error);
                        }
                    }

                    write_calendar(&mut output_file, &months.into_values().flatten().collect());
                }
                Err(error) => {
                    eprintln!("Failed to open output file: {}", error);
                }
            }
        }
        Err(error) => {
            eprintln!("Failed to open input file: {}", error);
        }
    }
}

fn load_events<R: io::Read>(input_stream: &mut R) -> Result<Months, util::Error> {
    let mut input_stream = DecodeReaderBytesBuilder::new()
        .encoding(Some(encoding_rs::WINDOWS_1252))
        .build(input_stream);
    let dom = html5ever::parse_document(RcDom::default(), ParseOpts {
        ..Default::default()
    })
        .from_utf8()
        .read_from(&mut input_stream)
        .expect("Failed to parse input!");

    let document = dom.document;
    let html = document.get_node_by_tag_name("html").expect("Document does not have an html tag!");
    let body = html.get_node_by_tag_name("body").expect("Document does not have a body tag!");

    let mut months = Months::new();
    for handle in body.get_nodes_by_tag_name("div") {
        if let Some(val) = handle.get_attribute_value("class") {
            if val == "calendar" {
                if let Some((month, events)) = load_month(handle)? {
                    months.insert(month, events);
                }
            }
        }
    }

    Ok(months)
}

fn load_month(month_handle: Handle) -> Result<Option<(String, Vec<Event>)>, util::Error> {
    let mut events = Vec::new();

    if let Some(table_handle) = month_handle.get_node_by_tag_name("table") {
        if let Some(tbody_handle) = table_handle.get_node_by_tag_name("tbody") {
            for row_handle in tbody_handle.get_nodes_by_tag_name("tr") {
                for cell_handle in row_handle.get_nodes_by_tag_name("td") {
                    if cell_handle.get_attribute_value("class").map_or(true, |val| val != "month_cell") {
                        continue;
                    }

                    match load_day(cell_handle) {
                        Ok(day_events) => {
                            if let Some(day_events) = day_events {
                                events.extend(day_events)
                            }
                        },
                        Err(error) => eprintln!("Error in day: {:?}", error),
                    }
                }
            }
        }
    }

    if events.is_empty() {
        return Ok(None)
    }
    return Ok(Some(( events.first().unwrap().end.format("%Y%m").to_string(), events )))
}

fn load_day(cell_handle: Handle) -> Result<Option<Vec<Event>>, Error> {
    let divs = cell_handle.get_nodes_by_tag_name("div");
    if divs.len() < 2 { // The first div is always contains the number of the day
        return Ok(None);
    }

    let mut divs = divs.into_iter();
    divs.next();

    let mut events = Vec::with_capacity(divs.len());
    for div in divs {
        if div.get_attribute_value("class").map_or(true, |val| val != "month_block") {
            eprintln!("Skipping potential event, class={:?}, content={:?}", div.get_attribute_value("class"), div.get_content());
            continue;
        }

        match process_event(div) {
            Ok(event) => events.push(event),
            Err(error) => eprintln!("Error in event: {:?}", error),
        }
    }

    Ok(Some(events))
}

fn process_event(event_handle: Handle) -> Result<Event, Error> {
    let link_handle = event_handle.get_node_by_tag_name("a").ok_or("No containing link in event!")?;
    let tooltip_handle = link_handle.get_node_by_tag_name("span").ok_or("No tooltip in event!")?;
    let event_type_handle = tooltip_handle.get_node_by_tag_name("strong").ok_or("Could not identify event type!")?;
    let event_type_raw = event_type_handle.get_content().unwrap_or_else(String::new);
    let metadata_handle = tooltip_handle.get_node_by_tag_name("table").ok_or("No event metadata found!")?;

    let tooltip_divs = tooltip_handle.get_nodes_by_tag_name("div");
    if tooltip_divs.len() < 2 {
        return Err("Missing metadata on event!".into());
    }

    // Parse creation/changed info
    let cc_text = tooltip_divs.get(0).unwrap().get_content().ok_or("No creation/changed time found for event!")?;
    let cc_text = cc_text.trim_start().strip_prefix("erstellt am").ok_or("No creation prefix found for event!")?;
    if cc_text.len() < 14 {
        return Err(format!("Invalid creation text on event: {}", cc_text).into());
    }
    let creation_text = cc_text.split_at(14).0;
    let creation = Berlin.datetime_from_str(creation_text, "%d.%m.%y%H:%M")
        .map_err(|err| format!("Failed to parse begin time of event: {:?}", err))?
        .with_timezone(&Utc);

    // Parse begin and end
    let date_time_text = tooltip_divs.get(1).unwrap().get_content().ok_or("Missing datetime in event!")?;
    let mut date_time_split = date_time_text.split(&[' ', '-'][..]);
    date_time_split.next(); // Discard day of week
    let date = date_time_split.next().ok_or("No date in event datetime!")?;
    let begin_time = date_time_split.next().ok_or("No begin time in event datetime!")?;
    let end_time = date_time_split.next().ok_or("No end time in event datetime!")?;

    let begin = Berlin.datetime_from_str(format!("{}{}", date, begin_time).as_str(), "%d.%m.%y%H:%M")
        .map_err(|err| format!("Failed to parse begin time of event: {:?}", err))?
        .with_timezone(&Utc);
    let end = Berlin.datetime_from_str(format!("{}{}", date, end_time).as_str(), "%d.%m.%y%H:%M")
        .map_err(|err| format!("Failed to parse end time of event: {:?}", err))?
        .with_timezone(&Utc);

    // Parse metadata
    let metadata = parse_metadata(metadata_handle);

    let mut courses: Vec<String> = Vec::new();
    let mut locations: Vec<String> = Vec::new();

    // Resources are a comma-separated list, that begins with groups and ends with locations
    if let Some(resources) = metadata.get("Ressourcen") {
        let res_split = resources.split(",");
        lazy_static! {
                static ref GROUP_PATTERN: Regex = Regex::new(r"^[A-Z]{3}-[A-Z0-9 ]+$").unwrap();
            }

        for resource in res_split {
            if GROUP_PATTERN.is_match(resource) {
                courses.push(resource.to_string());
            } else {
                locations.push(resource.to_string());
            }
        }
    }

    let event = Event {
        creation,
        creator: metadata.get("reserviert von").map(|val| val.clone()),
        begin,
        end,
        name: metadata.get("Veranstaltungsname")
            .or_else(|| metadata.get("Titel"))
            .or_else(|| metadata.get("Name"))
            .map_or_else(String::new, |val| val.clone()),
        lecturers: vec![],
        locations,
        courses,
        data: match event_type_raw.as_str() {
            "Lehrveranstaltung" => {
                EventData::Lecture {
                    number: metadata.get("Veranstaltungsnummer").map_or_else(String::new, |val| val.clone()),
                    language: metadata.get("Sprache").map(|val| val.clone()),
                    kind: metadata.get("Veranstaltungsart").map(|val| val.clone()),
                    categories: metadata.get("Veranstaltungskategorie").map_or_else(
                        Vec::new,
                        |categories| categories.split(",").map(|val| String::from(val.trim())).collect()
                    ),
                    total_hours: metadata.get("Soll-Stunden").and_then(|val| val.parse().ok()),
                }
            },
            "Prüfung" => EventData::Exam,
            "Sonstiger Termin" => EventData::Other,
            _ => {
                return Err(format!("Failed to resolve event type: {}", event_type_raw).into());
            },
        }
    };
    Ok(event)
}

fn parse_metadata(metadata_handle: Handle) -> HashMap<String, String> {
    let mut metadata = HashMap::new();
    if let Some(tbody_handle) = metadata_handle.get_node_by_tag_name("tbody") {
        for row_handle in tbody_handle.get_nodes_by_tag_name("tr") {
            let cells = row_handle.get_nodes_by_tag_name("td");
            if cells.len() < 2 {
                continue;
            }
            let key = cells.get(0).unwrap().get_content()
                .map(|key| {
                    if let Some(stripped) = key.strip_suffix(":") {
                        stripped.to_string()
                    } else {
                        key
                    }
                })
                .unwrap_or_else(String::new);
            if let Some(value) = cells.get(1).unwrap().get_content() {
                if value.is_empty() {
                    continue;
                }
                metadata.insert(key, value);
            }
        }
    }
    metadata
}
