//! Turns the registry's network descriptions into names people recognise.
//!
//! The data mixes two styles. American and Asian registries write
//! `HANDLE - Organisation`, e.g. "CLOUDFLARENET - Cloudflare, Inc.". European
//! ones write `HANDLE Organisation` with no dash, e.g. "DTAG Deutsche Telekom AG".
//! The handle is a short technical label; the organisation is what a person
//! wants to read. Some descriptions are only a handle ("LEVEL3"), only a name
//! ("Telia Company AB"), or a handle and a street address; those keep the
//! handle.

/// The friendly owner name for a network description.
///
/// `country` is the network's two-letter country, used to drop a trailing
/// ", US"-style suffix that repeats it.
pub fn friendly_name(description: &str, country: Option<&str>) -> String {
    let description = tidy(description);
    // "Angola Telecom - Angola Telecom": African registry names come twice
    // and may hold " - " themselves ("Spacetel - Guine-Bissau"), so the half
    // is kept as it is.
    if let Some(half) = repeated_half(&description) {
        return strip_country_suffix(half, country).to_owned();
    }
    let name = match description.split_once(" - ") {
        Some((handle, rest)) => {
            let rest = rest.trim();
            if looks_like_name(rest) {
                rest.to_owned()
            } else {
                tidy_handle(handle.trim(), country)
            }
        }
        None => strip_leading_handle(&description).to_owned(),
    };
    let name = strip_country_suffix(&name, country);
    if name.is_empty() { description } else { name.to_owned() }
}

/// Collapses runs of white space and trims the ends, along with stray dashes
/// as in "-Reserved AS-".
fn tidy(text: &str) -> String {
    let words = text.split_whitespace().collect::<Vec<_>>().join(" ");
    words.trim_matches(|c: char| c == '-' || c.is_whitespace()).to_owned()
}

/// "X - X" gives "X".
fn repeated_half(text: &str) -> Option<&str> {
    let half = text.len().checked_sub(3)? / 2;
    let (first, second) = (text.get(..half)?, text.get(half..)?);
    (!first.is_empty() && second.strip_prefix(" - ") == Some(first)).then_some(first)
}

/// Drops ", US" from the end when it repeats the network's country.
fn strip_country_suffix<'a>(name: &'a str, country: Option<&str>) -> &'a str {
    let Some(country) = country else { return name };
    match name.rsplit_once(", ") {
        Some((head, tail)) if tail.eq_ignore_ascii_case(country) && !head.trim().is_empty() => {
            head.trim_end_matches([',', ' '])
        }
        _ => name,
    }
}

/// Words that are a company form, not a name: "Corporation" alone is no name.
const LEGAL_FORMS: &[&str] = &[
    "ab", "ag", "as", "bv", "co", "corp", "corporation", "company", "doo", "gmbh", "inc", "kft", "kg", "limited",
    "llc", "lp", "ltd", "nv", "oy", "oyj", "plc", "pte", "pty", "sa", "sarl", "sas", "se", "sl", "spa", "srl", "sro",
];

fn is_legal_form(word: &str) -> bool {
    let word: String = word.chars().filter(|c| c.is_alphanumeric()).collect::<String>().to_lowercase();
    LEGAL_FORMS.contains(&word.as_str())
}

/// Whatever follows " - " is a name, unless it is empty or a street address.
fn looks_like_name(rest: &str) -> bool {
    if !rest.chars().any(char::is_alphabetic) {
        return false;
    }
    !looks_like_address(rest)
}

/// Words that give away a postal address. Only checked when the text starts
/// with a number, so "24 Hour Fitness USA, Inc." is still a name.
const ADDRESS_WORDS: &[&str] = &[
    "street", "st", "road", "rd", "avenue", "ave", "lane", "floor", "fl", "tower", "building", "bldg", "plaza", "marg",
    "jalan", "unit", "suite", "sector", "nagar", "highway", "blvd", "boulevard", "district", "city", "centre",
    "center", "park", "house", "block", "estate", "complex", "chowk", "colony", "soi", "moo", "jl", "f",
];

