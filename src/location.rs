//! Where a posting can be done from, versus where the user is.
//!
//! Job location text is free-form ("Hybrid - San Francisco", "Berlin",
//! "USA Only", "Remote - EMEA", "Richardson, Texas"). We look for country
//! names, major cities, US states and regions, and decide whether the
//! posting is open to the user's country. Text naming nothing we know
//! ("Remote", "", "N/A") is `Unknown`, which callers show rather than hide.

use crate::filter::contains_word;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Fit {
    /// Names the user's country, a region containing it, or "worldwide".
    Open,
    /// Names only places elsewhere.
    Elsewhere,
    /// Names nothing recognisable.
    Unknown,
}

/// (code, display name, lower-case aliases: names, native names, major cities).
const COUNTRIES: &[(&str, &str, &[&str])] = &[
    ("US", "United States", &[
        "united states", "united states of america", "usa", "u.s.a.", "u.s.", "us",
        "new york", "nyc", "new york city", "brooklyn", "manhattan", "san francisco", "sf bay area", "bay area",
        "silicon valley", "los angeles", "seattle", "austin", "boston", "chicago", "denver", "atlanta", "miami",
        "dallas", "houston", "philadelphia", "washington dc", "washington, d.c.", "palo alto", "menlo park",
        "mountain view", "san jose", "san diego", "sunnyvale", "redwood city", "pittsburgh", "phoenix",
        "minneapolis", "detroit", "nashville", "raleigh", "salt lake city", "portland", "baltimore",
        "charlotte", "columbus", "indianapolis", "san antonio", "st. louis", "kansas city", "orlando", "tampa",
        "las vegas", "sacramento", "boulder", "irvine", "cambridge, ma", "plano", "frisco", "richardson",
        "dfw",
        // States (names; two-letter codes are handled separately).
        "alabama", "alaska", "arizona", "arkansas", "california", "colorado", "connecticut", "delaware",
        "florida", "georgia", "hawaii", "idaho", "illinois", "indiana", "iowa", "kansas", "kentucky",
        "louisiana", "maine", "maryland", "massachusetts", "michigan", "minnesota", "mississippi", "missouri",
        "montana", "nebraska", "nevada", "new hampshire", "new jersey", "new mexico", "north carolina",
        "north dakota", "ohio", "oklahoma", "oregon", "pennsylvania", "rhode island", "south carolina",
        "south dakota", "tennessee", "texas", "utah", "vermont", "virginia", "washington", "west virginia",
        "wisconsin", "wyoming", "district of columbia", "puerto rico",
    ]),
    ("CA", "Canada", &["canada", "toronto", "vancouver", "montreal", "montréal", "ottawa", "calgary", "waterloo", "edmonton", "ontario", "quebec", "québec", "british columbia", "alberta"]),
    ("MX", "Mexico", &["mexico", "méxico", "mexico city", "ciudad de méxico", "cdmx", "guadalajara", "monterrey"]),
    ("BR", "Brazil", &["brazil", "brasil", "são paulo", "sao paulo", "rio de janeiro", "belo horizonte"]),
    ("AR", "Argentina", &["argentina", "buenos aires"]),
    ("CO", "Colombia", &["colombia", "bogotá", "bogota", "medellín", "medellin"]),
    ("CL", "Chile", &["chile", "santiago de chile"]),
    ("PE", "Peru", &["peru", "perú", "lima, peru"]),
    ("UY", "Uruguay", &["uruguay", "montevideo"]),
    ("CR", "Costa Rica", &["costa rica", "san josé, costa rica"]),
    ("GB", "United Kingdom", &["united kingdom", "uk", "u.k.", "great britain", "britain", "england", "scotland", "wales", "northern ireland", "london", "manchester", "edinburgh", "glasgow", "oxford", "bristol", "leeds", "cardiff", "belfast", "reading, uk", "maidenhead", "windsor, uk"]),
    ("IE", "Ireland", &["ireland", "dublin", "cork", "galway"]),
    ("DE", "Germany", &["germany", "deutschland", "berlin", "munich", "münchen", "muenchen", "hamburg", "frankfurt", "cologne", "köln", "koeln", "düsseldorf", "dusseldorf", "stuttgart", "leipzig", "dresden", "hanover", "hannover", "nuremberg", "nürnberg", "bremen", "aachen", "karlsruhe", "bavaria", "bayern"]),
    ("FR", "France", &["france", "paris", "lyon", "marseille", "toulouse", "lille", "bordeaux", "nantes", "montpellier", "strasbourg", "île-de-france", "ile-de-france"]),
    ("NL", "Netherlands", &["netherlands", "the netherlands", "nederland", "holland", "amsterdam", "rotterdam", "utrecht", "the hague", "den haag", "eindhoven", "delft", "groningen"]),
    ("CH", "Switzerland", &["switzerland", "schweiz", "suisse", "svizzera", "zurich", "zürich", "geneva", "genève", "geneve", "basel", "lausanne", "bern", "zug", "lugano"]),
    ("BE", "Belgium", &["belgium", "belgië", "belgique", "brussels", "bruxelles", "antwerp", "ghent"]),
    ("LU", "Luxembourg", &["luxembourg"]),
    ("AT", "Austria", &["austria", "österreich", "vienna", "wien", "graz"]),
    ("ES", "Spain", &["spain", "españa", "madrid", "barcelona", "valencia", "seville", "málaga", "malaga"]),
    ("PT", "Portugal", &["portugal", "lisbon", "lisboa", "porto"]),
    ("IT", "Italy", &["italy", "italia", "milan", "milano", "rome", "roma", "turin", "torino"]),
    ("SE", "Sweden", &["sweden", "sverige", "stockholm", "gothenburg", "göteborg", "malmö", "malmo"]),
    ("DK", "Denmark", &["denmark", "danmark", "copenhagen", "københavn", "aarhus"]),
    ("NO", "Norway", &["norway", "norge", "oslo", "bergen"]),
    ("FI", "Finland", &["finland", "suomi", "helsinki", "espoo", "tampere"]),
    ("IS", "Iceland", &["iceland", "reykjavik"]),
    ("PL", "Poland", &["poland", "polska", "warsaw", "warszawa", "kraków", "krakow", "wrocław", "wroclaw", "gdańsk", "gdansk", "poznań", "poznan"]),
    ("CZ", "Czechia", &["czechia", "czech republic", "prague", "praha", "brno"]),
    ("SK", "Slovakia", &["slovakia", "bratislava"]),
    ("HU", "Hungary", &["hungary", "budapest"]),
    ("RO", "Romania", &["romania", "bucharest", "bucurești", "cluj"]),
    ("BG", "Bulgaria", &["bulgaria", "sofia"]),
    ("GR", "Greece", &["greece", "athens"]),
    ("HR", "Croatia", &["croatia", "zagreb"]),
    ("RS", "Serbia", &["serbia", "belgrade"]),
    ("SI", "Slovenia", &["slovenia", "ljubljana"]),
    ("EE", "Estonia", &["estonia", "tallinn"]),
    ("LV", "Latvia", &["latvia", "riga"]),
    ("LT", "Lithuania", &["lithuania", "vilnius"]),
    ("UA", "Ukraine", &["ukraine", "kyiv", "kiev", "lviv"]),
    ("CY", "Cyprus", &["cyprus", "limassol", "nicosia"]),
    ("MT", "Malta", &["malta"]),
    ("TR", "Turkey", &["turkey", "türkiye", "turkiye", "istanbul", "ankara"]),
    ("IL", "Israel", &["israel", "tel aviv", "tel-aviv", "jerusalem", "haifa"]),
    ("AE", "United Arab Emirates", &["united arab emirates", "uae", "dubai", "abu dhabi"]),
    ("SA", "Saudi Arabia", &["saudi arabia", "riyadh", "jeddah"]),
    ("QA", "Qatar", &["qatar", "doha"]),
    ("EG", "Egypt", &["egypt", "cairo"]),
    ("ZA", "South Africa", &["south africa", "cape town", "johannesburg"]),
    ("NG", "Nigeria", &["nigeria", "lagos", "abuja"]),
    ("KE", "Kenya", &["kenya", "nairobi"]),
    ("MA", "Morocco", &["morocco", "casablanca", "rabat"]),
    ("IN", "India", &["india", "bangalore", "bengaluru", "hyderabad", "mumbai", "pune", "delhi", "new delhi", "chennai", "gurgaon", "gurugram", "noida", "kolkata", "ahmedabad"]),
    ("PK", "Pakistan", &["pakistan", "karachi", "lahore", "islamabad"]),
    ("BD", "Bangladesh", &["bangladesh", "dhaka"]),
    ("SG", "Singapore", &["singapore"]),
    ("MY", "Malaysia", &["malaysia", "kuala lumpur"]),
    ("ID", "Indonesia", &["indonesia", "jakarta"]),
    ("TH", "Thailand", &["thailand", "bangkok"]),
    ("VN", "Vietnam", &["vietnam", "viet nam", "ho chi minh", "hanoi"]),
    ("PH", "Philippines", &["philippines", "manila", "cebu"]),
    ("JP", "Japan", &["japan", "tokyo", "osaka"]),
    ("KR", "South Korea", &["south korea", "korea", "seoul"]),
    ("CN", "China", &["china", "beijing", "shanghai", "shenzhen", "hangzhou"]),
    ("HK", "Hong Kong", &["hong kong"]),
    ("TW", "Taiwan", &["taiwan", "taipei"]),
    ("AU", "Australia", &["australia", "sydney", "melbourne", "brisbane", "perth, australia", "adelaide", "canberra"]),
    ("NZ", "New Zealand", &["new zealand", "auckland", "wellington"]),
];

