//! Store credentials in the operating system's secure credential vault.

use serde::{Deserialize, Serialize};

/// Credential vault service identifier.
pub const SERVICE: &str = "io.github.desenrolado_s.Legenda";

/// Credential vault account identifier.
pub const ACCOUNT: &str = "opensubtitles";

/// Stored OpenSubtitles credentials.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Credentials {
    pub key: String,
    pub user: String,
    pub password: String,
}

/// Trait abstracting a system credential vault for deterministic testing.
pub trait Vault {
    fn get_password(&self, service: &str, account: &str) -> Result<Option<String>, String>;
    fn set_password(&self, service: &str, account: &str, password: &str) -> Result<(), String>;
    fn delete_password(&self, service: &str, account: &str) -> Result<(), String>;
}

/// Encodes credentials into the JSON payload stored in the system keychain.
pub fn encode_credentials(key: &str, user: &str, password: &str) -> String {
    serde_json::to_string(&Credentials {
        key: key.to_string(),
        user: user.to_string(),
        password: password.to_string(),
    })
    .expect("serializing Credentials cannot fail")
}

/// Decodes credentials from a JSON payload stored in the system keychain.
pub fn decode_credentials(raw: &str) -> Result<Option<Credentials>, String> {
    if raw.trim().is_empty() {
        return Ok(None);
    }
    let value: serde_json::Value =
        serde_json::from_str(raw).map_err(|_| "Invalid stored credentials".to_string())?;
    let Some(obj) = value.as_object() else {
        return Err("Invalid stored credentials".to_string());
    };
    let Some(key) = obj.get("key").and_then(|v| v.as_str()) else {
        return Err("Invalid stored credentials".to_string());
    };
    let Some(user) = obj.get("user").and_then(|v| v.as_str()) else {
        return Err("Invalid stored credentials".to_string());
    };
    let Some(password) = obj.get("password").and_then(|v| v.as_str()) else {
        return Err("Invalid stored credentials".to_string());
    };
    Ok(Some(Credentials {
        key: key.to_string(),
        user: user.to_string(),
        password: password.to_string(),
    }))
}

/// Loads credentials from the given vault backend.
pub fn load_with(vault: &dyn Vault) -> Result<Option<Credentials>, String> {
    let Some(raw) = vault.get_password(SERVICE, ACCOUNT)? else {
        return Ok(None);
    };
    decode_credentials(&raw)
}

/// Saves credentials into the given vault backend.
pub fn save_with(
    vault: &dyn Vault,
    key: &str,
    user: &str,
    password: &str,
) -> Result<(), String> {
    let payload = encode_credentials(key, user, password);
    vault.set_password(SERVICE, ACCOUNT, &payload)
}

/// Deletes stored credentials from the given vault backend if present.
pub fn forget_with(vault: &dyn Vault) -> Result<(), String> {
    if vault.get_password(SERVICE, ACCOUNT)?.is_some() {
        vault.delete_password(SERVICE, ACCOUNT)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[derive(Default)]
    struct MockVault {
        stored: RefCell<Option<(String, String, String)>>,
        deleted: RefCell<Vec<(String, String)>>,
        locked: bool,
    }

    impl Vault for MockVault {
        fn get_password(&self, service: &str, account: &str) -> Result<Option<String>, String> {
            if self.locked {
                return Err("locked".to_string());
            }
            let borrow = self.stored.borrow();
            match &*borrow {
                Some((s, a, p)) if s == service && a == account => Ok(Some(p.clone())),
                _ => Ok(None),
            }
        }

        fn set_password(
            &self,
            service: &str,
            account: &str,
            password: &str,
        ) -> Result<(), String> {
            if self.locked {
                return Err("locked".to_string());
            }
            *self.stored.borrow_mut() =
                Some((service.to_string(), account.to_string(), password.to_string()));
            Ok(())
        }

        fn delete_password(&self, service: &str, account: &str) -> Result<(), String> {
            if self.locked {
                return Err("locked".to_string());
            }
            self.deleted
                .borrow_mut()
                .push((service.to_string(), account.to_string()));
            *self.stored.borrow_mut() = None;
            Ok(())
        }
    }

    #[test]
    fn test_save_load_forget() {
        let vault = MockVault::default();
        assert_eq!(load_with(&vault).unwrap(), None);

        save_with(&vault, "key", "user", "secret").unwrap();
        let loaded = load_with(&vault).unwrap().unwrap();
        assert_eq!(loaded.key, "key");
        assert_eq!(loaded.user, "user");
        assert_eq!(loaded.password, "secret");

        forget_with(&vault).unwrap();
        assert_eq!(
            *vault.deleted.borrow(),
            vec![(SERVICE.to_string(), ACCOUNT.to_string())]
        );
        assert_eq!(load_with(&vault).unwrap(), None);
    }

    #[test]
    fn test_failure_has_no_fallback() {
        let vault = MockVault {
            locked: true,
            ..Default::default()
        };
        assert_eq!(save_with(&vault, "key", "user", "secret"), Err("locked".to_string()));
    }
}
