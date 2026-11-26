//! ISO 3166-1 alpha-3 codes and the English spellings OFAC and the UN list use.

/// ISO 3166-1 alpha-3 code and its ISO short name.
const ISO_NAMES: &[(&str, &str)] = &[
    ("ABW", "Aruba"),
    ("AFG", "Afghanistan"),
    ("AGO", "Angola"),
    ("AIA", "Anguilla"),
    ("ALA", "Åland Islands"),
    ("ALB", "Albania"),
    ("AND", "Andorra"),
    ("ARE", "United Arab Emirates"),
    ("ARG", "Argentina"),
    ("ARM", "Armenia"),
    ("ASM", "American Samoa"),
    ("ATA", "Antarctica"),
    ("ATF", "French Southern Territories"),
    ("ATG", "Antigua and Barbuda"),
    ("AUS", "Australia"),
    ("AUT", "Austria"),
    ("AZE", "Azerbaijan"),
    ("BDI", "Burundi"),
    ("BEL", "Belgium"),
    ("BEN", "Benin"),
    ("BES", "Bonaire, Sint Eustatius and Saba"),
    ("BFA", "Burkina Faso"),
    ("BGD", "Bangladesh"),
    ("BGR", "Bulgaria"),
    ("BHR", "Bahrain"),
    ("BHS", "Bahamas"),
    ("BIH", "Bosnia and Herzegovina"),
    ("BLM", "Saint Barthélemy"),
    ("BLR", "Belarus"),
    ("BLZ", "Belize"),
    ("BMU", "Bermuda"),
    ("BOL", "Bolivia, Plurinational State of"),
    ("BRA", "Brazil"),
    ("BRB", "Barbados"),
    ("BRN", "Brunei Darussalam"),
    ("BTN", "Bhutan"),
    ("BVT", "Bouvet Island"),
    ("BWA", "Botswana"),
    ("CAF", "Central African Republic"),
    ("CAN", "Canada"),
    ("CCK", "Cocos (Keeling) Islands"),
    ("CHE", "Switzerland"),
    ("CHL", "Chile"),
    ("CHN", "China"),
    ("CIV", "Côte d'Ivoire"),
    ("CMR", "Cameroon"),
    ("COD", "Congo, The Democratic Republic of the"),
    ("COG", "Congo"),
    ("COK", "Cook Islands"),
    ("COL", "Colombia"),
    ("COM", "Comoros"),
    ("CPV", "Cabo Verde"),
    ("CRI", "Costa Rica"),
    ("CUB", "Cuba"),
    ("CUW", "Curaçao"),
    ("CXR", "Christmas Island"),
    ("CYM", "Cayman Islands"),
    ("CYP", "Cyprus"),
    ("CZE", "Czechia"),
    ("DEU", "Germany"),
    ("DJI", "Djibouti"),
    ("DMA", "Dominica"),
    ("DNK", "Denmark"),
    ("DOM", "Dominican Republic"),
    ("DZA", "Algeria"),
    ("ECU", "Ecuador"),
    ("EGY", "Egypt"),
    ("ERI", "Eritrea"),
    ("ESH", "Western Sahara"),
    ("ESP", "Spain"),
    ("EST", "Estonia"),
    ("ETH", "Ethiopia"),
    ("FIN", "Finland"),
    ("FJI", "Fiji"),
    ("FLK", "Falkland Islands (Malvinas)"),
    ("FRA", "France"),
    ("FRO", "Faroe Islands"),
    ("FSM", "Micronesia, Federated States of"),
    ("GAB", "Gabon"),
    ("GBR", "United Kingdom"),
    ("GEO", "Georgia"),
    ("GGY", "Guernsey"),
    ("GHA", "Ghana"),
    ("GIB", "Gibraltar"),
    ("GIN", "Guinea"),
    ("GLP", "Guadeloupe"),
    ("GMB", "Gambia"),
    ("GNB", "Guinea-Bissau"),
    ("GNQ", "Equatorial Guinea"),
    ("GRC", "Greece"),
    ("GRD", "Grenada"),
    ("GRL", "Greenland"),
    ("GTM", "Guatemala"),
    ("GUF", "French Guiana"),
    ("GUM", "Guam"),
    ("GUY", "Guyana"),
    ("HKG", "Hong Kong"),
    ("HMD", "Heard Island and McDonald Islands"),
    ("HND", "Honduras"),
    ("HRV", "Croatia"),
    ("HTI", "Haiti"),
    ("HUN", "Hungary"),
    ("IDN", "Indonesia"),
    ("IMN", "Isle of Man"),
    ("IND", "India"),
    ("IOT", "British Indian Ocean Territory"),
    ("IRL", "Ireland"),
    ("IRN", "Iran, Islamic Republic of"),
    ("IRQ", "Iraq"),
    ("ISL", "Iceland"),
    ("ISR", "Israel"),
    ("ITA", "Italy"),
    ("JAM", "Jamaica"),
    ("JEY", "Jersey"),
    ("JOR", "Jordan"),
    ("JPN", "Japan"),
    ("KAZ", "Kazakhstan"),
    ("KEN", "Kenya"),
    ("KGZ", "Kyrgyzstan"),
    ("KHM", "Cambodia"),
    ("KIR", "Kiribati"),
    ("KNA", "Saint Kitts and Nevis"),
    ("KOR", "Korea, Republic of"),
    ("KWT", "Kuwait"),
    ("LAO", "Lao People's Democratic Republic"),
    ("LBN", "Lebanon"),
    ("LBR", "Liberia"),
    ("LBY", "Libya"),
    ("LCA", "Saint Lucia"),
    ("LIE", "Liechtenstein"),
    ("LKA", "Sri Lanka"),
    ("LSO", "Lesotho"),
    ("LTU", "Lithuania"),
    ("LUX", "Luxembourg"),
    ("LVA", "Latvia"),
    ("MAC", "Macao"),
    ("MAF", "Saint Martin (French part)"),
    ("MAR", "Morocco"),
    ("MCO", "Monaco"),
    ("MDA", "Moldova, Republic of"),
    ("MDG", "Madagascar"),
    ("MDV", "Maldives"),
    ("MEX", "Mexico"),
    ("MHL", "Marshall Islands"),
    ("MKD", "North Macedonia"),
    ("MLI", "Mali"),
    ("MLT", "Malta"),
    ("MMR", "Myanmar"),
    ("MNE", "Montenegro"),
    ("MNG", "Mongolia"),
    ("MNP", "Northern Mariana Islands"),
    ("MOZ", "Mozambique"),
    ("MRT", "Mauritania"),
    ("MSR", "Montserrat"),
    ("MTQ", "Martinique"),
    ("MUS", "Mauritius"),
    ("MWI", "Malawi"),
    ("MYS", "Malaysia"),
    ("MYT", "Mayotte"),
    ("NAM", "Namibia"),
    ("NCL", "New Caledonia"),
    ("NER", "Niger"),
    ("NFK", "Norfolk Island"),
    ("NGA", "Nigeria"),
    ("NIC", "Nicaragua"),
    ("NIU", "Niue"),
    ("NLD", "Netherlands"),
    ("NOR", "Norway"),
    ("NPL", "Nepal"),
    ("NRU", "Nauru"),
    ("NZL", "New Zealand"),
    ("OMN", "Oman"),
    ("PAK", "Pakistan"),
    ("PAN", "Panama"),
    ("PCN", "Pitcairn"),
    ("PER", "Peru"),
    ("PHL", "Philippines"),
    ("PLW", "Palau"),
    ("PNG", "Papua New Guinea"),
    ("POL", "Poland"),
    ("PRI", "Puerto Rico"),
    ("PRK", "Korea, Democratic People's Republic of"),
    ("PRT", "Portugal"),
    ("PRY", "Paraguay"),
    ("PSE", "Palestine, State of"),
    ("PYF", "French Polynesia"),
    ("QAT", "Qatar"),
    ("REU", "Réunion"),
    ("ROU", "Romania"),
    ("RUS", "Russian Federation"),
    ("RWA", "Rwanda"),
    ("SAU", "Saudi Arabia"),
    ("SDN", "Sudan"),
    ("SEN", "Senegal"),
    ("SGP", "Singapore"),
    ("SGS", "South Georgia and the South Sandwich Islands"),
    ("SHN", "Saint Helena, Ascension and Tristan da Cunha"),
    ("SJM", "Svalbard and Jan Mayen"),
    ("SLB", "Solomon Islands"),
    ("SLE", "Sierra Leone"),
    ("SLV", "El Salvador"),
    ("SMR", "San Marino"),
    ("SOM", "Somalia"),
    ("SPM", "Saint Pierre and Miquelon"),
    ("SRB", "Serbia"),
    ("SSD", "South Sudan"),
    ("STP", "Sao Tome and Principe"),
    ("SUR", "Suriname"),
    ("SVK", "Slovakia"),
    ("SVN", "Slovenia"),
    ("SWE", "Sweden"),
    ("SWZ", "Eswatini"),
    ("SXM", "Sint Maarten (Dutch part)"),
    ("SYC", "Seychelles"),
    ("SYR", "Syrian Arab Republic"),
    ("TCA", "Turks and Caicos Islands"),
    ("TCD", "Chad"),
    ("TGO", "Togo"),
    ("THA", "Thailand"),
    ("TJK", "Tajikistan"),
    ("TKL", "Tokelau"),
    ("TKM", "Turkmenistan"),
    ("TLS", "Timor-Leste"),
    ("TON", "Tonga"),
    ("TTO", "Trinidad and Tobago"),
    ("TUN", "Tunisia"),
    ("TUR", "Türkiye"),
    ("TUV", "Tuvalu"),
    ("TWN", "Taiwan, Province of China"),
    ("TZA", "Tanzania, United Republic of"),
    ("UGA", "Uganda"),
    ("UKR", "Ukraine"),
    ("UMI", "United States Minor Outlying Islands"),
    ("URY", "Uruguay"),
    ("USA", "United States"),
    ("UZB", "Uzbekistan"),
    ("VAT", "Holy See (Vatican City State)"),
    ("VCT", "Saint Vincent and the Grenadines"),
    ("VEN", "Venezuela, Bolivarian Republic of"),
    ("VGB", "Virgin Islands, British"),
    ("VIR", "Virgin Islands, U.S."),
    ("VNM", "Viet Nam"),
    ("VUT", "Vanuatu"),
    ("WLF", "Wallis and Futuna"),
    ("WSM", "Samoa"),
    ("YEM", "Yemen"),
    ("ZAF", "South Africa"),
    ("ZMB", "Zambia"),
    ("ZWE", "Zimbabwe"),
];

