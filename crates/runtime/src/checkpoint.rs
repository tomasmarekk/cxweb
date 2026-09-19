//! Versioned local checkpoint encryption; never a native provider artifact.
use aes_gcm::{
    Aes256Gcm, KeyInit, Nonce,
    aead::{Aead, Payload},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use cxweb_platform::{atomic_file::Snapshot, secret};
use serde_json::json;
use std::path::Path;
use zeroize::Zeroizing;

const LIMIT: usize = 2 * 1024 * 1024;
const DOMAIN: &str = "cxweb.checkpoint.aes256gcm.v1";

/// These are runtime-owned scope values, never trusted from an encrypted item.
/// Session is the stable native task identity, not its changing context-window ID.
pub struct Binding<'a> {
    pub installation: &'a str,
    pub session: &'a str,
    pub account: &'a str,
    pub workspace: &'a str,
    pub route: &'a str,
    pub epoch: u64,
    pub codec: &'a str,
}
impl Binding<'_> {
    fn aad(&self) -> Result<Vec<u8>, &'static str> {
        if [
            self.installation,
            self.session,
            self.account,
            self.workspace,
            self.route,
            self.codec,
        ]
        .iter()
        .any(|s| s.is_empty() || s.len() > 512 || s.chars().any(char::is_control))
            || !self.route.starts_with("webbridge/")
            || self.route == "webbridge/"
        {
            return Err("E_CHECKPOINT_SCOPE");
        }
        serde_json::to_vec(&json!([
            DOMAIN,
            self.installation,
            self.session,
            self.account,
            self.workspace,
            self.route,
            self.epoch,
            self.codec
        ]))
        .map_err(|_| "E_CHECKPOINT_SCOPE")
    }
}