/// Openings that, before a number, are an address: "No.31, ...", "Unit 5, ...".
/// Not "Level", since "Level 3 Parent, LLC" is a company.
const ADDRESS_OPENINGS: &[&str] = &["no.", "no ", "unit ", "room ", "flat ", "floor ", "suite ", "lot ", "plot "];

fn looks_like_address(text: &str) -> bool {
    let lower = text.to_lowercase();
    // "No.31", "Unit A17,9/F": the next word has a number in it.
    let opening = ADDRESS_OPENINGS.iter().find_map(|start| lower.strip_prefix(start));
    let next_word = opening.and_then(|after| after.split_whitespace().next());
    if next_word.is_some_and(|word| word.chars().any(|c| c.is_ascii_digit())) {
        return true;
    }
    if !lower.starts_with(|c: char| c.is_ascii_digit()) {
        return false;
    }
    lower
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .any(|word| ADDRESS_WORDS.contains(&word))
}

/// A handle shown on its own loses the technical bits around it:
/// "AS-SANCHARONLINE-IN" becomes "SANCHARONLINE".
fn tidy_handle(handle: &str, country: Option<&str>) -> String {
    let mut name = handle;
    for prefix in ["ASN-", "AS-"] {
        if let Some(rest) = name.strip_prefix(prefix) {
            name = rest;
        }
    }
    loop {
        let before = name;
        for suffix in ["-AS", "-ASN", "-AP", "-NET-AP"] {
            if let Some(rest) = name.strip_suffix(suffix) {
                name = rest;
            }
        }
        if let Some(cc) = country
            && let Some((rest, tail)) = name.rsplit_once('-')
            && tail.eq_ignore_ascii_case(cc)
        {
            name = rest;
        }
        if name == before {
            break;
        }
    }
    if name.is_empty() { handle.to_owned() } else { name.to_owned() }
}

/// European style: "M247 M247 Europe SRL" becomes "M247 Europe SRL". The first
/// word is dropped only when it is clearly a handle, so "Telia Company AB" and
/// "University of Innsbruck" stay whole.
fn strip_leading_handle(description: &str) -> &str {
    let Some((first, rest)) = description.split_once(' ') else { return description };
    if !is_handle(first, rest) {
        return description;
    }
    // "Corporation" or "GmbH" alone is no name.
    if !rest.chars().any(char::is_alphabetic) || (!rest.contains(' ') && is_legal_form(rest)) {
        return description;
    }
    rest
}

/// Letters and digits only, lower case, for comparing "QDSverigeAB" with "QD Sverige AB".
fn squash(text: &str) -> String {
    text.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

fn is_handle(first: &str, rest: &str) -> bool {
    // Registry handles use letters, digits, '-' and '_' only.
    if first.is_empty() || !first.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return false;
    }
    // "AKAMAI-ASN1", "AS_BBT", "M247": no ordinary word looks like this.
    if first.chars().any(|c| c == '-' || c == '_' || c.is_ascii_digit()) {
        return true;
    }
    let squashed_first = squash(first);
    let squashed_rest = squash(rest);
    // "Clouvider Clouvider Limited", "QDSverigeAB QD Sverige AB".
    if squashed_rest.starts_with(&squashed_first) {
        return true;
    }
    // "APINET SAS APINET", "Ahold Koninklijke Ahold Delhaize N.V.": the handle
    // is repeated inside the name.
    if squashed_first.len() >= 4 && squashed_rest.contains(&squashed_first) {
        return true;
    }
    // "ASUBI UBICENTREX SAS": "AS" and the start of the name.
    if let Some(tail) = squashed_first.strip_prefix("as")
        && tail.len() >= 3
        && squashed_rest.starts_with(tail)
    {
        return true;
    }
    let has_upper = first.chars().any(|c| c.is_ascii_uppercase());
    let has_lower = first.chars().any(|c| c.is_ascii_lowercase());
    let starts_upper = first.starts_with(|c: char| c.is_ascii_uppercase());
    // "uspc Ural State Polzunov College": names don't begin with a lower-case word.
    if !has_upper {
        return rest.starts_with(|c: char| c.is_uppercase() || c.is_ascii_digit() || c == '"');
    }
    // "SimpleThings", "iNES", "MTSNet": capitals inside one word.
    if has_lower && (!starts_upper || first.chars().skip(1).any(|c| c.is_ascii_uppercase())) {
        return true;
    }
    if !has_lower {
        // All capitals, like "DTAG Deutsche Telekom AG" or "UBP UNION BANCAIRE PRIVEE".
        let rest_has_title_word = rest.split_whitespace().any(|word| {
            let mut chars = word.chars();
            chars.next().is_some_and(char::is_uppercase) && chars.any(char::is_lowercase)
        });
        return first.len() >= 2 && (rest_has_title_word || is_initials(&squashed_first, rest));
    }
    // A capitalised word ("Nikhef", "Amadeus") is likely part of the name.
    false
}