/// Spellings the lists use that differ from the ISO short name.
const ALIASES: &[(&str, &str)] = &[
    ("Bahamas, The", "BHS"),
    ("The Gambia", "GMB"),
    ("Bolivia", "BOL"),
    ("Brunei", "BRN"),
    ("Burma", "MMR"),
    ("Cabo Verde", "CPV"),
    ("Cape Verde", "CPV"),
    ("Congo, Democratic Republic of the", "COD"),
    ("Democratic Republic of the Congo", "COD"),
    ("Congo, Republic of the", "COG"),
    ("Republic of the Congo", "COG"),
    ("Cote d Ivoire", "CIV"),
    ("Côte d'Ivoire", "CIV"),
    ("Curacao", "CUW"),
    ("Czech Republic", "CZE"),
    ("Czechia", "CZE"),
    ("Hong Kong", "HKG"),
    ("China, Hong Kong Special Administrative Region", "HKG"),
    ("Macau", "MAC"),
    ("Macao", "MAC"),
    ("Iran", "IRN"),
    ("Iran (Islamic Republic of)", "IRN"),
    ("Korea, North", "PRK"),
    ("Democratic People's Republic of Korea", "PRK"),
    ("North Korea", "PRK"),
    ("Korea, South", "KOR"),
    ("South Korea", "KOR"),
    ("Republic of Korea", "KOR"),
    ("Laos", "LAO"),
    ("Man, Isle of", "IMN"),
    ("Moldova", "MDA"),
    ("Republic of Moldova", "MDA"),
    ("North Macedonia, The Republic of", "MKD"),
    ("North Macedonia", "MKD"),
    ("Palestinian", "PSE"),
    ("State of Palestine", "PSE"),
    ("West Bank", "PSE"),
    ("Gaza", "PSE"),
    ("Russia", "RUS"),
    ("Russian Federation", "RUS"),
    ("Syria", "SYR"),
    ("Syrian Arab Republic", "SYR"),
    ("Taiwan", "TWN"),
    ("Tanzania", "TZA"),
    ("United Republic of Tanzania", "TZA"),
    ("Turkey", "TUR"),
    ("Türkiye", "TUR"),
    ("Venezuela", "VEN"),
    ("Vietnam", "VNM"),
    ("Kosovo", "XKX"),
    ("British Virgin Islands", "VGB"),
    ("United Kingdom", "GBR"),
    (
        "United Kingdom of Great Britain and Northern Ireland",
        "GBR",
    ),
    ("United States", "USA"),
    ("United States of America", "USA"),
    ("Eswatini", "SWZ"),
    ("Swaziland", "SWZ"),
    ("East Timor", "TLS"),
    ("Timor-Leste", "TLS"),
    ("Holy See", "VAT"),
];

