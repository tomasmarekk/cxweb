//! Wall-clock observation timestamps, not deadlines or elapsed-time accounting.
#[cfg(windows)]
pub fn utc_timestamp() -> Option<String> {
    use windows::Win32::{
        Foundation::SYSTEMTIME,
        System::{SystemInformation::GetSystemTimeAsFileTime, Time::FileTimeToSystemTime},
    };
    let mut time = SYSTEMTIME::default();
    // SAFETY: the OS returns a value and writes only to the live SYSTEMTIME.
    unsafe {
        FileTimeToSystemTime(&GetSystemTimeAsFileTime(), &mut time).ok()?;
    }
    Some(format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        time.wYear,
        time.wMonth,
        time.wDay,
        time.wHour,
        time.wMinute,
        time.wSecond,
        time.wMilliseconds
    ))
}

#[cfg(not(windows))]
pub fn utc_timestamp() -> Option<String> {
    let time = time::OffsetDateTime::now_utc();
    Some(format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        time.year(),
        time.month() as u8,
        time.day(),
        time.hour(),
        time.minute(),
        time.second(),
        time.millisecond()
    ))
}

#[cfg(test)]
mod tests {
    #[test]
    fn observation_timestamp_has_utc_millisecond_precision() {
        let timestamp = super::utc_timestamp().expect("system clock observation");
        assert_eq!(timestamp.len(), 24);
        for (index, byte) in timestamp.bytes().enumerate() {
            match index {
                4 | 7 => assert_eq!(byte, b'-'),
                10 => assert_eq!(byte, b'T'),
                13 | 16 => assert_eq!(byte, b':'),
                19 => assert_eq!(byte, b'.'),
                23 => assert_eq!(byte, b'Z'),
                _ => assert!(byte.is_ascii_digit()),
            }
        }
    }
}
