use std::{env, io};
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::fs::File;
use std::hash::{Hash, Hasher};
use std::option::Option::Some;

use chrono::{Datelike, TimeZone};
use chrono_tz::Europe::Berlin;
use encoding_rs_io::{DecodeReaderBytesBuilder};
use html5ever::ParseOpts;
use html5ever::tendril::TendrilSink;
use markup5ever_rcdom::{Handle, RcDom};

use crate::util::{Error, EventHash, HandleExtensions};

mod util;

fn main() {
    let args: Vec<String> = env::args().collect();
    if let Some(html_file_name) = args.get(1) {
        let mut file = File::open(html_file_name).expect("Not a valid file given");
        if let Err(e) = process(&mut file) {
            eprintln!("An error occured: {:?}", e);
        }
    } else {
        if let Err(e) = process(&mut io::stdin()) {
            eprintln!("An error occured: {:?}", e);
        }
    }
}

fn process<R>(input_stream: &mut R) -> Result<(), util::Error> where R: io::Read {
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

    println!("BEGIN:VCALENDAR");
    println!("VERSION:2.0");
    println!("PRODID:-//Siphalor//DHiCalnigma//DE");

    let html = document.get_node_by_tag_name("html").expect("Document does not have an html tag!");
    let body = html.get_node_by_tag_name("body").expect("Document does not have a body tag!");
    for handle in body.get_nodes_by_tag_name("div") {
        if let Some(val) = handle.get_attribute_value("class") {
            if val == "calendar" {
                process_month(handle)?;
            }
        }
    }

    println!("END:VCALENDAR");

    Ok(())
}

fn process_month(month_handle: Handle) -> Result<(), util::Error> {
    if let Some(table_handle) = month_handle.get_node_by_tag_name("table") {
        if let Some(tbody_handle) = table_handle.get_node_by_tag_name("tbody") {
            for row_handle in tbody_handle.get_nodes_by_tag_name("tr") {
                for cell_handle in row_handle.get_nodes_by_tag_name("td") {
                    if cell_handle.get_attribute_value("class").map_or(true, |val| val != "month_cell") {
                        continue;
                    }

                    if let Err(e) = process_day(cell_handle) {
                        eprintln!("Error in day: {:?}", e);
                    }
                }
            }
        }
    }

    Ok(())
}

fn process_day(cell_handle: Handle) -> Result<(), Error> {
    let divs = cell_handle.get_nodes_by_tag_name("div");
    if divs.len() < 2 { // The first div is always contains the number of the day
        return Ok(());
    }

    let mut divs = divs.into_iter();
    divs.next();

    for div in divs {
        if div.get_attribute_value("class").map_or(true, |val| val != "month_block") {
            eprintln!("Skipping potential event, class={:?}, content={:?}", div.get_attribute_value("class"), div.get_content());
            continue;
        }

        if let Err(e) = process_event(div) {
            eprintln!("Error in event: {:?}", e);
        }
    }

    Ok(())
}

fn process_event(event_handle: Handle) -> Result<(), Error> {
    let link_handle = event_handle.get_node_by_tag_name("a").ok_or("No containing link in event!")?;
    let tooltip_handle = link_handle.get_node_by_tag_name("span").ok_or("No tooltip in event!")?;
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
        .map_err(|err| format!("Failed to parse begin time of event: {:?}", err))?;

    // Parse begin and end
    let date_time_text = tooltip_divs.get(1).unwrap().get_content().ok_or("Missing datetime in event!")?;
    let mut date_time_split = date_time_text.split(&[' ', '-'][..]);
    date_time_split.next(); // Discard day of week
    let date = date_time_split.next().ok_or("No date in event datetime!")?;
    let begin_time = date_time_split.next().ok_or("No begin time in event datetime!")?;
    let end_time = date_time_split.next().ok_or("No end time in event datetime!")?;

    let begin = Berlin.datetime_from_str(format!("{}{}", date, begin_time).as_str(), "%d.%m.%y%H:%M")
        .map_err(|err| format!("Failed to parse begin time of event: {:?}", err))?;
    let end = Berlin.datetime_from_str(format!("{}{}", date, end_time).as_str(), "%d.%m.%y%H:%M")
        .map_err(|err| format!("Failed to parse end time of event: {:?}", err))?;

    // Parse metadata
    let metadata = parse_metadata(metadata_handle);

    let event_hash = EventHash {
        creation_time: creation.timestamp(),
        creator: metadata.get("reserviert von"),
        event_id: metadata.get("Veranstaltungsnummer"),
        year: begin.year(),
        month: begin.month(),
        day: begin.day()
    };

    println!("BEGIN:VEVENT");
    let mut hasher = DefaultHasher::new();
    event_hash.hash(&mut hasher);
    println!("UID:{}@mosbach.dhbw.de", hasher.finish());
    println!("DTSTAMP:{}00Z", creation.format("%Y%m%dT%H%M"));
    println!("DTSTART:{}00Z", begin.format("%Y%m%dT%H%M"));
    println!("DTEND:{}00Z", end.format("%Y%m%dT%H%M"));

    if let Some(summary) = metadata.get("Veranstaltungsname").or_else(|| metadata.get("Titel")) {
        if let Some(event_type) = metadata.get("Veranstaltungsart") {
            println!("SUMMARY:{} - {}", summary, event_type);
        } else {
            println!("SUMMARY:{}", summary);
        }
    }

    if let Some(resources) = metadata.get("Ressourcen") {
        // Ressourcen = <Kurs>,Raum
        if let Some((_, room)) = resources.split_once(",") {
            println!("LOCATION:{}", room);
        }
    }

    if let Some(organizer) = metadata.get("Personen") {
        println!("ORGANIZER:{}", organizer);
    }

    if let Some(category) = metadata.get("Veranstaltungskategorie") {
        println!("DESCRIPTION:{}", category);
    }

    println!("END:VEVENT");
    Ok(())
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
            let value = cells.get(1).unwrap().get_content().unwrap_or_else(String::new);
            metadata.insert(key, value);
        }
    }
    metadata
}
