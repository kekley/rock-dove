pub(crate) const PLAYLIST_SEARCH_ARGS: &[&str] = &[
    "--dump-json",
    "--skip-download",
    "--flat-playlist",
    "--no-check-certificate",
    "--geo-bypass",
    "--cookies",
    "./cookies.txt",
    "--no-abort-on-error",
    "--default-search",
    "ytsearch",
];

pub(crate) const VIDEO_SEARCH_ARGS: &[&str] = &[
    "--dump-json",
    "--default-search",
    "ytsearch",
    "--skip-download",
    "--cookies",
    "./cookies.txt",
    "--no-playlist",
    "--no-check-certificate",
    "--geo-bypass",
    "--no-abort-on-error",
];

pub(crate) const VIDEO_STREAM_SEARCH_ARGS: &[&str] = &[
    "--dump-json",
    "--no-playlist",
    "--no-check-certificate",
    "--geo-bypass",
    "--cookies",
    "./cookies.txt",
    "--skip-download",
    "--no-abort-on-error",
    "--default-search",
    "ytsearch",
];