const EUROPE: &[&str] = &["GB", "IE", "DE", "FR", "NL", "CH", "BE", "LU", "AT", "ES", "PT", "IT", "SE", "DK", "NO", "FI", "IS", "PL", "CZ", "SK", "HU", "RO", "BG", "GR", "HR", "RS", "SI", "EE", "LV", "LT", "UA", "CY", "MT"];
const LATAM: &[&str] = &["MX", "BR", "AR", "CO", "CL", "PE", "UY", "CR"];
const MIDDLE_EAST: &[&str] = &["TR", "IL", "AE", "SA", "QA", "EG"];
const AFRICA: &[&str] = &["EG", "ZA", "NG", "KE", "MA"];
const APAC: &[&str] = &["IN", "PK", "BD", "SG", "MY", "ID", "TH", "VN", "PH", "JP", "KR", "CN", "HK", "TW", "AU", "NZ"];

/// Region aliases -> member countries. `None` = anywhere.
fn regions() -> Vec<(&'static [&'static str], Option<Vec<&'static str>>)> {
    let cat = |parts: &[&[&'static str]]| Some(parts.iter().flat_map(|p| p.iter().copied()).collect::<Vec<_>>());
    vec![
        (&["anywhere", "anywhere in the world", "worldwide", "world wide", "global", "globally", "international", "any location", "all locations", "everywhere"][..], None),
        (&["north america", "n. america"][..], Some(vec!["US", "CA", "MX"])),
        (&["americas", "the americas"][..], cat(&[&["US", "CA"], LATAM])),
        (&["latam", "latin america", "south america", "central america"][..], cat(&[LATAM])),
        (&["europe", "european union", "eu", "eea"][..], cat(&[EUROPE])),
        (&["emea"][..], cat(&[EUROPE, MIDDLE_EAST, AFRICA])),
        (&["dach"][..], Some(vec!["DE", "AT", "CH"])),
        (&["nordics", "nordic", "scandinavia"][..], Some(vec!["SE", "DK", "NO", "FI", "IS"])),
        (&["benelux"][..], Some(vec!["BE", "NL", "LU"])),
        (&["uk&i", "uki"][..], Some(vec!["GB", "IE"])),
        (&["middle east", "mena"][..], cat(&[MIDDLE_EAST])),
        (&["africa"][..], cat(&[AFRICA])),
        (&["apac", "asia", "asia pacific", "asia-pacific", "anz", "oceania"][..], cat(&[APAC])),
    ]
}

const US_STATE_CODES: &[&str] = &[
    "AL", "AK", "AZ", "AR", "CA", "CO", "CT", "DE", "FL", "GA", "HI", "ID", "IL", "IN", "IA", "KS", "KY", "LA", "ME", "MD",
    "MA", "MI", "MN", "MS", "MO", "MT", "NE", "NV", "NH", "NJ", "NM", "NY", "NC", "ND", "OH", "OK", "OR", "PA", "RI", "SC",
    "SD", "TN", "TX", "UT", "VT", "VA", "WA", "WV", "WI", "WY", "DC",
];

/// Country code for a user-typed place ("United States", "usa", "Texas",
/// "Berlin", "DE"), if recognisable.
pub fn resolve_country(place: &str) -> Option<&'static str> {
    let p = place.trim();
    let up = p.to_uppercase();
    if let Some((code, _, _)) = COUNTRIES.iter().find(|(c, _, _)| *c == up) {
        return Some(code);
    }
    let found = countries_in(p);
    // "Dallas, TX, US" names one country; a list of several is ambiguous.
    if found.len() == 1 {
        return Some(found[0]);
    }
    None
}

