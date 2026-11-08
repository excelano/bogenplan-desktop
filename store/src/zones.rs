// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use chrono_tz::Tz;

#[derive(Debug, Clone, Default)]
pub(crate) struct ZoneHint {
    pub tzid: String,
    pub location: Option<String>,
    pub standard_offset_s: Option<i32>,
}

pub(crate) fn resolve(tzid: &str, hints: &[ZoneHint]) -> Tz {
    let tzid = tzid.trim_matches('"');
    if let Some(zone) = iana(tzid) {
        return zone;
    }
    if let Some(zone) = WINDOWS_TO_IANA
        .iter()
        .find(|(windows, _)| *windows == tzid)
        .and_then(|(_, name)| iana(name))
    {
        return zone;
    }
    if let Some(zone) = gmt_offset(tzid) {
        return zone;
    }
    let hint = hints
        .iter()
        .find(|hint| hint.tzid.trim_matches('"') == tzid);
    if let Some(zone) = hint
        .and_then(|hint| hint.location.as_deref())
        .and_then(iana)
    {
        return zone;
    }
    hint.and_then(|hint| hint.standard_offset_s)
        .and_then(whole_hour_zone)
        .unwrap_or(Tz::UTC)
}

fn iana(name: &str) -> Option<Tz> {
    if let Ok(zone) = name.parse() {
        return Some(zone);
    }
    name.match_indices('/')
        .find_map(|(i, _)| name[i + 1..].parse().ok())
}

fn gmt_offset(name: &str) -> Option<Tz> {
    let rest = name
        .strip_prefix("GMT")
        .or_else(|| name.strip_prefix("UTC"))?;
    let rest = rest.split_whitespace().next().unwrap_or("");
    if rest.is_empty() {
        return Some(Tz::UTC);
    }
    let (sign, digits) = match rest.as_bytes()[0] {
        b'+' => (1, &rest[1..]),
        b'-' => (-1, &rest[1..]),
        _ => return None,
    };
    let digits: String = digits.chars().filter(char::is_ascii_digit).collect();
    let (hours, minutes) = match digits.len() {
        1 | 2 => (digits.parse::<i32>().ok()?, 0),
        3 | 4 => {
            let split = digits.len() - 2;
            (
                digits[..split].parse().ok()?,
                digits[split..].parse::<i32>().ok()?,
            )
        }
        _ => return None,
    };
    whole_hour_zone(sign * (hours * 3600 + minutes * 60))
}

pub(crate) fn whole_hour_zone(offset_east_s: i32) -> Option<Tz> {
    if offset_east_s % 3600 != 0 {
        return None;
    }
    let posix_hours = -offset_east_s / 3600;
    let name = match posix_hours {
        0 => "Etc/GMT".to_string(),
        h if h > 0 => format!("Etc/GMT+{h}"),
        h => format!("Etc/GMT{h}"),
    };
    name.parse().ok()
}

