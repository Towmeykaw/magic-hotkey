use keyring::Entry;
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::SERVICE_NAME;

const TEMP_SERVICE: &str = "magic-hotkey-temp";

pub struct SecretInfo {
    pub name: String,
    pub expires_at: Option<u64>,
}

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn check_name(name: &str) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err("Secret name cannot be empty".into());
    }
    Ok(())
}

fn entry(service: &str, name: &str) -> Result<Entry, String> {
    check_name(name)?;
    Entry::new(service, name).map_err(|e| format!("Failed to access keychain: {}", e))
}

fn read(service: &str, name: &str) -> Result<Option<String>, String> {
    match entry(service, name)?.get_password() {
        Ok(v) => Ok(Some(v)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(format!("Failed to read secret '{}': {}", name, e)),
    }
}

fn remove(service: &str, name: &str) -> Result<bool, String> {
    match entry(service, name)?.delete_credential() {
        Ok(()) => Ok(true),
        Err(keyring::Error::NoEntry) => Ok(false),
        Err(e) => Err(format!("Failed to delete secret '{}': {}", name, e)),
    }
}

fn index_path() -> PathBuf {
    crate::config_dir().join("temp-secrets.json")
}

fn load_index() -> BTreeMap<String, u64> {
    fs::read_to_string(index_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_index(index: &BTreeMap<String, u64>) -> Result<(), String> {
    let path = index_path();
    let tmp = path.with_extension(format!("{}.tmp", std::process::id()));
    let json = serde_json::to_string_pretty(index).map_err(|e| format!("Serialize error: {}", e))?;
    fs::write(&tmp, json).map_err(|e| format!("Write error: {}", e))?;
    fs::rename(&tmp, &path).map_err(|e| format!("Write error: {}", e))
}

fn live_expiry(name: &str) -> Option<u64> {
    load_index().get(name).copied().filter(|&t| t > now())
}

pub fn get(name: &str) -> Result<String, String> {
    if live_expiry(name).is_some() {
        if let Some(v) = read(TEMP_SERVICE, name)? {
            return Ok(v);
        }
    }
    read(SERVICE_NAME, name)?.ok_or_else(|| format!("Secret '{}' not found", name))
}

pub fn set(name: &str, value: &str) -> Result<(), String> {
    if live_expiry(name).is_some() {
        return Err(format!(
            "A temporary secret named '{}' exists; delete it first",
            name
        ));
    }
    entry(SERVICE_NAME, name)?
        .set_password(value)
        .map_err(|e| format!("Failed to store secret: {}", e))
}

pub fn set_temp(name: &str, value: &str, ttl: Duration) -> Result<u64, String> {
    check_name(name)?;
    if platform::list_names(SERVICE_NAME)?.iter().any(|n| n == name) {
        return Err(format!(
            "A permanent secret named '{}' exists; pick another name",
            name
        ));
    }
    let expires_at = now() + ttl.as_secs();
    let mut index = load_index();
    index.insert(name.to_string(), expires_at);
    save_index(&index)?;
    if let Err(e) = platform::write_ephemeral(TEMP_SERVICE, name, value) {
        let mut index = load_index();
        index.remove(name);
        save_index(&index).ok();
        return Err(e);
    }
    Ok(expires_at)
}

pub fn delete(name: &str) -> Result<(), String> {
    let mut index = load_index();
    if index.remove(name).is_some() {
        save_index(&index)?;
        remove(TEMP_SERVICE, name)?;
        return Ok(());
    }
    if remove(SERVICE_NAME, name)? {
        Ok(())
    } else {
        Err(format!("Secret '{}' not found", name))
    }
}

pub fn list() -> Result<Vec<SecretInfo>, String> {
    let permanent = platform::list_names(SERVICE_NAME)?;
    let temporary = platform::list_names(TEMP_SERVICE)?;
    let index = load_index();
    let t = now();
    let mut out: Vec<SecretInfo> = permanent
        .into_iter()
        .map(|name| SecretInfo { name, expires_at: None })
        .collect();
    for name in temporary {
        if let Some(&exp) = index.get(&name).filter(|&&exp| exp > t) {
            out.push(SecretInfo { name, expires_at: Some(exp) });
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

pub fn purge_temp() -> Result<usize, String> {
    let names = platform::list_names(TEMP_SERVICE)?;
    for name in &names {
        remove(TEMP_SERVICE, name)?;
    }
    save_index(&BTreeMap::new())?;
    Ok(names.len())
}

// Enumerate before loading the index: set_temp writes the index before the keychain,
// so anything already stored is guaranteed to be in the index read afterwards.
pub fn sweep_expired() -> Result<usize, String> {
    let stored = platform::list_names(TEMP_SERVICE)?;
    let index = load_index();
    let t = now();
    let mut removed = 0;
    for name in &stored {
        let live = index.get(name).is_some_and(|&exp| exp > t);
        if !live && remove(TEMP_SERVICE, name)? {
            removed += 1;
        }
    }
    let mut fresh = load_index();
    let before = fresh.len();
    fresh.retain(|_, exp| *exp > t);
    if fresh.len() != before {
        save_index(&fresh)?;
    }
    Ok(removed)
}

pub fn parse_ttl(spec: &str) -> Result<Duration, String> {
    let spec = spec.trim();
    let err = || format!("Invalid duration '{}': use e.g. 30s, 15m, 2h, 1d", spec);
    let split = spec.find(|c: char| !c.is_ascii_digit()).unwrap_or(spec.len());
    let (num, unit) = spec.split_at(split);
    let n: u64 = num.parse().map_err(|_| err())?;
    let unit_secs = match unit {
        "s" => 1,
        "m" => 60,
        "h" => 3600,
        "d" => 86400,
        _ => return Err(err()),
    };
    if n == 0 {
        return Err(err());
    }
    n.checked_mul(unit_secs).map(Duration::from_secs).ok_or_else(err)
}

#[cfg(windows)]
mod platform {
    use std::ptr::null_mut;
    use windows_sys::Win32::Foundation::{ERROR_NOT_FOUND, FILETIME};
    use windows_sys::Win32::Security::Credentials::{
        CredEnumerateW, CredFree, CredWriteW, CREDENTIALW, CRED_PERSIST_SESSION, CRED_TYPE_GENERIC,
    };

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }

    unsafe fn from_wide(p: *const u16) -> String {
        if p.is_null() {
            return String::new();
        }
        let len = (0..).take_while(|&i| *p.add(i) != 0).count();
        String::from_utf16_lossy(std::slice::from_raw_parts(p, len))
    }

    // Targets are "{name}.{service}" and CredEnumerateW only filters by prefix,
    // so enumerate everything and match the suffix.
    pub fn list_names(service: &str) -> Result<Vec<String>, String> {
        let suffix = format!(".{}", service);
        let mut count = 0u32;
        let mut creds: *mut *mut CREDENTIALW = null_mut();
        if unsafe { CredEnumerateW(std::ptr::null(), 0, &mut count, &mut creds) } == 0 {
            let err = std::io::Error::last_os_error();
            if err.raw_os_error() == Some(ERROR_NOT_FOUND as i32) {
                return Ok(Vec::new());
            }
            return Err(format!("Failed to list keychain: {}", err));
        }
        let mut names = Vec::new();
        for i in 0..count as usize {
            let cred = unsafe { &**creds.add(i) };
            if cred.Type != CRED_TYPE_GENERIC {
                continue;
            }
            let target = unsafe { from_wide(cred.TargetName) };
            if let Some(name) = target.strip_suffix(&suffix) {
                if !name.is_empty() {
                    names.push(name.to_string());
                }
            }
        }
        unsafe { CredFree(creds as *const _) };
        Ok(names)
    }

    // Bypasses Entry only to get session persistence (gone at logoff); the target and
    // UTF-16LE blob must stay readable by `read` and `remove`.
    pub fn write_ephemeral(service: &str, name: &str, value: &str) -> Result<(), String> {
        let mut target = wide(&format!("{}.{}", name, service));
        let mut user = wide(name);
        let mut blob: Vec<u8> = value.encode_utf16().flat_map(u16::to_le_bytes).collect();
        let cred = CREDENTIALW {
            Flags: 0,
            Type: CRED_TYPE_GENERIC,
            TargetName: target.as_mut_ptr(),
            Comment: null_mut(),
            LastWritten: FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 },
            CredentialBlobSize: blob.len() as u32,
            CredentialBlob: blob.as_mut_ptr(),
            Persist: CRED_PERSIST_SESSION,
            AttributeCount: 0,
            Attributes: null_mut(),
            TargetAlias: null_mut(),
            UserName: user.as_mut_ptr(),
        };
        let ok = unsafe { CredWriteW(&cred, 0) } != 0;
        let err = std::io::Error::last_os_error();
        blob.fill(0);
        if ok {
            Ok(())
        } else {
            Err(format!("Failed to store secret: {}", err))
        }
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use security_framework::item::{ItemClass, ItemSearchOptions, Limit};

    const ERR_SEC_ITEM_NOT_FOUND: i32 = -25300;

    pub fn list_names(service: &str) -> Result<Vec<String>, String> {
        let results = match ItemSearchOptions::new()
            .class(ItemClass::generic_password())
            .service(service)
            .load_attributes(true)
            .limit(Limit::All)
            .search()
        {
            Ok(r) => r,
            Err(e) if e.code() == ERR_SEC_ITEM_NOT_FOUND => return Ok(Vec::new()),
            Err(e) => return Err(format!("Failed to list keychain: {}", e)),
        };
        Ok(results
            .iter()
            .filter_map(|r| r.simplify_dict())
            .filter_map(|d| d.get("acct").cloned())
            .collect())
    }

    pub fn write_ephemeral(service: &str, name: &str, value: &str) -> Result<(), String> {
        super::entry(service, name)?
            .set_password(value)
            .map_err(|e| format!("Failed to store secret: {}", e))
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
mod platform {
    pub fn list_names(_service: &str) -> Result<Vec<String>, String> {
        Err("Listing secrets is not supported on this platform".into())
    }

    pub fn write_ephemeral(service: &str, name: &str, value: &str) -> Result<(), String> {
        super::entry(service, name)?
            .set_password(value)
            .map_err(|e| format!("Failed to store secret: {}", e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ttl_units() {
        assert_eq!(parse_ttl("30s").unwrap(), Duration::from_secs(30));
        assert_eq!(parse_ttl("15m").unwrap(), Duration::from_secs(900));
        assert_eq!(parse_ttl("2h").unwrap(), Duration::from_secs(7200));
        assert_eq!(parse_ttl("1d").unwrap(), Duration::from_secs(86400));
    }

    #[test]
    fn ttl_rejects_bad_input() {
        for bad in ["", "30", "0m", "m", "5w", "1.5h", "-1h", "99999999999999999999d"] {
            assert!(parse_ttl(bad).is_err(), "{bad} should be rejected");
        }
    }
}
