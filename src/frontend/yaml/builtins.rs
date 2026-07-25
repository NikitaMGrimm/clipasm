pub(super) const PROGRAM_SYNTAX: &[(&str, Option<&str>, bool)] = &[
    ("image", Some("path"), false),
    ("video", Some("path"), false),
    ("audio", Some("path"), false),
    ("concat", Some("type"), false),
    ("repeat", Some("count"), false),
    ("trim", Some("range"), false),
    ("drop", Some("type"), false),
    ("zoom", Some("percent"), false),
    ("wobble", Some("pixels"), false),
    ("flash", Some("frames"), false),
    ("crossfade", Some("duration"), false),
    ("during", Some("range"), true),
];
