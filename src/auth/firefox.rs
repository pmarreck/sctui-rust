use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use rusqlite::{Connection, OpenFlags, OptionalExtension};

use super::Token;

const COOKIE_QUERY: &str = "SELECT value FROM moz_cookies \
    WHERE host LIKE '%soundcloud.com' AND name = 'oauth_token' \
    ORDER BY LENGTH(value) DESC LIMIT 1";

/// Searches Firefox-family profiles for SoundCloud's browser-session cookie,
/// querying a private snapshot so a live WAL database remains undisturbed.
pub fn find() -> Option<Token> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let appdata = std::env::var_os("APPDATA").map(PathBuf::from);
    let roots = firefox_profile_roots(std::env::consts::OS, home.as_deref(), appdata.as_deref());

    find_in_roots(&roots)
}

/// Tries sorted Firefox profiles until one contains a matching SoundCloud
/// session, allowing missing and unrelated profiles to fail softly.
fn find_in_roots(roots: &[PathBuf]) -> Option<Token> {
    cookie_databases(roots).into_iter().find_map(|db| {
        let value = query_token(&db).ok().flatten()?;
        let profile = db.parent()?.file_name()?.to_string_lossy().into_owned();
        Some(Token::from_firefox(value, profile))
    })
}

/// Maps supported operating systems to Firefox-family profile roots without
/// reading process-global state, keeping platform behavior directly testable.
fn firefox_profile_roots(
    operating_system: &str,
    home: Option<&Path>,
    appdata: Option<&Path>,
) -> Vec<PathBuf> {
    match operating_system {
        "macos" => home
            .map(|path| path.join("Library/Application Support/Firefox/Profiles"))
            .into_iter()
            .collect(),
        "windows" => appdata
            .map(|path| path.join("Mozilla/Firefox/Profiles"))
            .into_iter()
            .collect(),
        _ => home
            .map(|path| {
                vec![
                    path.join(".mozilla/firefox"),
                    path.join("snap/firefox/common/.mozilla/firefox"),
                    path.join(".var/app/org.mozilla.firefox/.mozilla/firefox"),
                ]
            })
            .unwrap_or_default(),
    }
}

/// Enumerates every profile cookie database as a sorted set so fallback
/// selection is deterministic across filesystem enumeration orders.
fn cookie_databases(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut databases = roots
        .iter()
        .filter_map(|root| fs::read_dir(root).ok())
        .flatten()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .map(|entry| entry.path().join("cookies.sqlite"))
        .filter(|path| path.is_file())
        .collect::<Vec<_>>();
    databases.sort();
    databases
}

