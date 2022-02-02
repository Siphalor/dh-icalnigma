use std::fmt::Write;
use std::io;
use chrono::Utc;

use crate::model::{Event, EventData};

const ICAL_DATETIME_FORMAT: &str = "%Y%m%dT%H%M";

pub fn write_calendar<W: io::Write>(write: &mut W, events: &Vec<Event>) {
    write!(write, "BEGIN:VCALENDAR\r\n").ok();
    write!(write, "VERSION:2.0\r\n").ok();
    write!(write, "PRODID:-//Siphalor//DHiCalnigma//DE\r\n").ok();
    write!(write, "X-ICALNIGMA-TIME:{}\r\n", Utc::now().format("%d.%m.%Y %H:%M")).ok();

    for event in events {
        write_lecture(write, event);
    }
    write!(write, "END:VCALENDAR\r\n").ok();
}

pub fn write_lecture<W: io::Write>(write: &mut W, event: &Event) {

    write!(write, "BEGIN:VEVENT\r\n").ok();
    write_ical_field(write, "UID", format!("{}@icalnigma", event.hash()));
    if let Some(creation) = event.creation {
        write!(write, "CREATED:{}00Z\r\n", creation.format(ICAL_DATETIME_FORMAT)).ok();
    }
    write!(write, "DTSTART:{}00Z\r\n", event.begin.format(ICAL_DATETIME_FORMAT)).ok();
    write!(write, "DTEND:{}00Z\r\n", event.end.format(ICAL_DATETIME_FORMAT)).ok();
    write!(write, "SUMMARY:{}\r\n", event.title()).ok();

    if !event.locations.is_empty() {
        write_ical_field(write, "LOCATION", event.locations.join(", "));
    }

    let mut description = String::new();

    if let EventData::Lecture{categories, language, total_hours, ..} = &event.data {
        if !categories.is_empty() {
            write!(description, "{}\\n\\n", categories.join(", ")).ok();
            write_ical_line(write, format!("CATEGORIES:{}", categories.join(",")).as_str());
        }

        if let Some(language) = language {
            write!(description, "Sprache: {}\\n", language).ok();
        }

        if let Some(total_hours) = total_hours {
            write!(description, "Insgesamte Stunden: {}\\n", total_hours).ok();
        }
    }

    if !event.lecturers.is_empty() {
        write_ical_line(write, format!(r#"ORGANIZER;CN="{}":noreply@siphalor.de"#, event.lecturers.first().unwrap().name).as_str());

        write!(
            description, "Dozent:innen: {}\\n",
            event.lecturers.iter().map(|l| l.name.as_str()).collect::<Vec<&str>>().join(", ")
        ).ok();
        for lecturer in &event.lecturers {
            write_ical_line(write, format!(r#"ATTENDEE;CN="{}":noreply@siphalor.de"#, lecturer.name).as_str());
        }
    } else {
        description.push_str("Dozent:innen sind aufgrund von Datenschutzbedenken der DHBW nicht mehr öffentlich!")
    }

    for course in &event.courses {
        write_ical_line(write, format!(r#"ATTENDEE;CN="{}":noreply@siphalor.de"#, course).as_str());
    }

    write_ical_field(write, "DESCRIPTION", description);
    write!(write, "END:VEVENT\r\n").ok();
}

pub fn write_ical_field<W, K, V>(output: &mut W, key: K, value: V)
    where W: io::Write, K: Into<String>, V: Into<String> {
    let key = key.into();
    let value = value.into().replace(",", "\\,");
    let line = format!("{}:{}", &key, &value);
    write_ical_line(output, line.as_str());
}

pub fn write_ical_line<W>(output: &mut W, line: &str) where W: io::Write {
    let mut line_rest = line;

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