/// Lowercases a name and keeps only its ASCII letters and digits.
fn normalize(name: &str) -> String {
    name.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|character| character.to_ascii_lowercase())
        .collect()
}

/// Strips the `Region: ` prefix OFAC puts on sub-national and regional values.
fn strip_region(name: &str) -> &str {
    name.strip_prefix("Region: ").unwrap_or(name)
}

/// The normalized spellings of one country, for matching list values.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CountryNames {
    alpha3: String,
    names: Vec<String>,
}

impl CountryNames {
    /// Looks up an uppercase ISO 3166-1 alpha-3 code.
    ///
    /// Returns `None` for codes absent from the ISO table and the alias list.
    #[must_use]
    pub fn for_alpha3(alpha3: &str) -> Option<Self> {
        let mut names: Vec<String> = ISO_NAMES
            .iter()
            .filter(|(code, _)| *code == alpha3)
            .map(|(_, name)| normalize(name))
            .collect();
        names.extend(
            ALIASES
                .iter()
                .filter(|(_, code)| *code == alpha3)
                .map(|(name, _)| normalize(name)),
        );
        names.sort();
        names.dedup();
        (!names.is_empty()).then(|| Self {
            alpha3: alpha3.to_string(),
            names,
        })
    }

    /// The alpha-3 code these names belong to.
    #[must_use]
    pub fn alpha3(&self) -> &str {
        &self.alpha3
    }

    /// True when a list's country value names this country.
    #[must_use]
    pub fn matches(&self, value: &str) -> bool {
        let normalized = normalize(strip_region(value.trim()));
        self.names.contains(&normalized)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_iso_and_list_spellings() {
        let iran = CountryNames::for_alpha3("IRN").expect("IRN known");
        assert!(iran.matches("Iran"));
        assert!(iran.matches("Region: Iran"));
        assert!(iran.matches("Iran (Islamic Republic of)"));
        assert!(!iran.matches("Iraq"));
        let congo = CountryNames::for_alpha3("COD").expect("COD known");
        assert!(congo.matches("Congo, Democratic Republic of the"));
        assert!(congo.matches("Democratic Republic of the Congo"));
        assert!(!congo.matches("Congo, Republic of the"));
    }

    #[test]
    fn unknown_code_has_no_names() {
        assert!(CountryNames::for_alpha3("ZZZ").is_none());
    }
}
