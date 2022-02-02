use std::io;
use std::fs::{File, OpenOptions};
use std::num::ParseIntError;
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
use crate::util::{Day, Error, get_month_from_german, HandleExtensions, Month, Year};

mod util;
mod model;
mod icalendar;
mod archive;

#[derive(Parser)]
#[clap(
    version = "0.3.2",
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

    if let Some(heading_handle) = month_handle.get_node_by_tag_name("h2") {
        if let Some(heading_text) = heading_handle.get_content() {
            let mut parts = heading_text.split_ascii_whitespace();
            let month: Month = get_month_from_german(parts.next().ok_or("Invalid month heading text (empty)")?)?;
            let year: Year = parts.next().ok_or("Invalid month heading text (year is missing!)")?
                .parse().map_err(|err: ParseIntError| err.to_string())?;

            if let Some(table_handle) = month_handle.get_node_by_tag_name("table") {
                if let Some(tbody_handle) = table_handle.get_node_by_tag_name("tbody") {
                    for row_handle in tbody_handle.get_nodes_by_tag_name("tr") {
                        for cell_handle in row_handle.get_nodes_by_tag_name("td") {
                            if cell_handle.get_attribute_value("class").map_or(true, |val| val != "month_cell") {
                                continue;
                            }

                            match load_day(cell_handle, year, month) {
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
        }
    }

    if events.is_empty() {
        return Ok(None)
    }
    return Ok(Some(( events.first().unwrap().end.format("%Y%m").to_string(), events )))
}

fn load_day(cell_handle: Handle, year: Year, month: Month) -> Result<Option<Vec<Event>>, Error> {
    let divs = cell_handle.get_nodes_by_tag_name("div");
    if divs.len() < 2 { // The first div always contains the number of the day
        return Ok(None);
    }

    let mut divs = divs.into_iter();
    let day: Day = divs.next().unwrap().get_content()
        .ok_or("No day number found!")?
        .parse().map_err(|err| format!("Failed to parse day number: {}", err))?;

    let mut events = Vec::with_capacity(divs.len());
    for div in divs {
        if div.get_attribute_value("class").map_or(true, |val| val != "month_block") {
            eprintln!("Skipping potential event, class={:?}, content={:?}", div.get_attribute_value("class"), div.get_content());
            continue;
        }

        match process_event(div, year, month, day) {
            Ok(event) => events.push(event),
            Err(error) => eprintln!("Error in event: {:?}", error),
        }
    }

    Ok(Some(events))
}

fn process_event(event_handle: Handle, year: Year, month: Month, day: Day) -> Result<Event, Error> {
    let link_handle = event_handle.get_node_by_tag_name("a").ok_or("No containing link in event!")?;
    let mut title_lines = link_handle.get_text_nodes().into_iter();

    if let Some(metadata_line) = title_lines.next() {
        lazy_static!{
            static ref TIME_PATTERN: Regex = Regex::new(r"^(\d{1,2}):(\d{1,2})\s*-\s*(\d{1,2}):(\d{1,2})").unwrap();
        }
        if let Some(captures) = TIME_PATTERN.captures(metadata_line.as_str()) {
            let metadata_rest: &str = &metadata_line[captures.get(0).unwrap().end()..];
            let date: chrono::Date<chrono_tz::Tz> = Berlin.ymd(year, month, day);
            let begin = date.and_hms(
                captures.get(1).unwrap().as_str().parse().unwrap(),
                captures.get(2).unwrap().as_str().parse().unwrap(),
                0
            ).with_timezone(&Utc);
            let end = date.and_hms(
                captures.get(3).unwrap().as_str().parse().unwrap(),
                captures.get(4).unwrap().as_str().parse().unwrap(),
                0
            ).with_timezone(&Utc);


            let mut courses: Vec<String> = Vec::new();
            let mut locations: Vec<String> = Vec::new();
            for resource in metadata_rest.split(",") {
                lazy_static! {
                    static ref COURSE_PATTERN: Regex = Regex::new(r"^[A-Z]{3}-[A-Z0-9 ]+$").unwrap();
                }
                let trimmed = resource.trim();
                if COURSE_PATTERN.is_match(trimmed) {
                    courses.push(trimmed.to_string());
                } else {
                    locations.push(trimmed.to_string());
                }
            };

            Ok(Event {
                creation: None,
                creator: None,
                begin,
                end,
                name: title_lines.next().unwrap_or_else(|| "missingno".to_string()),
                lecturers: vec![],
                locations,
                courses,
                data: EventData::Exam
            })
        } else {
            Err("Failed to parse event metadata!".into())
        }
    } else {
        Err("Encountered empty event!".into())
    }
}
