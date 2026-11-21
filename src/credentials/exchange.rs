use super::{Credential, MAX_BYTES, MAX_ENTRIES, Result, Secret, Site, Totp};
use serde::Deserialize;
use zeroize::{Zeroize, Zeroizing};

#[derive(Deserialize)]
struct Bitwarden {
    #[serde(default)]
    encrypted: bool,
    items: Vec<BitwardenItem>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BitwardenItem {
    #[serde(rename = "type")]
    kind: u8,
    name: Secret,
    #[serde(default)]
    notes: Option<Secret>,
    login: Option<BitwardenLogin>,
}
#[derive(Deserialize)]
struct BitwardenLogin {
    #[serde(default, rename = "fido2Credentials")]
    passkeys: Option<Vec<serde::de::IgnoredAny>>,
    #[serde(default)]
    username: Option<Secret>,
    #[serde(default)]
    password: Option<Secret>,
    #[serde(default)]
    totp: Option<Secret>,
    #[serde(default)]
    uris: Vec<BitwardenUri>,
}
#[derive(Deserialize)]
struct BitwardenUri {
    uri: Option<String>,
}

pub(crate) fn import(bytes: &[u8], container: Option<u64>) -> Result<Vec<Credential>> {
    if bytes.len() > MAX_BYTES {
        return Err("Choose a password export smaller than 16 MiB.".into());
    }
    if bytes.iter().find(|b| !b.is_ascii_whitespace()) == Some(&b'{') {
        let data: Bitwarden = serde_json::from_slice(bytes).map_err(|_| {
            "Choose an unencrypted Bitwarden JSON export or a supported CSV export.".to_owned()
        })?;
        if data.encrypted {
            return Err("Use an unencrypted Bitwarden export for migration, or import an enzim encrypted backup.".into());
        }
        if data.items.len() > MAX_ENTRIES {
            return Err("The import contains too many items.".into());
        }
        let mut entries = Vec::new();
        for item in data.items {
            if item.kind != 1 {
                continue;
            }
            let Some(login) = item.login else {
                continue;
            };
            if login.passkeys.is_some_and(|keys| !keys.is_empty()) {
                return Err("This export contains passkeys. Passkey migration needs the pending native WebAuthn provider; no items were imported. Use a password-only export for now.".into());
            }
            let totp = login
                .totp
                .filter(|s| !s.is_empty())
                .map(|s| Totp::parse(s.expose()))
                .transpose()?;
            for uri in login.uris {
                let Some(uri) = uri.uri else {
                    continue;
                };
                let site = Site::parse(&uri, container)?;
                if entries.iter().any(|e: &Credential| {
                    e.site == site && e.username == login.username.clone().unwrap_or_default()
                }) {
                    continue;
                }
                let mut entry = Credential::new(site)?;
                entry.name = item.name.clone();
                entry.username = login.username.clone().unwrap_or_default();
                entry.password = login.password.clone().unwrap_or_default();
                entry.notes = item.notes.clone().unwrap_or_default();
                entry.totp.clone_from(&totp);
                entry.validate()?;
                entries.push(entry);
                if entries.len() > MAX_ENTRIES {
                    return Err("The import contains too many website logins.".into());
                }
            }
        }
        return Ok(entries);
    }
    import_csv(bytes, container)
}

fn import_csv(bytes: &[u8], container: Option<u64>) -> Result<Vec<Credential>> {
    let mut parser = csv_core::Reader::new();
    let mut output = Zeroizing::new(vec![0; bytes.len().saturating_add(1)]);
    let mut ends = [0; 256];
    let mut offset = 0;
    let mut written = 0;
    let mut fields = 0;
    struct Columns {
        url: usize,
        username: usize,
        password: usize,
        name: Option<usize>,
        notes: Option<usize>,
        totp: Option<usize>,
        expected: usize,
    }
    let mut columns: Option<Columns> = None;
    let mut entries = Vec::new();
    loop {
        let (result, consumed, produced, count) = parser.read_record(
            &bytes[offset..],
            &mut output[written..],
            &mut ends[fields..],
        );
        offset += consumed;
        written += produced;
        fields += count;
        match result {
            csv_core::ReadRecordResult::End => break,
            csv_core::ReadRecordResult::InputEmpty => continue,
            csv_core::ReadRecordResult::OutputFull | csv_core::ReadRecordResult::OutputEndsFull => {
                return Err("The CSV contains an oversized row.".into());
            }
            csv_core::ReadRecordResult::Record => {}
        }
        let field = |index: usize| -> Result<&str> {
            let start = if index == 0 {
                0
            } else {
                *ends.get(index - 1).ok_or("Invalid CSV row.")?
            };
            let end = *ends.get(index).ok_or("Invalid CSV row.")?;
            std::str::from_utf8(output.get(start..end).ok_or("Invalid CSV field.")?)
                .map_err(|_| "The CSV must use UTF-8 text.".into())
        };
        if let Some(Columns {
            url,
            username,
            password,
            name,
            notes,
            totp,
            expected,
        }) = columns
        {
            if fields != expected {
                return Err("CSV rows must have the same columns as the header.".into());
            }
            let mut entry = Credential::new(Site::parse(field(url)?, container)?)?;
            entry.username = Secret::new(field(username)?.to_owned());
            entry.password = Secret::new(field(password)?.to_owned());
            if let Some(name) = name {
                entry.name = Secret::new(field(name)?.to_owned());
            }
            if let Some(notes) = notes {
                entry.notes = Secret::new(field(notes)?.to_owned());
            }
            if let Some(index) = totp {
                let value = field(index)?;
                if !value.is_empty() {
                    entry.totp = Some(Totp::parse(value)?);
                }
            }
            entry.validate()?;
            entries.push(entry);
            if entries.len() > MAX_ENTRIES {
                return Err("The import contains too many logins.".into());
            }
        } else {
            let find = |names: &[&str]| -> Option<usize> {
                (0..fields).find(|&index| {
                    field(index).is_ok_and(|value| {
                        names.iter().any(|name| {
                            value
                                .trim_start_matches('\u{feff}')
                                .trim()
                                .eq_ignore_ascii_case(name)
                        })
                    })
                })
            };
            columns = Some(Columns {
                url: find(&["url", "origin", "hostname", "login_uri"])
                    .ok_or("The CSV needs a URL column.")?,
                username: find(&["username", "login_username"])
                    .ok_or("The CSV needs a username column.")?,
                password: find(&["password", "login_password"])
                    .ok_or("The CSV needs a password column.")?,
                name: find(&["name", "title"]),
                notes: find(&["note", "notes"]),
                totp: find(&["totp", "login_totp", "otpauth"]),
                expected: fields,
            });
        }
        output[..written].zeroize();
        written = 0;
        fields = 0;
    }
    if columns.is_none() {
        return Err("The CSV has no header.".into());
    }
    Ok(entries)
}

pub(crate) fn export_csv(entries: &[Credential]) -> Result<Zeroizing<Vec<u8>>> {
    let mut output = Zeroizing::new(Vec::new());
    output.extend_from_slice(b"name,url,username,password,note,totp\r\n");
    for entry in entries {
        let totp = entry
            .totp
            .as_ref()
            .map(Totp::uri_secret)
            .unwrap_or_default();
        for (index, field) in [
            entry.name.expose(),
            &entry.site.origin,
            entry.username.expose(),
            entry.password.expose(),
            entry.notes.expose(),
            totp.expose(),
        ]
        .into_iter()
        .enumerate()
        {
            if index != 0 {
                output.push(b',');
            }
            output.push(b'"');
            for byte in field.bytes() {
                output.push(byte);
                if byte == b'"' {
                    output.push(byte);
                }
            }
            output.push(b'"');
        }
        output.extend_from_slice(b"\r\n");
        if output.len() > MAX_BYTES {
            return Err("The CSV export exceeds 16 MiB.".into());
        }
    }
    Ok(output)
}