pub fn country_name(code: &str) -> &'static str {
    COUNTRIES.iter().find(|(c, _, _)| *c == code).map(|(_, n, _)| *n).unwrap_or("")
}

/// Countries named in `text`, longest aliases first so "new mexico" is the
/// US and not Mexico, and "paris, france" counts once.
fn countries_in(text: &str) -> Vec<&'static str> {
    countries_in_opt(text, true)
}

fn countries_in_opt(text: &str, us_state_codes: bool) -> Vec<&'static str> {
    let mut hay = format!(" {} ", text.to_lowercase().replace(['-', '_', '|', '/', '·', '(', ')', ';', '•'], " "));
    let mut aliases: Vec<(&str, &str)> = COUNTRIES.iter().flat_map(|(code, _, al)| al.iter().map(move |a| (*code, *a))).collect();
    aliases.sort_by_key(|(_, a)| std::cmp::Reverse(a.len()));
    let mut found: Vec<&'static str> = Vec::new();
    for (code, alias) in aliases {
        if contains_word(&hay, alias) {
            if !found.contains(&code) {
                found.push(code);
            }
            hay = blank(&hay, alias);
        }
    }
    // Two-letter US state codes: only as their own segment ("Austin, TX",
    // "Remote - NY"), and only when no other country is named: "IN-Bengaluru"
    // is India and "Darmstadt, DE" is Germany, not Indiana or Delaware.
    if us_state_codes && found.is_empty() {
        for seg in text.split([',', '-', '/', '|', '(', ')', ';', '·']) {
            let seg = seg.trim();
            if seg.len() == 2 && seg.chars().all(|c| c.is_ascii_uppercase()) && US_STATE_CODES.contains(&seg) {
                found.push("US");
                break;
            }
        }
    }
    found
}