const WINDOWS_TO_IANA: &[(&str, &str)] = &[
    ("Dateline Standard Time", "Etc/GMT+12"),
    ("UTC-11", "Etc/GMT+11"),
    ("Aleutian Standard Time", "America/Adak"),
    ("Hawaiian Standard Time", "Pacific/Honolulu"),
    ("Marquesas Standard Time", "Pacific/Marquesas"),
    ("Alaskan Standard Time", "America/Anchorage"),
    ("UTC-09", "Etc/GMT+9"),
    ("Pacific Standard Time (Mexico)", "America/Tijuana"),
    ("UTC-08", "Etc/GMT+8"),
    ("Pacific Standard Time", "America/Los_Angeles"),
    ("US Mountain Standard Time", "America/Phoenix"),
    ("Mountain Standard Time (Mexico)", "America/Mazatlan"),
    ("Mountain Standard Time", "America/Denver"),
    ("Yukon Standard Time", "America/Whitehorse"),
    ("Central America Standard Time", "America/Guatemala"),
    ("Central Standard Time", "America/Chicago"),
    ("Easter Island Standard Time", "Pacific/Easter"),
    ("Central Standard Time (Mexico)", "America/Mexico_City"),
    ("Canada Central Standard Time", "America/Regina"),
    ("SA Pacific Standard Time", "America/Bogota"),
    ("Eastern Standard Time (Mexico)", "America/Cancun"),
    ("Eastern Standard Time", "America/New_York"),
    ("Haiti Standard Time", "America/Port-au-Prince"),
    ("Cuba Standard Time", "America/Havana"),
    ("US Eastern Standard Time", "America/Indianapolis"),
    ("Turks And Caicos Standard Time", "America/Grand_Turk"),
    ("Paraguay Standard Time", "America/Asuncion"),
    ("Atlantic Standard Time", "America/Halifax"),
    ("Venezuela Standard Time", "America/Caracas"),
    ("Central Brazilian Standard Time", "America/Cuiaba"),
    ("SA Western Standard Time", "America/La_Paz"),
    ("Pacific SA Standard Time", "America/Santiago"),
    ("Newfoundland Standard Time", "America/St_Johns"),
    ("Tocantins Standard Time", "America/Araguaina"),
    ("E. South America Standard Time", "America/Sao_Paulo"),
    ("SA Eastern Standard Time", "America/Cayenne"),
    ("Argentina Standard Time", "America/Buenos_Aires"),
    ("Greenland Standard Time", "America/Godthab"),
    ("Montevideo Standard Time", "America/Montevideo"),
    ("Magallanes Standard Time", "America/Punta_Arenas"),
    ("Saint Pierre Standard Time", "America/Miquelon"),
    ("Bahia Standard Time", "America/Bahia"),
    ("UTC-02", "Etc/GMT+2"),
    ("Azores Standard Time", "Atlantic/Azores"),
    ("Cape Verde Standard Time", "Atlantic/Cape_Verde"),
    ("UTC", "Etc/UTC"),
    ("GMT Standard Time", "Europe/London"),
    ("Greenwich Standard Time", "Atlantic/Reykjavik"),
    ("Sao Tome Standard Time", "Africa/Sao_Tome"),
    ("Morocco Standard Time", "Africa/Casablanca"),
    ("W. Europe Standard Time", "Europe/Berlin"),
    ("Central Europe Standard Time", "Europe/Budapest"),
    ("Romance Standard Time", "Europe/Paris"),
    ("Central European Standard Time", "Europe/Warsaw"),
    ("W. Central Africa Standard Time", "Africa/Lagos"),
    ("Jordan Standard Time", "Asia/Amman"),
    ("GTB Standard Time", "Europe/Bucharest"),
    ("Middle East Standard Time", "Asia/Beirut"),
    ("Egypt Standard Time", "Africa/Cairo"),
    ("E. Europe Standard Time", "Europe/Chisinau"),
    ("Syria Standard Time", "Asia/Damascus"),
    ("West Bank Standard Time", "Asia/Hebron"),
    ("South Africa Standard Time", "Africa/Johannesburg"),
    ("FLE Standard Time", "Europe/Kiev"),
    ("Israel Standard Time", "Asia/Jerusalem"),
    ("South Sudan Standard Time", "Africa/Juba"),
    ("Kaliningrad Standard Time", "Europe/Kaliningrad"),
    ("Sudan Standard Time", "Africa/Khartoum"),
    ("Libya Standard Time", "Africa/Tripoli"),
    ("Namibia Standard Time", "Africa/Windhoek"),
    ("Arabic Standard Time", "Asia/Baghdad"),
    ("Turkey Standard Time", "Europe/Istanbul"),
    ("Arab Standard Time", "Asia/Riyadh"),
    ("Belarus Standard Time", "Europe/Minsk"),
    ("Russian Standard Time", "Europe/Moscow"),
    ("E. Africa Standard Time", "Africa/Nairobi"),
    ("Volgograd Standard Time", "Europe/Volgograd"),
    ("Iran Standard Time", "Asia/Tehran"),
    ("Arabian Standard Time", "Asia/Dubai"),
    ("Astrakhan Standard Time", "Europe/Astrakhan"),
    ("Azerbaijan Standard Time", "Asia/Baku"),
    ("Russia Time Zone 3", "Europe/Samara"),
    ("Mauritius Standard Time", "Indian/Mauritius"),
    ("Saratov Standard Time", "Europe/Saratov"),
    ("Georgian Standard Time", "Asia/Tbilisi"),
    ("Caucasus Standard Time", "Asia/Yerevan"),
    ("Afghanistan Standard Time", "Asia/Kabul"),
    ("West Asia Standard Time", "Asia/Tashkent"),
    ("Ekaterinburg Standard Time", "Asia/Yekaterinburg"),
    ("Pakistan Standard Time", "Asia/Karachi"),
    ("Qyzylorda Standard Time", "Asia/Qyzylorda"),
    ("India Standard Time", "Asia/Calcutta"),
    ("Sri Lanka Standard Time", "Asia/Colombo"),
    ("Nepal Standard Time", "Asia/Katmandu"),
    ("Central Asia Standard Time", "Asia/Almaty"),
    ("Bangladesh Standard Time", "Asia/Dhaka"),
    ("Omsk Standard Time", "Asia/Omsk"),
    ("Myanmar Standard Time", "Asia/Rangoon"),
    ("SE Asia Standard Time", "Asia/Bangkok"),
    ("Altai Standard Time", "Asia/Barnaul"),
    ("W. Mongolia Standard Time", "Asia/Hovd"),
    ("North Asia Standard Time", "Asia/Krasnoyarsk"),
    ("N. Central Asia Standard Time", "Asia/Novosibirsk"),
    ("Tomsk Standard Time", "Asia/Tomsk"),
    ("China Standard Time", "Asia/Shanghai"),
    ("North Asia East Standard Time", "Asia/Irkutsk"),
    ("Singapore Standard Time", "Asia/Singapore"),
    ("W. Australia Standard Time", "Australia/Perth"),
    ("Taipei Standard Time", "Asia/Taipei"),
    ("Ulaanbaatar Standard Time", "Asia/Ulaanbaatar"),
    ("Aus Central W. Standard Time", "Australia/Eucla"),
    ("Transbaikal Standard Time", "Asia/Chita"),
    ("Tokyo Standard Time", "Asia/Tokyo"),
    ("North Korea Standard Time", "Asia/Pyongyang"),
    ("Korea Standard Time", "Asia/Seoul"),
    ("Yakutsk Standard Time", "Asia/Yakutsk"),
    ("Cen. Australia Standard Time", "Australia/Adelaide"),
    ("AUS Central Standard Time", "Australia/Darwin"),
    ("E. Australia Standard Time", "Australia/Brisbane"),
    ("AUS Eastern Standard Time", "Australia/Sydney"),
    ("West Pacific Standard Time", "Pacific/Port_Moresby"),
    ("Tasmania Standard Time", "Australia/Hobart"),
    ("Vladivostok Standard Time", "Asia/Vladivostok"),
    ("Lord Howe Standard Time", "Australia/Lord_Howe"),
    ("Bougainville Standard Time", "Pacific/Bougainville"),
    ("Russia Time Zone 10", "Asia/Srednekolymsk"),
    ("Magadan Standard Time", "Asia/Magadan"),
    ("Norfolk Standard Time", "Pacific/Norfolk"),
    ("Sakhalin Standard Time", "Asia/Sakhalin"),
    ("Central Pacific Standard Time", "Pacific/Guadalcanal"),
    ("Russia Time Zone 11", "Asia/Kamchatka"),
    ("New Zealand Standard Time", "Pacific/Auckland"),
    ("UTC+12", "Etc/GMT-12"),
    ("Fiji Standard Time", "Pacific/Fiji"),
    ("Chatham Islands Standard Time", "Pacific/Chatham"),
    ("UTC+13", "Etc/GMT-13"),
    ("Tonga Standard Time", "Pacific/Tongatapu"),
    ("Samoa Standard Time", "Pacific/Apia"),
    ("Line Islands Standard Time", "Pacific/Kiritimati"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_resolve() {
        assert_eq!(resolve("America/Chicago", &[]), Tz::America__Chicago);
        assert_eq!(resolve("US/Central", &[]), Tz::US__Central);
        assert_eq!(resolve("Central Standard Time", &[]), Tz::America__Chicago);
        assert_eq!(
            resolve("/mozilla.org/20050126_1/America/New_York", &[]),
            Tz::America__New_York
        );
        assert_eq!(resolve("\"Europe/Berlin\"", &[]), Tz::Europe__Berlin);
    }

    #[test]
    fn offsets_resolve_to_etc_zones() {
        assert_eq!(resolve("GMT-0500", &[]), Tz::Etc__GMTPlus5);
        assert_eq!(resolve("GMT+0100", &[]), Tz::Etc__GMTMinus1);
        assert_eq!(resolve("UTC", &[]), Tz::UTC);
    }

    #[test]
    fn the_resource_decides_an_unknown_name() {
        let hints = [
            ZoneHint {
                tzid: "Custom 1".into(),
                location: Some("Asia/Tokyo".into()),
                standard_offset_s: None,
            },
            ZoneHint {
                tzid: "Custom 2".into(),
                location: None,
                standard_offset_s: Some(-6 * 3600),
            },
        ];
        assert_eq!(resolve("Custom 1", &hints), Tz::Asia__Tokyo);
        assert_eq!(resolve("Custom 2", &hints), Tz::Etc__GMTPlus6);
        assert_eq!(resolve("Custom 3", &hints), Tz::UTC);
    }
}