/// Copies Firefox's database and WAL sidecars into a private temporary
/// directory, then reads the snapshot with bundled SQLite.
fn query_token(database: &Path) -> Result<Option<String>> {
    let snapshot = tempfile::tempdir().context("create Firefox cookie snapshot directory")?;
    let destination = snapshot.path().join("cookies.sqlite");
    fs::copy(database, &destination).context("copy Firefox cookie database")?;

    for suffix in ["-wal", "-shm"] {
        let _ = fs::copy(
            with_suffix(database, suffix),
            with_suffix(&destination, suffix),
        );
    }

    let connection = Connection::open_with_flags(
        destination,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .context("open Firefox cookie snapshot")?;
    connection
        .query_row(COOKIE_QUERY, [], |row| row.get::<_, String>(0))
        .optional()
        .context("query Firefox SoundCloud session")
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut value = OsString::from(path.as_os_str());
    value.push(suffix);
    value.into()
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    use rusqlite::Connection;

    use super::{cookie_databases, find_in_roots, firefox_profile_roots, query_token};

    #[test]
    fn profile_roots_classify_supported_operating_systems() {
        let home = Path::new("/home/me");
        let appdata = Path::new(r"C:\Users\me\AppData\Roaming");

        assert_eq!(
            firefox_profile_roots("linux", Some(home), Some(appdata)),
            vec![
                PathBuf::from("/home/me/.mozilla/firefox"),
                PathBuf::from("/home/me/snap/firefox/common/.mozilla/firefox"),
                PathBuf::from("/home/me/.var/app/org.mozilla.firefox/.mozilla/firefox"),
            ]
        );
        assert_eq!(
            firefox_profile_roots("macos", Some(home), Some(appdata)),
            vec![PathBuf::from(
                "/home/me/Library/Application Support/Firefox/Profiles"
            )]
        );
        assert_eq!(
            firefox_profile_roots("windows", Some(home), Some(appdata)),
            vec![appdata.join("Mozilla/Firefox/Profiles")]
        );
        assert!(firefox_profile_roots("windows", Some(home), None).is_empty());
    }

    #[test]
    fn cookie_database_discovery_returns_the_set_of_profile_databases() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("Profiles");
        let first = root.join("a.default").join("cookies.sqlite");
        let second = root.join("b.default").join("cookies.sqlite");
        fs::create_dir_all(first.parent().unwrap()).unwrap();
        fs::create_dir_all(second.parent().unwrap()).unwrap();
        fs::create_dir_all(root.join("no-cookie-db")).unwrap();
        fs::write(&first, []).unwrap();
        fs::write(&second, []).unwrap();

        assert_eq!(cookie_databases(&[root]), vec![first, second]);
    }

    #[test]
    fn query_selects_only_the_soundcloud_oauth_cookie() {
        let temp = tempfile::tempdir().unwrap();
        let db = temp.path().join("cookies.sqlite");
        let connection = Connection::open(&db).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE moz_cookies(name TEXT, value TEXT, host TEXT);\
                 INSERT INTO moz_cookies VALUES('oauth_token','browser-token','soundcloud.com');\
                 INSERT INTO moz_cookies VALUES('sc_anonymous_id','wrong-name','.soundcloud.com');\
                 INSERT INTO moz_cookies VALUES('oauth_token','wrong-host','example.com');",
            )
            .unwrap();
        drop(connection);

        assert_eq!(query_token(&db).unwrap().as_deref(), Some("browser-token"));
    }

    #[test]
    fn query_includes_committed_wal_data_from_a_live_database() {
        let temp = tempfile::tempdir().unwrap();
        let db = temp.path().join("cookies.sqlite");
        let connection = Connection::open(&db).unwrap();
        connection
            .pragma_update(None, "journal_mode", "WAL")
            .unwrap();
        connection
            .execute_batch(
                "CREATE TABLE moz_cookies(name TEXT, value TEXT, host TEXT);\
                 PRAGMA wal_checkpoint(TRUNCATE);\
                 INSERT INTO moz_cookies VALUES('oauth_token','wal-token','.soundcloud.com');",
            )
            .unwrap();

        assert!(db.with_extension("sqlite-wal").exists());
        assert_eq!(query_token(&db).unwrap().as_deref(), Some("wal-token"));
    }

    #[test]
    fn fallback_skips_profiles_without_a_soundcloud_session() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("Profiles");
        for (profile, rows) in [
            (
                "a.empty",
                "INSERT INTO moz_cookies VALUES('oauth_token','wrong-host','example.com');",
            ),
            (
                "b.signed-in",
                "INSERT INTO moz_cookies VALUES('oauth_token','browser-token','.soundcloud.com');",
            ),
        ] {
            let directory = root.join(profile);
            fs::create_dir_all(&directory).unwrap();
            let connection = Connection::open(directory.join("cookies.sqlite")).unwrap();
            connection
                .execute_batch(&format!(
                    "CREATE TABLE moz_cookies(name TEXT, value TEXT, host TEXT);{rows}"
                ))
                .unwrap();
        }

        let token = find_in_roots(&[root]).unwrap();
        assert_eq!(token.access_token, "browser-token");
        assert_eq!(token.source_label(), "Firefox (b.signed-in)");
    }
}