fn blank(hay: &str, word: &str) -> String {
    let mut out = hay.to_string();
    while let Some(pos) = find_word(&out, word) {
        out.replace_range(pos..pos + word.len(), &" ".repeat(word.len()));
    }
    out
}

fn find_word(hay: &str, word: &str) -> Option<usize> {
    let mut from = 0;
    while let Some(p) = hay[from..].find(word) {
        let s = from + p;
        let e = s + word.len();
        let ok_b = hay[..s].chars().next_back().map(|c| !c.is_alphanumeric()).unwrap_or(true);
        let ok_a = hay[e..].chars().next().map(|c| !c.is_alphanumeric()).unwrap_or(true);
        if ok_b && ok_a {
            return Some(s);
        }
        from = s + hay[s..].chars().next().map(|c| c.len_utf8()).unwrap_or(1);
    }
    None
}

/// Does a posting located at `text` fit a user in `country`?
pub fn fit(text: &str, country: &str) -> Fit {
    fit_opt(text, country, true)
}

/// Fit for a posting from a board that covers one region only: a location
/// naming nothing we know is taken to be in that region, and US state codes
/// are not read (a European board's "Darmstadt, DE" means Germany).
pub fn fit_regional(text: &str, country: &str, region: &[&str]) -> Fit {
    match fit_opt(text, country, false) {
        Fit::Unknown if region.contains(&country) => Fit::Open,
        Fit::Unknown => Fit::Elsewhere,
        f => f,
    }
}

pub const EUROPE_REGION: &[&str] = EUROPE;

