//! Current-user DPAPI key wrapping. No UI, machine-wide key or plaintext fallback.
use crate::state::LocalAllocation;
use std::ptr::{null, null_mut};
use windows_sys::Win32::Security::Cryptography::{
    CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
};
use zeroize::{Zeroize, Zeroizing};

fn transform(
    input: &[u8],
    context: &[u8],
    protect: bool,
) -> Result<Zeroizing<Vec<u8>>, &'static str> {
    if input.is_empty() || input.len() > 64 * 1024 || context.is_empty() || context.len() > 4096 {
        return Err("E_SECRET_STORE");
    }
    let input = CRYPT_INTEGER_BLOB {
        cbData: input.len() as u32,
        pbData: input.as_ptr().cast_mut(),
    };
    let entropy = CRYPT_INTEGER_BLOB {
        cbData: context.len() as u32,
        pbData: context.as_ptr().cast_mut(),
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: null_mut(),
    };
    // SAFETY: input buffers remain live and are read-only according to DPAPI's
    // contract. Output is owned LocalAlloc storage. Only current-user protection
    // is requested, with no prompt or machine flag. Plaintext OS storage is
    // zeroized before LocalFree, as is the owned return buffer on its later drop.
    unsafe {
        let success = if protect {
            CryptProtectData(
                &input,
                null(),
                &entropy,
                null(),
                null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        } else {
            CryptUnprotectData(
                &input,
                null_mut(),
                &entropy,
                null(),
                null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        };
        if success == 0 {
            return Err("E_SECRET_STORE");
        }
        let _allocation = LocalAllocation(output.pbData.cast());
        if output.pbData.is_null() {
            return Err("E_SECRET_STORE");
        }
        let bytes = std::slice::from_raw_parts_mut(output.pbData, output.cbData as usize);
        let result = if bytes.len() <= 64 * 1024 {
            Ok(Zeroizing::new(bytes.to_vec()))
        } else {
            Err("E_SECRET_STORE")
        };
        bytes.zeroize();
        result
    }
}

pub fn protect_key(key: &[u8; 32], context: &[u8]) -> Result<Vec<u8>, &'static str> {
    Ok(transform(key, context, true)?.to_vec())
}

pub fn unprotect_key(wrapped: &[u8], context: &[u8]) -> Result<Zeroizing<[u8; 32]>, &'static str> {
    let bytes = transform(wrapped, context, false)?;
    if bytes.len() != 32 {
        return Err("E_SECRET_STORE");
    }
    let mut key = Zeroizing::new([0u8; 32]);
    key.copy_from_slice(&bytes);
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_user_key_round_trip_rejects_changed_context_and_corruption() {
        let key = [42u8; 32];
        let mut wrapped = protect_key(&key, b"cxweb-test-installation").unwrap();
        assert!(!wrapped.windows(key.len()).any(|window| window == key));
        assert!(unprotect_key(&wrapped, b"other-installation").is_err());
        assert_eq!(
            *unprotect_key(&wrapped, b"cxweb-test-installation").unwrap(),
            key
        );
        let end = wrapped.len() - 1;
        wrapped[end] ^= 1;
        assert!(unprotect_key(&wrapped, b"cxweb-test-installation").is_err());
        assert!(unprotect_key(&[], b"cxweb-test-installation").is_err());
    }
}