// No Debug/Serialize implementation: this contains key material.
pub struct Codec {
    key: Zeroizing<[u8; 32]>,
    installation: String,
}
impl Codec {
    /// Caller holds the installation lock. Existing unreadable/corrupt key state
    /// fails closed and is never replaced, regenerated or read as plaintext.
    pub fn load_or_create(path: &Path, installation: &str) -> Result<Self, &'static str> {
        if installation.is_empty()
            || installation.len() > 512
            || installation.chars().any(char::is_control)
        {
            return Err("E_CHECKPOINT_SCOPE");
        }
        let context =
            serde_json::to_vec(&json!([DOMAIN, installation])).map_err(|_| "E_CHECKPOINT_SCOPE")?;
        let snapshot = Snapshot::capture(path).map_err(|_| "E_CHECKPOINT_KEY")?;
        let key = if snapshot.existed() {
            let key = secret::unprotect_key(snapshot.original(), &context)
                .map_err(|_| "E_CHECKPOINT_KEY")?;
            snapshot
                .verify_unchanged()
                .map_err(|_| "E_CHECKPOINT_KEY")?;
            key
        } else {
            let key = Zeroizing::new(rand::random::<[u8; 32]>());
            let wrapped = secret::protect_key(&key, &context).map_err(|_| "E_CHECKPOINT_KEY")?;
            let stage = snapshot
                .stage(
                    &format!(".cxweb-key-{:032x}.tmp", rand::random::<u128>()),
                    &wrapped,
                )
                .map_err(|_| "E_CHECKPOINT_KEY")?;
            snapshot
                .commit(&stage, &wrapped)
                .map_err(|_| "E_CHECKPOINT_KEY")?;
            key
        };
        Ok(Self {
            key,
            installation: installation.into(),
        })
    }

    fn aad(&self, binding: &Binding<'_>) -> Result<Vec<u8>, &'static str> {
        if binding.installation != self.installation {
            return Err("E_CHECKPOINT_SCOPE");
        }
        binding.aad()
    }

    pub fn seal(&self, binding: &Binding<'_>, plaintext: &[u8]) -> Result<String, &'static str> {
        if plaintext.is_empty() || plaintext.len() > LIMIT {
            return Err("E_CHECKPOINT_LIMIT");
        }
        let aad = self.aad(binding)?;
        let cipher =
            Aes256Gcm::new_from_slice(self.key.as_ref()).map_err(|_| "E_CHECKPOINT_KEY")?;
        let nonce = rand::random::<[u8; 12]>();
        let ciphertext = cipher
            .encrypt(
                &Nonce::from(nonce),
                Payload {
                    msg: plaintext,
                    aad: &aad,
                },
            )
            .map_err(|_| "E_CHECKPOINT_INVALID")?;
        let mut token = nonce.to_vec();
        token.extend(ciphertext);
        Ok(format!("wbr1:{}", URL_SAFE_NO_PAD.encode(token)))
    }

    pub fn unseal(
        &self,
        binding: &Binding<'_>,
        token: &str,
    ) -> Result<Zeroizing<Vec<u8>>, &'static str> {
        let aad = self.aad(binding)?;
        if token.len() > 5 + (LIMIT + 28).div_ceil(3) * 4 {
            return Err("E_CHECKPOINT_LIMIT");
        }
        let encoded = token.strip_prefix("wbr1:").ok_or("E_CHECKPOINT_INVALID")?;
        let bytes = URL_SAFE_NO_PAD
            .decode(encoded)
            .map_err(|_| "E_CHECKPOINT_INVALID")?;
        if bytes.len() <= 28 || bytes.len() > LIMIT + 28 {
            return Err("E_CHECKPOINT_INVALID");
        }
        let nonce: [u8; 12] = bytes[..12].try_into().map_err(|_| "E_CHECKPOINT_INVALID")?;
        let cipher =
            Aes256Gcm::new_from_slice(self.key.as_ref()).map_err(|_| "E_CHECKPOINT_KEY")?;
        cipher
            .decrypt(
                &Nonce::from(nonce),
                Payload {
                    msg: &bytes[12..],
                    aad: &aad,
                },
            )
            .map(Zeroizing::new)
            .map_err(|_| "E_CHECKPOINT_INVALID")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn binding() -> Binding<'static> {
        Binding {
            installation: "fixture",
            session: "task",
            account: "account",
            workspace: "workspace",
            route: "webbridge/fixture",
            epoch: 1,
            codec: "responses-v2-fixture",
        }
    }
    fn codec() -> Codec {
        Codec {
            key: Zeroizing::new(rand::random()),
            installation: "fixture".into(),
        }
    }

    #[test]
    fn checkpoint_is_authenticated_randomized_and_bound_to_every_scope_dimension() {
        let codec = codec();
        let token = codec.seal(&binding(), b"synthetic summary").unwrap();
        assert!(!token.contains("synthetic summary"));
        assert_ne!(token, codec.seal(&binding(), b"synthetic summary").unwrap());
        assert_eq!(
            codec.unseal(&binding(), &token).unwrap().as_slice(),
            b"synthetic summary"
        );
        let different_key = Codec {
            key: Zeroizing::new(rand::random()),
            installation: "fixture".into(),
        };
        assert!(different_key.unseal(&binding(), &token).is_err());
        for field in 0..7 {
            let mut scope = binding();
            match field {
                0 => scope.installation = "other",
                1 => scope.session = "other",
                2 => scope.account = "other",
                3 => scope.workspace = "other",
                4 => scope.route = "webbridge/other",
                5 => scope.epoch = 2,
                _ => scope.codec = "other",
            }
            assert!(codec.unseal(&scope, &token).is_err());
        }
        let mut bytes = URL_SAFE_NO_PAD
            .decode(token.strip_prefix("wbr1:").unwrap())
            .unwrap();
        for index in [0, 12, bytes.len() - 1] {
            bytes[index] ^= 1;
            assert!(
                codec
                    .unseal(
                        &binding(),
                        &format!("wbr1:{}", URL_SAFE_NO_PAD.encode(&bytes))
                    )
                    .is_err()
            );
            bytes[index] ^= 1;
        }
        for invalid in ["", "native-encrypted", "wbr2:abc", "wbr1:a", "wbr1:"] {
            assert!(codec.unseal(&binding(), invalid).is_err());
        }
        assert!(codec.seal(&binding(), &vec![0; LIMIT + 1]).is_err());
    }

    #[test]
    fn dpapi_key_survives_reopen_and_corruption_never_regenerates_it() {
        let directory =
            std::env::temp_dir().join(format!("cxweb-checkpoint-{:032x}", rand::random::<u128>()));
        cxweb_platform::state::protected_directory(&directory).unwrap();
        let path = directory.join("key.dpapi");
        let first = Codec::load_or_create(&path, "fixture").unwrap();
        let token = first.seal(&binding(), b"synthetic summary").unwrap();
        let original = std::fs::read(&path).unwrap();
        assert!(!original.windows(32).any(|part| part == first.key.as_ref()));
        drop(first);
        assert!(Codec::load_or_create(&path, "other").is_err());
        assert_eq!(std::fs::read(&path).unwrap(), original);
        let reopened = Codec::load_or_create(&path, "fixture").unwrap();
        assert_eq!(
            reopened.unseal(&binding(), &token).unwrap().as_slice(),
            b"synthetic summary"
        );
        std::fs::write(&path, b"corrupt wrapped key").unwrap();
        assert!(Codec::load_or_create(&path, "fixture").is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"corrupt wrapped key");
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(directory).unwrap();
    }
}