fn fit_opt(text: &str, country: &str, us_state_codes: bool) -> Fit {
    let lower = format!(" {} ", text.to_lowercase().replace(['-', '_', '|', '/', '·', '(', ')', ';'], " "));
    let mut named_any = false;
    // "Anywhere in France" is France, not the world.
    let scoped_anywhere = lower.contains("anywhere in ") && !lower.contains("anywhere in the world");
    for (aliases, members) in regions() {
        if aliases.iter().any(|a| contains_word(&lower, a) && !(scoped_anywhere && *a == "anywhere")) {
            named_any = true;
            match members {
                None => return Fit::Open,
                Some(m) if m.contains(&country) => return Fit::Open,
                _ => {}
            }
        }
    }
    let countries = countries_in_opt(text, us_state_codes);
    if countries.contains(&country) {
        return Fit::Open;
    }
    if named_any || !countries.is_empty() {
        Fit::Elsewhere
    } else {
        Fit::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn us_user() {
        for open in [
            "New York, NY", "Hybrid - San Francisco, New York City, Austin", "Richardson, Texas", "US-Remote",
            "USA Only", "US / Canada", "United States", "Remote - US", "Chicago, IL", "Anywhere in the World",
            "Worldwide", "North America Only", "LATAM, Europe, USA, Canada, APAC", "Hybrid - London, New York",
            "Austin, TX", "Albuquerque, New Mexico", "Remote (Americas)",
        ] {
            assert_eq!(fit(open, "US"), Fit::Open, "{open}");
        }
        for away in [
            "Berlin", "London", "9th arrondissement of Paris, 75009, Paris", "Zürich", "Toronto, Canada", "EMEA Only",
            "Asia Only", "Bengaluru", "Hybrid - London, Berlin", "Mexico City", "Canada Only", "Germany (Weimar)",
            "Amsterdam , Noord-Holland, Nederland", "Remote - EMEA", "Europe Only", "Sydney Or Melbourne",
        ] {
            assert_eq!(fit(away, "US"), Fit::Elsewhere, "{away}");
        }
        for unclear in ["", "Remote", "N/A", "Remote job", "Hybrid", "Any State"] {
            assert_eq!(fit(unclear, "US"), Fit::Unknown, "{unclear}");
        }
    }

    #[test]
    fn other_users() {
        assert_eq!(fit("Berlin", "DE"), Fit::Open);
        assert_eq!(fit("Remote - EMEA", "DE"), Fit::Open);
        assert_eq!(fit("USA Only", "DE"), Fit::Elsewhere);
        assert_eq!(fit("Toronto", "CA"), Fit::Open);
        // A Canadian province code is not a US state code.
        assert_eq!(fit("Toronto, ON", "US"), Fit::Elsewhere);
    }

    #[test]
    fn ambiguous_codes() {
        assert_eq!(fit("IN-Bengaluru", "US"), Fit::Elsewhere);
        assert_eq!(fit("Dover, DE", "US"), Fit::Open);
        // On a European board, DE is Germany and an unknown town is Europe.
        assert_eq!(fit_regional("Darmstadt, DE", "US", EUROPE_REGION), Fit::Elsewhere);
        assert_eq!(fit_regional("Magdeburg", "US", EUROPE_REGION), Fit::Elsewhere);
        assert_eq!(fit_regional("Magdeburg", "DE", EUROPE_REGION), Fit::Open);
        assert_eq!(fit_regional("Remote job", "FR", EUROPE_REGION), Fit::Open);
        assert_eq!(fit_regional("Anywhere in the World", "US", EUROPE_REGION), Fit::Open);
        assert_eq!(fit("Anywhere in France", "US"), Fit::Elsewhere);
        assert_eq!(fit("Anywhere in the US", "US"), Fit::Open);
    }

    #[test]
    fn resolving_places() {
        assert_eq!(resolve_country("United States"), Some("US"));
        assert_eq!(resolve_country("usa"), Some("US"));
        assert_eq!(resolve_country("Texas"), Some("US"));
        assert_eq!(resolve_country("Dallas, TX"), Some("US"));
        assert_eq!(resolve_country("de"), Some("DE"));
        assert_eq!(resolve_country("Berlin"), Some("DE"));
        assert_eq!(resolve_country("Narnia"), None);
        assert_eq!(country_name("US"), "United States");
    }
}