/// Whether `letters` are the first letters of the words that follow, as in
/// "ABH Anadolu Bilisim Hizmetleri" or "WDH WeDoHosting".
fn is_initials(letters: &str, rest: &str) -> bool {
    let rest = squash(rest);
    if letters.chars().next() != rest.chars().next() {
        return false;
    }
    let mut rest = rest.chars();
    letters.chars().all(|wanted| rest.any(|c| c == wanted))
}

#[cfg(test)]
mod tests {
    use super::friendly_name;

    fn check(description: &str, country: &str, expected: &str) {
        let country = (!country.is_empty()).then_some(country);
        assert_eq!(friendly_name(description, country), expected, "for {description:?}");
    }

    #[test]
    fn dash_style_keeps_the_organisation() {
        check("CLOUDFLARENET - Cloudflare, Inc.", "US", "Cloudflare, Inc.");
        check("GOOGLE - Google LLC", "US", "Google LLC");
        check("AMAZON-02 - Amazon.com, Inc.", "US", "Amazon.com, Inc.");
        check("MICROSOFT-CORP-MSN-AS-BLOCK - Microsoft Corporation", "US", "Microsoft Corporation");
        check("AKAMAI-AS - Akamai Technologies, Inc.", "US", "Akamai Technologies, Inc.");
        check("FASTLY - Fastly, Inc.", "US", "Fastly, Inc.");
        check("DIGITALOCEAN-ASN - DigitalOcean, LLC", "US", "DigitalOcean, LLC");
        check("HETZNER-AS - Hetzner Online GmbH", "DE", "Hetzner Online GmbH");
        check("COGENT-174 - Cogent Communications, LLC", "US", "Cogent Communications, LLC");
        check("COMCAST-7922 - Comcast Cable Communications, LLC", "US", "Comcast Cable Communications, LLC");
        check("LEVEL3 - Level 3 Parent, LLC", "US", "Level 3 Parent, LLC");
        check("ATT-INTERNET4 - AT&T Enterprises, LLC", "US", "AT&T Enterprises, LLC");
        check("UUNET - Verizon Business", "US", "Verizon Business");
        check("AS8151 - UNINET", "MX", "UNINET");
        check("RCN-AS - RCN", "US", "RCN");
        check("AS-VULTR - The Constant Company, LLC", "US", "The Constant Company, LLC");
        check("24HOURFITNESS - 24 Hour Fitness USA, Inc.", "US", "24 Hour Fitness USA, Inc.");
        check("CANAD112 - 12393239 Canada Inc", "CA", "12393239 Canada Inc");
        check("Dimension Data - Dimension Data", "ZA", "Dimension Data");
        check(
            "Internet service provider association - DRC - Internet service provider association - DRC",
            "CD",
            "Internet service provider association - DRC",
        );
        check("Spacetel - Guine-Bissau - Spacetel - Guine-Bissau", "GW", "Spacetel - Guine-Bissau");
    }

