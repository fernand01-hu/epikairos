pub mod client;

pub use client::{
    filter_courses, generate_ics_from_courses, generate_pkce_pair, get_default_school_year_range,
    normalize_course, AurigaClient,
};
