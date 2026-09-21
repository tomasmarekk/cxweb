//! Current-user package registration; no activation, process or credential access.
use std::io;
use windows_sys::Win32::{
    Foundation::{ERROR_INSUFFICIENT_BUFFER, ERROR_SUCCESS},
    Storage::Packaging::Appx::{FindPackagesByPackageFamily, PACKAGE_FILTER_HEAD},
};

fn presence(result: u32, count: u32) -> io::Result<bool> {
    match (result, count) {
        (ERROR_SUCCESS, 0) => Ok(false),
        (ERROR_INSUFFICIENT_BUFFER, 1..) => Ok(true),
        // Unexpected output is not evidence of absence.
        (ERROR_SUCCESS | ERROR_INSUFFICIENT_BUFFER, _) => {
            Err(io::Error::other("E_PACKAGE_INVENTORY"))
        }
        _ => Err(io::Error::from_raw_os_error(result as i32)),
    }
}

pub fn codex_registered() -> io::Result<bool> {
    // Independently observed official package family. An App backend cache is
    // not a registration, and registration alone does not qualify its backend.
    registered("OpenAI.Codex_2p2nqsd0c76g0")
}

fn registered(family: &str) -> io::Result<bool> {
    let family: Vec<u16> = family.encode_utf16().chain(std::iter::once(0)).collect();
    let mut count = 0;
    let mut length = 0;
    // SAFETY: family is NUL terminated and both output counters are writable.
    // The documented sizing call takes null buffers. Only existence is needed;
    // names and account identity are neither allocated nor returned.
    let result = unsafe {
        FindPackagesByPackageFamily(
            family.as_ptr(),
            PACKAGE_FILTER_HEAD,
            &mut count,
            std::ptr::null_mut(),
            &mut length,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    presence(result, count)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_successful_empty_inventory_proves_absence() {
        assert!(!presence(ERROR_SUCCESS, 0).unwrap());
        assert!(presence(ERROR_INSUFFICIENT_BUFFER, 1).unwrap());
        assert!(presence(ERROR_INSUFFICIENT_BUFFER, 9).unwrap());
        for (status, count) in [
            (5, 0),
            (5, 1),
            (ERROR_SUCCESS, 1),
            (ERROR_INSUFFICIENT_BUFFER, 0),
        ] {
            assert!(presence(status, count).is_err());
        }
    }

    #[test]
    #[ignore = "requires the official Codex App registered for the current Windows user"]
    fn registered_codex_package_is_observed_without_launching_it() {
        assert!(codex_registered().unwrap());
        assert!(!registered("Cxweb.AbsentInventoryFixture_2p2nqsd0c76g0").unwrap());
    }
}