    #[test]
    fn addresses_fall_back_to_the_handle() {
        check("CHINANET-BACKBONE - No.31,Jin-rong Street", "CN", "CHINANET-BACKBONE");
        check("ANMM-AU - 2 Murray Street", "AU", "ANMM");
        check("INFINEON-SG - 8 Kallang Sector", "SG", "INFINEON");
        check("YASHTEL1-AS-AP - 2713/4 lakshmi building adipampa road", "IN", "YASHTEL1");
        check("AEDAS-HK - 31/F One Island East", "HK", "AEDAS");
        check("SH2206-AP - UNIT A17,9/F SILVERCORP INT'L TOWER 707-713 NATHAN RD", "HK", "SH2206");
        check("SUITE SOLUTIONS FIBER INTERNET - Suite Solutions Inc", "CA", "Suite Solutions Inc");
    }

    #[test]
    fn european_style_drops_the_handle() {
        check("AKAMAI-ASN1 Akamai International B.V.", "NL", "Akamai International B.V.");
        check("GTT-BACKBONE GTT Communications Inc.", "US", "GTT Communications Inc.");
        check("M247 M247 Europe SRL", "RO", "M247 Europe SRL");
        check("DTAG Deutsche Telekom AG", "DE", "Deutsche Telekom AG");
        check("ROSTELECOM-AS PJSC Rostelecom", "RU", "PJSC Rostelecom");
        check("HostRoyale HostRoyale Technologies Pvt Ltd", "IN", "HostRoyale Technologies Pvt Ltd");
        check("Clouvider Clouvider Limited", "GB", "Clouvider Limited");
        check("CDNEXT Datacamp Limited", "GB", "Datacamp Limited");
        check("UNI2-AS Orange Espagne SA", "ES", "Orange Espagne SA");
        check("TELIA-TCN Telia Company AB", "SE", "Telia Company AB");
        check("QDSverigeAB QD Sverige AB", "SE", "QD Sverige AB");
        check("SimpleThings SimpleThings GmbH", "DE", "SimpleThings GmbH");
        check("uspc Ural State Polzunov College", "RU", "Ural State Polzunov College");
        check("ABH Anadolu Bilisim Hizmetleri A.S.", "TR", "Anadolu Bilisim Hizmetleri A.S.");
        check("UBP UNION BANCAIRE PRIVEE, UBP SA", "CH", "UNION BANCAIRE PRIVEE, UBP SA");
        check("MTSNet MTS PJSC", "RU", "MTS PJSC");
        check("APINET SAS APINET", "FR", "SAS APINET");
        check("Ahold Koninklijke Ahold Delhaize N.V.", "NL", "Koninklijke Ahold Delhaize N.V.");
        check("ASUBI UBICENTREX SAS", "FR", "UBICENTREX SAS");
        check("ASWASKO WASKO S.A.", "PL", "WASKO S.A.");
    }

    #[test]
    fn plain_names_stay_whole() {
        check("Telia Company AB", "SE", "Telia Company AB");
        check("University of Innsbruck", "AT", "University of Innsbruck");
        check("Amadeus GmbH", "DE", "Amadeus GmbH");
        check("Danfoss A/S", "DK", "Danfoss A/S");
        check("British Telecommunications Limited", "GB", "British Telecommunications Limited");
        check("Banque Pictet & Cie SA", "CH", "Banque Pictet & Cie SA");
        check("IBM Corporation", "US", "IBM Corporation");
    }

    #[test]
    fn handles_alone_stay() {
        check("LEVEL3", "US", "LEVEL3");
        check("Paradigm", "BE", "Paradigm");
        check("TeleYemen TeleYemen", "YE", "TeleYemen");
    }

    #[test]
    fn trailing_country_is_dropped_only_when_it_repeats() {
        check("GOOGLE - Google LLC, US", "US", "Google LLC");
        check("GOOGLE, US", "US", "GOOGLE");
        check("BLOOMBERG-NET - Bloomberg, LP", "US", "Bloomberg, LP");
        check("ASIAKOMNET-ID-AP - Asiakomnet Multimedia, PT", "ID", "Asiakomnet Multimedia, PT");
        check("Some Network, US", "", "Some Network, US");
    }

    #[test]
    fn white_space_is_tidied() {
        check("  FASTLY  -   Fastly,  Inc. ", "US", "Fastly, Inc.");
        check("-Reserved AS-", "", "Reserved AS");
        check("", "", "");
    }
}
