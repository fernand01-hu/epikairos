pub mod client;

pub use client::{
    extract_and_validate_form_action, filter_courses, format_ics_datetime,
    generate_ics_from_courses, generate_pkce_pair, get_default_school_year_range,
    is_valid_date_format, normalize_course, safe_truncate, AurigaClient,
};
